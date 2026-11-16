//! x86-64 System V AMD64 ABI (`SysV64` / `Darwin`) and Windows x64 ABI (`Win64` / `Msvc`).

use alloc::vec::Vec;
use obsidian_ir::{Abi, CallConv, Target, Type};
use obsidian_isa_x86::regs::{Gpr, GprMask};
use crate::loc::{align_up, AbiSig, ArgLoc};

/// System V AMD64 integer/pointer parameter registers in order (`RDI, RSI, RDX, RCX, R8, R9`).
pub const SYSV64_GPR_ARGS: [Gpr; 6] = [
    Gpr::Rdi,
    Gpr::Rsi,
    Gpr::Rdx,
    Gpr::Rcx,
    Gpr::R8,
    Gpr::R9,
];

/// System V AMD64 integer/pointer return registers (`RAX, RDX`, plus `RCX, R8` in `CallConv::Fast`).
pub const SYSV64_GPR_RETS: [Gpr; 4] = [Gpr::Rax, Gpr::Rdx, Gpr::Rcx, Gpr::R8];

/// Windows x64 positional integer/pointer parameter registers (`RCX, RDX, R8, R9`).
pub const WIN64_GPR_ARGS: [Gpr; 4] = [Gpr::Rcx, Gpr::Rdx, Gpr::R8, Gpr::R9];

/// System V AMD64 caller-saved (volatile) GPRs (`RAX, RCX, RDX, RSI, RDI, R8, R9, R10, R11`).
pub const SYSV64_CALLER_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::Rax,
    Gpr::Rcx,
    Gpr::Rdx,
    Gpr::Rsi,
    Gpr::Rdi,
    Gpr::R8,
    Gpr::R9,
    Gpr::R10,
    Gpr::R11,
]);

/// System V AMD64 callee-saved (non-volatile) GPRs (`RBX, RBP, R12, R13, R14, R15`).
pub const SYSV64_CALLEE_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::Rbx,
    Gpr::Rbp,
    Gpr::R12,
    Gpr::R13,
    Gpr::R14,
    Gpr::R15,
]);

/// Windows x64 caller-saved (volatile) GPRs (`RAX, RCX, RDX, R8, R9, R10, R11` — note `RSI` and `RDI` are callee-saved!).
pub const WIN64_CALLER_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::Rax,
    Gpr::Rcx,
    Gpr::Rdx,
    Gpr::R8,
    Gpr::R9,
    Gpr::R10,
    Gpr::R11,
]);

/// Windows x64 callee-saved (non-volatile) GPRs (`RBX, RBP, RDI, RSI, R12, R13, R14, R15`).
pub const WIN64_CALLEE_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::Rbx,
    Gpr::Rbp,
    Gpr::Rdi,
    Gpr::Rsi,
    Gpr::R12,
    Gpr::R13,
    Gpr::R14,
    Gpr::R15,
]);

/// Computes the x86-64 [`AbiSig`] for `target` (`SysV64` vs `Win64`).
pub fn compute_x86_64(
    target: Target,
    call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    if matches!(target.abi, Abi::Msvc) || matches!(target.os, obsidian_ir::Os::Windows) {
        compute_win64(call_conv, params, returns)
    } else {
        compute_sysv64(call_conv, params, returns)
    }
}

/// Computes the System V AMD64 ABI (`Linux`, `macOS`, `FreeBSD`).
pub fn compute_sysv64(
    call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    let mut param_locs = Vec::with_capacity(params.len());
    let mut gpr_idx = 0usize;
    let mut xmm_idx = 0usize;
    let mut stack_off = 0u32;

    for &ty in params {
        match ty {
            Type::F32 | Type::F64 | Type::V128 => {
                if xmm_idx < 8 {
                    param_locs.push(ArgLoc::float_vec(xmm_idx as u8));
                    xmm_idx += 1;
                } else {
                    let (size, align) = if ty == Type::V128 { (16, 16) } else { (8, 8) };
                    stack_off = align_up(stack_off, align);
                    param_locs.push(ArgLoc::Stack {
                        offset: stack_off as i32,
                        size: size as u8,
                        align: align as u8,
                    });
                    stack_off += size;
                }
            }
            Type::I128 => {
                if gpr_idx + 1 < SYSV64_GPR_ARGS.len() {
                    let lo = SYSV64_GPR_ARGS[gpr_idx].hw_id();
                    let hi = SYSV64_GPR_ARGS[gpr_idx + 1].hw_id();
                    param_locs.push(ArgLoc::gpr_pair(lo, hi));
                    gpr_idx += 2;
                } else {
                    stack_off = align_up(stack_off, 16);
                    param_locs.push(ArgLoc::Stack {
                        offset: stack_off as i32,
                        size: 16,
                        align: 16,
                    });
                    stack_off += 16;
                }
            }
            _ => {
                if gpr_idx < SYSV64_GPR_ARGS.len() {
                    param_locs.push(ArgLoc::gpr(SYSV64_GPR_ARGS[gpr_idx].hw_id()));
                    gpr_idx += 1;
                } else {
                    stack_off = align_up(stack_off, 8);
                    param_locs.push(ArgLoc::Stack {
                        offset: stack_off as i32,
                        size: 8,
                        align: 8,
                    });
                    stack_off += 8;
                }
            }
        }
    }

    let max_gpr_rets = if matches!(call_conv, CallConv::Fast) { 4 } else { 2 };
    let mut ret_locs = Vec::with_capacity(returns.len());
    let mut ret_gpr = 0usize;
    let mut ret_xmm = 0usize;

    for &ty in returns {
        match ty {
            Type::F32 | Type::F64 | Type::V128 => {
                if ret_xmm < 2 {
                    ret_locs.push(ArgLoc::float_vec(ret_xmm as u8));
                    ret_xmm += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 8, align: 8 });
                }
            }
            Type::I128 => {
                if ret_gpr + 1 < max_gpr_rets {
                    let lo = SYSV64_GPR_RETS[ret_gpr].hw_id();
                    let hi = SYSV64_GPR_RETS[ret_gpr + 1].hw_id();
                    ret_locs.push(ArgLoc::gpr_pair(lo, hi));
                    ret_gpr += 2;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 16, align: 16 });
                }
            }
            _ => {
                if ret_gpr < max_gpr_rets {
                    ret_locs.push(ArgLoc::gpr(SYSV64_GPR_RETS[ret_gpr].hw_id()));
                    ret_gpr += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 8, align: 8 });
                }
            }
        }
    }

    AbiSig {
        params: param_locs,
        returns: ret_locs,
        stack_arg_bytes: align_up(stack_off, 16),
        shadow_space_bytes: 0,
        stack_align: 16,
        red_zone_bytes: 128,
        caller_saved_gpr_mask: SYSV64_CALLER_SAVED.0 as u32,
        callee_saved_gpr_mask: SYSV64_CALLEE_SAVED.0 as u32,
        reserved_gpr_mask: 1u32 << (Gpr::Rsp as u8),
    }
}

/// Computes the Windows x64 (`Win64` / `Msvc`) ABI.
///
/// ### Critical Windows x64 ABI Rules
/// 1. **Positional Register Slots (`0..4`):** Slot `i` uses either `WIN64_GPR_ARGS[i]` (`RCX, RDX, R8, R9`)
///    OR `XMMi` (`XMM0..XMM3`). Passing a float in slot 1 consumes slot 1 for integers too!
/// 2. **32-Byte Shadow Space:** The caller always allocates 32 bytes (`[RSP + 0..32]`) of shadow space,
///    so stack argument #5 starts at `[RSP + 32]`.
/// 3. **No Red Zone (`red_zone_bytes = 0`):** Windows kernel/APC interrupts can clobber memory below `RSP` at any time.
pub fn compute_win64(
    _call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    let mut param_locs = Vec::with_capacity(params.len());
    let mut slot = 0usize;
    let mut stack_off = 32u32; // Starts after 32-byte home/shadow space!

    for &ty in params {
        if slot < 4 && ty != Type::I128 && ty != Type::V128 {
            match ty {
                Type::F32 | Type::F64 => {
                    param_locs.push(ArgLoc::float_vec(slot as u8));
                }
                _ => {
                    param_locs.push(ArgLoc::gpr(WIN64_GPR_ARGS[slot].hw_id()));
                }
            }
            slot += 1;
        } else {
            let (size, align) = if matches!(ty, Type::I128 | Type::V128) {
                (16, 16)
            } else {
                (8, 8)
            };
            stack_off = align_up(stack_off, align);
            param_locs.push(ArgLoc::Stack {
                offset: stack_off as i32,
                size: size as u8,
                align: align as u8,
            });
            stack_off += size;
            slot += 1;
        }
    }

    let mut ret_locs = Vec::with_capacity(returns.len());
    for (i, &ty) in returns.iter().enumerate() {
        match (i, ty) {
            (0, Type::F32 | Type::F64 | Type::V128) => ret_locs.push(ArgLoc::float_vec(0)),
            (0, _) => ret_locs.push(ArgLoc::gpr(Gpr::Rax.hw_id())),
            (1, _) => ret_locs.push(ArgLoc::gpr(Gpr::Rdx.hw_id())),
            _ => ret_locs.push(ArgLoc::Stack { offset: 0, size: 8, align: 8 }),
        }
    }

    AbiSig {
        params: param_locs,
        returns: ret_locs,
        stack_arg_bytes: if params.is_empty() { 32 } else { align_up(stack_off, 16) },
        shadow_space_bytes: 32,
        stack_align: 16,
        red_zone_bytes: 0,
        caller_saved_gpr_mask: WIN64_CALLER_SAVED.0 as u32,
        callee_saved_gpr_mask: WIN64_CALLEE_SAVED.0 as u32,
        reserved_gpr_mask: 1u32 << (Gpr::Rsp as u8),
    }
}
