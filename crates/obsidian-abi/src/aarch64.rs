//! 64-bit ARM Procedure Call Standard (`AAPCS64`) and Apple Darwin ARM64 ABI.

use alloc::vec::Vec;
use obsidian_ir::{Abi, CallConv, Os, Target, Type};
use obsidian_isa_aarch64::regs::{Gpr, GprMask};
use crate::loc::{align_up, AbiSig, ArgLoc};

/// Computes the AArch64 [`AbiSig`] for `target` (standard `AAPCS64` vs Apple `Darwin` ARM64).
pub fn compute_aarch64(
    target: Target,
    call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    let is_darwin = matches!(target.abi, Abi::Darwin) || matches!(target.os, Os::MacOs | Os::Ios);
    let is_windows = matches!(target.abi, Abi::Msvc) || matches!(target.os, Os::Windows);

    let mut param_locs = Vec::with_capacity(params.len());
    let mut gpr_idx = 0u8; // X0..X7 (0..=7)
    let mut vec_idx = 0u8; // V0..V7 (0..=7)
    let mut stack_off = 0u32;

    for &ty in params {
        match ty {
            Type::F32 | Type::F64 | Type::V128 => {
                if vec_idx < 8 {
                    param_locs.push(ArgLoc::float_vec(vec_idx));
                    vec_idx += 1;
                } else {
                    let (size, align) = match (is_darwin, ty) {
                        (_, Type::V128)    => (16u32, 16u32),
                        (true, Type::F32)  => (4, 4),
                        _                  => (8, 8),
                    };
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
                // On standard AAPCS64, 128-bit integer arguments in registers must start at an even Xn!
                if !is_darwin && (gpr_idx & 1) != 0 {
                    gpr_idx += 1;
                }
                if gpr_idx + 1 < 8 {
                    param_locs.push(ArgLoc::gpr_pair(gpr_idx, gpr_idx + 1));
                    gpr_idx += 2;
                } else {
                    gpr_idx = 8;
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
                if gpr_idx < 8 {
                    param_locs.push(ArgLoc::gpr(gpr_idx));
                    gpr_idx += 1;
                } else {
                    // Apple Darwin ARM64 packs narrow stack arguments to their natural width (1, 2, 4, 8B),
                    // whereas standard Linux AAPCS64 allocates an 8-byte slot for every stack argument!
                    let (size, align) = if is_darwin {
                        match ty {
                            Type::I1 | Type::I8 => (1u32, 1u32),
                            Type::I16           => (2, 2),
                            Type::I32           => (4, 4),
                            _                   => (8, 8),
                        }
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
                }
            }
        }
    }

    let max_gpr_rets = if matches!(call_conv, CallConv::Fast) { 4u8 } else { 2u8 };
    let mut ret_locs = Vec::with_capacity(returns.len());
    let mut ret_gpr = 0u8;
    let mut ret_vec = 0u8;

    for &ty in returns {
        match ty {
            Type::F32 | Type::F64 | Type::V128 => {
                if ret_vec < 4 {
                    ret_locs.push(ArgLoc::float_vec(ret_vec));
                    ret_vec += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 8, align: 8 });
                }
            }
            Type::I128 => {
                if ret_gpr + 1 < max_gpr_rets {
                    ret_locs.push(ArgLoc::gpr_pair(ret_gpr, ret_gpr + 1));
                    ret_gpr += 2;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 16, align: 16 });
                }
            }
            _ => {
                if ret_gpr < max_gpr_rets {
                    ret_locs.push(ArgLoc::gpr(ret_gpr));
                    ret_gpr += 1;
                } else {
                    ret_locs.push(ArgLoc::Stack { offset: 0, size: 8, align: 8 });
                }
            }
        }
    }

    // Platform register X18 is strictly reserved on Apple Darwin and Windows ARM64!
    let mut reserved = (1u32 << (Gpr::FP as u8)) | (1u32 << (Gpr::LR as u8));
    if is_darwin || is_windows {
        reserved |= 1u32 << (Gpr::X18 as u8);
    }

    AbiSig {
        params: param_locs,
        returns: ret_locs,
        stack_arg_bytes: align_up(stack_off, 16),
        shadow_space_bytes: 0,
        stack_align: 16,
        red_zone_bytes: if is_darwin { 128 } else { 0 },
        caller_saved_gpr_mask: GprMask::AAPCS64_CALLER_SAVED.0 & !reserved,
        callee_saved_gpr_mask: GprMask::AAPCS64_CALLEE_SAVED.0,
        reserved_gpr_mask: reserved,
    }
}
