//! IBM SystemZ (`s390x` ELF64 ABI) calling convention.
//!
//! ### Key `s390x` ABI Rules
//! - Integer / pointer parameters: `R2, R3, R4, R5, R6` (`5` registers, hardware IDs `2..=6`).
//! - Floating-point parameters: `F0, F2, F4, F6` (`4` registers, hardware IDs `0, 2, 4, 6`).
//! - Return values: `R2` (or `R2, R3` for multi-return) and `F0` (or `F0, F2`).
//! - Caller allocates a 160-byte register save area at `[R15 + 0..160]`, so outgoing stack
//!   arguments start at `[R15 + 160]`!

use alloc::vec::Vec;
use obsidian_ir::{CallConv, Type};
use obsidian_isa_systemz::regs::{Gpr, GprMask};
use crate::loc::{align_up, AbiSig, ArgLoc};

pub const S390X_GPR_ARGS: [Gpr; 5] = [Gpr::R2, Gpr::R3, Gpr::R4, Gpr::R5, Gpr::R6];
pub const S390X_FPR_ARGS: [u8; 4] = [0, 2, 4, 6];

/// Linux `s390x` caller-saved (volatile) GPRs (`R0..R5`).
pub const S390X_CALLER_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::R0,
    Gpr::R1,
    Gpr::R2,
    Gpr::R3,
    Gpr::R4,
    Gpr::R5,
]);

/// Linux `s390x` callee-saved (non-volatile) GPRs (`R6..R13`).
pub const S390X_CALLEE_SAVED: GprMask = GprMask::from_slice(&[
    Gpr::R6,
    Gpr::R7,
    Gpr::R8,
    Gpr::R9,
    Gpr::R10,
    Gpr::R11,
    Gpr::R12,
    Gpr::R13,
]);

pub fn compute_systemz(
    _call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    let mut param_locs = Vec::with_capacity(params.len());
    let mut gpr_idx = 0usize;
    let mut fpr_idx = 0usize;
    let mut stack_off = 160u32; // Starts after 160-byte s390x register save area!

    for &ty in params {
        match ty {
            Type::F32 | Type::F64 => {
                if fpr_idx < S390X_FPR_ARGS.len() {
                    param_locs.push(ArgLoc::float_vec(S390X_FPR_ARGS[fpr_idx]));
                    fpr_idx += 1;
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
            Type::I128 => {
                if gpr_idx + 1 < S390X_GPR_ARGS.len() {
                    let lo = S390X_GPR_ARGS[gpr_idx].hw_id();
                    let hi = S390X_GPR_ARGS[gpr_idx + 1].hw_id();
                    param_locs.push(ArgLoc::gpr_pair(lo, hi));
                    gpr_idx += 2;
                } else {
                    stack_off = align_up(stack_off, 8);
                    param_locs.push(ArgLoc::Stack {
                        offset: stack_off as i32,
                        size: 16,
                        align: 8,
                    });
                    stack_off += 16;
                }
            }
            _ => {
                if gpr_idx < S390X_GPR_ARGS.len() {
                    param_locs.push(ArgLoc::gpr(S390X_GPR_ARGS[gpr_idx].hw_id()));
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

    let mut ret_locs = Vec::with_capacity(returns.len());
    let mut ret_gpr = 0usize;
    let mut ret_fpr = 0usize;

    for &ty in returns {
        match ty {
            Type::F32 | Type::F64 => {
                if ret_fpr < 2 {
                    ret_locs.push(ArgLoc::float_vec(S390X_FPR_ARGS[ret_fpr]));
                    ret_fpr += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 160, size: 8, align: 8 });
                }
            }
            _ => {
                if ret_gpr < 2 {
                    ret_locs.push(ArgLoc::gpr((Gpr::R2 as u8) + (ret_gpr as u8)));
                    ret_gpr += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 160, size: 8, align: 8 });
                }
            }
        }
    }

    AbiSig {
        params: param_locs,
        returns: ret_locs,
        stack_arg_bytes: if params.is_empty() { 160 } else { align_up(stack_off, 8) },
        shadow_space_bytes: 160,
        stack_align: 8,
        red_zone_bytes: 0,
        caller_saved_gpr_mask: S390X_CALLER_SAVED.0 as u32,
        callee_saved_gpr_mask: S390X_CALLEE_SAVED.0 as u32,
        reserved_gpr_mask: (1u32 << (Gpr::R14 as u8)) | (1u32 << (Gpr::R15 as u8)),
    }
}
