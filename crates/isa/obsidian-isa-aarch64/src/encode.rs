//! Stateless 32-bit Little-Endian instruction encoders for AArch64 (`A64`).
//!
//! Every AArch64 instruction is a fixed-width 32-bit word (`u32`) stored in
//! Little-Endian byte order in instruction memory.

use crate::addr::{LogicalImm, PairOffset7, Shift};
use crate::cond::CondCode;
use crate::regs::{Gpr, GprOrSp, GprOrXzr};

/// A 32-bit encoded AArch64 (`A64`) instruction word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncodedInst(pub u32);

impl EncodedInst {
    /// Returns the raw 32-bit instruction word.
    #[inline(always)]
    pub const fn word(self) -> u32 {
        self.0
    }

    /// Returns the 4-byte Little-Endian representation stored in instruction memory.
    #[inline(always)]
    pub const fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    /// Every `A64` instruction is strictly 4 bytes long.
    #[inline(always)]
    pub const fn len(self) -> usize {
        4
    }

    /// Always `false` (every `A64` instruction is 4 bytes).
    #[inline(always)]
    pub const fn is_empty(self) -> bool {
        false
    }
}

// ============================================================================
// 1. Move Wide Immediate & PC-Relative Addressing
// ============================================================================

/// Encodes `MOVZ Xd, #imm16, LSL #(hw * 16)` (Move 16-bit immediate to 64-bit register, zeroing other bits).
/// `hw` must be `0..=3` (corresponding to `LSL #0`, `#16`, `#32`, `#48`).
#[inline]
pub const fn movz64(rd: GprOrXzr, imm16: u16, hw: u8) -> EncodedInst {
    let w = 0xD280_0000
        | (((hw & 0x3) as u32) << 21)
        | ((imm16 as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `MOVN Xd, #imm16, LSL #(hw * 16)` (Move inverted 16-bit immediate to 64-bit register).
#[inline]
pub const fn movn64(rd: GprOrXzr, imm16: u16, hw: u8) -> EncodedInst {
    let w = 0x9280_0000
        | (((hw & 0x3) as u32) << 21)
        | ((imm16 as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `MOVK Xd, #imm16, LSL #(hw * 16)` (Move 16-bit immediate to 64-bit register, keeping other bits).
#[inline]
pub const fn movk64(rd: GprOrXzr, imm16: u16, hw: u8) -> EncodedInst {
    let w = 0xF280_0000
        | (((hw & 0x3) as u32) << 21)
        | ((imm16 as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ADR Xd, #imm21` (Form PC-relative byte address within `±1 MiB`).
#[inline]
pub const fn adr(rd: GprOrXzr, imm21_bytes: i32) -> EncodedInst {
    let u = (imm21_bytes as u32) & 0x1F_FFFF;
    let immlo = u & 0x3;
    let immhi = (u >> 2) & 0x7_FFFF;
    EncodedInst((immlo << 29) | 0x1000_0000 | (immhi << 5) | (rd.hw_id5() as u32))
}

/// Encodes `ADRP Xd, #imm21_pages` (Form PC-relative 4 KiB page address within `±4 GiB`).
#[inline]
pub const fn adrp(rd: GprOrXzr, imm21_pages: i32) -> EncodedInst {
    let u = (imm21_pages as u32) & 0x1F_FFFF;
    let immlo = u & 0x3;
    let immhi = (u >> 2) & 0x7_FFFF;
    EncodedInst(0x8000_0000 | (immlo << 29) | 0x1000_0000 | (immhi << 5) | (rd.hw_id5() as u32))
}

// ============================================================================
// 2. Add/Subtract (Immediate, 64-bit) — Register 31 is SP (or XZR for S-variant Rd)
// ============================================================================

/// Encodes `ADD Xd|SP, Xn|SP, #imm12 {, LSL #12}` (64-bit Add Immediate, no flags).
#[inline]
pub const fn add64_imm(rd: GprOrSp, rn: GprOrSp, imm12: u16, shift12: bool) -> EncodedInst {
    let sh = (shift12 as u32) << 22;
    let w = 0x9100_0000
        | sh
        | (((imm12 & 0xFFF) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ADDS Xd|XZR, Xn|SP, #imm12 {, LSL #12}` (64-bit Add Immediate, setting `NZCV`).
#[inline]
pub const fn adds64_imm(rd: GprOrXzr, rn: GprOrSp, imm12: u16, shift12: bool) -> EncodedInst {
    let sh = (shift12 as u32) << 22;
    let w = 0xB100_0000
        | sh
        | (((imm12 & 0xFFF) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `SUB Xd|SP, Xn|SP, #imm12 {, LSL #12}` (64-bit Subtract Immediate, no flags).
#[inline]
pub const fn sub64_imm(rd: GprOrSp, rn: GprOrSp, imm12: u16, shift12: bool) -> EncodedInst {
    let sh = (shift12 as u32) << 22;
    let w = 0xD100_0000
        | sh
        | (((imm12 & 0xFFF) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `SUBS Xd|XZR, Xn|SP, #imm12 {, LSL #12}` (64-bit Subtract Immediate, setting `NZCV`).
#[inline]
pub const fn subs64_imm(rd: GprOrXzr, rn: GprOrSp, imm12: u16, shift12: bool) -> EncodedInst {
    let sh = (shift12 as u32) << 22;
    let w = 0xF100_0000
        | sh
        | (((imm12 & 0xFFF) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `CMP Xn|SP, #imm12 {, LSL #12}` (Hardware alias for `SUBS XZR, Xn|SP, #imm12`).
#[inline]
pub const fn cmp64_imm(rn: GprOrSp, imm12: u16, shift12: bool) -> EncodedInst {
    subs64_imm(GprOrXzr::Xzr, rn, imm12, shift12)
}

// ============================================================================
// 3. Add/Subtract & Logical (Shifted Register, 64-bit) — Register 31 is XZR
// ============================================================================

/// Encodes `ADD Xd, Xn, Xm {, shift #amount}` (64-bit Add Shifted Register, no flags).
#[inline]
pub const fn add64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0x8B00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ADDS Xd, Xn, Xm {, shift #amount}` (64-bit Add Shifted Register, setting `NZCV`).
#[inline]
pub const fn adds64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0xAB00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `SUB Xd, Xn, Xm {, shift #amount}` (64-bit Subtract Shifted Register, no flags).
#[inline]
pub const fn sub64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0xCB00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `SUBS Xd, Xn, Xm {, shift #amount}` (64-bit Subtract Shifted Register, setting `NZCV`).
#[inline]
pub const fn subs64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0xEB00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `CMP Xn, Xm {, shift #amount}` (Hardware alias for `SUBS XZR, Xn, Xm {, shift}`).
#[inline]
pub const fn cmp64_shifted(rn: GprOrXzr, rm: GprOrXzr, shift: Shift) -> EncodedInst {
    subs64_shifted(GprOrXzr::Xzr, rn, rm, shift)
}

/// Encodes `AND Xd, Xn, Xm {, shift #amount}` (64-bit Bitwise AND Shifted Register).
#[inline]
pub const fn and64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0x8A00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ORR Xd, Xn, Xm {, shift #amount}` (64-bit Bitwise OR Shifted Register).
#[inline]
pub const fn orr64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0xAA00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `EOR Xd, Xn, Xm {, shift #amount}` (64-bit Bitwise XOR Shifted Register).
#[inline]
pub const fn eor64_shifted(
    rd: GprOrXzr,
    rn: GprOrXzr,
    rm: GprOrXzr,
    shift: Shift,
) -> EncodedInst {
    let w = 0xCA00_0000
        | ((shift.kind as u32) << 22)
        | ((rm.hw_id5() as u32) << 16)
        | (((shift.amount & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `MOV Xd, Xm` for general-purpose registers (Hardware alias for `ORR Xd, XZR, Xm`).
#[inline]
pub const fn mov64_rr(rd: Gpr, rm: Gpr) -> EncodedInst {
    orr64_shifted(GprOrXzr::Gpr(rd), GprOrXzr::Xzr, GprOrXzr::Gpr(rm), Shift::NONE)
}

// ============================================================================
// 4. Logical Immediate (64-bit, 13-bit N:immr:imms Bitmask)
// ============================================================================

/// Encodes `AND Xd|SP, Xn, #bitmask` using a pre-validated 13-bit [`LogicalImm`].
#[inline]
pub const fn and64_imm(rd: GprOrSp, rn: GprOrXzr, imm: LogicalImm) -> EncodedInst {
    let w = 0x9200_0000
        | (imm.hw_bits13() << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ORR Xd|SP, Xn, #bitmask` using a pre-validated 13-bit [`LogicalImm`].
#[inline]
pub const fn orr64_imm(rd: GprOrSp, rn: GprOrXzr, imm: LogicalImm) -> EncodedInst {
    let w = 0xB200_0000
        | (imm.hw_bits13() << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `EOR Xd|SP, Xn, #bitmask` using a pre-validated 13-bit [`LogicalImm`].
#[inline]
pub const fn eor64_imm(rd: GprOrSp, rn: GprOrXzr, imm: LogicalImm) -> EncodedInst {
    let w = 0xD200_0000
        | (imm.hw_bits13() << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `ANDS Xd|XZR, Xn, #bitmask` (sets `NZCV`; `Rd = XZR` is `TST Xn, #bitmask`).
#[inline]
pub const fn ands64_imm(rd: GprOrXzr, rn: GprOrXzr, imm: LogicalImm) -> EncodedInst {
    let w = 0xF200_0000
        | (imm.hw_bits13() << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

// ============================================================================
// 5. Multiply / Divide, Bitfield, Bit Manipulation & Conditional Select
// ============================================================================

/// Encodes `MADD Xd, Xn, Xm, Xa` (`Xd = Xa + Xn * Xm`).
#[inline]
pub const fn madd64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr, ra: GprOrXzr) -> EncodedInst {
    let w = 0x9B00_0000
        | ((rm.hw_id5() as u32) << 16)
        | ((ra.hw_id5() as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `MSUB Xd, Xn, Xm, Xa` (`Xd = Xa - Xn * Xm`).
#[inline]
pub const fn msub64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr, ra: GprOrXzr) -> EncodedInst {
    let w = 0x9B00_8000
        | ((rm.hw_id5() as u32) << 16)
        | ((ra.hw_id5() as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `MUL Xd, Xn, Xm` (Hardware alias for `MADD Xd, Xn, Xm, XZR`).
#[inline]
pub const fn mul64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr) -> EncodedInst {
    madd64(rd, rn, rm, GprOrXzr::Xzr)
}

/// Encodes `UDIV Xd, Xn, Xm` (64-bit Unsigned Divide; division by zero returns 0 in hardware).
#[inline]
pub const fn udiv64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr) -> EncodedInst {
    let w = 0x9AC0_0800
        | ((rm.hw_id5() as u32) << 16)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `SDIV Xd, Xn, Xm` (64-bit Signed Divide; division by zero returns 0 in hardware).
#[inline]
pub const fn sdiv64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr) -> EncodedInst {
    let w = 0x9AC0_0C00
        | ((rm.hw_id5() as u32) << 16)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `UBFM Xd, Xn, #immr, #imms` (64-bit Unsigned Bitfield Move; backing opcode for `LSL`/`LSR`/`UBFX`).
#[inline]
pub const fn ubfm64(rd: GprOrXzr, rn: GprOrXzr, immr: u8, imms: u8) -> EncodedInst {
    let w = 0xD340_0000
        | (((immr & 0x3F) as u32) << 16)
        | (((imms & 0x3F) as u32) << 10)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `LSL Xd, Xn, #shift` (Hardware alias for `UBFM Xd, Xn, #(-shift MOD 64), #(63 - shift)`).
#[inline]
pub const fn lsl64_imm(rd: GprOrXzr, rn: GprOrXzr, shift: u8) -> EncodedInst {
    let s = shift & 0x3F;
    ubfm64(rd, rn, (64 - s) & 0x3F, 63 - s)
}

/// Encodes `LSR Xd, Xn, #shift` (Hardware alias for `UBFM Xd, Xn, #shift, #63`).
#[inline]
pub const fn lsr64_imm(rd: GprOrXzr, rn: GprOrXzr, shift: u8) -> EncodedInst {
    ubfm64(rd, rn, shift & 0x3F, 63)
}

/// Encodes `RBIT Xd, Xn` (Reverse all 64 bits).
#[inline]
pub const fn rbit64(rd: GprOrXzr, rn: GprOrXzr) -> EncodedInst {
    EncodedInst(0xDAC0_0000 | ((rn.hw_id5() as u32) << 5) | (rd.hw_id5() as u32))
}

/// Encodes `REV Xd, Xn` (Reverse bytes in 64-bit register — `bswap64`).
#[inline]
pub const fn rev64(rd: GprOrXzr, rn: GprOrXzr) -> EncodedInst {
    EncodedInst(0xDAC0_0C00 | ((rn.hw_id5() as u32) << 5) | (rd.hw_id5() as u32))
}

/// Encodes `CLZ Xd, Xn` (Count Leading Zeros in 64-bit register).
#[inline]
pub const fn clz64(rd: GprOrXzr, rn: GprOrXzr) -> EncodedInst {
    EncodedInst(0xDAC0_1000 | ((rn.hw_id5() as u32) << 5) | (rd.hw_id5() as u32))
}

/// Encodes `CSEL Xd, Xn, Xm, cond` (`Xd = cond ? Xn : Xm`).
#[inline]
pub const fn csel64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr, cond: CondCode) -> EncodedInst {
    let w = 0x9A80_0000
        | ((rm.hw_id5() as u32) << 16)
        | ((cond.hw_nibble() as u32) << 12)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `CSINC Xd, Xn, Xm, cond` (`Xd = cond ? Xn : (Xm + 1)`).
#[inline]
pub const fn csinc64(rd: GprOrXzr, rn: GprOrXzr, rm: GprOrXzr, cond: CondCode) -> EncodedInst {
    let w = 0x9A80_0400
        | ((rm.hw_id5() as u32) << 16)
        | ((cond.hw_nibble() as u32) << 12)
        | ((rn.hw_id5() as u32) << 5)
        | (rd.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `CSET Xd, cond` (`Xd = cond ? 1 : 0`, hardware alias for `CSINC Xd, XZR, XZR, invert(cond)`).
#[inline]
pub const fn cset64(rd: GprOrXzr, cond: CondCode) -> EncodedInst {
    csinc64(rd, GprOrXzr::Xzr, GprOrXzr::Xzr, cond.invert())
}

// ============================================================================
// 6. Loads and Stores (Single & Paired 64-bit)
// ============================================================================

/// Encodes `LDR Xt, [Xn|SP, #pimm]` where `imm12_scaled` is the unsigned offset divided by 8 (`0..=4095`).
#[inline]
pub const fn ldr64_uimm(rt: GprOrXzr, base: GprOrSp, imm12_scaled: u16) -> EncodedInst {
    let w = 0xF940_0000
        | (((imm12_scaled & 0xFFF) as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `STR Xt, [Xn|SP, #pimm]` where `imm12_scaled` is the unsigned offset divided by 8 (`0..=4095`).
#[inline]
pub const fn str64_uimm(rt: GprOrXzr, base: GprOrSp, imm12_scaled: u16) -> EncodedInst {
    let w = 0xF900_0000
        | (((imm12_scaled & 0xFFF) as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `LDUR Xt, [Xn|SP, #simm9]` (64-bit Load Register with unscaled signed 9-bit byte offset `-256..=255`).
#[inline]
pub const fn ldur64(rt: GprOrXzr, base: GprOrSp, simm9: i16) -> EncodedInst {
    let w = 0xF840_0000
        | (((simm9 as u32) & 0x1FF) << 12)
        | ((base.hw_id5() as u32) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `STUR Xt, [Xn|SP, #simm9]` (64-bit Store Register with unscaled signed 9-bit byte offset `-256..=255`).
#[inline]
pub const fn stur64(rt: GprOrXzr, base: GprOrSp, simm9: i16) -> EncodedInst {
    let w = 0xF800_0000
        | (((simm9 as u32) & 0x1FF) << 12)
        | ((base.hw_id5() as u32) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `LDP Xt1, Xt2, [Xn|SP, #imm]` (Signed offset, no writeback).
#[inline]
pub const fn ldp64_offset(
    rt1: GprOrXzr,
    rt2: GprOrXzr,
    base: GprOrSp,
    offset: PairOffset7,
) -> EncodedInst {
    let w = 0xA940_0000
        | (offset.hw_bits7() << 15)
        | ((rt2.hw_id5() as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt1.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `STP Xt1, Xt2, [Xn|SP, #imm]` (Signed offset, no writeback).
#[inline]
pub const fn stp64_offset(
    rt1: GprOrXzr,
    rt2: GprOrXzr,
    base: GprOrSp,
    offset: PairOffset7,
) -> EncodedInst {
    let w = 0xA900_0000
        | (offset.hw_bits7() << 15)
        | ((rt2.hw_id5() as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt1.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `STP Xt1, Xt2, [Xn|SP, #imm]!` (Pre-indexed writeback — classic function prologue push).
#[inline]
pub const fn stp64_pre(
    rt1: GprOrXzr,
    rt2: GprOrXzr,
    base: GprOrSp,
    offset: PairOffset7,
) -> EncodedInst {
    let w = 0xA980_0000
        | (offset.hw_bits7() << 15)
        | ((rt2.hw_id5() as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt1.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `LDP Xt1, Xt2, [Xn|SP], #imm` (Post-indexed writeback — classic function epilogue pop).
#[inline]
pub const fn ldp64_post(
    rt1: GprOrXzr,
    rt2: GprOrXzr,
    base: GprOrSp,
    offset: PairOffset7,
) -> EncodedInst {
    let w = 0xA8C0_0000
        | (offset.hw_bits7() << 15)
        | ((rt2.hw_id5() as u32) << 10)
        | ((base.hw_id5() as u32) << 5)
        | (rt1.hw_id5() as u32);
    EncodedInst(w)
}

// ============================================================================
// 7. Control Flow & System Instructions
// ============================================================================

/// Encodes `B label` where `imm26_words` is the signed PC-relative offset in 4-byte instructions (`±128 MiB`).
#[inline]
pub const fn b_rel26(imm26_words: i32) -> EncodedInst {
    EncodedInst(0x1400_0000 | ((imm26_words as u32) & 0x03FF_FFFF))
}

/// Encodes `BL label` where `imm26_words` is the signed PC-relative offset in 4-byte instructions (`±128 MiB`).
#[inline]
pub const fn bl_rel26(imm26_words: i32) -> EncodedInst {
    EncodedInst(0x9400_0000 | ((imm26_words as u32) & 0x03FF_FFFF))
}

/// Encodes `B.cond label` where `imm19_words` is the signed PC-relative offset in 4-byte instructions (`±1 MiB`).
#[inline]
pub const fn b_cond_rel19(cond: CondCode, imm19_words: i32) -> EncodedInst {
    let w = 0x5400_0000
        | (((imm19_words as u32) & 0x7_FFFF) << 5)
        | (cond.hw_nibble() as u32);
    EncodedInst(w)
}

/// Encodes `CBZ Xt, label` (64-bit Compare and Branch if Zero, `±1 MiB`).
#[inline]
pub const fn cbz64_rel19(rt: GprOrXzr, imm19_words: i32) -> EncodedInst {
    let w = 0xB400_0000
        | (((imm19_words as u32) & 0x7_FFFF) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `CBNZ Xt, label` (64-bit Compare and Branch if Non-Zero, `±1 MiB`).
#[inline]
pub const fn cbnz64_rel19(rt: GprOrXzr, imm19_words: i32) -> EncodedInst {
    let w = 0xB500_0000
        | (((imm19_words as u32) & 0x7_FFFF) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `TBZ Xt, #bit, label` (Test bit `0..=63` and Branch if Zero, `±32 KiB`).
#[inline]
pub const fn tbz_rel14(rt: GprOrXzr, bit: u8, imm14_words: i16) -> EncodedInst {
    let b5 = ((bit as u32) >> 5) & 1;
    let b40 = (bit as u32) & 0x1F;
    let w = (b5 << 31)
        | 0x3600_0000
        | (b40 << 19)
        | (((imm14_words as u32) & 0x3FFF) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `TBNZ Xt, #bit, label` (Test bit `0..=63` and Branch if Non-Zero, `±32 KiB`).
#[inline]
pub const fn tbnz_rel14(rt: GprOrXzr, bit: u8, imm14_words: i16) -> EncodedInst {
    let b5 = ((bit as u32) >> 5) & 1;
    let b40 = (bit as u32) & 0x1F;
    let w = (b5 << 31)
        | 0x3700_0000
        | (b40 << 19)
        | (((imm14_words as u32) & 0x3FFF) << 5)
        | (rt.hw_id5() as u32);
    EncodedInst(w)
}

/// Encodes `BR Xn` (Indirect Branch to Register).
#[inline]
pub const fn br(rn: Gpr) -> EncodedInst {
    EncodedInst(0xD61F_0000 | ((rn.hw_id5() as u32) << 5))
}

/// Encodes `BLR Xn` (Indirect Branch with Link to Register).
#[inline]
pub const fn blr(rn: Gpr) -> EncodedInst {
    EncodedInst(0xD63F_0000 | ((rn.hw_id5() as u32) << 5))
}

/// Encodes `RET Xn` (Subroutine Return via Register `Xn`).
#[inline]
pub const fn ret(rn: Gpr) -> EncodedInst {
    EncodedInst(0xD65F_0000 | ((rn.hw_id5() as u32) << 5))
}

/// Encodes `RET` (Subroutine Return via default Link Register `X30`/`LR` -> `0xD65F03C0`).
#[inline]
pub const fn ret_lr() -> EncodedInst {
    ret(Gpr::X30)
}

/// Encodes `SVC #imm16` (Supervisor Call / OS syscall).
#[inline]
pub const fn svc(imm16: u16) -> EncodedInst {
    EncodedInst(0xD400_0001 | ((imm16 as u32) << 5))
}

/// Encodes `BRK #imm16` (Software Breakpoint trap).
#[inline]
pub const fn brk(imm16: u16) -> EncodedInst {
    EncodedInst(0xD420_0000 | ((imm16 as u32) << 5))
}

/// Encodes `NOP` (`0xD503201F`).
#[inline]
pub const fn nop() -> EncodedInst {
    EncodedInst(0xD503_201F)
}
