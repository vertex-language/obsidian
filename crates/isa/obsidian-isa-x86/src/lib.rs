#![no_std]
#![doc = include_str!("../README.md")]

pub mod addr;
pub mod cond;
pub mod encode;
pub mod opcode;
pub mod regs;
pub mod uarch;

pub use addr::{Disp, MemoryOperand, Scale};
pub use cond::{CondCode, FlagSet};
pub use encode::EncodedInst;
pub use opcode::{InstDesc, MacroFusionGroup, Opcode};
pub use regs::{Gpr, GprMask, GprSize, HighByteReg, KReg, RegClass, SimdReg, VecWidth};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpr_hardware_encoding_quirks() {
        // RSP (4) and R12 (12) share low3() == 4, requiring a SIB byte when used as a base pointer.
        assert!(Gpr::Rsp.base_requires_sib());
        assert!(Gpr::R12.base_requires_sib());
        assert!(!Gpr::Rax.base_requires_sib());
        assert!(!Gpr::Rbp.base_requires_sib());

        // RBP (5) and R13 (13) share low3() == 5, requiring at least a 1-byte disp8=0 when used as a base pointer.
        assert!(Gpr::Rbp.base_requires_disp8_zero());
        assert!(Gpr::R13.base_requires_disp8_zero());
        assert!(!Gpr::Rcx.base_requires_disp8_zero());

        // RSP cannot be a scaled index register in a SIB byte, whereas R12 CAN!
        assert!(!Gpr::Rsp.can_be_index());
        assert!(Gpr::R12.can_be_index());

        // R8..R15 require REX extension bits.
        assert!(!Gpr::Rdi.needs_rex_ext());
        assert!(Gpr::R8.needs_rex_ext());
        assert!(Gpr::R15.needs_rex_ext());
    }

    #[test]
    fn test_cond_code_inversions_and_commutations() {
        for cc in CondCode::ALL {
            // Flipping bit 0 twice returns the original condition code:
            assert_eq!(cc.invert().invert(), cc);
        }
        assert_eq!(CondCode::E.invert(), CondCode::NE);
        assert_eq!(CondCode::L.invert(), CondCode::GE);
        assert_eq!(CondCode::B.invert(), CondCode::AE);
        assert_eq!(CondCode::LE.invert(), CondCode::G);

        // Commuting operands (`a < b` <=> `b > a`):
        assert_eq!(CondCode::L.commute(), Some(CondCode::G));
        assert_eq!(CondCode::LE.commute(), Some(CondCode::GE));
        assert_eq!(CondCode::B.commute(), Some(CondCode::A));
        assert_eq!(CondCode::E.commute(), Some(CondCode::E));
    }

    #[test]
    fn test_opcode_descriptors_and_uarch_rules() {
        // LEA is non-destructive and touches zero flags:
        let lea = Opcode::Lea64.desc();
        assert!(!lea.tied_dst_src0);
        assert!(lea.flags_written.is_empty());
        assert_eq!(lea.uops, 1);

        // ADD is destructive 2-operand and writes all 6 arithmetic flags:
        let add = Opcode::Add64rr.desc();
        assert!(add.tied_dst_src0);
        assert_eq!(add.flags_written, FlagSet::ALL_ARITH);

        // INC/DEC preserve CF, so they cannot macro-fuse with unsigned CF jumps (JB/JAE/JBE/JA):
        assert!(!Opcode::Inc64r.desc().flags_written.contains(FlagSet::CF));
        assert!(uarch::can_macro_fuse(Opcode::Inc64r, CondCode::E));
        assert!(uarch::can_macro_fuse(Opcode::Inc64r, CondCode::L));
        assert!(!uarch::can_macro_fuse(Opcode::Inc64r, CondCode::B));
        assert!(!uarch::can_macro_fuse(Opcode::Inc64r, CondCode::A));

        // TZCNT, LZCNT, POPCNT, SQRTSD, CVTSI2SD carry false destination dependencies:
        assert!(Opcode::Tzcnt64rr.desc().has_false_dep);
        assert!(Opcode::Lzcnt64rr.desc().has_false_dep);
        assert!(Opcode::Popcnt64rr.desc().has_false_dep);
        assert!(Opcode::SqrtsdRr.desc().has_false_dep);
        assert!(Opcode::Cvtsi2sd64Rr.desc().has_false_dep);

        // Un-lamination detection on `[base + index * scale]` + immediate:
        let indexed = MemoryOperand::base_index_disp(Gpr::Rsi, Gpr::Rdi, Scale::S1, 0);
        assert!(uarch::causes_unlamination(&indexed, true));
        assert!(!uarch::causes_unlamination(&indexed, false));

        // Zero-cycle RAT move elimination:
        assert!(uarch::is_rat_zero_cycle_move(Opcode::Mov64rr, Gpr::Rax, Gpr::Rbx));
        assert!(!uarch::is_rat_zero_cycle_move(Opcode::Mov32rr, Gpr::Rax, Gpr::Rax)); // self-move fails!
    }

    #[test]
    fn test_memory_addressing_exact_byte_lengths() {
        // `mov eax, [rcx]` -> 2 bytes (`8B 01`)
        let i_rcx = encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rcx));
        assert_eq!(i_rcx.as_slice(), &[0x8B, 0x01]);

        // `mov eax, [rsp]` -> 3 bytes (`8B 04 24` — RSP forces SIB byte!)
        let i_rsp = encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rsp));
        assert_eq!(i_rsp.as_slice(), &[0x8B, 0x04, 0x24]);

        // `mov eax, [rbp]` -> 3 bytes (`8B 45 00` — RBP forces disp8=0!)
        let i_rbp = encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rbp));
        assert_eq!(i_rbp.as_slice(), &[0x8B, 0x45, 0x00]);

        // `mov r8d, [r12]` -> 4 bytes (`45 8B 04 24` — REX.RB + SIB!)
        let i_r12 = encode::mov32_rm(Gpr::R8, MemoryOperand::base(Gpr::R12));
        assert_eq!(i_r12.as_slice(), &[0x45, 0x8B, 0x04, 0x24]);

        // `mov r8d, [r13]` -> 4 bytes (`45 8B 45 00` — REX.RB + disp8=0!)
        let i_r13 = encode::mov32_rm(Gpr::R8, MemoryOperand::base(Gpr::R13));
        assert_eq!(i_r13.as_slice(), &[0x45, 0x8B, 0x45, 0x00]);

        // Explicit ForceDisp32 widening (used by SSA 32-byte DSB window packer to add +3 bytes without NOPs):
        let widened = encode::mov32_rm(
            Gpr::Rax,
            MemoryOperand::BaseIndex {
                base: Some(Gpr::Rcx),
                index: None,
                disp: Disp::ForceDisp32(0),
            },
        );
        assert_eq!(widened.as_slice(), &[0x8B, 0x81, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(widened.len, 6);
    }

    #[test]
    fn test_stateless_instruction_encodings() {
        // `add r8, r9` -> `4D 01 C8`
        assert_eq!(encode::add64_rr(Gpr::R8, Gpr::R9).as_slice(), &[0x4D, 0x01, 0xC8]);

        // `lea rax, [rdi + rsi*8 + 16]` -> `48 8D 44 F7 10`
        let lea = encode::lea64(
            Gpr::Rax,
            MemoryOperand::base_index_disp(Gpr::Rdi, Gpr::Rsi, Scale::S8, 16),
        );
        assert_eq!(lea.as_slice(), &[0x48, 0x8D, 0x44, 0xF7, 0x10]);

        // `tzcnt eax, ecx` -> `F3 48 0F BC C1` (64-bit `tzcnt rax, rcx`)
        assert_eq!(
            encode::tzcnt64_rr(Gpr::Rax, Gpr::Rcx).as_slice(),
            &[0xF3, 0x48, 0x0F, 0xBC, 0xC1]
        );

        // BMI2 `shlx rax, rdi, rsi` -> `C4 E2 C9 F7 C7`
        assert_eq!(
            encode::shlx64_rrr(Gpr::Rax, Gpr::Rdi, Gpr::Rsi).as_slice(),
            &[0xC4, 0xE2, 0xC9, 0xF7, 0xC7]
        );

        // AVX 2-byte VEX `vxorps xmm0, xmm0, xmm0` -> `C5 F8 57 C0`
        assert_eq!(
            encode::vxorps_xmm(SimdReg::XMM0, SimdReg::XMM0, SimdReg::XMM0).as_slice(),
            &[0xC5, 0xF8, 0x57, 0xC0]
        );

        // FMA3 `vfmadd213sd xmm0, xmm1, xmm2` -> `C4 E2 F1 A9 C2`
        assert_eq!(
            encode::vfmadd213sd_xmm(SimdReg::XMM0, SimdReg::XMM1, SimdReg::XMM2).as_slice(),
            &[0xC4, 0xE2, 0xF1, 0xA9, 0xC2]
        );
    }
}
