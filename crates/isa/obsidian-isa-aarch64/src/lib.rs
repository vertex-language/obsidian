#![no_std]
#![doc = include_str!("../README.md")]

pub mod addr;
pub mod cond;
pub mod encode;
pub mod opcode;
pub mod regs;
pub mod uarch;

#[cfg(test)]
mod tests {
    use crate::addr::{LogicalImm, PairOffset7};
    use crate::cond::CondCode;
    use crate::encode;
    use crate::opcode::Opcode;
    use crate::regs::{Gpr, GprMask, GprOrSp, GprOrXzr, PredReg, VecReg};
    use crate::uarch;

    #[test]
    fn test_register_encodings_and_masks() {
        assert_eq!(Gpr::X0.hw_id5(), 0);
        assert_eq!(Gpr::FP.hw_id5(), 29);
        assert_eq!(Gpr::LR.hw_id5(), 30);
        assert_eq!(GprOrXzr::Xzr.hw_id5(), 31);
        assert_eq!(GprOrSp::Sp.hw_id5(), 31);
        assert_eq!(VecReg::V31.hw_id5(), 31);
        assert_eq!(PredReg::P15 as u8, 15);

        assert!(!GprMask::AAPCS64_CALLER_SAVED.contains(Gpr::X19));
        assert!(GprMask::AAPCS64_CALLEE_SAVED.contains(Gpr::X19));
        assert!(GprMask::AAPCS64_CALLER_SAVED.contains(Gpr::X0));
    }

    #[test]
    fn test_cond_codes_and_logical_imm() {
        for raw in 0..16u8 {
            let cc = CondCode::from_hw_nibble(raw).unwrap();
            assert_eq!(cc.invert().invert(), cc);
        }
        assert_eq!(CondCode::LT.commute(), Some(CondCode::GT));

        // 0xFF00FF00FF00FF00 -> repeating 16-bit element 0xFF00 (8 ones rotated right by 8 in 16-bit element -> immr = 8, imms = 0b100111 = 39, N = 0)
        let imm = LogicalImm::try_encode64(0xFF00_FF00_FF00_FF00).unwrap();
        assert_eq!(imm.n, 0);
        assert_eq!(imm.immr, 8);
        assert_eq!(imm.imms, 0b10_0111);
    }

    #[test]
    fn test_instruction_encodings() {
        // movz x0, #0x1234, lsl #16 -> 0xD2A24680
        assert_eq!(
            encode::movz64(GprOrXzr::Gpr(Gpr::X0), 0x1234, 1).word(),
            0xD2A2_4680
        );

        // sub sp, sp, #32 -> 0xD10083FF
        assert_eq!(
            encode::sub64_imm(GprOrSp::Sp, GprOrSp::Sp, 32, false).word(),
            0xD100_83FF
        );

        // cmp x0, #0 -> subs xzr, x0, #0 -> 0xF100001F
        assert_eq!(
            encode::cmp64_imm(GprOrSp::Gpr(Gpr::X0), 0, false).word(),
            0xF100_001F
        );

        // mov x0, x1 -> orr x0, xzr, x1 -> 0xAA0103E0
        assert_eq!(encode::mov64_rr(Gpr::X0, Gpr::X1).word(), 0xAA01_03E0);

        // mul x0, x1, x2 -> madd x0, x1, x2, xzr -> 0x9B027C20
        assert_eq!(
            encode::mul64(
                GprOrXzr::Gpr(Gpr::X0),
                GprOrXzr::Gpr(Gpr::X1),
                GprOrXzr::Gpr(Gpr::X2)
            )
            .word(),
            0x9B02_7C20
        );

        // lsl x0, x1, #3 -> ubfm x0, x1, #61, #60 -> 0xD37DF020
        assert_eq!(
            encode::lsl64_imm(GprOrXzr::Gpr(Gpr::X0), GprOrXzr::Gpr(Gpr::X1), 3).word(),
            0xD37D_F020
        );

        // cset x0, eq -> csinc x0, xzr, xzr, ne -> 0x9A9F17E0
        assert_eq!(
            encode::cset64(GprOrXzr::Gpr(Gpr::X0), CondCode::EQ).word(),
            0x9A9F_17E0
        );

        // ldr x0, [sp, #16] (imm12_scaled = 2) -> 0xF9400BE0
        assert_eq!(
            encode::ldr64_uimm(GprOrXzr::Gpr(Gpr::X0), GprOrSp::Sp, 2).word(),
            0xF940_0BE0
        );

        // cbz x0, #4 (1 instruction forward) -> 0xB4000020
        assert_eq!(
            encode::cbz64_rel19(GprOrXzr::Gpr(Gpr::X0), 1).word(),
            0xB400_0020
        );

        // nop -> 0xD503201F
        assert_eq!(encode::nop().word(), 0xD503_201F);
    }

    #[test]
    fn test_uarch_and_pair_offsets() {
        assert!(uarch::can_macro_fuse_bcond(Opcode::SubsImm, CondCode::EQ));
        assert!(!uarch::can_macro_fuse_bcond(Opcode::AddImm, CondCode::EQ));
        assert!(uarch::is_single_cycle_arith_shift(3));
        assert!(!uarch::is_single_cycle_arith_shift(8));
        assert_eq!(PairOffset7::try_new_64(-512).unwrap().byte_offset_64(), -512);
        assert_eq!(PairOffset7::try_new_64(504).unwrap().byte_offset_64(), 504);
        assert!(PairOffset7::try_new_64(512).is_none());
    }
}
