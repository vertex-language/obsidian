//! Motorola 680x0 (`M68k`) Calling Convention (`Fast` split `D0..D1`/`A0..A1` register ABI & `C` stack ABI).
//!
//! ### Split Register Bank Rule on M68k
//! Because M68k silicon separates Data Registers (`D0..D7`) from Address Registers (`A0..A7`):
//! - In `CallConv::Fast` (and register-passing conventions), pointer arguments (`Type::Ptr`)
//!   are passed directly in **Address Registers (`A0, A1`, `RegBank::Addr`)** so the callee can
//!   dereference `(A0)` immediately without a register-to-register move!
//! - Integer arguments (`Type::I1..I32`) are passed in **Data Registers (`D0, D1`, `RegBank::Gpr`)**,
//!   and 64-bit `Type::I64` arguments use the `D0:D1` register pair!

use alloc::vec::Vec;
use obsidian_ir::{CallConv, Type};
use obsidian_isa_m68k::regs::{AddrReg, DataReg};
use crate::loc::{align_up, AbiSig, ArgLoc};

pub fn compute_m68k(
    call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    let mut param_locs = Vec::with_capacity(params.len());
    let mut data_idx = 0u8; // D0, D1 (0, 1)
    let mut addr_idx = 0u8; // A0, A1 (0, 1)
    let mut stack_off = 4u32; // Starts at [SP + 4] (above 4-byte return address pushed by JSR/BSR)

    let use_reg_args = !matches!(call_conv, CallConv::C);

    for &ty in params {
        if use_reg_args && ty == Type::Ptr && addr_idx < 2 {
            param_locs.push(ArgLoc::addr_reg(addr_idx));
            addr_idx += 1;
        } else if use_reg_args && ty == Type::I64 && data_idx == 0 {
            param_locs.push(ArgLoc::gpr_pair(DataReg::D0.hw_id(), DataReg::D1.hw_id()));
            data_idx = 2;
        } else if use_reg_args && ty != Type::I64 && ty != Type::I128 && data_idx < 2 {
            param_locs.push(ArgLoc::gpr(data_idx));
            data_idx += 1;
        } else {
            let size = match ty {
                Type::I64 | Type::F64 => 8u32,
                Type::I128 | Type::V128 => 16,
                _ => 4, // M68k C ABI widens 8/16-bit stack arguments to 4-byte longwords
            };
            stack_off = align_up(stack_off, 4);
            param_locs.push(ArgLoc::Stack {
                offset: stack_off as i32,
                size: size as u8,
                align: 4,
            });
            stack_off += size;
        }
    }

    let mut ret_locs = Vec::with_capacity(returns.len());
    let mut ret_data = 0u8;
    let mut ret_addr = 0u8;

    for &ty in returns {
        if ty == Type::Ptr && ret_addr == 0 {
            ret_locs.push(ArgLoc::addr_reg(AddrReg::A0.hw_id()));
            ret_addr += 1;
        } else if ty == Type::I64 && ret_data == 0 {
            ret_locs.push(ArgLoc::gpr_pair(DataReg::D0.hw_id(), DataReg::D1.hw_id()));
            ret_data = 2;
        } else if ret_data < 2 {
            ret_locs.push(ArgLoc::gpr(ret_data));
            ret_data += 1;
        } else {
            ret_locs.push(ArgLoc::Stack { offset: 4, size: 4, align: 4 });
        }
    }

    // Unified 16-bit mask: bits 0..=7 = D0..D7, bits 8..=15 = A0..A7
    // Caller-saved: D0, D1, A0, A1 -> 0x0303
    // Callee-saved: D2..D7, A2..A5 -> 0x3CFC
    // Reserved: A6 (FP) and A7 (SP) -> 0xC000
    AbiSig {
        params: param_locs,
        returns: ret_locs,
        stack_arg_bytes: align_up(stack_off - 4, 4),
        shadow_space_bytes: 0,
        stack_align: 4,
        red_zone_bytes: 0,
        caller_saved_gpr_mask: 0x0000_0303,
        callee_saved_gpr_mask: 0x0000_3CFC,
        reserved_gpr_mask: 0x0000_C000,
    }
}
