#![no_std]
#![doc = include_str!("../README.md")]

pub mod addr;
pub mod cond;
pub mod encode;
pub mod opcode;
pub mod regs;
pub mod uarch;

pub use addr::{BriefIndex, EffAddr, IndexScale};
pub use cond::{CcrFlags, CondCode};
pub use encode::EncodedInst;
pub use opcode::{InstDesc, Opcode};
pub use regs::{AddrReg, DataReg, FpReg, OpSize, Reg, RegListMask};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_data_and_address_register_rules() {
        // Address registers forbid 8-bit Byte operations:
        for d in DataReg::ALL {
            assert!(d.to_reg().supports_size(OpSize::Byte));
            assert!(d.to_reg().supports_size(OpSize::Word));
            assert!(d.to_reg().supports_size(OpSize::Long));
        }
        for a in AddrReg::ALL {
            assert!(!a.to_reg().supports_size(OpSize::Byte));
            assert!(a.to_reg().supports_size(OpSize::Word));
            assert!(a.to_reg().supports_size(OpSize::Long));
        }

        // A7 (SP) steps by 2 bytes on Byte post-inc / pre-dec:
        assert_eq!(AddrReg::A0.auto_inc_dec_step(OpSize::Byte), 1);
        assert_eq!(AddrReg::A7.auto_inc_dec_step(OpSize::Byte), 2);

        // MOVEM mask bit-reversal on pre-decrement `-(An)`:
        let list = RegListMask::from_regs(&[Reg::Data(DataReg::D0), Reg::Addr(AddrReg::A6)]);
        assert_eq!(list.movem_extension_word(false), (1 << 0) | (1 << 14));
        assert_eq!(
            list.movem_extension_word(true),
            ((1u16 << 0) | (1u16 << 14)).reverse_bits()
        );
    }

    #[test]
    fn test_quick_1_to_8_encoding_and_decoding() {
        assert_eq!(uarch::encode_quick_1_to_8(0), None);
        assert_eq!(uarch::encode_quick_1_to_8(9), None);
        for v in 1..=8u8 {
            let bits = uarch::encode_quick_1_to_8(v).unwrap();
            if v == 8 {
                assert_eq!(bits, 0);
            } else {
                assert_eq!(bits, v);
            }
            assert_eq!(uarch::decode_quick_1_to_8(bits), v);
        }
    }

    #[test]
    fn test_cond_code_inversions_and_commutations() {
        for cc in CondCode::ALL {
            assert_eq!(cc.invert().invert(), cc);
        }
        assert_eq!(CondCode::HI.invert(), CondCode::LS);
        assert_eq!(CondCode::CC.invert(), CondCode::CS);
        assert_eq!(CondCode::NE.invert(), CondCode::EQ);
        assert_eq!(CondCode::GE.invert(), CondCode::LT);
        assert_eq!(CondCode::GT.invert(), CondCode::LE);

        assert_eq!(CondCode::GT.commute(), Some(CondCode::LT));
        assert_eq!(CondCode::HI.commute(), Some(CondCode::CS));
        assert_eq!(CondCode::LE.commute(), Some(CondCode::GE));
    }

    #[test]
    fn test_big_endian_m68k_instruction_encodings() {
        // `moveq #42, d3` -> `76 2A`
        assert_eq!(encode::moveq(DataReg::D3, 42).as_slice(), &[0x76, 0x2A]);

        // `link.w a6, #-32` -> `4E 56 FF E0`
        assert_eq!(
            encode::link_w(AddrReg::FP, -32).as_slice(),
            &[0x4E, 0x56, 0xFF, 0xE0]
        );

        // `unlk a6` -> `4E 5E`
        assert_eq!(encode::unlk(AddrReg::FP).as_slice(), &[0x4E, 0x5E]);

        // `rts` -> `4E 75`
        assert_eq!(encode::rts().as_slice(), &[0x4E, 0x75]);
    }
}
