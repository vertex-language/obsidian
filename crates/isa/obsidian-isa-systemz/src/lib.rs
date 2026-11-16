#![no_std]
#![doc = include_str!("../README.md")]

pub mod addr;
pub mod cond;
pub mod encode;
pub mod opcode;
pub mod regs;
pub mod uarch;

pub use addr::{Disp12, Disp20, MemBx};
pub use cond::{CcValue, CondMask};
pub use encode::EncodedInst;
pub use opcode::{InstDesc, InstFormat, Opcode};
pub use regs::{AccessReg, Gpr, GprMask, GprPair, GprPart, VecReg};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_r0_base_index_and_even_odd_pair_rules() {
        // R0 cannot be a base or index register on SystemZ:
        assert!(!Gpr::R0.can_be_base_or_index());
        for &r in &Gpr::ALL[1..] {
            assert!(r.can_be_base_or_index());
        }
        assert!(!MemBx::base(Gpr::R0).is_legal_in_silicon());
        assert!(MemBx::base(Gpr::R1).is_legal_in_silicon());

        // Even:Odd register pairs:
        assert_eq!(GprPair::from_even(Gpr::R0), Some(GprPair::R0R1));
        assert_eq!(GprPair::from_even(Gpr::R1), None);
        assert_eq!(GprPair::R4R5.even(), Gpr::R4);
        assert_eq!(GprPair::R4R5.odd(), Gpr::R5);
        assert!(GprPair::R4R5.mask().contains(Gpr::R4));
        assert!(GprPair::R4R5.mask().contains(Gpr::R5));
        assert!(!GprPair::R4R5.mask().contains(Gpr::R6));
    }

    #[test]
    fn test_disp12_and_disp20_splitting() {
        assert!(Disp12::try_new(0).is_some());
        assert!(Disp12::try_new(4095).is_some());
        assert!(Disp12::try_new(-1).is_none());
        assert!(Disp12::try_new(4096).is_none());

        // -8 in 20-bit two's complement is `0xFFFF8`: DL = `0xFF8`, DH = `0xFF`
        let d_neg8 = Disp20::try_new(-8).unwrap();
        assert_eq!(d_neg8.dl_12(), 0xFF8);
        assert_eq!(d_neg8.dh_8(), 0xFF);
        assert!(!d_neg8.fits_disp12());

        // +4096 in 20-bit is `0x01000`: DL = `0x000`, DH = `0x01`
        let d_4096 = Disp20::try_new(4096).unwrap();
        assert_eq!(d_4096.dl_12(), 0x000);
        assert_eq!(d_4096.dh_8(), 0x01);
        assert!(!d_4096.fits_disp12());
    }

    #[test]
    fn test_cond_mask_inversions_and_commutations() {
        for mask_bits in 0..16u8 {
            let m = CondMask(mask_bits);
            assert_eq!(m.invert_all().invert_all(), m);
            assert_eq!(m.commute_cmp().commute_cmp(), m);
        }
        assert_eq!(CondMask::E.invert_cmp(), CondMask::NE_CMP);
        assert_eq!(CondMask::L.invert_cmp(), CondMask::HE);
        assert_eq!(CondMask::H.invert_cmp(), CondMask::LE);
        assert_eq!(CondMask::L.commute_cmp(), CondMask::H);
        assert_eq!(CondMask::LE.commute_cmp(), CondMask::HE);
    }

    #[test]
    fn test_big_endian_encodings_and_length_prefix_bits() {
        // 2-byte RR: `lr %r1, %r2` -> `18 12`
        let lr = encode::lr(Gpr::R1, Gpr::R2);
        assert_eq!(lr.as_slice(), &[0x18, 0x12]);
        assert_eq!(uarch::inst_byte_len_from_first_byte(lr.bytes[0]), 2);

        // 4-byte RI: `lghi %r3, -1` -> `A7 39 FF FF`
        let lghi = encode::lghi(Gpr::R3, -1);
        assert_eq!(lghi.as_slice(), &[0xA7, 0x39, 0xFF, 0xFF]);
        assert_eq!(uarch::inst_byte_len_from_first_byte(lghi.bytes[0]), 4);

        // 6-byte RSY: `sllg %r2, %r3, 5` -> `EB 23 00 05 00 0D`
        let sllg = encode::sllg(Gpr::R2, Gpr::R3, None, Disp20::try_new(5).unwrap());
        assert_eq!(sllg.as_slice(), &[0xEB, 0x23, 0x00, 0x05, 0x00, 0x0D]);
        assert_eq!(uarch::inst_byte_len_from_first_byte(sllg.bytes[0]), 6);

        // 6-byte RIE: `risbgn %r2, %r3, 32, 63, 0, true` (zero-extend low 32 bits in 1 cycle, 0 CC clobber)
        let risbgn = encode::risbgn(Gpr::R2, Gpr::R3, 32, 63, 0, true);
        assert_eq!(risbgn.as_slice(), &[0xEC, 0x23, 0x20, 0xBF, 0x00, 0x59]);
        assert_eq!(uarch::inst_byte_len_from_first_byte(risbgn.bytes[0]), 6);
    }
}
