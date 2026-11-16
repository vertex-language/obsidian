//! Stateless 1-instruction Motorola 680x0 (`M68k`) Big-Endian 16-bit word-stream encoders.
//!
//! Every M68k instruction consists of a 16-bit Big-Endian opcode word followed by `0..=4`
//! 16-bit Big-Endian extension words (`2..=10` bytes total, always 2-byte aligned).

use crate::addr::EffAddr;
use crate::cond::CondCode;
use crate::regs::{AddrReg, DataReg, OpSize, RegListMask};
use crate::uarch::encode_quick_1_to_8;

/// A single encoded M68k machine instruction (`2..=10` Big-Endian bytes, always even length).
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EncodedInst {
    pub bytes: [u8; 10],
    pub len: u8,
}

impl EncodedInst {
    pub const EMPTY: Self = Self {
        bytes: [0; 10],
        len: 0,
    };

    /// Returns the active instruction bytes (`&self.bytes[..self.len]`).
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        // Safety: `self.len` is always `<= 10`.
        unsafe { core::slice::from_raw_parts(self.bytes.as_ptr(), self.len as usize) }
    }

    /// Appends a 16-bit Big-Endian word (`2` bytes).
    #[inline]
    pub const fn push_u16_be(&mut self, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        self.bytes[self.len as usize] = hi;
        self.bytes[(self.len as usize) + 1] = lo;
        self.len += 2;
    }

    /// Appends a 32-bit Big-Endian longword (`4` bytes).
    #[inline]
    pub const fn push_u32_be(&mut self, long: u32) {
        let [b0, b1, b2, b3] = long.to_be_bytes();
        self.bytes[self.len as usize] = b0;
        self.bytes[(self.len as usize) + 1] = b1;
        self.bytes[(self.len as usize) + 2] = b2;
        self.bytes[(self.len as usize) + 3] = b3;
        self.len += 4;
    }

    /// Appends any 16-bit / 32-bit extension words required by `ea` for operand width `size`.
    #[inline]
    pub const fn push_ea_extensions(&mut self, ea: &EffAddr, size: OpSize) {
        match *ea {
            EffAddr::DataDirect(_)
            | EffAddr::AddrDirect(_)
            | EffAddr::AddrIndirect(_)
            | EffAddr::AddrPostInc(_)
            | EffAddr::AddrPreDec(_) => {}
            EffAddr::AddrDisp16(_, d16)
            | EffAddr::AbsShort(d16)
            | EffAddr::PcDisp16(d16) => {
                self.push_u16_be(d16 as u16);
            }
            EffAddr::AddrIndex(_, brief) | EffAddr::PcIndex(brief) => {
                self.push_u16_be(brief.to_extension_word());
            }
            EffAddr::AbsLong(addr32) => {
                self.push_u32_be(addr32);
            }
            EffAddr::Immediate(imm32) => match size {
                OpSize::Byte => self.push_u16_be((imm32 & 0xFF) as u16),
                OpSize::Word => self.push_u16_be((imm32 & 0xFFFF) as u16),
                OpSize::Long => self.push_u32_be(imm32),
            },
        }
    }
}

// =============================================================================
// 1. Data & Address Movement (`MOVEQ`, `MOVE`, `MOVEA`, `MOVEM`, `LEA`, `PEA`, `SWAP`, `EXG`)
// =============================================================================

/// `moveq #imm8, Dn` (`0111 | Dn:3 | 0 | imm8:8`, 2 bytes).
///
/// Sign-extends `imm8` (`-128..=127`) to a full 32-bit value in `dst` and updates `N, Z, V=0, C=0`.
#[inline]
pub const fn moveq(dst: DataReg, imm8: i8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x7000 | ((dst.hw_id() as u16) << 9) | ((imm8 as u8) as u16);
    out.push_u16_be(word);
    out
}

/// `move.<b/w/l> <src_ea>, <dst_ea>` (`00 | size:2 | dst_reg:3 | dst_mode:3 | src_mode:3 | src_reg:3`, 2..=10 bytes).
///
/// Note: In M68k silicon, the destination 6-bit EA field in `MOVE` has `reg` and `mode` reversed (`reg:3 | mode:3`)!
#[inline]
pub const fn move_ea_ea(size: OpSize, src: EffAddr, dst: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let sz = (size.move_size_bits() as u16) << 12;
    let dst_ea = (dst.move_dst_ea6_reg_mode() as u16) << 6;
    let src_ea = src.ea6_mode_reg() as u16;
    out.push_u16_be(sz | dst_ea | src_ea);
    out.push_ea_extensions(&src, size);
    out.push_ea_extensions(&dst, size);
    out
}

/// `movea.<w/l> <src_ea>, An` (`00 | size:2 | An:3 | 001 | src_mode:3 | src_reg:3`, 2..=6 bytes).
///
/// Sign-extends `.w` sources to 32 bits in `dst` and **never modifies `CCR` flags**!
#[inline]
pub const fn movea(size: OpSize, src: EffAddr, dst: AddrReg) -> EncodedInst {
    move_ea_ea(size, src, EffAddr::AddrDirect(dst))
}

/// `movem.<w/l> <reg_list>, <dst_ea>` (store multiple registers to memory, `0x4880 | sz<<6 | ea`, 4..=8 bytes).
///
/// Automatically bit-reverses the 16-bit register mask when `dst` is `EffAddr::AddrPreDec` (`-(An)`)!
#[inline]
pub const fn movem_to_mem(is_long: bool, regs: RegListMask, dst: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let sz_bit = if is_long { 1u16 << 6 } else { 0 };
    let is_predec = matches!(dst, EffAddr::AddrPreDec(_));
    out.push_u16_be(0x4880 | sz_bit | (dst.ea6_mode_reg() as u16));
    out.push_u16_be(regs.movem_extension_word(is_predec));
    out.push_ea_extensions(&dst, if is_long { OpSize::Long } else { OpSize::Word });
    out
}

/// `movem.<w/l> <src_ea>, <reg_list>` (load multiple registers from memory, `0x4C80 | sz<<6 | ea`, 4..=8 bytes).
#[inline]
pub const fn movem_from_mem(is_long: bool, src: EffAddr, regs: RegListMask) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let sz_bit = if is_long { 1u16 << 6 } else { 0 };
    out.push_u16_be(0x4C80 | sz_bit | (src.ea6_mode_reg() as u16));
    out.push_u16_be(regs.movem_extension_word(false));
    out.push_ea_extensions(&src, if is_long { OpSize::Long } else { OpSize::Word });
    out
}

/// `lea <src_ctrl_ea>, An` (`0100 | An:3 | 111 | ea:6`, 2..=6 bytes, 0 `CCR` change).
#[inline]
pub const fn lea(src_ctrl_ea: EffAddr, dst: AddrReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x41C0 | ((dst.hw_id() as u16) << 9) | (src_ctrl_ea.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src_ctrl_ea, OpSize::Long);
    out
}

/// `pea <src_ctrl_ea>` (`0x4840 | ea:6`, 2..=6 bytes, pushes effective address onto `-(sp)`).
#[inline]
pub const fn pea(src_ctrl_ea: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4840 | (src_ctrl_ea.ea6_mode_reg() as u16));
    out.push_ea_extensions(&src_ctrl_ea, OpSize::Long);
    out
}

/// `swap Dn` (`0x4840 | Dn:3`, 2 bytes, swaps high and low 16-bit halves of `Dn`).
#[inline]
pub const fn swap(dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4840 | (dst.hw_id() as u16));
    out
}

/// `exg Dx, Dy` (`0xC140 | Dx<<9 | Dy`, 2 bytes, swaps two 32-bit Data Registers with 0 `CCR` change).
#[inline]
pub const fn exg_dd(dx: DataReg, dy: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0xC140 | ((dx.hw_id() as u16) << 9) | (dy.hw_id() as u16));
    out
}

// =============================================================================
// 2. Integer Arithmetic (`ADD`, `ADDA`, `ADDQ`, `SUB`, `SUBA`, `SUBQ`, `CMP`, `CMPA`, `TST`, `CLR`, `NEG`)
// =============================================================================

/// `add.<b/w/l> <src_ea>, Dn` (`0xD000 | Dn<<9 | size<<6 | ea`, `Dn = Dn + <src_ea>`).
#[inline]
pub const fn add_ea_dn(size: OpSize, src: EffAddr, dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0xD000
        | ((dst.hw_id() as u16) << 9)
        | ((size.std_size_bits() as u16) << 6)
        | (src.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src, size);
    out
}

/// `adda.<w/l> <src_ea>, An` (`0xD0C0 | An<<9 | is_long<<8 | ea`, `An = An + sign_ext(<src_ea>)`, 0 `CCR` change!).
#[inline]
pub const fn adda(is_long: bool, src: EffAddr, dst: AddrReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let opmode = if is_long { 0b111u16 } else { 0b011u16 };
    let word = 0xD000 | ((dst.hw_id() as u16) << 9) | (opmode << 6) | (src.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src, if is_long { OpSize::Long } else { OpSize::Word });
    out
}

/// `addq.<b/w/l> #imm_1_to_8, <dst_ea>` (`0x5000 | imm3<<9 | size<<6 | ea`, 2..=6 bytes).
#[inline]
pub const fn addq(size: OpSize, imm_1_to_8: u8, dst: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let imm3 = match encode_quick_1_to_8(imm_1_to_8) {
        Some(b) => b as u16,
        None => 0,
    };
    let word = 0x5000 | (imm3 << 9) | ((size.std_size_bits() as u16) << 6) | (dst.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&dst, size);
    out
}

/// `sub.<b/w/l> <src_ea>, Dn` (`0x9000 | Dn<<9 | size<<6 | ea`, `Dn = Dn - <src_ea>`).
#[inline]
pub const fn sub_ea_dn(size: OpSize, src: EffAddr, dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x9000
        | ((dst.hw_id() as u16) << 9)
        | ((size.std_size_bits() as u16) << 6)
        | (src.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src, size);
    out
}

/// `suba.<w/l> <src_ea>, An` (`0x90C0 | An<<9 | is_long<<8 | ea`, `An = An - sign_ext(<src_ea>)`, 0 `CCR` change!).
#[inline]
pub const fn suba(is_long: bool, src: EffAddr, dst: AddrReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let opmode = if is_long { 0b111u16 } else { 0b011u16 };
    let word = 0x9000 | ((dst.hw_id() as u16) << 9) | (opmode << 6) | (src.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src, if is_long { OpSize::Long } else { OpSize::Word });
    out
}

/// `subq.<b/w/l> #imm_1_to_8, <dst_ea>` (`0x5100 | imm3<<9 | size<<6 | ea`, 2..=6 bytes).
#[inline]
pub const fn subq(size: OpSize, imm_1_to_8: u8, dst: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let imm3 = match encode_quick_1_to_8(imm_1_to_8) {
        Some(b) => b as u16,
        None => 0,
    };
    let word = 0x5100 | (imm3 << 9) | ((size.std_size_bits() as u16) << 6) | (dst.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&dst, size);
    out
}

/// `cmp.<b/w/l> <src_ea>, Dn` (`0xB000 | Dn<<9 | size<<6 | ea`, sets `CCR` from `Dn - <src_ea>`).
#[inline]
pub const fn cmp_ea_dn(size: OpSize, src: EffAddr, lhs: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0xB000
        | ((lhs.hw_id() as u16) << 9)
        | ((size.std_size_bits() as u16) << 6)
        | (src.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&src, size);
    out
}

/// `tst.<b/w/l> <ea>` (`0x4A00 | size<<6 | ea`, compares `<ea>` with `0`).
#[inline]
pub const fn tst(size: OpSize, ea: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x4A00 | ((size.std_size_bits() as u16) << 6) | (ea.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&ea, size);
    out
}

/// `clr.<b/w/l> <dst_ea>` (`0x4200 | size<<6 | ea`, zeroes `<dst_ea>`).
#[inline]
pub const fn clr(size: OpSize, dst: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x4200 | ((size.std_size_bits() as u16) << 6) | (dst.ea6_mode_reg() as u16);
    out.push_u16_be(word);
    out.push_ea_extensions(&dst, size);
    out
}

/// `ext.l Dn` (`0x48C0 | Dn`, 2 bytes, sign-extends 16-bit word in `Dn` to 32-bit long).
#[inline]
pub const fn ext_l(dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x48C0 | (dst.hw_id() as u16));
    out
}

/// `extb.l Dn` (`0x49C0 | Dn`, 2 bytes, 68020+ sign-extends 8-bit byte in `Dn` directly to 32-bit long).
#[inline]
pub const fn extb_l(dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x49C0 | (dst.hw_id() as u16));
    out
}

// =============================================================================
// 3. Shifts & Control Flow (`LSL`, `LSR`, `Bcc`, `BRA`, `BSR`, `DBcc`, `JSR`, `RTS`, `LINK`, `UNLK`)
// =============================================================================

/// `lsl.<b/w/l> #imm_1_to_8, Dn` (`0xE108 | imm3<<9 | size<<6 | Dn`, 2 bytes).
#[inline]
pub const fn lsl_imm(size: OpSize, imm_1_to_8: u8, dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let imm3 = match encode_quick_1_to_8(imm_1_to_8) {
        Some(b) => b as u16,
        None => 0,
    };
    let word = 0xE108 | (imm3 << 9) | ((size.std_size_bits() as u16) << 6) | (dst.hw_id() as u16);
    out.push_u16_be(word);
    out
}

/// `lsr.<b/w/l> #imm_1_to_8, Dn` (`0xE008 | imm3<<9 | size<<6 | Dn`, 2 bytes).
#[inline]
pub const fn lsr_imm(size: OpSize, imm_1_to_8: u8, dst: DataReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let imm3 = match encode_quick_1_to_8(imm_1_to_8) {
        Some(b) => b as u16,
        None => 0,
    };
    let word = 0xE008 | (imm3 << 9) | ((size.std_size_bits() as u16) << 6) | (dst.hw_id() as u16);
    out.push_u16_be(word);
    out
}

/// `bcc.b rel8` (2-byte short conditional branch; `rel8` must be non-zero and not `-1` (`0xFF`)).
#[inline]
pub const fn bcc_rel8(cc: CondCode, rel8: i8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let word = 0x6000 | ((cc.hw_nibble() as u16) << 8) | ((rel8 as u8) as u16);
    out.push_u16_be(word);
    out
}

/// `bcc.w rel16` (4-byte 16-bit PC-relative conditional branch: `0x6000 | cc<<8 | 0x00` + `rel16`).
#[inline]
pub const fn bcc_rel16(cc: CondCode, rel16: i16) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x6000 | ((cc.hw_nibble() as u16) << 8));
    out.push_u16_be(rel16 as u16);
    out
}

/// `dbcc Dn, rel16` (`0x50C8 | cc<<8 | Dn` + `rel16`, 4 bytes; `CondCode::F` is `dbra` / `dbf`).
#[inline]
pub const fn dbcc(cc: CondCode, counter: DataReg, rel16: i16) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x50C8 | ((cc.hw_nibble() as u16) << 8) | (counter.hw_id() as u16));
    out.push_u16_be(rel16 as u16);
    out
}

/// `jsr <target_ctrl_ea>` (`0x4E80 | ea:6`, 2..=6 bytes).
#[inline]
pub const fn jsr(target_ctrl_ea: EffAddr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E80 | (target_ctrl_ea.ea6_mode_reg() as u16));
    out.push_ea_extensions(&target_ctrl_ea, OpSize::Long);
    out
}

/// `rts` (`0x4E75`, 2 bytes, Return from Subroutine).
#[inline]
pub const fn rts() -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E75);
    out
}

/// `link.w An, #disp16` (`0x4E50 | An` + `disp16`, 4 bytes, allocates stack frame).
#[inline]
pub const fn link_w(frame_reg: AddrReg, frame_disp16: i16) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E50 | (frame_reg.hw_id() as u16));
    out.push_u16_be(frame_disp16 as u16);
    out
}

/// `unlk An` (`0x4E58 | An`, 2 bytes, deallocates stack frame).
#[inline]
pub const fn unlk(frame_reg: AddrReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E58 | (frame_reg.hw_id() as u16));
    out
}

/// `nop` (`0x4E71`, 2 bytes).
#[inline]
pub const fn nop() -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E71);
    out
}

/// `trap #vector4` (`0x4E40 | (vec4 & 0x0F)`, 2 bytes).
#[inline]
pub const fn trap(vector4: u8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push_u16_be(0x4E40 | ((vector4 & 0x0F) as u16));
    out
}
