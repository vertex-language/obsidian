//! Hardware-aware linear-scan register allocator (`RegAllocResult`, `allocate_registers`).

use alloc::vec;
use alloc::vec::Vec;
use obsidian_abi::{AbiSig, ArgLoc, FrameLayout, RegBank};
use obsidian_ir::{Arch, Function, Opcode, Target, Type, Value};
use crate::liveness::LivenessInfo;

/// Complete physical register assignment and stack frame layout for an [`ir::Function`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegAllocResult {
    /// Computed calling convention signature for the function.
    pub abi: AbiSig,
    /// Physical location (`ArgLoc::Reg`, `ArgLoc::RegPair`, or `ArgLoc::Stack` spill slot)
    /// assigned to each `Value(i)` (`locations[val.index()]`).
    pub locations: Vec<ArgLoc>,
    /// Live intervals and call-site points for the function.
    pub liveness: LivenessInfo,
    /// Bitmask of callee-saved GPRs allocated by this function (must be saved/restored in prologue/epilogue).
    pub dirty_callee_saved_mask: u32,
    /// Total stack frame bytes used by static `stack.slot`s plus register spill slots.
    pub local_and_spill_bytes: u32,
    /// `true` if the function makes zero calls (`Opcode::Call` / `CallIndirect`).
    pub is_leaf: bool,
    /// Final aligned prologue/epilogue stack frame layout (including leaf Red-Zone optimization).
    pub frame: FrameLayout,
}

impl RegAllocResult {
    /// Returns the physical [`ArgLoc`] assigned to `val`.
    #[inline]
    pub fn loc(&self, val: Value) -> ArgLoc {
        self.locations[val.index()]
    }
}

/// Active register occupancy state during the linear scan.
#[derive(Copy, Clone, Debug)]
struct ActiveReg {
    bank: RegBank,
    hw_reg: u8,
    end_point: u32,
}

/// Runs hardware-aware linear-scan register allocation on `func` for `target`.
pub fn allocate_registers(target: Target, func: &Function) -> RegAllocResult {
    let abi = AbiSig::for_function(target, func);

    // Extract return-register hints from `abi.returns`:
    let mut ret_hints = Vec::with_capacity(abi.returns.len());
    for loc in &abi.returns {
        match *loc {
            ArgLoc::Reg { hw_reg, .. } => ret_hints.push(Some(hw_reg)),
            _ => ret_hints.push(None),
        }
    }

    let liveness = LivenessInfo::compute(func, &ret_hints);
    let num_insts = func.insts.len();
    let mut locations = vec![
        ArgLoc::Stack {
            offset: 0,
            size: 0,
            align: 1,
        };
        num_insts
    ];

    let mut active: Vec<ActiveReg> = Vec::with_capacity(16);
    let mut occupied_gpr_mask = abi.reserved_gpr_mask;
    let mut occupied_addr_mask = 0u32; // Used on M68k for A0..A5
    let mut occupied_vec_mask = 0u32;
    let mut dirty_callee_saved_mask = 0u32;
    let mut spill_offset = liveness.static_slot_bytes;
    let mut false_dep_ring_cursor = 0u8;

    let is_two_addr_arch = matches!(target.arch, Arch::X86_64 | Arch::X86_32 | Arch::M68k);
    let ptr_bytes = target.ptr_bytes();

    // 1. Pre-color entry block parameters (`Block(0)`):
    let entry_params = &func.blocks[0].params;
    for (i, &param_val) in entry_params.iter().enumerate() {
        let range = liveness.ranges[param_val.index()];
        let abi_loc = abi.params[i];

        // If an entry parameter lives in a caller-saved register AND its live range crosses a `Call`,
        // allocate a callee-saved register for it so the prologue/entry move preserves it across the call!
        if range.spans_call && abi_loc.is_reg() {
            if let Some(callee_reg) = pick_free_bit(abi.callee_saved_gpr_mask & !occupied_gpr_mask) {
                let loc = ArgLoc::gpr(callee_reg);
                locations[param_val.index()] = loc;
                occupied_gpr_mask |= 1u32 << callee_reg;
                dirty_callee_saved_mask |= 1u32 << callee_reg;
                active.push(ActiveReg {
                    bank: RegBank::Gpr,
                    hw_reg: callee_reg,
                    end_point: range.last_use_point,
                });
                continue;
            }
        }

        locations[param_val.index()] = abi_loc;
        match abi_loc {
            ArgLoc::Reg { bank, hw_reg } => {
                mark_occupied(
                    bank,
                    hw_reg,
                    &mut occupied_gpr_mask,
                    &mut occupied_addr_mask,
                    &mut occupied_vec_mask,
                );
                if range.last_use_point > 0 {
                    active.push(ActiveReg {
                        bank,
                        hw_reg,
                        end_point: range.last_use_point,
                    });
                } else {
                    unmark_occupied(
                        bank,
                        hw_reg,
                        &mut occupied_gpr_mask,
                        &mut occupied_addr_mask,
                        &mut occupied_vec_mask,
                    );
                }
            }
            ArgLoc::RegPair { bank, lo_reg, hi_reg } => {
                mark_occupied(bank, lo_reg, &mut occupied_gpr_mask, &mut occupied_addr_mask, &mut occupied_vec_mask);
                mark_occupied(bank, hi_reg, &mut occupied_gpr_mask, &mut occupied_addr_mask, &mut occupied_vec_mask);
                if range.last_use_point > 0 {
                    active.push(ActiveReg { bank, hw_reg: lo_reg, end_point: range.last_use_point });
                    active.push(ActiveReg { bank, hw_reg: hi_reg, end_point: range.last_use_point });
                } else {
                    unmark_occupied(bank, lo_reg, &mut occupied_gpr_mask, &mut occupied_addr_mask, &mut occupied_vec_mask);
                    unmark_occupied(bank, hi_reg, &mut occupied_gpr_mask, &mut occupied_addr_mask, &mut occupied_vec_mask);
                }
            }
            ArgLoc::Stack { .. } => {}
        }
    }

    // 2. Walk blocks and instructions linearly in program-point order:
    for (b_idx, block) in func.blocks.iter().enumerate() {
        // Allocate registers for non-entry block parameters (`loop(%i, %sum):`):
        if b_idx > 0 && !block.params.is_empty() {
            let bp_point = liveness.ranges[block.params[0].index()].def_point;
            expire_old_intervals(
                bp_point,
                &mut active,
                &mut occupied_gpr_mask,
                &mut occupied_addr_mask,
                &mut occupied_vec_mask,
            );
            for &p_val in &block.params {
                let ty = func.insts[p_val.index()].ty;
                let range = liveness.ranges[p_val.index()];
                let loc = alloc_value_loc(
                    target,
                    &abi,
                    ty,
                    range,
                    None,
                    false,
                    &mut false_dep_ring_cursor,
                    &mut occupied_gpr_mask,
                    &mut occupied_addr_mask,
                    &mut occupied_vec_mask,
                    &mut dirty_callee_saved_mask,
                    &mut spill_offset,
                );
                locations[p_val.index()] = loc;
                register_active(loc, range.last_use_point, &mut active);
            }
        }

        for &inst_val in &block.insts {
            let idx = inst_val.index();
            let node = func.insts[idx];
            if node.op == Opcode::Tombstone || node.op == Opcode::ResultProj || node.ty == Type::Void {
                continue;
            }

            let point = liveness.inst_points[idx];

            // Expire any active registers whose `last_use_point <= point` BEFORE allocating `dst`,
            // so dead input registers (including two-address `src0`!) become available right at `point`!
            expire_old_intervals(
                point,
                &mut active,
                &mut occupied_gpr_mask,
                &mut occupied_addr_mask,
                &mut occupied_vec_mask,
            );

            // Check if this instruction is a two-address destructive operation where `src0` just died at `point`:
            let mut tied_src0_reg: Option<u8> = None;
            if is_two_addr_arch
                && matches!(
                    node.op,
                    Opcode::Add
                        | Opcode::Sub
                        | Opcode::Mul
                        | Opcode::And
                        | Opcode::Or
                        | Opcode::Xor
                        | Opcode::Shl
                        | Opcode::ShrU
                        | Opcode::ShrS
                )
            {
                let src0 = Value(node.args[0]);
                if liveness.ranges[src0.index()].last_use_point == point {
                    if let ArgLoc::Reg { bank: RegBank::Gpr, hw_reg } = locations[src0.index()] {
                        if (occupied_gpr_mask & (1u32 << hw_reg)) == 0 {
                            tied_src0_reg = Some(hw_reg);
                        }
                    }
                }
            }

            // Check if this instruction is an x86 false-dependency unary bit-count (`Clz`, `Ctz`, `Popcnt`):
            let is_false_dep_op = target.arch == Arch::X86_64
                && matches!(node.op, Opcode::Clz | Opcode::Ctz | Opcode::Popcnt);

            let range = liveness.ranges[idx];
            let loc = alloc_value_loc(
                target,
                &abi,
                node.ty,
                range,
                tied_src0_reg,
                is_false_dep_op,
                &mut false_dep_ring_cursor,
                &mut occupied_gpr_mask,
                &mut occupied_addr_mask,
                &mut occupied_vec_mask,
                &mut dirty_callee_saved_mask,
                &mut spill_offset,
            );
            locations[idx] = loc;
            if range.last_use_point > point {
                register_active(loc, range.last_use_point, &mut active);
            } else {
                // Value is dead immediately after definition: free its register right away!
                release_loc(
                    loc,
                    &mut occupied_gpr_mask,
                    &mut occupied_addr_mask,
                    &mut occupied_vec_mask,
                );
            }

            // If this instruction produces a second result (`ResultProj` at `idx + 1`), allocate it too:
            if node.has_second_result() && idx + 1 < num_insts {
                let ty2 = func.insts[idx + 1].ty;
                let range2 = liveness.ranges[idx + 1];
                let loc2 = alloc_value_loc(
                    target,
                    &abi,
                    ty2,
                    range2,
                    None,
                    false,
                    &mut false_dep_ring_cursor,
                    &mut occupied_gpr_mask,
                    &mut occupied_addr_mask,
                    &mut occupied_vec_mask,
                    &mut dirty_callee_saved_mask,
                    &mut spill_offset,
                );
                locations[idx + 1] = loc2;
                if range2.last_use_point > point {
                    register_active(loc2, range2.last_use_point, &mut active);
                } else {
                    release_loc(
                        loc2,
                        &mut occupied_gpr_mask,
                        &mut occupied_addr_mask,
                        &mut occupied_vec_mask,
                    );
                }
            }
        }
    }

    let is_leaf = liveness.call_points.is_empty();
    let has_pushed_ret = matches!(target.arch, Arch::X86_64 | Arch::X86_32 | Arch::M68k);
    let frame = FrameLayout::compute(
        &abi,
        spill_offset,
        if is_leaf { 0 } else { abi.stack_arg_bytes },
        is_leaf,
        dirty_callee_saved_mask,
        ptr_bytes,
        has_pushed_ret,
    );

    RegAllocResult {
        abi,
        locations,
        liveness,
        dirty_callee_saved_mask,
        local_and_spill_bytes: spill_offset,
        is_leaf,
        frame,
    }
}

#[allow(clippy::too_many_arguments)]
fn alloc_value_loc(
    target: Target,
    abi: &AbiSig,
    ty: Type,
    range: crate::liveness::LiveRange,
    tied_src0_reg: Option<u8>,
    is_false_dep_op: bool,
    false_dep_ring_cursor: &mut u8,
    occupied_gpr_mask: &mut u32,
    occupied_addr_mask: &mut u32,
    occupied_vec_mask: &mut u32,
    dirty_callee_saved_mask: &mut u32,
    spill_offset: &mut u32,
) -> ArgLoc {
    // 1. Floating-point and SIMD vector types (`F32`, `F64`, `V128`):
    if matches!(ty, Type::F32 | Type::F64 | Type::V128) {
        let free_vec = !*occupied_vec_mask & 0x0000_FFFF;
        if let Some(vreg) = pick_free_bit(free_vec) {
            *occupied_vec_mask |= 1u32 << vreg;
            return ArgLoc::float_vec(vreg);
        }
        return alloc_spill_slot(ty, target.ptr_bytes(), spill_offset);
    }

    // 2. M68k Address Register bank (`A0..A5`) for pointer values (`Type::Ptr`):
    if target.arch == Arch::M68k && ty == Type::Ptr {
        let free_addr = !*occupied_addr_mask & 0b0011_1111; // A0..A5 (A6=FP, A7=SP reserved)
        if let Some(areg) = pick_free_bit(free_addr) {
            *occupied_addr_mask |= 1u32 << areg;
            if areg >= 2 {
                // A2..A5 are callee-saved on M68k (bits 10..=13 in unified mask):
                *dirty_callee_saved_mask |= 1u32 << (areg + 8);
            }
            return ArgLoc::addr_reg(areg);
        }
        return alloc_spill_slot(ty, target.ptr_bytes(), spill_offset);
    }

    // 3. General-Purpose Integer / Pointer Register bank (`RegBank::Gpr`):
    //    3a. If this value does not cross a call and has a return-register hint (`RAX`/`X0`), prefer it!
    if !range.spans_call {
        if let Some(hint) = range.hint_reg {
            if (*occupied_gpr_mask & (1u32 << hint)) == 0 {
                *occupied_gpr_mask |= 1u32 << hint;
                if (abi.callee_saved_gpr_mask & (1u32 << hint)) != 0 {
                    *dirty_callee_saved_mask |= 1u32 << hint;
                }
                return ArgLoc::gpr(hint);
            }
        }
    }

    //    3b. Two-address destructive operand tie (`tied_src0_reg`):
    if !range.spans_call && !is_false_dep_op {
        if let Some(src0_reg) = tied_src0_reg {
            if (*occupied_gpr_mask & (1u32 << src0_reg)) == 0 {
                *occupied_gpr_mask |= 1u32 << src0_reg;
                if (abi.callee_saved_gpr_mask & (1u32 << src0_reg)) != 0 {
                    *dirty_callee_saved_mask |= 1u32 << src0_reg;
                }
                return ArgLoc::gpr(src0_reg);
            }
        }
    }

    //    3c. 4-Register False-Dependency Rotation Ring (`RAX, RCX, RDX, R8..R11` on x86-64):
    if is_false_dep_op && !range.spans_call {
        let free_caller = abi.caller_saved_gpr_mask & !*occupied_gpr_mask;
        if let Some(rot_reg) = pick_nth_free_bit(free_caller, *false_dep_ring_cursor) {
            *false_dep_ring_cursor = (*false_dep_ring_cursor + 1) & 3;
            *occupied_gpr_mask |= 1u32 << rot_reg;
            return ArgLoc::gpr(rot_reg);
        }
    }

    //    3d. Standard caller-saved vs callee-saved selection based on `range.spans_call`:
    let primary_pool = if range.spans_call {
        abi.callee_saved_gpr_mask & !*occupied_gpr_mask
    } else {
        abi.caller_saved_gpr_mask & !*occupied_gpr_mask
    };

    if let Some(reg) = pick_free_bit(primary_pool) {
        *occupied_gpr_mask |= 1u32 << reg;
        if (abi.callee_saved_gpr_mask & (1u32 << reg)) != 0 {
            *dirty_callee_saved_mask |= 1u32 << reg;
        }
        return ArgLoc::gpr(reg);
    }

    //    3e. Fallback to the secondary pool (callee-saved if caller-saved is full):
    let fallback_pool = abi.callee_saved_gpr_mask & !*occupied_gpr_mask;
    if let Some(reg) = pick_free_bit(fallback_pool) {
        *occupied_gpr_mask |= 1u32 << reg;
        *dirty_callee_saved_mask |= 1u32 << reg;
        return ArgLoc::gpr(reg);
    }

    //    3f. All physical registers are occupied -> allocate a stack spill slot!
    alloc_spill_slot(ty, target.ptr_bytes(), spill_offset)
}

fn alloc_spill_slot(ty: Type, ptr_bytes: u8, spill_offset: &mut u32) -> ArgLoc {
    let size = match ty {
        Type::I1 | Type::I8 => 1u32,
        Type::I16 => 2,
        Type::I32 | Type::F32 => 4,
        Type::I128 | Type::V128 => 16,
        Type::Ptr => ptr_bytes as u32,
        _ => 8,
    };
    let align = size.min(16);
    *spill_offset = (*spill_offset + align - 1) & !(align - 1);
    let loc = ArgLoc::Stack {
        offset: *spill_offset as i32,
        size: size as u8,
        align: align as u8,
    };
    *spill_offset += size;
    loc
}

#[inline]
fn pick_free_bit(mask: u32) -> Option<u8> {
    if mask == 0 {
        None
    } else {
        Some(mask.trailing_zeros() as u8)
    }
}

/// Picks the `nth` (modulo popcount) set bit in `mask` for 4-register ring rotation.
#[inline]
fn pick_nth_free_bit(mask: u32, nth: u8) -> Option<u8> {
    let count = mask.count_ones();
    if count == 0 {
        return None;
    }
    let target_idx = (nth as u32) % count;
    let mut m = mask;
    let mut i = 0;
    while i < target_idx {
        m &= m - 1;
        i += 1;
    }
    Some(m.trailing_zeros() as u8)
}

#[inline]
fn mark_occupied(
    bank: RegBank,
    hw_reg: u8,
    gpr_mask: &mut u32,
    addr_mask: &mut u32,
    vec_mask: &mut u32,
) {
    match bank {
        RegBank::Gpr      => *gpr_mask |= 1u32 << hw_reg,
        RegBank::Addr     => *addr_mask |= 1u32 << hw_reg,
        RegBank::FloatVec => *vec_mask |= 1u32 << hw_reg,
    }
}

#[inline]
fn unmark_occupied(
    bank: RegBank,
    hw_reg: u8,
    gpr_mask: &mut u32,
    addr_mask: &mut u32,
    vec_mask: &mut u32,
) {
    match bank {
        RegBank::Gpr      => *gpr_mask &= !(1u32 << hw_reg),
        RegBank::Addr     => *addr_mask &= !(1u32 << hw_reg),
        RegBank::FloatVec => *vec_mask &= !(1u32 << hw_reg),
    }
}

fn register_active(loc: ArgLoc, end_point: u32, active: &mut Vec<ActiveReg>) {
    match loc {
        ArgLoc::Reg { bank, hw_reg } => {
            active.push(ActiveReg { bank, hw_reg, end_point });
        }
        ArgLoc::RegPair { bank, lo_reg, hi_reg } => {
            active.push(ActiveReg { bank, hw_reg: lo_reg, end_point });
            active.push(ActiveReg { bank, hw_reg: hi_reg, end_point });
        }
        ArgLoc::Stack { .. } => {}
    }
}

fn release_loc(
    loc: ArgLoc,
    gpr_mask: &mut u32,
    addr_mask: &mut u32,
    vec_mask: &mut u32,
) {
    match loc {
        ArgLoc::Reg { bank, hw_reg } => unmark_occupied(bank, hw_reg, gpr_mask, addr_mask, vec_mask),
        ArgLoc::RegPair { bank, lo_reg, hi_reg } => {
            unmark_occupied(bank, lo_reg, gpr_mask, addr_mask, vec_mask);
            unmark_occupied(bank, hi_reg, gpr_mask, addr_mask, vec_mask);
        }
        ArgLoc::Stack { .. } => {}
    }
}

fn expire_old_intervals(
    current_point: u32,
    active: &mut Vec<ActiveReg>,
    gpr_mask: &mut u32,
    addr_mask: &mut u32,
    vec_mask: &mut u32,
) {
    let mut i = 0;
    while i < active.len() {
        if active[i].end_point <= current_point {
            let expired = active.swap_remove(i);
            unmark_occupied(expired.bank, expired.hw_reg, gpr_mask, addr_mask, vec_mask);
        } else {
            i += 1;
        }
    }
}
