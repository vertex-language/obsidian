//! Linear-time SSA live-interval analysis (`LiveRange`, `LivenessInfo`).

use alloc::vec;
use alloc::vec::Vec;
use obsidian_ir::{Function, Opcode, Type, Value};

/// Half-open live interval `[def_point, last_use_point]` for a single SSA [`Value`].
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct LiveRange {
    /// Linear program point (`0..num_points`) where this value is defined.
    pub def_point: u32,
    /// Linear program point (`>= def_point`) of the last instruction that reads this value.
    pub last_use_point: u32,
    /// `true` if a function call (`Call`, `CallIndirect`, `CallUnwind`) occurs strictly
    /// while this value is live (`def_point < call_point < last_use_point`).
    pub spans_call: bool,
    /// Optional preferred physical register hint (e.g., return register `RAX`/`X0` if this value is returned).
    pub hint_reg: Option<u8>,
}

impl LiveRange {
    /// Returns `true` if this value's lifetime expires at or before `point`.
    #[inline]
    pub const fn expires_at_or_before(self, point: u32) -> bool {
        self.last_use_point <= point
    }
}

/// Complete liveness and call-site table for an [`ir::Function`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LivenessInfo {
    /// Indexed by `Value(i).index()`.
    pub ranges: Vec<LiveRange>,
    /// Linear program point assigned to each `Value(i)`.
    pub inst_points: Vec<u32>,
    /// Program points of all non-tail `Call` / `CallIndirect` instructions.
    pub call_points: Vec<u32>,
    /// Total bytes requested by static `stack.slot` instructions in the function.
    pub static_slot_bytes: u32,
}

impl LivenessInfo {
    /// Computes live intervals, call-spanning flags, and return-register hints in $O(N)$ time.
    pub fn compute(func: &Function, ret_hint_regs: &[Option<u8>]) -> Self {
        let num_insts = func.insts.len();
        let mut ranges = vec![
            LiveRange {
                def_point: 0,
                last_use_point: 0,
                spans_call: false,
                hint_reg: None,
            };
            num_insts
        ];
        let mut inst_points = vec![0u32; num_insts];
        let mut call_points = Vec::new();
        let mut static_slot_bytes = 0u32;

        let mut point = 1u32;

        // Entry parameters are live from program point 0:
        for &p in &func.blocks[0].params {
            ranges[p.index()].def_point = 0;
            ranges[p.index()].last_use_point = 0;
        }

        // Walk blocks and instructions in layout order:
        for (b_idx, block) in func.blocks.iter().enumerate() {
            let block_start_point = point;
            if b_idx > 0 {
                for &p in &block.params {
                    ranges[p.index()].def_point = point;
                    ranges[p.index()].last_use_point = point;
                }
                point += 1;
            }

            for &inst_val in &block.insts {
                let idx = inst_val.index();
                let node = func.insts[idx];
                if node.op == Opcode::Tombstone {
                    continue;
                }

                inst_points[idx] = point;
                ranges[idx].def_point = point;
                if ranges[idx].last_use_point < point {
                    ranges[idx].last_use_point = point;
                }
                if node.has_second_result() && idx + 1 < num_insts {
                    inst_points[idx + 1] = point;
                    ranges[idx + 1].def_point = point;
                    if ranges[idx + 1].last_use_point < point {
                        ranges[idx + 1].last_use_point = point;
                    }
                }

                // Extend `last_use_point` for all operands read by `node`:
                match node.op {
                    Opcode::Const | Opcode::PtrSym | Opcode::FrameAddr | Opcode::ReturnAddr | Opcode::Trap | Opcode::SpinHint | Opcode::MemFence => {}
                    Opcode::StackSlot => {
                        let size = node.args[0];
                        let align = node.args[1].max(1);
                        static_slot_bytes = ((static_slot_bytes + align - 1) & !(align - 1)) + size;
                    }
                    Opcode::Load | Opcode::LoadExt | Opcode::Clz | Opcode::Ctz | Opcode::Popcnt | Opcode::ByteSwap | Opcode::BitReverse | Opcode::Neg | Opcode::Not | Opcode::Abs | Opcode::ExtU | Opcode::ExtS | Opcode::Trunc | Opcode::Bitcast | Opcode::FromFloatSatS | Opcode::FromFloatSatU | Opcode::FromFloatTruncS | Opcode::FromFloatTruncU | Opcode::FloatFromIntS | Opcode::FloatFromIntU | Opcode::FloatPromote | Opcode::FloatDemote | Opcode::FSqrt | Opcode::FFloor | Opcode::FCeil | Opcode::FTrunc | Opcode::FNearest | Opcode::AtomicLoad | Opcode::ResumeUnwind => {
                        extend_use(&mut ranges, Value(node.args[0]), point);
                    }
                    Opcode::Select => {
                        extend_use(&mut ranges, Value(node.aux16 as u16 as u32), point);
                        extend_use(&mut ranges, Value(node.args[0]), point);
                        extend_use(&mut ranges, Value(node.args[1]), point);
                    }
                    Opcode::Call => {
                        call_points.push(point);
                        let args = &func.value_lists[node.args[1] as usize];
                        for &arg in args {
                            extend_use(&mut ranges, arg, point);
                        }
                    }
                    Opcode::Br => {
                        let target_bb = node.args[0] as usize;
                        let args = &func.value_lists[node.args[1] as usize];
                        for &arg in args {
                            extend_use(&mut ranges, arg, point);
                        }
                        // If this is a backward loop edge (`target_bb <= b_idx`), any value live at
                        // the top of `target_bb` must remain live through the end of the loop (`point`)!
                        if target_bb <= b_idx {
                            extend_loop_carried(&mut ranges, block_start_point, point);
                        }
                    }
                    Opcode::BrIf => {
                        extend_use(&mut ranges, Value(node.args[0]), point);
                        let targets = &func.br_if_targets[node.args[1] as usize];
                        for &arg in &targets.then_args {
                            extend_use(&mut ranges, arg, point);
                        }
                        for &arg in &targets.else_args {
                            extend_use(&mut ranges, arg, point);
                        }
                        if targets.then_bb.index() <= b_idx || targets.else_bb.index() <= b_idx {
                            extend_loop_carried(&mut ranges, block_start_point, point);
                        }
                    }
                    Opcode::Ret => {
                        let vals = &func.value_lists[node.args[0] as usize];
                        for (ret_i, &v) in vals.iter().enumerate() {
                            extend_use(&mut ranges, v, point);
                            if let Some(Some(hw_reg)) = ret_hint_regs.get(ret_i) {
                                ranges[v.index()].hint_reg = Some(*hw_reg);
                            }
                        }
                    }
                    _ => {
                        extend_use(&mut ranges, Value(node.args[0]), point);
                        if !node.has_inline_imm1() {
                            extend_use(&mut ranges, Value(node.args[1]), point);
                        }
                    }
                }

                point += 1;
            }
        }

        // Mark values whose live interval `[def_point, last_use_point]` spans a `Call`:
        if !call_points.is_empty() {
            for (idx, r) in ranges.iter_mut().enumerate() {
                if func.insts[idx].ty == Type::Void {
                    continue;
                }
                for &cp in &call_points {
                    if r.def_point < cp && cp < r.last_use_point {
                        r.spans_call = true;
                        break;
                    }
                }
            }
        }

        Self {
            ranges,
            inst_points,
            call_points,
            static_slot_bytes,
        }
    }
}

#[inline]
fn extend_use(ranges: &mut [LiveRange], val: Value, point: u32) {
    if let Some(r) = ranges.get_mut(val.index()) {
        if r.last_use_point < point {
            r.last_use_point = point;
        }
    }
}

fn extend_loop_carried(ranges: &mut [LiveRange], loop_header_point: u32, backedge_point: u32) {
    for r in ranges.iter_mut() {
        if r.def_point < loop_header_point && r.last_use_point >= loop_header_point && r.last_use_point < backedge_point {
            r.last_use_point = backedge_point;
        }
    }
}
