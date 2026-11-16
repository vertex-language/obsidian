//! Static micro-architectural and silicon legality queries for Motorola 680x0 (`M68k`).
//!
//! This module does not perform optimization passes; it provides the hardware rules
//! consumed by `obsidian-ssa-m68k` and `obsidian-regalloc`.

use crate::addr::EffAddr;
use crate::opcode::Opcode;
use crate::regs::OpSize;

/// Returns `true` if `val` (`-128..=127`) can be materialized into a 32-bit `DataReg` (`D0..D7`)
/// using the single-word (2-byte) `MOVEQ #imm8, Dn` instruction instead of a 6-byte `MOVE.L #imm32, Dn`.
#[inline]
pub const fn fits_moveq_i8(val: i32) -> bool {
    val >= (i8::MIN as i32) && val <= (i8::MAX as i32)
}

/// Converts a constant `1..=8` into the 3-bit hardware quick-immediate field used by
/// `ADDQ`, `SUBQ`, and immediate shifts (`LSL`, `LSR`, `ASL`, `ASR`, `ROL`, `ROR`).
///
/// **Silicon Encoding Rule:** Values `1..=7` encode directly as `1..=7`, whereas the
/// value `8` encodes as `0b000` (`0`) in hardware! Returns `None` if `val` is outside `1..=8`.
#[inline]
pub const fn encode_quick_1_to_8(val: u8) -> Option<u8> {
    if val >= 1 && val <= 8 {
        Some(val & 0b0111) // 1..=7 -> 1..=7, 8 -> 0!
    } else {
        None
    }
}

/// Decodes the 3-bit hardware quick-immediate field (`0..=7`) back into its mathematical
/// constant (`1..=8`, where `0` decodes to `8`).
#[inline]
pub const fn decode_quick_1_to_8(bits3: u8) -> u8 {
    let b = bits3 & 0b0111;
    if b == 0 { 8 } else { b }
}

/// Returns `true` if executing `op` with destination `dst_ea` leaves the Condition Code
/// Register (`CCR`: `X, N, Z, V, C`) completely untouched in M68k silicon.
///
/// ### Silicon Rule
/// On M68k, `ADDQ` and `SUBQ` normally update all 5 `CCR` flags (`X, N, Z, V, C`), **EXCEPT**
/// when their destination `<ea>` is an Address Register (`An`, `EffAddr::AddrDirect`), in which
/// case silicon suppresses all flag updates (just like `ADDA`, `SUBA`, `MOVEA`, and `LEA`)!
#[inline]
pub const fn preserves_ccr_flags(op: Opcode, dst_ea: Option<&EffAddr>) -> bool {
    if matches!(
        op,
        Opcode::Movea | Opcode::Adda | Opcode::Suba | Opcode::Lea | Opcode::Pea
    ) {
        return true;
    }
    if matches!(op, Opcode::Addq | Opcode::Subq) {
        if let Some(EffAddr::AddrDirect(_)) = dst_ea {
            return true;
        }
    }
    op.desc().flags_written.is_empty()
}

/// Returns the exact total byte length (`2..=10` bytes, always a multiple of 2) of a single-EA
/// instruction with `base_words` (`1` or `2` 16-bit opcode words) and effective address `ea`.
#[inline]
pub const fn single_ea_inst_byte_len(base_words: u8, ea: &EffAddr, size: OpSize) -> u8 {
    (base_words * 2) + ea.extension_byte_len(size)
}
