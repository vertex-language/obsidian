#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod aarch64;
pub mod loc;
pub mod m68k;
pub mod systemz;
pub mod x86;

pub use crate::loc::{AbiSig, ArgLoc, FrameLayout, RegBank};

use obsidian_ir::{Arch, CallConv, Target, Type};

/// Computes the physical [`AbiSig`] for `(target, call_conv, params, returns)`.
pub fn compute_abi_sig(
    target: Target,
    call_conv: CallConv,
    params: &[Type],
    returns: &[Type],
) -> AbiSig {
    match target.arch {
        Arch::X86_64 => x86::compute_x86_64(target, call_conv, params, returns),
        Arch::AArch64 => aarch64::compute_aarch64(target, call_conv, params, returns),
        Arch::SystemZ => systemz::compute_systemz(call_conv, params, returns),
        Arch::M68k => m68k::compute_m68k(call_conv, params, returns),
        // Default 8-register RISC convention (`X0..X7` / `A0..A7`) for RISC-V / LoongArch / ARM / etc.:
        _ => aarch64::compute_aarch64(target, call_conv, params, returns),
    }
}

#[cfg(test)]
mod tests {
    use crate::{AbiSig, ArgLoc, FrameLayout};
    use obsidian_ir as ir;
    use obsidian_isa_aarch64::regs as aarch64_regs;
    use obsidian_isa_m68k::regs as m68k_regs;
    use obsidian_isa_systemz::regs as systemz_regs;
    use obsidian_isa_x86::regs as x86_regs;

    #[test]
    fn test_sysv64_and_win64_abi_rules() {
        let params = [
            ir::Type::I64,
            ir::Type::F64,
            ir::Type::Ptr,
            ir::Type::I64,
            ir::Type::I64,
        ];
        let rets = [ir::Type::Ptr, ir::Type::I64];

        let sysv = AbiSig::compute(ir::Target::x86_64_linux(), ir::CallConv::Fast, &params, &rets);
        assert_eq!(sysv.params[0].as_x86_gpr(), Some(x86_regs::Gpr::Rdi));
        assert_eq!(sysv.params[1].as_x86_simd(), Some(x86_regs::SimdReg::XMM0));
        assert_eq!(sysv.params[2].as_x86_gpr(), Some(x86_regs::Gpr::Rsi));
        assert_eq!(sysv.params[3].as_x86_gpr(), Some(x86_regs::Gpr::Rdx));
        assert_eq!(sysv.params[4].as_x86_gpr(), Some(x86_regs::Gpr::Rcx));
        assert_eq!(sysv.returns[0].as_x86_gpr(), Some(x86_regs::Gpr::Rax));
        assert_eq!(sysv.returns[1].as_x86_gpr(), Some(x86_regs::Gpr::Rdx));

        let win = AbiSig::compute(ir::Target::x86_64_windows(), ir::CallConv::C, &params, &rets);
        assert_eq!(win.params[0].as_x86_gpr(), Some(x86_regs::Gpr::Rcx));
        assert_eq!(win.params[1].as_x86_simd(), Some(x86_regs::SimdReg::XMM1));
        assert_eq!(win.params[2].as_x86_gpr(), Some(x86_regs::Gpr::R8));
        assert_eq!(win.params[3].as_x86_gpr(), Some(x86_regs::Gpr::R9));
        // 5th parameter spills to `[RSP + 32]` (after 32-byte Win64 shadow space!):
        assert_eq!(
            win.params[4],
            ArgLoc::Stack {
                offset: 32,
                size: 8,
                align: 8
            }
        );
    }

    #[test]
    fn test_systemz_s390x_abi_rules() {
        let params = [
            ir::Type::Ptr,
            ir::Type::I64,
            ir::Type::I64,
            ir::Type::I64,
            ir::Type::I64,
            ir::Type::I64, // 6th GPR spills to `[R15 + 160]`!
        ];
        let s390x = AbiSig::compute(ir::Target::systemz_linux(), ir::CallConv::C, &params, &[ir::Type::I64]);
        assert_eq!(s390x.params[0].as_systemz_gpr(), Some(systemz_regs::Gpr::R2));
        assert_eq!(s390x.params[4].as_systemz_gpr(), Some(systemz_regs::Gpr::R6));
        assert_eq!(
            s390x.params[5],
            ArgLoc::Stack {
                offset: 160,
                size: 8,
                align: 8
            }
        );
        assert_eq!(s390x.returns[0].as_systemz_gpr(), Some(systemz_regs::Gpr::R2));
        assert_eq!(s390x.shadow_space_bytes, 160);
    }

    #[test]
    fn test_m68k_split_addr_and_data_abi_rules() {
        let params = [ir::Type::Ptr, ir::Type::I32, ir::Type::Ptr, ir::Type::I32];
        let m68k = AbiSig::compute(ir::Target::m68k_bare(), ir::CallConv::Fast, &params, &[ir::Type::I64]);
        assert_eq!(m68k.params[0].as_m68k_addr(), Some(m68k_regs::AddrReg::A0));
        assert_eq!(m68k.params[1].as_m68k_data(), Some(m68k_regs::DataReg::D0));
        assert_eq!(m68k.params[2].as_m68k_addr(), Some(m68k_regs::AddrReg::A1));
        assert_eq!(m68k.params[3].as_m68k_data(), Some(m68k_regs::DataReg::D1));
        assert_eq!(m68k.returns[0], ArgLoc::gpr_pair(0, 1)); // D0:D1 pair for 64-bit return
    }

    #[test]
    fn test_aarch64_and_frame_layout() {
        let macos = AbiSig::compute(
            ir::Target::aarch64_macos(),
            ir::CallConv::Fast,
            &[ir::Type::Ptr, ir::Type::F64],
            &[ir::Type::I64],
        );
        assert_eq!(macos.params[0].as_aarch64_gpr(), Some(aarch64_regs::Gpr::X0));
        assert_eq!(macos.params[1].as_aarch64_vec(), Some(aarch64_regs::VecReg::V0));
        assert_eq!(macos.red_zone_bytes, 128);

        let frame = FrameLayout::compute(&macos, 48, 0, true, 0, 8, false);
        assert!(frame.uses_red_zone);
        assert_eq!(frame.sp_sub_bytes, 0);
    }
}
