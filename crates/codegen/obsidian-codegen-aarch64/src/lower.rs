//! AArch64 (`A64`) instruction selection, inline barrel-shifter fusion, `LDP`/`STP` 128-bit pairing,
//! `LogicalImm` bitmask encoding, `CBZ`/`CBNZ` flagless branching, and 32-bit word emission.

use alloc::vec;
use alloc::vec::Vec;
use obsidian_abi::AbiSig;
use obsidian_ir as ir;
use obsidian_isa_aarch64::{
    addr::{LogicalImm, PairOffset7, Shift},
    cond::CondCode,
    encode::{self, EncodedInst},
    regs::{Gpr, GprOrSp, GprOrXzr},
    uarch,
};
use obsidian_obj::{Artifact, Reloc, RelocKind, SectionId};
use obsidian_regalloc::{allocate_registers, RegAllocResult};

/// Output of compiling a single [`ir::Function`] to AArch64 (`A64`) machine code.
#[derive(Clone, Debug)]
pub struct CompiledFunction {
    /// Raw Little-Endian `A64` machine bytes (always a multiple of 4 bytes).
    pub code: Vec<u8>,
    /// Relocations referencing module symbols, with `offset` relative to the start of `code`.
    pub relocs: Vec<Reloc>,
    /// Register allocation and stack frame layout used for this function.
    pub regalloc: RegAllocResult,
}

#[derive(Copy, Clone)]
enum BranchPatchKind {
    /// 26-bit unconditional branch `B` (`bits 0..26`).
    B26,
    /// 19-bit conditional branch `B.cond` (`bits 5..24`, preserving cond in `0..4`).
    BCond19,
    /// 19-bit compare-and-branch `CBZ` / `CBNZ` (`bits 5..24`, preserving Rt in `0..5`).
    Cbz19,
}

struct BranchPatch {
    /// Byte offset of the 4-byte branch instruction in `code`.
    inst_offset: usize,
    kind: BranchPatchKind,
    target_block: ir::Block,
}

/// Compiles an entire [`ir::Module`] into an [`obsidian_obj::Artifact`] for AArch64.
pub fn compile_module(module: &ir::Module) -> Artifact {
    let mut art = Artifact::new(&module.name, module.target);
    let nop_bytes = encode::nop().to_le_bytes();

    for (name, kind) in &module.symbols {
        match *kind {
            ir::SymKind::Const(idx) => {
                let c = &module.consts[idx];
                art.define_rodata(name, ir::Linkage::Local, c.attrs.align, &c.bytes);
            }
            ir::SymKind::Global(idx) => {
                let g = &module.globals[idx];
                let bytes = g.init_bits.to_le_bytes();
                art.define_data(name, ir::Linkage::Export, g.attrs.align, &bytes);
            }
            ir::SymKind::Tls(idx) => {
                let t = &module.tls_vars[idx];
                let bytes = (t.init_bits as u32).to_le_bytes();
                art.define_tls_data(name, ir::Linkage::Local, t.attrs.align, &bytes);
            }
            ir::SymKind::ExternFn(_) => {
                art.declare_import(name);
            }
            ir::SymKind::Function(idx) => {
                let func = &module.functions[idx];
                let compiled = compile_function(module.target, func);
                let align = if func.attrs.align > 0 {
                    func.attrs.align as u32
                } else {
                    4
                };
                let sym = art.define_function(name, func.linkage, align, nop_bytes[0], &compiled.code);
                let fn_base_off = art.symbols[sym.index()].offset;
                for mut rel in compiled.relocs {
                    rel.offset += fn_base_off;
                    art.add_reloc(rel);
                }
            }
        }
    }

    art
}

/// Compiles a single [`ir::Function`] into 32-bit Little-Endian `A64` machine code.
pub fn compile_function(target: ir::Target, func: &ir::Function) -> CompiledFunction {
    let ra = allocate_registers(target, func);
    let mut code = Vec::with_capacity(func.insts.len() * 4 + 16);
    let mut relocs = Vec::new();
    let mut block_offsets = vec![0usize; func.blocks.len()];
    let mut patches: Vec<BranchPatch> = Vec::new();

    let folded = compute_folded_nodes(func);

    // Collect callee-saved GPRs modified by this function:
    let mut saved_gprs: Vec<Gpr> = Vec::new();
    for reg in Gpr::ALL {
        if (ra.dirty_callee_saved_mask & (1u32 << (reg as u8))) != 0 {
            saved_gprs.push(reg);
        }
    }

    let needs_fp_lr_save = !ra.is_leaf || !saved_gprs.is_empty();

    // =========================================================================
    // 1. Function Prologue
    // =========================================================================
    if needs_fp_lr_save {
        // Save FP (X29) and LR (X30) via `stp x29, x30, [sp, #-16]!`
        let neg16 = PairOffset7::try_new_64(-16).unwrap();
        emit(&mut code, encode::stp64_pre(
            GprOrXzr::Gpr(Gpr::FP),
            GprOrXzr::Gpr(Gpr::LR),
            GprOrSp::Sp,
            neg16,
        ));
    }
    for &reg in &saved_gprs {
        let neg16 = PairOffset7::try_new_64(-16).unwrap();
        emit(&mut code, encode::stp64_pre(
            GprOrXzr::Gpr(reg),
            GprOrXzr::Xzr,
            GprOrSp::Sp,
            neg16,
        ));
    }
    if ra.frame.sp_sub_bytes > 0 {
        emit(&mut code, encode::sub64_imm(
            GprOrSp::Sp,
            GprOrSp::Sp,
            (ra.frame.sp_sub_bytes & 0xFFF) as u16,
            false,
        ));
    }

    // Move any call-spanning entry parameters into their promoted callee-saved registers:
    for (i, &param_val) in func.blocks[0].params.iter().enumerate() {
        let assigned = gpr_of(&ra, param_val);
        if let Some(abi_reg) = ra.abi.params[i].as_aarch64_gpr() {
            if assigned != abi_reg {
                emit(&mut code, encode::mov64_rr(assigned, abi_reg));
            }
        }
    }

    // =========================================================================
    // 2. Basic Block & Instruction Emission
    // =========================================================================
    for (b_idx, block) in func.blocks.iter().enumerate() {
        block_offsets[b_idx] = code.len();
        let inst_list = &block.insts;
        let mut i = 0usize;

        while i < inst_list.len() {
            let inst_val = inst_list[i];
            let idx = inst_val.index();
            if folded[idx] {
                i += 1;
                continue;
            }
            let node = func.insts[idx];

            // --- Adjacent 64-Bit Load Pair (`LDP`) Fusion Check ---
            if node.op == ir::Opcode::Load && matches!(node.ty, ir::Type::I64 | ir::Type::Ptr) && i + 1 < inst_list.len() {
                let next_val = inst_list[i + 1];
                let next_node = func.insts[next_val.index()];
                if !folded[next_val.index()]
                    && next_node.op == ir::Opcode::Load
                    && matches!(next_node.ty, ir::Type::I64 | ir::Type::Ptr)
                    && node.args[0] == next_node.args[0]
                {
                    let rt1 = gpr_of(&ra, inst_val);
                    let rt2 = gpr_of(&ra, next_val);
                    let base_reg = GprOrSp::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    if let Some(pair_off) = uarch::can_fuse_into_ldp64(
                        rt1,
                        rt2,
                        base_reg,
                        node.aux16,
                        next_node.aux16,
                        false,
                    ) {
                        emit(
                            &mut code,
                            encode::ldp64_offset(
                                GprOrXzr::Gpr(rt1),
                                GprOrXzr::Gpr(rt2),
                                base_reg,
                                pair_off,
                            ),
                        );
                        i += 2; // Consumed both loads in 1 128-bit `LDP` instruction!
                        continue;
                    }
                }
            }

            // --- Adjacent 64-Bit Store Pair (`STP`) Fusion Check ---
            if node.op == ir::Opcode::Store && i + 1 < inst_list.len() {
                let next_val = inst_list[i + 1];
                let next_node = func.insts[next_val.index()];
                if next_node.op == ir::Opcode::Store && node.args[0] == next_node.args[0] {
                    if let Some(pair_off) = uarch::can_fuse_into_stp64(node.aux16, next_node.aux16) {
                        let rt1 = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));
                        let rt2 = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(next_node.args[1])));
                        let base_reg = GprOrSp::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                        emit(&mut code, encode::stp64_offset(rt1, rt2, base_reg, pair_off));
                        i += 2; // Consumed both stores in 1 128-bit `STP` instruction!
                        continue;
                    }
                }
            }

            match node.op {
                ir::Opcode::Tombstone | ir::Opcode::BlockParam | ir::Opcode::ResultProj => {}

                ir::Opcode::Const => {
                    let dst = gpr_of(&ra, inst_val);
                    let bits = func.const_bits(inst_val).unwrap_or(0);
                    emit_materialize_const(&mut code, dst, bits);
                }

                ir::Opcode::PtrSym => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let sym = ir::Sym(node.args[0]);
                    let offset = node.args[1] as i64;

                    let adrp_off = code.len() as u32;
                    emit(&mut code, encode::adrp(dst, 0));
                    relocs.push(Reloc {
                        section: SectionId::Text,
                        offset: adrp_off,
                        sym,
                        kind: RelocKind::AArch64AdrpPage21,
                        addend: offset,
                    });

                    let add_off = code.len() as u32;
                    let dst_sp = GprOrSp::Gpr(gpr_of(&ra, inst_val));
                    emit(&mut code, encode::add64_imm(dst_sp, dst_sp, 0, false));
                    relocs.push(Reloc {
                        section: SectionId::Text,
                        offset: add_off,
                        sym,
                        kind: RelocKind::AArch64PageOff12Add,
                        addend: offset,
                    });
                }

                ir::Opcode::PtrAdd | ir::Opcode::Add => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs = gpr_of(&ra, ir::Value(node.args[0]));
                    if node.has_inline_imm1() {
                        let imm = node.inline_imm1();
                        if imm >= 0 && imm <= 4095 {
                            emit(
                                &mut code,
                                encode::add64_imm(GprOrSp::Gpr(dst), GprOrSp::Gpr(lhs), imm as u16, false),
                            );
                        } else if imm < 0 && (-imm) <= 4095 {
                            emit(
                                &mut code,
                                encode::sub64_imm(GprOrSp::Gpr(dst), GprOrSp::Gpr(lhs), (-imm) as u16, false),
                            );
                        }
                    } else {
                        let rhs_val = ir::Value(node.args[1]);
                        let (rhs_reg, shift) = resolve_shifted_reg(func, &ra, rhs_val);
                        emit(
                            &mut code,
                            encode::add64_shifted(
                                GprOrXzr::Gpr(dst),
                                GprOrXzr::Gpr(lhs),
                                GprOrXzr::Gpr(rhs_reg),
                                shift,
                            ),
                        );
                    }
                }

                ir::Opcode::Sub | ir::Opcode::PtrDiff => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs = gpr_of(&ra, ir::Value(node.args[0]));
                    if node.has_inline_imm1() {
                        let imm = node.inline_imm1();
                        if imm >= 0 && imm <= 4095 {
                            emit(
                                &mut code,
                                encode::sub64_imm(GprOrSp::Gpr(dst), GprOrSp::Gpr(lhs), imm as u16, false),
                            );
                        }
                    } else {
                        let rhs_val = ir::Value(node.args[1]);
                        let (rhs_reg, shift) = resolve_shifted_reg(func, &ra, rhs_val);
                        emit(
                            &mut code,
                            encode::sub64_shifted(
                                GprOrXzr::Gpr(dst),
                                GprOrXzr::Gpr(lhs),
                                GprOrXzr::Gpr(rhs_reg),
                                shift,
                            ),
                        );
                    }
                }

                ir::Opcode::Mul => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let lhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let rhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));
                    emit(&mut code, encode::mul64(dst, lhs, rhs));
                }

                ir::Opcode::DivU => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let lhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let rhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));
                    emit(&mut code, encode::udiv64(dst, lhs, rhs));
                }

                ir::Opcode::DivS => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let lhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let rhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));
                    emit(&mut code, encode::sdiv64(dst, lhs, rhs));
                }

                ir::Opcode::And | ir::Opcode::Or | ir::Opcode::Xor => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    if node.has_inline_imm1() {
                        let imm_u64 = (node.inline_imm1() as i64) as u64;
                        if let Some(limm) = LogicalImm::try_encode64(imm_u64) {
                            let dst_sp = GprOrSp::Gpr(dst);
                            match node.op {
                                ir::Opcode::And => emit(&mut code, encode::and64_imm(dst_sp, lhs, limm)),
                                ir::Opcode::Or  => emit(&mut code, encode::orr64_imm(dst_sp, lhs, limm)),
                                _               => emit(&mut code, encode::eor64_imm(dst_sp, lhs, limm)),
                            }
                        }
                    } else {
                        let (rhs_reg, shift) = resolve_shifted_reg(func, &ra, ir::Value(node.args[1]));
                        let dst_xzr = GprOrXzr::Gpr(dst);
                        let rhs_xzr = GprOrXzr::Gpr(rhs_reg);
                        match node.op {
                            ir::Opcode::And => emit(&mut code, encode::and64_shifted(dst_xzr, lhs, rhs_xzr, shift)),
                            ir::Opcode::Or  => emit(&mut code, encode::orr64_shifted(dst_xzr, lhs, rhs_xzr, shift)),
                            _               => emit(&mut code, encode::eor64_shifted(dst_xzr, lhs, rhs_xzr, shift)),
                        }
                    }
                }

                ir::Opcode::Shl | ir::Opcode::ShrU => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let lhs = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let shift = if node.has_inline_imm1() {
                        node.inline_imm1() as u8
                    } else {
                        func.const_bits(ir::Value(node.args[1])).unwrap_or(0) as u8
                    };
                    if node.op == ir::Opcode::Shl {
                        emit(&mut code, encode::lsl64_imm(dst, lhs, shift));
                    } else {
                        emit(&mut code, encode::lsr64_imm(dst, lhs, shift));
                    }
                }

                ir::Opcode::Clz => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let src = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    emit(&mut code, encode::clz64(dst, src));
                }

                ir::Opcode::Ctz => {
                    // Tier-2 hardware synthesis on AArch64: `rbit xd, xn` + `clz xd, xd` (2 cycles)!
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let src = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    emit(&mut code, encode::rbit64(dst, src));
                    emit(&mut code, encode::clz64(dst, dst));
                }

                ir::Opcode::BitReverse => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let src = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    emit(&mut code, encode::rbit64(dst, src));
                }

                ir::Opcode::ByteSwap => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let src = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    emit(&mut code, encode::rev64(dst, src));
                }

                ir::Opcode::ICmp => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let cc = emit_icmp_flags(&mut code, &ra, &node);
                    emit(&mut code, encode::cset64(dst, cc));
                }

                ir::Opcode::Select => {
                    let dst = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let cond_val = ir::Value(node.aux16 as u16 as u32);
                    let if_true = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let if_false = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));

                    let cond_node = func.insts[cond_val.index()];
                    let cc = if cond_node.op == ir::Opcode::ICmp {
                        emit_icmp_flags(&mut code, &ra, &cond_node)
                    } else {
                        let cond_reg = GprOrSp::Gpr(gpr_of(&ra, cond_val));
                        emit(&mut code, encode::cmp64_imm(cond_reg, 0, false));
                        CondCode::NE
                    };

                    emit(&mut code, encode::csel64(dst, if_true, if_false, cc));
                }

                ir::Opcode::Load => {
                    let rt = GprOrXzr::Gpr(gpr_of(&ra, inst_val));
                    let base = GprOrSp::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let off = node.aux16;
                    if off >= 0 && (off % 8) == 0 && (off / 8) <= 4095 {
                        emit(&mut code, encode::ldr64_uimm(rt, base, (off / 8) as u16));
                    } else {
                        emit(&mut code, encode::ldur64(rt, base, off));
                    }
                }

                ir::Opcode::Store => {
                    let rt = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(node.args[1])));
                    let base = GprOrSp::Gpr(gpr_of(&ra, ir::Value(node.args[0])));
                    let off = node.aux16;
                    if off >= 0 && (off % 8) == 0 && (off / 8) <= 4095 {
                        emit(&mut code, encode::str64_uimm(rt, base, (off / 8) as u16));
                    } else {
                        emit(&mut code, encode::stur64(rt, base, off));
                    }
                }

                ir::Opcode::Call => {
                    let sym = ir::Sym(node.args[0]);
                    let call_args = &func.value_lists[node.args[1] as usize];
                    let arg_types: Vec<ir::Type> = call_args.iter().map(|&v| func.value_type(v)).collect();
                    let call_abi = AbiSig::compute(target, ir::CallConv::C, &arg_types, &[node.ty]);

                    for (arg_i, &arg_val) in call_args.iter().enumerate() {
                        let src_reg = gpr_of(&ra, arg_val);
                        if let Some(dst_reg) = call_abi.params[arg_i].as_aarch64_gpr() {
                            if src_reg != dst_reg {
                                emit(&mut code, encode::mov64_rr(dst_reg, src_reg));
                            }
                        }
                    }

                    let bl_off = code.len() as u32;
                    emit(&mut code, encode::bl_rel26(0));
                    relocs.push(Reloc {
                        section: SectionId::Text,
                        offset: bl_off,
                        sym,
                        kind: RelocKind::AArch64Call26,
                        addend: 0,
                    });

                    if node.ty != ir::Type::Void {
                        let dst = gpr_of(&ra, inst_val);
                        if dst != Gpr::X0 {
                            emit(&mut code, encode::mov64_rr(dst, Gpr::X0));
                        }
                    }
                }

                ir::Opcode::Br => {
                    let target_bb = ir::Block(node.args[0]);
                    let args = &func.value_lists[node.args[1] as usize];
                    emit_block_arg_moves(&mut code, func, &ra, target_bb, args);
                    if target_bb.index() != b_idx + 1 {
                        let inst_offset = code.len();
                        emit(&mut code, encode::b_rel26(0));
                        patches.push(BranchPatch {
                            inst_offset,
                            kind: BranchPatchKind::B26,
                            target_block: target_bb,
                        });
                    }
                }

                ir::Opcode::BrIf => {
                    let cond_val = ir::Value(node.args[0]);
                    let targets = &func.br_if_targets[node.args[1] as usize];
                    let cond_node = func.insts[cond_val.index()];

                    // Check if we can emit a single flagless `CBZ` / `CBNZ` (comparing against 0):
                    if cond_node.op == ir::Opcode::ICmp
                        && cond_node.has_inline_imm1()
                        && cond_node.inline_imm1() == 0
                        && matches!(
                            ir::IntCC::from_raw(cond_node.subop),
                            Some(ir::IntCC::Eq | ir::IntCC::Ne)
                        )
                        && targets.then_args.is_empty()
                        && targets.else_args.is_empty()
                        && (targets.else_bb.index() == b_idx + 1 || targets.then_bb.index() == b_idx + 1)
                    {
                        let rt = GprOrXzr::Gpr(gpr_of(&ra, ir::Value(cond_node.args[0])));
                        let inst_offset = code.len();
                        let is_eq = ir::IntCC::from_raw(cond_node.subop) == Some(ir::IntCC::Eq);
                        let (branch_on_zero, target_block) = if targets.else_bb.index() == b_idx + 1 {
                            (is_eq, targets.then_bb)
                        } else {
                            (!is_eq, targets.else_bb)
                        };
                        if branch_on_zero {
                            emit(&mut code, encode::cbz64_rel19(rt, 0));
                        } else {
                            emit(&mut code, encode::cbnz64_rel19(rt, 0));
                        }
                        patches.push(BranchPatch {
                            inst_offset,
                            kind: BranchPatchKind::Cbz19,
                            target_block,
                        });
                        i += 1;
                        continue;
                    }

                    let cc = if cond_node.op == ir::Opcode::ICmp {
                        emit_icmp_flags(&mut code, &ra, &cond_node)
                    } else {
                        let cond_reg = GprOrSp::Gpr(gpr_of(&ra, cond_val));
                        emit(&mut code, encode::cmp64_imm(cond_reg, 0, false));
                        CondCode::NE
                    };

                    if targets.then_args.is_empty() && targets.else_bb.index() == b_idx + 1 {
                        let inst_offset = code.len();
                        emit(&mut code, encode::b_cond_rel19(cc, 0));
                        patches.push(BranchPatch {
                            inst_offset,
                            kind: BranchPatchKind::BCond19,
                            target_block: targets.then_bb,
                        });
                        emit_block_arg_moves(&mut code, func, &ra, targets.else_bb, &targets.else_args);
                    } else {
                        emit_block_arg_moves(&mut code, func, &ra, targets.then_bb, &targets.then_args);
                        let inst_offset = code.len();
                        emit(&mut code, encode::b_cond_rel19(cc, 0));
                        patches.push(BranchPatch {
                            inst_offset,
                            kind: BranchPatchKind::BCond19,
                            target_block: targets.then_bb,
                        });
                        emit_block_arg_moves(&mut code, func, &ra, targets.else_bb, &targets.else_args);
                        if targets.else_bb.index() != b_idx + 1 {
                            let b_offset = code.len();
                            emit(&mut code, encode::b_rel26(0));
                            patches.push(BranchPatch {
                                inst_offset: b_offset,
                                kind: BranchPatchKind::B26,
                                target_block: targets.else_bb,
                            });
                        }
                    }
                }

                ir::Opcode::Ret => {
                    let vals = &func.value_lists[node.args[0] as usize];
                    for (ret_i, &v) in vals.iter().enumerate() {
                        if let Some(ret_reg) = ra.abi.returns.get(ret_i).and_then(|l| l.as_aarch64_gpr()) {
                            let src_reg = gpr_of(&ra, v);
                            if src_reg != ret_reg {
                                emit(&mut code, encode::mov64_rr(ret_reg, src_reg));
                            }
                        }
                    }

                    if ra.frame.sp_sub_bytes > 0 {
                        emit(&mut code, encode::add64_imm(
                            GprOrSp::Sp,
                            GprOrSp::Sp,
                            (ra.frame.sp_sub_bytes & 0xFFF) as u16,
                            false,
                        ));
                    }
                    let pos16 = PairOffset7::try_new_64(16).unwrap();
                    for &reg in saved_gprs.iter().rev() {
                        emit(&mut code, encode::ldp64_post(
                            GprOrXzr::Gpr(reg),
                            GprOrXzr::Xzr,
                            GprOrSp::Sp,
                            pos16,
                        ));
                    }
                    if needs_fp_lr_save {
                        emit(&mut code, encode::ldp64_post(
                            GprOrXzr::Gpr(Gpr::FP),
                            GprOrXzr::Gpr(Gpr::LR),
                            GprOrSp::Sp,
                            pos16,
                        ));
                    }
                    emit(&mut code, encode::ret_lr());
                }

                ir::Opcode::Trap => {
                    emit(&mut code, encode::brk(1));
                }

                _ => {}
            }

            i += 1;
        }
    }

    // =========================================================================
    // 3. Patch Intra-Function Branch Displacements (in 4-Byte Words)
    // =========================================================================
    for patch in patches {
        let target_byte_off = block_offsets[patch.target_block.index()] as i32;
        let inst_byte_off = patch.inst_offset as i32;
        let delta_words = (target_byte_off - inst_byte_off) / 4;
        let mut word = u32::from_le_bytes([
            code[patch.inst_offset],
            code[patch.inst_offset + 1],
            code[patch.inst_offset + 2],
            code[patch.inst_offset + 3],
        ]);
        match patch.kind {
            BranchPatchKind::B26 => {
                word = (word & 0xFC00_0000) | ((delta_words as u32) & 0x03FF_FFFF);
            }
            BranchPatchKind::BCond19 | BranchPatchKind::Cbz19 => {
                word = (word & 0xFF00_001F) | (((delta_words as u32) & 0x7_FFFF) << 5);
            }
        }
        code[patch.inst_offset..patch.inst_offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    CompiledFunction {
        code,
        relocs,
        regalloc: ra,
    }
}

/// Identifies single-use `Shl` and `ICmp` nodes folded into downstream AArch64 instructions.
fn compute_folded_nodes(func: &ir::Function) -> Vec<bool> {
    let n = func.insts.len();
    let mut folded = vec![false; n];

    for block in &func.blocks {
        for &inst_val in &block.insts {
            let node = func.insts[inst_val.index()];
            match node.op {
                ir::Opcode::PtrAdd | ir::Opcode::Add | ir::Opcode::Sub | ir::Opcode::And | ir::Opcode::Or | ir::Opcode::Xor
                    if !node.has_inline_imm1() =>
                {
                    let rhs = ir::Value(node.args[1]);
                    let rhs_node = func.insts[rhs.index()];
                    if matches!(rhs_node.op, ir::Opcode::Shl | ir::Opcode::ShrU)
                        && rhs_node.use_count == 1
                        && rhs_node.has_inline_imm1()
                        && (rhs_node.inline_imm1() as u32) < 64
                    {
                        folded[rhs.index()] = true;
                    }
                }
                ir::Opcode::BrIf => {
                    let cond = ir::Value(node.args[0]);
                    if func.insts[cond.index()].op == ir::Opcode::ICmp
                        && func.insts[cond.index()].use_count == 1
                    {
                        folded[cond.index()] = true;
                    }
                }
                ir::Opcode::Select => {
                    let cond = ir::Value(node.aux16 as u16 as u32);
                    if func.insts[cond.index()].op == ir::Opcode::ICmp
                        && func.insts[cond.index()].use_count == 1
                    {
                        folded[cond.index()] = true;
                    }
                }
                _ => {}
            }
        }
    }

    folded
}

fn resolve_shifted_reg(
    func: &ir::Function,
    ra: &RegAllocResult,
    val: ir::Value,
) -> (Gpr, Shift) {
    let node = func.insts[val.index()];
    if node.use_count == 1 && node.has_inline_imm1() {
        let amt = (node.inline_imm1() & 0x3F) as u8;
        match node.op {
            ir::Opcode::Shl  => return (gpr_of(ra, ir::Value(node.args[0])), Shift::lsl(amt)),
            ir::Opcode::ShrU => return (gpr_of(ra, ir::Value(node.args[0])), Shift::lsr(amt)),
            _ => {}
        }
    }
    (gpr_of(ra, val), Shift::NONE)
}

fn emit_icmp_flags(
    code: &mut Vec<u8>,
    ra: &RegAllocResult,
    icmp_node: &ir::InstNode,
) -> CondCode {
    let cc = map_int_cc(ir::IntCC::from_raw(icmp_node.subop).unwrap_or(ir::IntCC::Eq));
    let lhs = gpr_of(ra, ir::Value(icmp_node.args[0]));
    if icmp_node.has_inline_imm1() {
        let imm = icmp_node.inline_imm1();
        if imm >= 0 && imm <= 4095 {
            emit(code, encode::cmp64_imm(GprOrSp::Gpr(lhs), imm as u16, false));
        }
    } else {
        let rhs = gpr_of(ra, ir::Value(icmp_node.args[1]));
        emit(
            code,
            encode::cmp64_shifted(GprOrXzr::Gpr(lhs), GprOrXzr::Gpr(rhs), Shift::NONE),
        );
    }
    cc
}

const fn map_int_cc(cc: ir::IntCC) -> CondCode {
    match cc {
        ir::IntCC::Eq  => CondCode::EQ,
        ir::IntCC::Ne  => CondCode::NE,
        ir::IntCC::Slt => CondCode::LT,
        ir::IntCC::Sle => CondCode::LE,
        ir::IntCC::Sgt => CondCode::GT,
        ir::IntCC::Sge => CondCode::GE,
        ir::IntCC::Ult => CondCode::LO,
        ir::IntCC::Ule => CondCode::LS,
        ir::IntCC::Ugt => CondCode::HI,
        ir::IntCC::Uge => CondCode::HS,
    }
}

fn emit_materialize_const(code: &mut Vec<u8>, dst: Gpr, bits: u64) {
    let rd = GprOrXzr::Gpr(dst);
    if bits == 0 {
        emit(code, encode::movz64(rd, 0, 0));
        return;
    }
    if bits == u64::MAX {
        emit(code, encode::movn64(rd, 0, 0));
        return;
    }

    let mut first = true;
    for hw in 0..4u8 {
        let chunk = ((bits >> (hw * 16)) & 0xFFFF) as u16;
        if chunk != 0 {
            if first {
                emit(code, encode::movz64(rd, chunk, hw));
                first = false;
            } else {
                emit(code, encode::movk64(rd, chunk, hw));
            }
        }
    }
}

fn emit_block_arg_moves(
    code: &mut Vec<u8>,
    func: &ir::Function,
    ra: &RegAllocResult,
    target_bb: ir::Block,
    args: &[ir::Value],
) {
    let params = &func.blocks[target_bb.index()].params;
    for (&param_val, &arg_val) in params.iter().zip(args.iter()) {
        let dst = gpr_of(ra, param_val);
        let src = gpr_of(ra, arg_val);
        if dst != src {
            emit(code, encode::mov64_rr(dst, src));
        }
    }
}

#[inline]
fn gpr_of(ra: &RegAllocResult, val: ir::Value) -> Gpr {
    ra.loc(val).as_aarch64_gpr().unwrap_or(Gpr::X0)
}

#[inline]
fn emit(code: &mut Vec<u8>, inst: EncodedInst) {
    code.extend_from_slice(&inst.to_le_bytes());
}
