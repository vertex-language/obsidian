//! Stateless 1-instruction IBM SystemZ (`s390x`) big-endian byte encoders.
//!
//! Every SystemZ instruction is `2`, `4`, or `6` bytes in **Big-Endian** byte order,
//! and its top 2 bits (`bytes[0] >> 6`) always match its exact length (`00` = 2B,
//! `01`/`10` = 4B, `11` = 6B).

use crate::addr::{Disp12, Disp20, MemBx};
use crate::cond::CondMask;
use crate::regs::{Gpr, GprPair, VecReg};

/// A single encoded SystemZ instruction (`2`, `4`, or `6` bytes in Big-Endian order).
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EncodedInst {
    pub bytes: [u8; 6],
    pub len: u8,
}

impl EncodedInst {
    /// Returns the active instruction bytes (`&self.bytes[..self.len]`).
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        // Safety: `self.len` is always `2`, `4`, or `6` (`<= 6`).
        unsafe { core::slice::from_raw_parts(self.bytes.as_ptr(), self.len as usize) }
    }

    #[inline]
    pub const fn from_2(b0: u8, b1: u8) -> Self {
        Self {
            bytes: [b0, b1, 0, 0, 0, 0],
            len: 2,
        }
    }

    #[inline]
    pub const fn from_4(b0: u8, b1: u8, b2: u8, b3: u8) -> Self {
        Self {
            bytes: [b0, b1, b2, b3, 0, 0],
            len: 4,
        }
    }

    #[inline]
    pub const fn from_6(b0: u8, b1: u8, b2: u8, b3: u8, b4: u8, b5: u8) -> Self {
        Self {
            bytes: [b0, b1, b2, b3, b4, b5],
            len: 6,
        }
    }
}

// =============================================================================
// Internal Format Bit-Packers (RR, RRE, RRF, RI, RX, RXY, RSY, RIL, RIE, SS, VRR)
// =============================================================================

/// 2-byte `RR` format: `[op:8][r1:4][r2:4]`
#[inline]
pub const fn encode_rr(op: u8, r1: u8, r2: u8) -> EncodedInst {
    EncodedInst::from_2(op, ((r1 & 0x0F) << 4) | (r2 & 0x0F))
}

/// 4-byte `RRE` format: `[op_hi:8][op_lo:8][0x00:8][r1:4][r2:4]`
#[inline]
pub const fn encode_rre(op16: u16, r1: u8, r2: u8) -> EncodedInst {
    let [op_hi, op_lo] = op16.to_be_bytes();
    EncodedInst::from_4(op_hi, op_lo, 0x00, ((r1 & 0x0F) << 4) | (r2 & 0x0F))
}

/// 4-byte `RRF` format: `[op_hi:8][op_lo:8][r3:4][m4:4][r1:4][r2:4]`
#[inline]
pub const fn encode_rrf(op16: u16, r3: u8, m4: u8, r1: u8, r2: u8) -> EncodedInst {
    let [op_hi, op_lo] = op16.to_be_bytes();
    EncodedInst::from_4(
        op_hi,
        op_lo,
        ((r3 & 0x0F) << 4) | (m4 & 0x0F),
        ((r1 & 0x0F) << 4) | (r2 & 0x0F),
    )
}

/// 4-byte `RI` format: `[op1:8][r1:4][op2:4][i2:16 (BE)]`
#[inline]
pub const fn encode_ri(op1: u8, r1: u8, op2_nibble: u8, imm16: i16) -> EncodedInst {
    let [imm_hi, imm_lo] = imm16.to_be_bytes();
    EncodedInst::from_4(
        op1,
        ((r1 & 0x0F) << 4) | (op2_nibble & 0x0F),
        imm_hi,
        imm_lo,
    )
}

/// 4-byte `RX` format: `[op:8][r1:4][x2:4][b2:4][d2_hi:4][d2_lo:8]`
#[inline]
pub const fn encode_rx(op: u8, r1: u8, x2: u8, b2: u8, disp12: Disp12) -> EncodedInst {
    let d = disp12.raw();
    EncodedInst::from_4(
        op,
        ((r1 & 0x0F) << 4) | (x2 & 0x0F),
        ((b2 & 0x0F) << 4) | (((d >> 8) as u8) & 0x0F),
        (d & 0xFF) as u8,
    )
}

/// 6-byte `RXY` format: `[op1:8][r1:4][x2:4][b2:4][dl_hi:4][dl_lo:8][dh:8][op2:8]`
#[inline]
pub const fn encode_rxy(op1: u8, r1: u8, mem: MemBx<Gpr>, op2: u8) -> EncodedInst {
    let dl = mem.disp.dl_12();
    let dh = mem.disp.dh_8();
    EncodedInst::from_6(
        op1,
        ((r1 & 0x0F) << 4) | mem.index_nibble(),
        (mem.base_nibble() << 4) | (((dl >> 8) as u8) & 0x0F),
        (dl & 0xFF) as u8,
        dh,
        op2,
    )
}

/// 6-byte `RSY` format: `[op1:8][r1:4][r3:4][b2:4][dl_hi:4][dl_lo:8][dh:8][op2:8]`
#[inline]
pub const fn encode_rsy(
    op1: u8,
    r1: u8,
    r3: u8,
    b2: Option<Gpr>,
    disp20: Disp20,
    op2: u8,
) -> EncodedInst {
    let b_nib = match b2 {
        Some(b) => b.hw_id(),
        None => 0,
    };
    let dl = disp20.dl_12();
    let dh = disp20.dh_8();
    EncodedInst::from_6(
        op1,
        ((r1 & 0x0F) << 4) | (r3 & 0x0F),
        (b_nib << 4) | (((dl >> 8) as u8) & 0x0F),
        (dl & 0xFF) as u8,
        dh,
        op2,
    )
}

/// 6-byte `RIL` format: `[op1:8][r1:4][op2:4][i2:32 (BE)]`
#[inline]
pub const fn encode_ril(op1: u8, r1: u8, op2_nibble: u8, imm32: i32) -> EncodedInst {
    let [b2, b3, b4, b5] = imm32.to_be_bytes();
    EncodedInst::from_6(op1, ((r1 & 0x0F) << 4) | (op2_nibble & 0x0F), b2, b3, b4, b5)
}

/// 6-byte `SS` (Storage-to-Storage) format: `[op:8][l:8][b1:4][d1:12][b2:4][d2:12]`
#[inline]
pub const fn encode_ss(
    op: u8,
    len_minus_1: u8,
    b1: Gpr,
    d1: Disp12,
    b2: Gpr,
    d2: Disp12,
) -> EncodedInst {
    let raw1 = d1.raw();
    let raw2 = d2.raw();
    EncodedInst::from_6(
        op,
        len_minus_1,
        (b1.hw_id() << 4) | (((raw1 >> 8) as u8) & 0x0F),
        (raw1 & 0xFF) as u8,
        (b2.hw_id() << 4) | (((raw2 >> 8) as u8) & 0x0F),
        (raw2 & 0xFF) as u8,
    )
}

// =============================================================================
// 1. Register Moves, Sign/Zero Extensions & Conditional Moves
// =============================================================================

/// `lgr r1, r2` (64-bit register move, `B9 04 00 r1r2`, 4 bytes)
#[inline]
pub const fn lgr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rre(0xB904, dst.hw_id(), src.hw_id())
}

/// `lr r1, r2` (32-bit low register move, `18 r1r2`, 2 bytes, preserves high 32 bits!)
#[inline]
pub const fn lr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rr(0x18, dst.hw_id(), src.hw_id())
}

/// `lgfr r1, r2` (32-to-64 sign extend, `B9 14 00 r1r2`, 4 bytes)
#[inline]
pub const fn lgfr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rre(0xB914, dst.hw_id(), src.hw_id())
}

/// `llgfr r1, r2` (32-to-64 zero extend, `B9 16 00 r1r2`, 4 bytes)
#[inline]
pub const fn llgfr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rre(0xB916, dst.hw_id(), src.hw_id())
}

/// `locgr r1, r2, m3` (64-bit conditional move on `CondMask`, `B9 E2 m30 r1r2`, 4 bytes)
#[inline]
pub const fn locgr(dst: Gpr, src: Gpr, mask: CondMask) -> EncodedInst {
    encode_rrf(0xB9E2, mask.hw_nibble(), 0, dst.hw_id(), src.hw_id())
}

/// `lrvgr r1, r2` (64-bit byte-swap, `B9 0F 00 r1r2`, 4 bytes)
#[inline]
pub const fn lrvgr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rre(0xB90F, dst.hw_id(), src.hw_id())
}

// =============================================================================
// 2. Immediate Materialization & PC-Relative Address Generation
// =============================================================================

/// `lghi r1, imm16` (sign-extend 16-bit immediate to 64 bits, `A7 r19 i2`, 4 bytes)
#[inline]
pub const fn lghi(dst: Gpr, imm16: i16) -> EncodedInst {
    encode_ri(0xA7, dst.hw_id(), 0x9, imm16)
}

/// `lhi r1, imm16` (sign-extend 16-bit immediate to low 32 bits, `A7 r18 i2`, 4 bytes)
#[inline]
pub const fn lhi(dst: Gpr, imm16: i16) -> EncodedInst {
    encode_ri(0xA7, dst.hw_id(), 0x8, imm16)
}

/// `lgfi r1, imm32` (sign-extend 32-bit immediate to 64 bits, `C0 r11 i2`, 6 bytes)
#[inline]
pub const fn lgfi(dst: Gpr, imm32: i32) -> EncodedInst {
    encode_ril(0xC0, dst.hw_id(), 0x1, imm32)
}

/// `llihf r1, uimm32` (load 32-bit immediate into high 32 bits, zero low 32, `C0 r1E i2`, 6 bytes)
#[inline]
pub const fn llihf(dst: Gpr, uimm32: u32) -> EncodedInst {
    encode_ril(0xC0, dst.hw_id(), 0xE, uimm32 as i32)
}

/// `llilf r1, uimm32` (load 32-bit immediate into low 32 bits, zero high 32, `C0 r1F i2`, 6 bytes)
#[inline]
pub const fn llilf(dst: Gpr, uimm32: u32) -> EncodedInst {
    encode_ril(0xC0, dst.hw_id(), 0xF, uimm32 as i32)
}

/// `larl r1, rel32_halfwords` (PC-relative address `PSW + 2 * rel32_hw`, `C0 r10 i2`, 6 bytes)
#[inline]
pub const fn larl(dst: Gpr, rel32_halfwords: i32) -> EncodedInst {
    encode_ril(0xC0, dst.hw_id(), 0x0, rel32_halfwords)
}

// =============================================================================
// 3. Memory Loads, Stores & Non-Destructive Address Generation (`LA` / `LAY`)
// =============================================================================

/// `lg r1, d20(x2, b2)` (64-bit load, `E3 r1x2 b2dl dh 04`, 6 bytes)
#[inline]
pub const fn lg(dst: Gpr, mem: MemBx<Gpr>) -> EncodedInst {
    encode_rxy(0xE3, dst.hw_id(), mem, 0x04)
}

/// `l r1, d12(x2, b2)` (32-bit load with 12-bit unsigned disp, `58 r1x2 b2d2`, 4 bytes)
#[inline]
pub const fn l(dst: Gpr, x2: Option<Gpr>, b2: Option<Gpr>, disp12: Disp12) -> EncodedInst {
    let x_nib = match x2 { Some(r) => r.hw_id(), None => 0 };
    let b_nib = match b2 { Some(r) => r.hw_id(), None => 0 };
    encode_rx(0x58, dst.hw_id(), x_nib, b_nib, disp12)
}

/// `stg r1, d20(x2, b2)` (64-bit store, `E3 r1x2 b2dl dh 24`, 6 bytes)
#[inline]
pub const fn stg(src: Gpr, mem: MemBx<Gpr>) -> EncodedInst {
    encode_rxy(0xE3, src.hw_id(), mem, 0x24)
}

/// `st r1, d12(x2, b2)` (32-bit store with 12-bit unsigned disp, `50 r1x2 b2d2`, 4 bytes)
#[inline]
pub const fn st(src: Gpr, x2: Option<Gpr>, b2: Option<Gpr>, disp12: Disp12) -> EncodedInst {
    let x_nib = match x2 { Some(r) => r.hw_id(), None => 0 };
    let b_nib = match b2 { Some(r) => r.hw_id(), None => 0 };
    encode_rx(0x50, src.hw_id(), x_nib, b_nib, disp12)
}

/// `lmg r1, r3, d20(b2)` (load multiple 64-bit registers `r1..=r3`, `EB r1r3 b2dl dh 04`, 6 bytes)
#[inline]
pub const fn lmg(first: Gpr, last: Gpr, base: Gpr, disp20: Disp20) -> EncodedInst {
    encode_rsy(0xEB, first.hw_id(), last.hw_id(), Some(base), disp20, 0x04)
}

/// `stmg r1, r3, d20(b2)` (store multiple 64-bit registers `r1..=r3`, `EB r1r3 b2dl dh 24`, 6 bytes)
#[inline]
pub const fn stmg(first: Gpr, last: Gpr, base: Gpr, disp20: Disp20) -> EncodedInst {
    encode_rsy(0xEB, first.hw_id(), last.hw_id(), Some(base), disp20, 0x24)
}

/// `la r1, d12(x2, b2)` (non-destructive 12-bit address generation `r1 = b2 + x2 + d12`, `41`, 4 bytes)
#[inline]
pub const fn la(dst: Gpr, x2: Option<Gpr>, b2: Option<Gpr>, disp12: Disp12) -> EncodedInst {
    let x_nib = match x2 { Some(r) => r.hw_id(), None => 0 };
    let b_nib = match b2 { Some(r) => r.hw_id(), None => 0 };
    encode_rx(0x41, dst.hw_id(), x_nib, b_nib, disp12)
}

/// `lay r1, d20(x2, b2)` (non-destructive 20-bit signed address generation `r1 = b2 + x2 + d20`, `E3..71`, 6 bytes)
#[inline]
pub const fn lay(dst: Gpr, mem: MemBx<Gpr>) -> EncodedInst {
    encode_rxy(0xE3, dst.hw_id(), mem, 0x71)
}

// =============================================================================
// 4. 2-Operand & 3-Operand (Distinct-Operands Facility) Integer ALU
// =============================================================================

/// `agr r1, r2` (64-bit 2-op add `r1 += r2`, `B9 08 00 r1r2`, 4 bytes)
#[inline] pub const fn agr(dst: Gpr, src: Gpr) -> EncodedInst { encode_rre(0xB908, dst.hw_id(), src.hw_id()) }
/// `ar r1, r2` (32-bit 2-op add `r1_low += r2_low`, `1A r1r2`, 2 bytes)
#[inline] pub const fn ar(dst: Gpr, src: Gpr) -> EncodedInst  { encode_rr(0x1A, dst.hw_id(), src.hw_id()) }
/// `aghi r1, imm16` (64-bit add 16-bit signed immediate, `A7 r1B i2`, 4 bytes)
#[inline] pub const fn aghi(dst: Gpr, imm16: i16) -> EncodedInst { encode_ri(0xA7, dst.hw_id(), 0xB, imm16) }

/// `sgr r1, r2` (64-bit 2-op subtract `r1 -= r2`, `B9 09 00 r1r2`, 4 bytes)
#[inline] pub const fn sgr(dst: Gpr, src: Gpr) -> EncodedInst { encode_rre(0xB909, dst.hw_id(), src.hw_id()) }
/// `sr r1, r2` (32-bit 2-op subtract `r1_low -= r2_low`, `1B r1r2`, 2 bytes)
#[inline] pub const fn sr(dst: Gpr, src: Gpr) -> EncodedInst  { encode_rr(0x1B, dst.hw_id(), src.hw_id()) }

/// `ngr r1, r2` (64-bit 2-op AND `r1 &= r2`, `B9 80 00 r1r2`, 4 bytes)
#[inline] pub const fn ngr(dst: Gpr, src: Gpr) -> EncodedInst { encode_rre(0xB980, dst.hw_id(), src.hw_id()) }
/// `ogr r1, r2` (64-bit 2-op OR `r1 |= r2`, `B9 81 00 r1r2`, 4 bytes)
#[inline] pub const fn ogr(dst: Gpr, src: Gpr) -> EncodedInst { encode_rre(0xB981, dst.hw_id(), src.hw_id()) }
/// `xgr r1, r2` (64-bit 2-op XOR `r1 ^= r2`, `B9 82 00 r1r2`, 4 bytes)
#[inline] pub const fn xgr(dst: Gpr, src: Gpr) -> EncodedInst { encode_rre(0xB982, dst.hw_id(), src.hw_id()) }

/// `agrk r1, r2, r3` (64-bit 3-operand non-destructive add `r1 = r2 + r3`, `B9 E8 r30 r1r2`, 4 bytes)
#[inline]
pub const fn agrk(dst: Gpr, lhs: Gpr, rhs: Gpr) -> EncodedInst {
    encode_rrf(0xB9E8, rhs.hw_id(), 0, dst.hw_id(), lhs.hw_id())
}

/// `sgrk r1, r2, r3` (64-bit 3-operand non-destructive subtract `r1 = r2 - r3`, `B9 E9 r30 r1r2`, 4 bytes)
#[inline]
pub const fn sgrk(dst: Gpr, lhs: Gpr, rhs: Gpr) -> EncodedInst {
    encode_rrf(0xB9E9, rhs.hw_id(), 0, dst.hw_id(), lhs.hw_id())
}

/// `ngrk r1, r2, r3` (64-bit 3-operand non-destructive AND `r1 = r2 & r3`, `B9 E4 r30 r1r2`, 4 bytes)
#[inline]
pub const fn ngrk(dst: Gpr, lhs: Gpr, rhs: Gpr) -> EncodedInst {
    encode_rrf(0xB9E4, rhs.hw_id(), 0, dst.hw_id(), lhs.hw_id())
}

/// `ogrk r1, r2, r3` (64-bit 3-operand non-destructive OR `r1 = r2 | r3`, `B9 E6 r30 r1r2`, 4 bytes)
#[inline]
pub const fn ogrk(dst: Gpr, lhs: Gpr, rhs: Gpr) -> EncodedInst {
    encode_rrf(0xB9E6, rhs.hw_id(), 0, dst.hw_id(), lhs.hw_id())
}

/// `xgrk r1, r2, r3` (64-bit 3-operand non-destructive XOR `r1 = r2 ^ r3`, `B9 E7 r30 r1r2`, 4 bytes)
#[inline]
pub const fn xgrk(dst: Gpr, lhs: Gpr, rhs: Gpr) -> EncodedInst {
    encode_rrf(0xB9E7, rhs.hw_id(), 0, dst.hw_id(), lhs.hw_id())
}

/// `msgr r1, r2` (64-bit single-register multiply `r1 *= r2`, `B9 0C 00 r1r2`, 4 bytes, 0 CC change)
#[inline]
pub const fn msgr(dst: Gpr, src: Gpr) -> EncodedInst {
    encode_rre(0xB90C, dst.hw_id(), src.hw_id())
}

/// `mlgr pair, r2` (128-bit unsigned multiply `pair.odd * r2 -> pair.even:pair.odd`, `B9 86 00 r1r2`, 4 bytes)
#[inline]
pub const fn mlgr(dst_pair: GprPair, src: Gpr) -> EncodedInst {
    encode_rre(0xB986, dst_pair.hw_id(), src.hw_id())
}

/// `dlgr pair, r2` (128/64 unsigned divide `pair / r2 -> rem pair.even, quot pair.odd`, `B9 87 00 r1r2`, 4 bytes)
#[inline]
pub const fn dlgr(dst_pair: GprPair, divisor: Gpr) -> EncodedInst {
    encode_rre(0xB987, dst_pair.hw_id(), divisor.hw_id())
}

// =============================================================================
// 5. Non-Destructive 3-Operand Shifts & Rotate-Then-Insert (`RISBG` / `RISBGN`)
// =============================================================================

/// `sllg r1, r3, d20(b2)` (64-bit logical shift left `r1 = r3 << (b2 + d20)`, `EB r1r3 b2dl dh 0D`, 6 bytes)
#[inline]
pub const fn sllg(dst: Gpr, src: Gpr, shift_base: Option<Gpr>, shift_imm: Disp20) -> EncodedInst {
    encode_rsy(0xEB, dst.hw_id(), src.hw_id(), shift_base, shift_imm, 0x0D)
}

/// `srlg r1, r3, d20(b2)` (64-bit logical shift right `r1 = r3 >> (b2 + d20)`, `EB r1r3 b2dl dh 0C`, 6 bytes)
#[inline]
pub const fn srlg(dst: Gpr, src: Gpr, shift_base: Option<Gpr>, shift_imm: Disp20) -> EncodedInst {
    encode_rsy(0xEB, dst.hw_id(), src.hw_id(), shift_base, shift_imm, 0x0C)
}

/// `srag r1, r3, d20(b2)` (64-bit arithmetic shift right, `EB r1r3 b2dl dh 0A`, 6 bytes)
#[inline]
pub const fn srag(dst: Gpr, src: Gpr, shift_base: Option<Gpr>, shift_imm: Disp20) -> EncodedInst {
    encode_rsy(0xEB, dst.hw_id(), src.hw_id(), shift_base, shift_imm, 0x0A)
}

/// `risbgn r1, r2, start_bit, end_bit, rotate_amount, zero_remaining` (`EC r1r2 i3 i4 i5 59`, 6 bytes).
///
/// Rotates `r2` left by `rotate_amount` bits (`0..=63`) and inserts IBM bits `start_bit..=end_bit`
/// (`0..=63`) into `r1`. If `zero_remaining` is `true` (sets bit 0 of `i4`), all other bits of `r1`
/// are zeroed (making it a pure non-destructive 1-cycle bitfield extract/mask without touching `CC`!).
#[inline]
pub const fn risbgn(
    dst: Gpr,
    src: Gpr,
    start_bit: u8,
    end_bit: u8,
    rotate_left: u8,
    zero_remaining: bool,
) -> EncodedInst {
    let i4 = (end_bit & 0x3F) | (if zero_remaining { 0x80 } else { 0x00 });
    EncodedInst::from_6(
        0xEC,
        (dst.hw_id() << 4) | src.hw_id(),
        start_bit & 0x3F,
        i4,
        rotate_left & 0x3F,
        0x59,
    )
}

/// `flogr pair, src` (Find Leftmost One: `pair.even = clz(src)`, `pair.odd = src & ~(1 << (63 - clz))`, `B9 83 00 r1r2`, 4 bytes)
#[inline]
pub const fn flogr(dst_pair: GprPair, src: Gpr) -> EncodedInst {
    encode_rre(0xB983, dst_pair.hw_id(), src.hw_id())
}

// =============================================================================
// 6. Comparisons, Fused Compare-and-Branch (`CGRJ` / `CGIJ`) & Control Flow
// =============================================================================

/// `cgr r1, r2` (64-bit signed compare, sets `PSW.CC`, `B9 20 00 r1r2`, 4 bytes)
#[inline] pub const fn cgr(lhs: Gpr, rhs: Gpr) -> EncodedInst  { encode_rre(0xB920, lhs.hw_id(), rhs.hw_id()) }
/// `clgr r1, r2` (64-bit unsigned compare, sets `PSW.CC`, `B9 21 00 r1r2`, 4 bytes)
#[inline] pub const fn clgr(lhs: Gpr, rhs: Gpr) -> EncodedInst { encode_rre(0xB921, lhs.hw_id(), rhs.hw_id()) }
/// `cghi r1, imm16` (64-bit signed compare with 16-bit immediate, `A7 r1F i2`, 4 bytes)
#[inline] pub const fn cghi(lhs: Gpr, imm16: i16) -> EncodedInst { encode_ri(0xA7, lhs.hw_id(), 0xF, imm16) }

/// `cgrj r1, r2, m3, rel16_halfwords` (64-bit signed compare & branch, `EC r1r2 rel16_be m30 64`, 6 bytes).
///
/// Compares `r1` and `r2` and branches to `PSW + 2 * rel16_halfwords` if `m3` matches,
/// **without modifying `PSW.CC`!**
#[inline]
pub const fn cgrj(lhs: Gpr, rhs: Gpr, mask: CondMask, rel16_halfwords: i16) -> EncodedInst {
    let [rel_hi, rel_lo] = rel16_halfwords.to_be_bytes();
    EncodedInst::from_6(
        0xEC,
        (lhs.hw_id() << 4) | rhs.hw_id(),
        rel_hi,
        rel_lo,
        mask.hw_nibble() << 4,
        0x64,
    )
}

/// `cgij r1, imm8, m3, rel16_halfwords` (64-bit signed compare immediate8 & branch, `EC r1m3 rel16_be imm8 7C`, 6 bytes).
///
/// Compares `r1` with sign-extended `imm8` and branches if `m3` matches, **without modifying `PSW.CC`!**
#[inline]
pub const fn cgij(lhs: Gpr, imm8: i8, mask: CondMask, rel16_halfwords: i16) -> EncodedInst {
    let [rel_hi, rel_lo] = rel16_halfwords.to_be_bytes();
    EncodedInst::from_6(
        0xEC,
        (lhs.hw_id() << 4) | mask.hw_nibble(),
        rel_hi,
        rel_lo,
        imm8 as u8,
        0x7C,
    )
}

/// `bcr m1, r2` (branch on condition to address in `r2`, `07 m1r2`, 2 bytes)
#[inline]
pub const fn bcr(mask: CondMask, target_reg: Gpr) -> EncodedInst {
    encode_rr(0x07, mask.hw_nibble(), target_reg.hw_id())
}

/// `br %r14` (standard SystemZ function return: `bcr 15, %r14` = `07 FE`, 2 bytes)
#[inline]
pub const fn ret_br_r14() -> EncodedInst {
    bcr(CondMask::ALWAYS, Gpr::R14)
}

/// `brc m1, rel16_halfwords` (PC-relative branch on condition, `A7 m14 rel16_be`, 4 bytes)
#[inline]
pub const fn brc(mask: CondMask, rel16_halfwords: i16) -> EncodedInst {
    encode_ri(0xA7, mask.hw_nibble(), 0x4, rel16_halfwords)
}

/// `brcl m1, rel32_halfwords` (long PC-relative branch on condition, `C0 m14 rel32_be`, 6 bytes)
#[inline]
pub const fn brcl(mask: CondMask, rel32_halfwords: i32) -> EncodedInst {
    encode_ril(0xC0, mask.hw_nibble(), 0x4, rel32_halfwords)
}

/// `brasl r1, rel32_halfwords` (PC-relative function call saving return address in `r1`, `C0 r15 rel32_be`, 6 bytes)
#[inline]
pub const fn brasl(link_reg: Gpr, rel32_halfwords: i32) -> EncodedInst {
    encode_ril(0xC0, link_reg.hw_id(), 0x5, rel32_halfwords)
}

/// `svc imm8` (Supervisor Call / Linux `s390x` syscall, `0A imm8`, 2 bytes)
#[inline]
pub const fn svc(syscall_num: u8) -> EncodedInst {
    EncodedInst::from_2(0x0A, syscall_num)
}

// =============================================================================
// 7. Storage-to-Storage (`SS`) & Vector (`VRR`) Instructions
// =============================================================================

/// `mvc d1(len, b1), d2(b2)` (Move `1..=256` bytes from `d2(b2)` to `d1(b1)` in 1 instruction, `D2`, 6 bytes).
///
/// Note: Hardware encodes `len - 1` (`0..=255`) in byte 1.
#[inline]
pub const fn mvc(
    dst_base: Gpr,
    dst_disp: Disp12,
    src_base: Gpr,
    src_disp: Disp12,
    byte_len: u16,
) -> EncodedInst {
    let len_m1 = (byte_len.saturating_sub(1) & 0xFF) as u8;
    encode_ss(0xD2, len_m1, dst_base, dst_disp, src_base, src_disp)
}

/// `xc d1(len, b1), d2(b2)` (XOR `1..=256` bytes in memory; when `dst == src`, zeroes memory in hardware! `D7`, 6 bytes).
#[inline]
pub const fn xc(
    dst_base: Gpr,
    dst_disp: Disp12,
    src_base: Gpr,
    src_disp: Disp12,
    byte_len: u16,
) -> EncodedInst {
    let len_m1 = (byte_len.saturating_sub(1) & 0xFF) as u8;
    encode_ss(0xD7, len_m1, dst_base, dst_disp, src_base, src_disp)
}

/// `vx v1, v2, v3` (128-bit vector XOR across `%v0..%v31` using `RXB` extension bits, `E7..6D`, 6 bytes).
#[inline]
pub const fn vx(dst: VecReg, lhs: VecReg, rhs: VecReg) -> EncodedInst {
    let rxb = (dst.rxb_bit() << 3) | (lhs.rxb_bit() << 2) | (rhs.rxb_bit() << 1);
    EncodedInst::from_6(
        0xE7,
        (dst.low4() << 4) | lhs.low4(),
        rhs.low4() << 4,
        0x00,
        rxb,
        0x6D,
    )
}
