//! x86-64 instruction selection, SIB address-mode fusion, `LEA` packing, and byte emission.

use alloc::vec;
use alloc::vec::Vec;
use obsidian_abi::AbiSig;
use obsidian_ir as ir;
use obsidian_isa_x86::{
    addr::{MemoryOperand, Scale},
    cond::CondCode,
    encode::{self, EncodedInst},
    regs::Gpr,
};
use obsidian_obj::{Artifact, Reloc, RelocKind, SectionId};
use obsidian_regalloc::{allocate_registers, RegAllocResult};

/// Output of compiling a single [`ir::Function`] to x86-64 machine code.
#[derive(Clone, Debug)]
pub struct CompiledFunction {
    /// Raw x86-64 machine bytes.
    pub code: Vec<u8>,
    /// Relocations referencing module symbols, with `offset` relative to the start of `code`.
    pub relocs: Vec<Reloc>,
    /// Register allocation and stack frame layout used for this function.
    pub regalloc: RegAllocResult,
}

/// Unresolved intra-function branch patch record.
struct BranchPatch {
    /// Offset in `code` where the 4-byte `rel32` displacement starts.
    disp32_offset: usize,
    /// Offset in `code` of the instruction immediately following this branch (`P + 4`).
    next_ip_offset: usize,
    /// Target basic block ID.
    target_block: ir::Block,
}

/// Compiles an entire [`ir::Module`] into an [`obsidian_obj::Artifact`] for x86-64.
pub fn compile_module(module: &ir::Module) -> Artifact {
    let mut art = Artifact::new(&module.name, module.target);

    // Pre-populate symbol table in the exact `Sym(0..N)` order used by `module.symbols`:
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
                    16
                };
                let sym = art.define_function(name, func.linkage, align, 0x90, &compiled.code);
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

/// Compiles a single [`ir::Function`] into x86-64 machine code bytes and relocations.
pub fn compile_function(target: ir::Target, func: &ir::Function) -> CompiledFunction {
    let ra = allocate_registers(target, func);
    let mut code = Vec::with_capacity(func.insts.len() * 4 + 16);
    let mut relocs = Vec::new();
    let mut block_offsets = vec![0usize; func.blocks.len()];
    let mut patches: Vec<BranchPatch> = Vec::new();

    // Determine which instructions are folded directly into downstream addressing modes or compares
    // so we don't emit redundant standalone instructions for them:
    let folded = compute_folded_nodes(func);

    // Collect callee-saved GPRs modified by this function in deterministic order:
    let mut saved_gprs: Vec<Gpr> = Vec::new();
    for reg in Gpr::ALL {
        if (ra.dirty_callee_saved_mask & (1u32 << (reg as u8))) != 0 {
            saved_gprs.push(reg);
        }
    }

    // =========================================================================
    // 1. Function Prologue
    // =========================================================================
    for &reg in &saved_gprs {
        emit(&mut code, encode::push64_r(reg));
    }
    if ra.frame.sp_sub_bytes > 0 {
        emit(&mut code, encode::sub64_ri(Gpr::Rsp, ra.frame.sp_sub_bytes as i32));
    }

    // If any entry parameter was promoted to a callee-saved register across a call, move it now:
    for (i, &param_val) in func.blocks[0].params.iter().enumerate() {
        let assigned = gpr_of(&ra, param_val);
        if let Some(abi_reg) = ra.abi.params[i].as_x86_gpr() {
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

        for &inst_val in &block.insts {
            let idx = inst_val.index();
            if folded[idx] {
                continue;
            }
            let node = func.insts[idx];

            match node.op {
                ir::Opcode::Tombstone | ir::Opcode::BlockParam | ir::Opcode::ResultProj => {}

                ir::Opcode::Const => {
                    let dst = gpr_of(&ra, inst_val);
                    let bits = func.const_bits(inst_val).unwrap_or(0) as i64;
                    emit_materialize_const(&mut code, dst, bits);
                }

                ir::Opcode::PtrSym => {
                    let dst = gpr_of(&ra, inst_val);
                    let sym = ir::Sym(node.args[0]);
                    let offset = node.args[1] as i32;
                    // `lea dst, [rip + disp32]` -> 7 bytes (`48 8D 05 xx xx xx xx`), disp32 at `len - 4`:
                    emit(&mut code, encode::lea64(dst, MemoryOperand::rip_rel(0)));
                    let disp_off = (code.len() - 4) as u32;
                    relocs.push(Reloc {
                        section: SectionId::Text,
                        offset: disp_off,
                        sym,
                        kind: RelocKind::PcRel32,
                        addend: (offset as i64) - 4,
                    });
                }

                ir::Opcode::StackSlot => {
                    let dst = gpr_of(&ra, inst_val);
                    let slot_off = ra.frame.locals_offset as i32;
                    emit(
                        &mut code,
                        encode::lea64(dst, MemoryOperand::base_disp(Gpr::Rsp, slot_off)),
                    );
                }

                ir::Opcode::PtrAdd | ir::Opcode::Add => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs = ir::Value(node.args[0]);
                    let lhs_reg = gpr_of(&ra, lhs);

                    if node.has_inline_imm1() {
                        let imm = node.inline_imm1();
                        if dst == lhs_reg {
                            emit(&mut code, encode::add64_ri(dst, imm));
                        } else {
                            // Non-destructive 3-operand addition via `LEA dst, [lhs + imm]`!
                            emit(
                                &mut code,
                                encode::lea64(dst, MemoryOperand::base_disp(lhs_reg, imm)),
                            );
                        }
                    } else {
                        let rhs = ir::Value(node.args[1]);
                        // Check if `rhs` is a single-use `shl idx, 1|2|3` folded into `LEA`:
                        if let Some((idx_reg, scale)) = try_extract_scaled_index(func, &ra, rhs) {
                            emit(
                                &mut code,
                                encode::lea64(
                                    dst,
                                    MemoryOperand::base_index_disp(lhs_reg, idx_reg, scale, 0),
                                ),
                            );
                        } else {
                            let rhs_reg = gpr_of(&ra, rhs);
                            if dst == lhs_reg {
                                emit(&mut code, encode::add64_rr(dst, rhs_reg));
                            } else if dst == rhs_reg {
                                emit(&mut code, encode::add64_rr(dst, lhs_reg));
                            } else {
                                // Non-destructive 3-operand register add via `LEA dst, [lhs + rhs]`!
                                emit(
                                    &mut code,
                                    encode::lea64(
                                        dst,
                                        MemoryOperand::base_index_disp(lhs_reg, rhs_reg, Scale::S1, 0),
                                    ),
                                );
                            }
                        }
                    }
                }

                ir::Opcode::Sub | ir::Opcode::PtrDiff => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs_reg = gpr_of(&ra, ir::Value(node.args[0]));
                    if node.has_inline_imm1() {
                        let imm = node.inline_imm1();
                        if dst == lhs_reg {
                            emit(&mut code, encode::sub64_ri(dst, imm));
                        } else {
                            emit(
                                &mut code,
                                encode::lea64(dst, MemoryOperand::base_disp(lhs_reg, -imm)),
                            );
                        }
                    } else {
                        let rhs_reg = gpr_of(&ra, ir::Value(node.args[1]));
                        if dst == lhs_reg {
                            emit(&mut code, encode::sub64_rr(dst, rhs_reg));
                        } else if dst != rhs_reg {
                            emit(&mut code, encode::mov64_rr(dst, lhs_reg));
                            emit(&mut code, encode::sub64_rr(dst, rhs_reg));
                        } else {
                            // `dst == rhs_reg` and `lhs_reg != rhs_reg`:
                            emit(&mut code, encode::sub64_rr(dst, lhs_reg));
                            // Negate `dst` (`xor + sub` or `imul -1`):
                            emit(&mut code, encode::xor64_ri(dst, -1));
                            emit(&mut code, encode::add64_ri(dst, 1));
                        }
                    }
                }

                ir::Opcode::Mul => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs_reg = gpr_of(&ra, ir::Value(node.args[0]));
                    let rhs_reg = gpr_of(&ra, ir::Value(node.args[1]));
                    if dst == lhs_reg {
                        emit(&mut code, encode::imul64_rr(dst, rhs_reg));
                    } else if dst == rhs_reg {
                        emit(&mut code, encode::imul64_rr(dst, lhs_reg));
                    } else {
                        emit(&mut code, encode::mov64_rr(dst, lhs_reg));
                        emit(&mut code, encode::imul64_rr(dst, rhs_reg));
                    }
                }

                ir::Opcode::And | ir::Opcode::Or | ir::Opcode::Xor => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs_reg = gpr_of(&ra, ir::Value(node.args[0]));
                    if node.has_inline_imm1() {
                        let imm = node.inline_imm1();
                        if dst != lhs_reg {
                            emit(&mut code, encode::mov64_rr(dst, lhs_reg));
                        }
                        match node.op {
                            ir::Opcode::And => emit(&mut code, encode::and64_ri(dst, imm)),
                            ir::Opcode::Or  => emit(&mut code, encode::or64_ri(dst, imm)),
                            _               => emit(&mut code, encode::xor64_ri(dst, imm)),
                        }
                    } else {
                        let rhs_reg = gpr_of(&ra, ir::Value(node.args[1]));
                        let other = if dst == lhs_reg {
                            rhs_reg
                        } else if dst == rhs_reg {
                            lhs_reg
                        } else {
                            emit(&mut code, encode::mov64_rr(dst, lhs_reg));
                            rhs_reg
                        };
                        match node.op {
                            ir::Opcode::And => emit(&mut code, encode::and64_rr(dst, other)),
                            ir::Opcode::Or  => emit(&mut code, encode::or64_rr(dst, other)),
                            _               => emit(&mut code, encode::xor64_rr(dst, other)),
                        }
                    }
                }

                ir::Opcode::Shl | ir::Opcode::ShrU => {
                    let dst = gpr_of(&ra, inst_val);
                    let lhs_reg = gpr_of(&ra, ir::Value(node.args[0]));
                    let imm8 = if node.has_inline_imm1() {
                        node.inline_imm1() as u8
                    } else {
                        func.const_bits(ir::Value(node.args[1])).unwrap_or(0) as u8
                    };
                    if dst != lhs_reg {
                        emit(&mut code, encode::mov64_rr(dst, lhs_reg));
                    }
                    if node.op == ir::Opcode::Shl {
                        emit(&mut code, encode::shl64_ri(dst, imm8));
                    } else {
                        emit(&mut code, encode::shr64_ri(dst, imm8));
                    }
                }

                ir::Opcode::Clz => {
                    let dst = gpr_of(&ra, inst_val);
                    let src = gpr_of(&ra, ir::Value(node.args[0]));
                    emit(&mut code, encode::lzcnt64_rr(dst, src));
                }

                ir::Opcode::Ctz => {
                    let dst = gpr_of(&ra, inst_val);
                    let src = gpr_of(&ra, ir::Value(node.args[0]));
                    emit(&mut code, encode::tzcnt64_rr(dst, src));
                }

                ir::Opcode::Popcnt => {
                    let dst = gpr_of(&ra, inst_val);
                    let src = gpr_of(&ra, ir::Value(node.args[0]));
                    emit(&mut code, encode::popcnt64_rr(dst, src));
                }

                ir::Opcode::ByteSwap => {
                    let dst = gpr_of(&ra, inst_val);
                    let src = gpr_of(&ra, ir::Value(node.args[0]));
                    if dst != src {
                        emit(&mut code, encode::mov64_rr(dst, src));
                    }
                    emit(&mut code, encode::bswap64_r(dst));
                }

                ir::Opcode::ICmp => {
                    // Standalone ICmp (not folded into `BrIf` or `Select`):
                    let dst = gpr_of(&ra, inst_val);
                    let cc = emit_icmp_flags(&mut code, &ra, &node);
                    emit(&mut code, encode::setcc8_r(cc, dst));
                    emit(&mut code, encode::movzx32_r8(dst, dst));
                }

                ir::Opcode::Select => {
                    let dst = gpr_of(&ra, inst_val);
                    let cond_val = ir::Value(node.aux16 as u16 as u32);
                    let if_true = gpr_of(&ra, ir::Value(node.args[0]));
                    let if_false = gpr_of(&ra, ir::Value(node.args[1]));

                    let cond_node = func.insts[cond_val.index()];
                    let cc = if cond_node.op == ir::Opcode::ICmp {
                        emit_icmp_flags(&mut code, &ra, &cond_node)
                    } else {
                        let cond_reg = gpr_of(&ra, cond_val);
                        emit(&mut code, encode::test32_rr(cond_reg, cond_reg));
                        CondCode::NE
                    };

                    if dst == if_true {
                        // `dst` already holds `if_true` -> conditionally overwrite with `if_false` when `!cc`!
                        emit(&mut code, encode::cmovcc64_rr(cc.invert(), dst, if_false));
                    } else {
                        if dst != if_false {
                            emit(&mut code, encode::mov64_rr(dst, if_false));
                        }
                        emit(&mut code, encode::cmovcc64_rr(cc, dst, if_true));
                    }
                }

                ir::Opcode::Load | ir::Opcode::LoadExt => {
                    let dst = gpr_of(&ra, inst_val);
                    let ptr_val = ir::Value(node.args[0]);
                    let mem = resolve_mem_operand(func, &ra, ptr_val, node.aux16 as i32);
                    if node.ty == ir::Type::I32 {
                        emit(&mut code, encode::mov32_rm(dst, mem));
                    } else {
                        emit(&mut code, encode::mov64_rm(dst, mem));
                    }
                }

                ir::Opcode::Store => {
                    let ptr_val = ir::Value(node.args[0]);
                    let val_reg = gpr_of(&ra, ir::Value(node.args[1]));
                    let mem = resolve_mem_operand(func, &ra, ptr_val, node.aux16 as i32);
                    let val_ty = func.insts[node.args[1] as usize].ty;
                    if val_ty == ir::Type::I32 {
                        emit(&mut code, encode::mov32_mr(mem, val_reg));
                    } else {
                        emit(&mut code, encode::mov64_mr(mem, val_reg));
                    }
                }

                ir::Opcode::Call => {
                    let sym = ir::Sym(node.args[0]);
                    let call_args = &func.value_lists[node.args[1] as usize];
                    let arg_types: Vec<ir::Type> = call_args
                        .iter()
                        .map(|&v| func.value_type(v))
                        .collect();
                    let call_abi = AbiSig::compute(target, ir::CallConv::C, &arg_types, &[node.ty]);

                    for (i, &arg_val) in call_args.iter().enumerate() {
                        let src_reg = gpr_of(&ra, arg_val);
                        if let Some(dst_reg) = call_abi.params[i].as_x86_gpr() {
                            if src_reg != dst_reg {
                                emit(&mut code, encode::mov64_rr(dst_reg, src_reg));
                            }
                        }
                    }

                    emit(&mut code, encode::call_rel32(0));
                    let disp_off = (code.len() - 4) as u32;
                    relocs.push(Reloc {
                        section: SectionId::Text,
                        offset: disp_off,
                        sym,
                        kind: RelocKind::PcRel32,
                        addend: -4,
                    });

                    if node.ty != ir::Type::Void {
                        let dst = gpr_of(&ra, inst_val);
                        if dst != Gpr::Rax {
                            emit(&mut code, encode::mov64_rr(dst, Gpr::Rax));
                        }
                    }
                }

                ir::Opcode::Br => {
                    let target_bb = ir::Block(node.args[0]);
                    let args = &func.value_lists[node.args[1] as usize];
                    emit_block_arg_moves(&mut code, func, &ra, target_bb, args);

                    // Omit jump if `target_bb` is the immediately following block in layout order:
                    if target_bb.index() != b_idx + 1 {
                        emit(&mut code, encode::jmp_rel32(0));
                        patches.push(BranchPatch {
                            disp32_offset: code.len() - 4,
                            next_ip_offset: code.len(),
                            target_block: target_bb,
                        });
                    }
                }

                ir::Opcode::BrIf => {
                    let cond_val = ir::Value(node.args[0]);
                    let targets = &func.br_if_targets[node.args[1] as usize];
                    let cond_node = func.insts[cond_val.index()];

                    // Emit the comparison right before `Jcc` so Intel/AMD frontends macro-fuse them!
                    let cc = if cond_node.op == ir::Opcode::ICmp {
                        emit_icmp_flags(&mut code, &ra, &cond_node)
                    } else {
                        let cond_reg = gpr_of(&ra, cond_val);
                        emit(&mut code, encode::test32_rr(cond_reg, cond_reg));
                        CondCode::NE
                    };

                    if targets.then_args.is_empty() && targets.else_bb.index() == b_idx + 1 {
                        // Fast path: `jcc then_bb` + fallthrough to `else_bb`!
                        emit(&mut code, encode::jcc_rel32(cc, 0));
                        patches.push(BranchPatch {
                            disp32_offset: code.len() - 4,
                            next_ip_offset: code.len(),
                            target_block: targets.then_bb,
                        });
                        emit_block_arg_moves(&mut code, func, &ra, targets.else_bb, &targets.else_args);
                    } else if targets.else_args.is_empty() && targets.then_bb.index() == b_idx + 1 {
                        // Fast path: `j!cc else_bb` + fallthrough to `then_bb`!
                        emit(&mut code, encode::jcc_rel32(cc.invert(), 0));
                        patches.push(BranchPatch {
                            disp32_offset: code.len() - 4,
                            next_ip_offset: code.len(),
                            target_block: targets.else_bb,
                        });
                        emit_block_arg_moves(&mut code, func, &ra, targets.then_bb, &targets.then_args);
                    } else {
                        // General case with block parameter moves on both edges:
                        emit_block_arg_moves(&mut code, func, &ra, targets.then_bb, &targets.then_args);
                        emit(&mut code, encode::jcc_rel32(cc, 0));
                        patches.push(BranchPatch {
                            disp32_offset: code.len() - 4,
                            next_ip_offset: code.len(),
                            target_block: targets.then_bb,
                        });
                        emit_block_arg_moves(&mut code, func, &ra, targets.else_bb, &targets.else_args);
                        if targets.else_bb.index() != b_idx + 1 {
                            emit(&mut code, encode::jmp_rel32(0));
                            patches.push(BranchPatch {
                                disp32_offset: code.len() - 4,
                                next_ip_offset: code.len(),
                                target_block: targets.else_bb,
                            });
                        }
                    }
                }

                ir::Opcode::Ret => {
                    let vals = &func.value_lists[node.args[0] as usize];
                    for (ret_i, &v) in vals.iter().enumerate() {
                        if let Some(ret_reg) = ra.abi.returns.get(ret_i).and_then(|l| l.as_x86_gpr()) {
                            let v_node = func.insts[v.index()];
                            if v_node.op == ir::Opcode::Const && folded[v.index()] {
                                let bits = func.const_bits(v).unwrap_or(0) as i64;
                                emit_materialize_const(&mut code, ret_reg, bits);
                            } else {
                                let src_reg = gpr_of(&ra, v);
                                if src_reg != ret_reg {
                                    emit(&mut code, encode::mov64_rr(ret_reg, src_reg));
                                }
                            }
                        }
                    }

                    // Epilogue: restore `RSP`, pop callee-saved registers in reverse, and `ret`!
                    if ra.frame.sp_sub_bytes > 0 {
                        emit(&mut code, encode::add64_ri(Gpr::Rsp, ra.frame.sp_sub_bytes as i32));
                    }
                    for &reg in saved_gprs.iter().rev() {
                        emit(&mut code, encode::pop64_r(reg));
                    }
                    emit(&mut code, encode::ret());
                }

                ir::Opcode::Trap => {
                    emit(&mut code, encode::ud2());
                }

                _ => {}
            }
        }
    }

    // =========================================================================
    // 3. Patch Intra-Function Branch Displacements
    // =========================================================================
    for patch in patches {
        let target_off = block_offsets[patch.target_block.index()] as i32;
        let next_ip = patch.next_ip_offset as i32;
        let rel32 = target_off - next_ip;
        code[patch.disp32_offset..patch.disp32_offset + 4].copy_from_slice(&rel32.to_le_bytes());
    }

    CompiledFunction {
        code,
        relocs,
        regalloc: ra,
    }
}

/// Identifies instructions that are folded into downstream SIB memory operands, `LEA`s, or `BrIf`/`Select`.
fn compute_folded_nodes(func: &ir::Function) -> Vec<bool> {
    let n = func.insts.len();
    let mut folded = vec![false; n];
    let mut mem_or_branch_uses = vec![0u16; n];

    for block in &func.blocks {
        for &inst_val in &block.insts {
            let node = func.insts[inst_val.index()];
            match node.op {
                ir::Opcode::Load | ir::Opcode::LoadExt | ir::Opcode::Store => {
                    let ptr = ir::Value(node.args[0]);
                    mem_or_branch_uses[ptr.index()] += 1;
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
                ir::Opcode::PtrAdd | ir::Opcode::Add if !node.has_inline_imm1() => {
                    let rhs = ir::Value(node.args[1]);
                    let rhs_node = func.insts[rhs.index()];
                    if rhs_node.op == ir::Opcode::Shl
                        && rhs_node.use_count == 1
                        && rhs_node.has_inline_imm1()
                        && matches!(rhs_node.inline_imm1(), 1..=3)
                    {
                        folded[rhs.index()] = true;
                    }
                }
                _ => {}
            }
        }
    }

    // If a `PtrAdd` is ONLY used as the pointer operand of `Load`/`Store` instructions,
    // its address math (`base + idx * scale`) folds directly into every `Load`/`Store`'s SIB byte!
    for idx in 0..n {
        let node = func.insts[idx];
        if node.op == ir::Opcode::PtrAdd
            && node.use_count > 0
            && node.use_count == mem_or_branch_uses[idx]
        {
            folded[idx] = true;
        }
    }

    folded
}

/// Folds `ptr_val` + `offset` into an x86-64 [`MemoryOperand<Gpr>`] (`[base + index * scale + disp]`).
fn resolve_mem_operand(
    func: &ir::Function,
    ra: &RegAllocResult,
    ptr_val: ir::Value,
    offset: i32,
) -> MemoryOperand<Gpr> {
    let ptr_node = func.insts[ptr_val.index()];
    if ptr_node.op == ir::Opcode::PtrAdd {
        let base_val = ir::Value(ptr_node.args[0]);
        let base_reg = gpr_of(ra, base_val);
        if ptr_node.has_inline_imm1() {
            return MemoryOperand::base_disp(base_reg, ptr_node.inline_imm1() + offset);
        }
        let rhs_val = ir::Value(ptr_node.args[1]);
        if let Some((idx_reg, scale)) = try_extract_scaled_index(func, ra, rhs_val) {
            return MemoryOperand::base_index_disp(base_reg, idx_reg, scale, offset);
        }
        let idx_reg = gpr_of(ra, rhs_val);
        if idx_reg.can_be_index() {
            return MemoryOperand::base_index_disp(base_reg, idx_reg, Scale::S1, offset);
        }
    }
    let base_reg = gpr_of(ra, ptr_val);
    MemoryOperand::base_disp(base_reg, offset)
}

fn try_extract_scaled_index(
    func: &ir::Function,
    ra: &RegAllocResult,
    val: ir::Value,
) -> Option<(Gpr, Scale)> {
    let node = func.insts[val.index()];
    if node.op == ir::Opcode::Shl && node.has_inline_imm1() {
        let scale = match node.inline_imm1() {
            1 => Scale::S2,
            2 => Scale::S4,
            3 => Scale::S8,
            _ => return None,
        };
        let idx_reg = gpr_of(ra, ir::Value(node.args[0]));
        if idx_reg.can_be_index() {
            return Some((idx_reg, scale));
        }
    }
    None
}

fn emit_icmp_flags(
    code: &mut Vec<u8>,
    ra: &RegAllocResult,
    icmp_node: &ir::InstNode,
) -> CondCode {
    let cc = map_int_cc(ir::IntCC::from_raw(icmp_node.subop).unwrap_or(ir::IntCC::Eq));
    let lhs_reg = gpr_of(ra, ir::Value(icmp_node.args[0]));
    if icmp_node.has_inline_imm1() {
        let imm = icmp_node.inline_imm1();
        if imm == 0 && matches!(cc, CondCode::E | CondCode::NE | CondCode::L | CondCode::GE) {
            emit(code, encode::test64_rr(lhs_reg, lhs_reg));
        } else {
            emit(code, encode::cmp64_ri(lhs_reg, imm));
        }
    } else {
        let rhs_reg = gpr_of(ra, ir::Value(icmp_node.args[1]));
        emit(code, encode::cmp64_rr(lhs_reg, rhs_reg));
    }
    cc
}

const fn map_int_cc(cc: ir::IntCC) -> CondCode {
    match cc {
        ir::IntCC::Eq  => CondCode::E,
        ir::IntCC::Ne  => CondCode::NE,
        ir::IntCC::Slt => CondCode::L,
        ir::IntCC::Sle => CondCode::LE,
        ir::IntCC::Sgt => CondCode::G,
        ir::IntCC::Sge => CondCode::GE,
        ir::IntCC::Ult => CondCode::B,
        ir::IntCC::Ule => CondCode::BE,
        ir::IntCC::Ugt => CondCode::A,
        ir::IntCC::Uge => CondCode::AE,
    }
}

fn emit_materialize_const(code: &mut Vec<u8>, dst: Gpr, val: i64) {
    if val == 0 {
        // 2-byte `xor r32, r32` zero-idiom (0 execution ports, breaks false dependencies):
        emit(code, encode::xor32_rr(dst, dst));
    } else if val > 0 && val <= (u32::MAX as i64) {
        // 5-byte `mov r32, imm32` (implicitly zero-extends to 64 bits!):
        emit(code, encode::mov32_ri(dst, val as u32 as i32));
    } else if val >= (i32::MIN as i64) && val <= (i32::MAX as i64) {
        // 7-byte `mov r64, sign_ext_imm32`:
        emit(code, encode::mov64_ri32(dst, val as i32));
    } else {
        // Full 10-byte `movabs r64, imm64`:
        emit(code, encode::movabs64_ri64(dst, val));
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
    ra.loc(val).as_x86_gpr().unwrap_or(Gpr::Rax)
}

#[inline]
fn emit(code: &mut Vec<u8>, inst: EncodedInst) {
    code.extend_from_slice(inst.as_slice());
}
