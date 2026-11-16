//! Stateless 1-instruction x86-64 hardware byte encoders and exact byte-length calculators.
//!
//! Everything in this module is pure silicon bit-packing from the Intel SDM / AMD64 manuals:
//! - No heap allocations (`EncodedInst` is a 16-byte stack value: `[u8; 15]` + `len: u8`).
//! - No optimization heuristics, no jump label tables, and no object file logic.
//! - Every helper is `const fn` so instruction encodings and exact byte lengths can even be
//!   evaluated at compile time.

use crate::addr::MemoryOperand;
use crate::cond::CondCode;
use crate::regs::{Gpr, SimdReg};

/// A single encoded x86-64 machine instruction (1 to 15 bytes, stack-allocated, `#[repr(C)]`).
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EncodedInst {
    pub bytes: [u8; 15],
    pub len: u8,
}

impl EncodedInst {
    pub const EMPTY: Self = Self {
        bytes: [0; 15],
        len: 0,
    };

    /// Returns the active instruction bytes (`&self.bytes[..self.len]`).
    #[inline]
    pub const fn as_slice(&self) -> &[u8] {
        // Safety: `self.len` is always `<= 15`.
        unsafe { core::slice::from_raw_parts(self.bytes.as_ptr(), self.len as usize) }
    }

    #[inline]
    pub const fn push(&mut self, byte: u8) {
        self.bytes[self.len as usize] = byte;
        self.len += 1;
    }

    #[inline]
    pub const fn push_i8(&mut self, val: i8) {
        self.push(val as u8);
    }

    #[inline]
    pub const fn push_i32_le(&mut self, val: i32) {
        let b = val.to_le_bytes();
        self.push(b[0]);
        self.push(b[1]);
        self.push(b[2]);
        self.push(b[3]);
    }

    #[inline]
    pub const fn push_i64_le(&mut self, val: i64) {
        let b = val.to_le_bytes();
        let mut i = 0;
        while i < 8 {
            self.push(b[i]);
            i += 1;
        }
    }
}

// =============================================================================
// Internal Hardware Bit-Packing Helpers (REX, VEX, ModR/M, SIB)
// =============================================================================

#[inline]
const fn modrm(mod_bits: u8, reg_low3: u8, rm_low3: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg_low3 & 0b111) << 3) | (rm_low3 & 0b111)
}

#[inline]
const fn sib(scale_bits: u8, index_low3: u8, base_low3: u8) -> u8 {
    ((scale_bits & 0b11) << 6) | ((index_low3 & 0b111) << 3) | (base_low3 & 0b111)
}

/// Emits a `REX` prefix byte (`0x40 | W<<3 | R<<2 | X<<1 | B`) if any bit is set
/// or if `force_rex` is true (needed for `SPL`/`BPL`/`SIL`/`DIL`).
#[inline]
const fn emit_rex(
    out: &mut EncodedInst,
    w: bool,
    r: u8,
    x: u8,
    b: u8,
    force_rex: bool,
) {
    let bits = ((w as u8) << 3) | ((r & 1) << 2) | ((x & 1) << 1) | (b & 1);
    if bits != 0 || force_rex {
        out.push(0x40 | bits);
    }
}

/// Extracts `(rex_x, rex_b)` extension bits required by a `MemoryOperand<Gpr>`.
#[inline]
const fn mem_rex_xb(mem: &MemoryOperand<Gpr>) -> (u8, u8) {
    match *mem {
        MemoryOperand::RipRelative { .. } => (0, 0),
        MemoryOperand::BaseIndex { base, index, .. } => {
            let x = match index {
                Some((idx, _)) => idx.rex_bit(),
                None => 0,
            };
            let b = match base {
                Some(base_reg) => base_reg.rex_bit(),
                None => 0,
            };
            (x, b)
        }
    }
}

/// Appends `ModR/M` + optional `SIB` + displacement bytes for `reg_field` (`0..=7`)
/// and `mem` (`MemoryOperand<Gpr>`), following all x86-64 silicon rules.
#[inline]
const fn emit_modrm_mem(out: &mut EncodedInst, reg_low3: u8, mem: &MemoryOperand<Gpr>) {
    match *mem {
        MemoryOperand::RipRelative { disp32 } => {
            // [rip + disp32]: mod = 0b00, rm = 0b101
            out.push(modrm(0b00, reg_low3, 0b101));
            out.push_i32_le(disp32);
        }
        MemoryOperand::BaseIndex {
            base: None,
            index,
            disp,
        } => {
            // Baseless SIB ([index * scale + disp32] or [disp32]):
            // mod = 0b00, rm = 0b100 (SIB), SIB.base = 0b101 + disp32
            out.push(modrm(0b00, reg_low3, 0b100));
            match index {
                Some((idx, scale)) => out.push(sib(scale.sib_bits(), idx.low3(), 0b101)),
                None => out.push(sib(0b00, 0b100, 0b101)),
            }
            out.push_i32_le(disp.value());
        }
        MemoryOperand::BaseIndex {
            base: Some(base_reg),
            index,
            disp,
        } => {
            // Determine displacement mode (0-byte mod=00, 1-byte mod=01, or 4-byte mod=10)
            let disp_len = mem.disp_byte_len();
            let mod_bits = match disp_len {
                0 => 0b00,
                1 => 0b01,
                _ => 0b10,
            };

            if let Some((idx_reg, scale)) = index {
                // Indexed SIB addressing: rm = 0b100
                out.push(modrm(mod_bits, reg_low3, 0b100));
                out.push(sib(scale.sib_bits(), idx_reg.low3(), base_reg.low3()));
            } else if base_reg.base_requires_sib() {
                // RSP (4) or R12 (12) without index: silicon requires SIB with index = 0b100 (none)
                out.push(modrm(mod_bits, reg_low3, 0b100));
                out.push(sib(0b00, 0b100, 0b100)); // 0x24
            } else {
                // Direct ModR/M base register
                out.push(modrm(mod_bits, reg_low3, base_reg.low3()));
            }

            if disp_len == 1 {
                out.push_i8(disp.value() as i8);
            } else if disp_len == 4 {
                out.push_i32_le(disp.value());
            }
        }
    }
}

/// Emits a 2-byte (`C5`) or 3-byte (`C4`) VEX prefix.
///
/// - `map_select`: `1` = `0F`, `2` = `0F 38`, `3` = `0F 3A`
/// - `pp`: `0` = none, `1` = `66`, `2` = `F3`, `3` = `F2`
#[inline]
const fn emit_vex(
    out: &mut EncodedInst,
    rex_w: bool,
    rex_r: u8,
    rex_x: u8,
    rex_b: u8,
    vvvv_reg: u8,
    vec_256: bool,
    map_select: u8,
    pp: u8,
) {
    let r_inv = (rex_r ^ 1) & 1;
    let x_inv = (rex_x ^ 1) & 1;
    let b_inv = (rex_b ^ 1) & 1;
    let vvvv_inv = (!vvvv_reg) & 0x0F;
    let l_bit = if vec_256 { 1 } else { 0 };

    // Can use compact 2-byte VEX (`0xC5`) iff `rex_w == false`, `rex_x == 0`, `rex_b == 0`, and `map_select == 1` (`0F`)
    if !rex_w && rex_x == 0 && rex_b == 0 && map_select == 1 {
        out.push(0xC5);
        out.push((r_inv << 7) | (vvvv_inv << 3) | (l_bit << 2) | (pp & 0b11));
    } else {
        out.push(0xC4);
        out.push((r_inv << 7) | (x_inv << 6) | (b_inv << 5) | (map_select & 0b1_1111));
        out.push(((rex_w as u8) << 7) | (vvvv_inv << 3) | (l_bit << 2) | (pp & 0b11));
    }
}

// =============================================================================
// 1. Data Movement, Zero/Sign Extension & Stack Instructions
// =============================================================================

/// `mov dst64, src64` (`REX.W + 89 /r`, 3 bytes)
#[inline]
pub const fn mov64_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, src.rex_bit(), 0, dst.rex_bit(), false);
    out.push(0x89);
    out.push(modrm(0b11, src.low3(), dst.low3()));
    out
}

/// `mov dst32, src32` (`89 /r`, 2 bytes, or 3 bytes if `R8D..R15D` is used)
#[inline]
pub const fn mov32_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, src.rex_bit(), 0, dst.rex_bit(), false);
    out.push(0x89);
    out.push(modrm(0b11, src.low3(), dst.low3()));
    out
}

/// `mov dst64, [mem]` (`REX.W + 8B /r`)
#[inline]
pub const fn mov64_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, true, dst.rex_bit(), x, b, false);
    out.push(0x8B);
    emit_modrm_mem(&mut out, dst.low3(), &mem);
    out
}

/// `mov dst32, [mem]` (`8B /r`, zero-extends to 64 bits)
#[inline]
pub const fn mov32_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, false, dst.rex_bit(), x, b, false);
    out.push(0x8B);
    emit_modrm_mem(&mut out, dst.low3(), &mem);
    out
}

/// `mov [mem], src64` (`REX.W + 89 /r`)
#[inline]
pub const fn mov64_mr(mem: MemoryOperand<Gpr>, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, true, src.rex_bit(), x, b, false);
    out.push(0x89);
    emit_modrm_mem(&mut out, src.low3(), &mem);
    out
}

/// `mov [mem], src32` (`89 /r`)
#[inline]
pub const fn mov32_mr(mem: MemoryOperand<Gpr>, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, false, src.rex_bit(), x, b, false);
    out.push(0x89);
    emit_modrm_mem(&mut out, src.low3(), &mem);
    out
}

/// `mov dst32, imm32` (`B8+rd id`, 5 bytes for `EAX..EDI`, 6 bytes for `R8D..R15D`)
#[inline]
pub const fn mov32_ri(dst: Gpr, imm32: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, 0, 0, dst.rex_bit(), false);
    out.push(0xB8 + dst.low3());
    out.push_i32_le(imm32);
    out
}

/// `mov dst64, sign_ext_imm32` (`REX.W + C7 /0 id`, 7 bytes, 1 DSB slot)
#[inline]
pub const fn mov64_ri32(dst: Gpr, imm32: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, 0, 0, dst.rex_bit(), false);
    out.push(0xC7);
    out.push(modrm(0b11, 0, dst.low3()));
    out.push_i32_le(imm32);
    out
}

/// `movabs dst64, imm64` (`REX.W + B8+rd io`, 10 bytes, consumes 2 DSB slots)
#[inline]
pub const fn movabs64_ri64(dst: Gpr, imm64: i64) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, 0, 0, dst.rex_bit(), false);
    out.push(0xB8 + dst.low3());
    out.push_i64_le(imm64);
    out
}

/// `movzx dst32, src8_low` (`0F B6 /r`)
#[inline]
pub const fn movzx32_r8(dst: Gpr, src8: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(
        &mut out,
        false,
        dst.rex_bit(),
        0,
        src8.rex_bit(),
        src8.needs_rex_for_byte_low(),
    );
    out.push(0x0F);
    out.push(0xB6);
    out.push(modrm(0b11, dst.low3(), src8.low3()));
    out
}

/// `movsxd dst64, src32` (`REX.W + 63 /r`)
#[inline]
pub const fn movsxd64_r32(dst: Gpr, src32: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, dst.rex_bit(), 0, src32.rex_bit(), false);
    out.push(0x63);
    out.push(modrm(0b11, dst.low3(), src32.low3()));
    out
}

/// `cmovcc dst64, src64` (`REX.W + 0F 40+cc /r`)
#[inline]
pub const fn cmovcc64_rr(cc: CondCode, dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0x40 + cc.hw_nibble());
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `push src64` (`50+rd` — 1 byte for `RAX..RDI`, 2 bytes for `R8..R15`)
#[inline]
pub const fn push64_r(src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, 0, 0, src.rex_bit(), false);
    out.push(0x50 + src.low3());
    out
}

/// `pop dst64` (`58+rd` — 1 byte for `RAX..RDI`, 2 bytes for `R8..R15`)
#[inline]
pub const fn pop64_r(dst: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, 0, 0, dst.rex_bit(), false);
    out.push(0x58 + dst.low3());
    out
}

/// `bswap dst64` (`REX.W + 0F C8+rd`, 3 bytes)
#[inline]
pub const fn bswap64_r(dst: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, 0, 0, dst.rex_bit(), false);
    out.push(0x0F);
    out.push(0xC8 + dst.low3());
    out
}

// =============================================================================
// 2. Non-Destructive 3-Way Address Math (`LEA`)
// =============================================================================

/// `lea dst64, [mem]` (`REX.W + 8D /r`)
#[inline]
pub const fn lea64(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, true, dst.rex_bit(), x, b, false);
    out.push(0x8D);
    emit_modrm_mem(&mut out, dst.low3(), &mem);
    out
}

/// `lea dst32, [mem]` (`8D /r`, zero-extends to 64 bits)
#[inline]
pub const fn lea32(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, false, dst.rex_bit(), x, b, false);
    out.push(0x8D);
    emit_modrm_mem(&mut out, dst.low3(), &mem);
    out
}

// =============================================================================
// 3. Binary Integer ALU (`ADD`, `SUB`, `AND`, `OR`, `XOR`, `CMP`, `TEST`)
// =============================================================================

#[inline]
const fn alu_rr(rex_w: bool, opcode_byte: u8, dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, rex_w, src.rex_bit(), 0, dst.rex_bit(), false);
    out.push(opcode_byte);
    out.push(modrm(0b11, src.low3(), dst.low3()));
    out
}

#[inline]
const fn alu_rm(rex_w: bool, opcode_byte: u8, dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, rex_w, dst.rex_bit(), x, b, false);
    out.push(opcode_byte);
    emit_modrm_mem(&mut out, dst.low3(), &mem);
    out
}

#[inline]
const fn alu_ri(rex_w: bool, ext_digit: u8, dst: Gpr, imm: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, rex_w, 0, 0, dst.rex_bit(), false);
    if imm >= (i8::MIN as i32) && imm <= (i8::MAX as i32) {
        out.push(0x83);
        out.push(modrm(0b11, ext_digit, dst.low3()));
        out.push_i8(imm as i8);
    } else {
        out.push(0x81);
        out.push(modrm(0b11, ext_digit, dst.low3()));
        out.push_i32_le(imm);
    }
    out
}

#[inline] pub const fn add64_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(true,  0x01, dst, src) }
#[inline] pub const fn add32_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(false, 0x01, dst, src) }
#[inline] pub const fn add64_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(true,  0x03, dst, mem) }
#[inline] pub const fn add32_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(false, 0x03, dst, mem) }
#[inline] pub const fn add64_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(true,  0, dst, imm) }
#[inline] pub const fn add32_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(false, 0, dst, imm) }

#[inline] pub const fn sub64_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(true,  0x29, dst, src) }
#[inline] pub const fn sub32_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(false, 0x29, dst, src) }
#[inline] pub const fn sub64_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(true,  0x2B, dst, mem) }
#[inline] pub const fn sub32_rm(dst: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(false, 0x2B, dst, mem) }
#[inline] pub const fn sub64_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(true,  5, dst, imm) }
#[inline] pub const fn sub32_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(false, 5, dst, imm) }

#[inline] pub const fn and64_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(true,  0x21, dst, src) }
#[inline] pub const fn and32_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(false, 0x21, dst, src) }
#[inline] pub const fn and64_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(true,  4, dst, imm) }
#[inline] pub const fn and32_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(false, 4, dst, imm) }

#[inline] pub const fn or64_rr(dst: Gpr, src: Gpr) -> EncodedInst  { alu_rr(true,  0x09, dst, src) }
#[inline] pub const fn or32_rr(dst: Gpr, src: Gpr) -> EncodedInst  { alu_rr(false, 0x09, dst, src) }
#[inline] pub const fn or64_ri(dst: Gpr, imm: i32) -> EncodedInst  { alu_ri(true,  1, dst, imm) }
#[inline] pub const fn or32_ri(dst: Gpr, imm: i32) -> EncodedInst  { alu_ri(false, 1, dst, imm) }

#[inline] pub const fn xor64_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(true,  0x31, dst, src) }
#[inline] pub const fn xor32_rr(dst: Gpr, src: Gpr) -> EncodedInst { alu_rr(false, 0x31, dst, src) }
#[inline] pub const fn xor64_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(true,  6, dst, imm) }
#[inline] pub const fn xor32_ri(dst: Gpr, imm: i32) -> EncodedInst { alu_ri(false, 6, dst, imm) }

#[inline] pub const fn cmp64_rr(lhs: Gpr, rhs: Gpr) -> EncodedInst { alu_rr(true,  0x39, lhs, rhs) }
#[inline] pub const fn cmp32_rr(lhs: Gpr, rhs: Gpr) -> EncodedInst { alu_rr(false, 0x39, lhs, rhs) }
#[inline] pub const fn cmp64_rm(lhs: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(true,  0x3B, lhs, mem) }
#[inline] pub const fn cmp32_rm(lhs: Gpr, mem: MemoryOperand<Gpr>) -> EncodedInst { alu_rm(false, 0x3B, lhs, mem) }
#[inline] pub const fn cmp64_mr(mem: MemoryOperand<Gpr>, rhs: Gpr) -> EncodedInst { alu_rm(true,  0x39, rhs, mem) }
#[inline] pub const fn cmp32_mr(mem: MemoryOperand<Gpr>, rhs: Gpr) -> EncodedInst { alu_rm(false, 0x39, rhs, mem) }
#[inline] pub const fn cmp64_ri(lhs: Gpr, imm: i32) -> EncodedInst { alu_ri(true,  7, lhs, imm) }
#[inline] pub const fn cmp32_ri(lhs: Gpr, imm: i32) -> EncodedInst { alu_ri(false, 7, lhs, imm) }

/// `cmp byte ptr [mem], rhs8_low` (`38 /r`)
#[inline]
pub const fn cmp8_mr(mem: MemoryOperand<Gpr>, rhs8: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(
        &mut out,
        false,
        rhs8.rex_bit(),
        x,
        b,
        rhs8.needs_rex_for_byte_low(),
    );
    out.push(0x38);
    emit_modrm_mem(&mut out, rhs8.low3(), &mem);
    out
}

/// `cmp byte ptr [mem], imm8` (`80 /7 ib`)
#[inline]
pub const fn cmp8_mi(mem: MemoryOperand<Gpr>, imm8: i8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    let (x, b) = mem_rex_xb(&mem);
    emit_rex(&mut out, false, 0, x, b, false);
    out.push(0x80);
    emit_modrm_mem(&mut out, 7, &mem);
    out.push_i8(imm8);
    out
}

#[inline] pub const fn test64_rr(lhs: Gpr, rhs: Gpr) -> EncodedInst { alu_rr(true,  0x85, lhs, rhs) }
#[inline] pub const fn test32_rr(lhs: Gpr, rhs: Gpr) -> EncodedInst { alu_rr(false, 0x85, lhs, rhs) }

// =============================================================================
// 4. Unary ALU, Shifts & Bit Manipulation (`ABM`, `BMI1`, `BMI2`)
// =============================================================================

/// `imul dst64, src64` (`REX.W + 0F AF /r`)
#[inline]
pub const fn imul64_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0xAF);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `shl dst64, imm8` (`REX.W + C1 /4 ib` or `REX.W + D1 /4` when `imm8 == 1`)
#[inline]
pub const fn shl64_ri(dst: Gpr, imm8: u8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, 0, 0, dst.rex_bit(), false);
    if imm8 == 1 {
        out.push(0xD1);
        out.push(modrm(0b11, 4, dst.low3()));
    } else {
        out.push(0xC1);
        out.push(modrm(0b11, 4, dst.low3()));
        out.push(imm8);
    }
    out
}

/// `shr dst64, imm8` (`REX.W + C1 /5 ib` or `REX.W + D1 /5` when `imm8 == 1`)
#[inline]
pub const fn shr64_ri(dst: Gpr, imm8: u8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, true, 0, 0, dst.rex_bit(), false);
    if imm8 == 1 {
        out.push(0xD1);
        out.push(modrm(0b11, 5, dst.low3()));
    } else {
        out.push(0xC1);
        out.push(modrm(0b11, 5, dst.low3()));
        out.push(imm8);
    }
    out
}

/// `popcnt dst64, src64` (`F3 + REX.W + 0F B8 /r`)
#[inline]
pub const fn popcnt64_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xF3);
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0xB8);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `tzcnt dst64, src64` (`F3 + REX.W + 0F BC /r`)
#[inline]
pub const fn tzcnt64_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xF3);
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0xBC);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `lzcnt dst64, src64` (`F3 + REX.W + 0F BD /r`)
#[inline]
pub const fn lzcnt64_rr(dst: Gpr, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xF3);
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0xBD);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// BMI1 `andn dst64, src1, src2` (`VEX.LZ.0F38.W1 F2 /r`, `dst = !src1 & src2`)
#[inline]
pub const fn andn64_rrr(dst: Gpr, src1: Gpr, src2: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_vex(&mut out, true, dst.rex_bit(), 0, src2.rex_bit(), src1.hw_id(), false, 2, 0);
    out.push(0xF2);
    out.push(modrm(0b11, dst.low3(), src2.low3()));
    out
}

/// BMI2 `shlx dst64, src, cnt` (`VEX.LZ.66.0F38.W1 F7 /r`, flagless 3-operand shift left!)
#[inline]
pub const fn shlx64_rrr(dst: Gpr, src: Gpr, cnt: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_vex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), cnt.hw_id(), false, 2, 1);
    out.push(0xF7);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// BMI2 `pext dst64, src, mask` (`VEX.LZ.F3.0F38.W1 F5 /r`)
#[inline]
pub const fn pext64_rrr(dst: Gpr, src: Gpr, mask: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_vex(&mut out, true, dst.rex_bit(), 0, mask.rex_bit(), src.hw_id(), false, 2, 2);
    out.push(0xF5);
    out.push(modrm(0b11, dst.low3(), mask.low3()));
    out
}

// =============================================================================
// 5. Control Flow, Flag Materialization & System Instructions
// =============================================================================

/// `jmp rel8` (`EB cb`, 2 bytes)
#[inline]
pub const fn jmp_rel8(rel8: i8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xEB);
    out.push_i8(rel8);
    out
}

/// `jmp rel32` (`E9 cd`, 5 bytes)
#[inline]
pub const fn jmp_rel32(rel32: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xE9);
    out.push_i32_le(rel32);
    out
}

/// `jcc rel8` (`70+cc cb`, 2 bytes)
#[inline]
pub const fn jcc_rel8(cc: CondCode, rel8: i8) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0x70 + cc.hw_nibble());
    out.push_i8(rel8);
    out
}

/// `jcc rel32` (`0F 80+cc cd`, 6 bytes)
#[inline]
pub const fn jcc_rel32(cc: CondCode, rel32: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0x0F);
    out.push(0x80 + cc.hw_nibble());
    out.push_i32_le(rel32);
    out
}

/// `setcc dst8_low` (`0F 90+cc /0`, 3–4 bytes)
#[inline]
pub const fn setcc8_r(cc: CondCode, dst8: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, 0, 0, dst8.rex_bit(), dst8.needs_rex_for_byte_low());
    out.push(0x0F);
    out.push(0x90 + cc.hw_nibble());
    out.push(modrm(0b11, 0, dst8.low3()));
    out
}

/// `call rel32` (`E8 cd`, 5 bytes)
#[inline]
pub const fn call_rel32(rel32: i32) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xE8);
    out.push_i32_le(rel32);
    out
}

/// `call r64` (`FF /2`, 2–3 bytes)
#[inline]
pub const fn call64_r(target: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, 0, 0, target.rex_bit(), false);
    out.push(0xFF);
    out.push(modrm(0b11, 2, target.low3()));
    out
}

/// `ret` (`C3`, 1 byte)
#[inline]
pub const fn ret() -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xC3);
    out
}

/// `ud2` (`0F 0B`, 2 bytes)
#[inline]
pub const fn ud2() -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0x0F);
    out.push(0x0B);
    out
}

/// `syscall` (`0F 05`, 2 bytes)
#[inline]
pub const fn syscall() -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0x0F);
    out.push(0x05);
    out
}

// =============================================================================
// 6. Scalar & Vector SSE / AVX / FMA (`XMM` / `YMM`)
// =============================================================================

/// `xorps dst, src` (`0F 57 /r`, 3–4 bytes)
#[inline]
pub const fn xorps_rr(dst: SimdReg, src: SimdReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_rex(&mut out, false, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0x57);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `sqrtsd dst, src` (`F2 0F 51 /r`, 4–5 bytes)
#[inline]
pub const fn sqrtsd_rr(dst: SimdReg, src: SimdReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xF2);
    emit_rex(&mut out, false, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0x51);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// `cvtsi2sd dst_xmm, src_gpr64` (`F2 + REX.W + 0F 2A /r`, 5 bytes)
#[inline]
pub const fn cvtsi2sd64_rr(dst: SimdReg, src: Gpr) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    out.push(0xF2);
    emit_rex(&mut out, true, dst.rex_bit(), 0, src.rex_bit(), false);
    out.push(0x0F);
    out.push(0x2A);
    out.push(modrm(0b11, dst.low3(), src.low3()));
    out
}

/// AVX `vxorps dst, src1, src2` (`VEX.128.0F.WIG 57 /r`, uses compact 2-byte `C5` VEX when `src2 < XMM8`!)
#[inline]
pub const fn vxorps_xmm(dst: SimdReg, src1: SimdReg, src2: SimdReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_vex(&mut out, false, dst.rex_bit(), 0, src2.rex_bit(), src1.hw_id(), false, 1, 0);
    out.push(0x57);
    out.push(modrm(0b11, dst.low3(), src2.low3()));
    out
}

/// FMA3 `vfmadd213sd dst, src1, src2` (`VEX.LIG.66.0F38.W1 A9 /r`, `dst = (dst * src1) + src2`)
#[inline]
pub const fn vfmadd213sd_xmm(dst: SimdReg, src1: SimdReg, src2: SimdReg) -> EncodedInst {
    let mut out = EncodedInst::EMPTY;
    emit_vex(&mut out, true, dst.rex_bit(), 0, src2.rex_bit(), src1.hw_id(), false, 2, 1);
    out.push(0xA9);
    out.push(modrm(0b11, dst.low3(), src2.low3()));
    out
}
