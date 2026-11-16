//! Static micro-architectural and silicon legality queries for AArch64 (`Apple M1–M4`, `Cortex-X`, `Neoverse`).
//!
//! This module does not perform optimization passes; it provides the hardware rules
//! consumed by `obsidian-ssa-aarch64` and `obsidian-regalloc`.

use crate::addr::PairOffset7;
use crate::cond::CondCode;
use crate::opcode::Opcode;
use crate::regs::{Gpr, GprOrSp};

/// Validates whether two adjacent 64-bit loads (`LDR rt1, [base, #off1]` and `LDR rt2, [base, #off2]`)
/// can be legally fused into a single 128-bit `LDP rt1, rt2, [base, #off1]` instruction in AArch64 silicon.
///
/// ### Critical Silicon Rules for `LDP`
/// 1. `off2` must equal `off1 + 8`, and `off1` must fit in a 7-bit scaled `PairOffset7` (`-512..=504`, multiple of 8).
/// 2. **`rt1 != rt2` Requirement:** In AArch64 silicon, encoding `ldp x0, x0, [base]` with identical
///    destination registers (`rt1 == rt2`) is **CONSTRAINED UNPREDICTABLE** (raises `SIGILL` on many cores)!
/// 3. If `has_writeback` (`pre-index` or `post-index`) is used, neither `rt1` nor `rt2` may equal `base`!
#[inline]
pub const fn can_fuse_into_ldp64(
    rt1: Gpr,
    rt2: Gpr,
    base: GprOrSp,
    byte_off1: i16,
    byte_off2: i16,
    has_writeback: bool,
) -> Option<PairOffset7> {
    // Rule 1: `LDP` with identical destination registers (`rt1 == rt2`) is illegal in silicon!
    if rt1 as u8 == rt2 as u8 {
        return None;
    }
    // Rule 2: Writeback `LDP` where `rt1 == base` or `rt2 == base` is illegal in silicon!
    if has_writeback {
        if let GprOrSp::Gpr(b) = base {
            if (b as u8 == rt1 as u8) || (b as u8 == rt2 as u8) {
                return None;
            }
        }
    }
    // Rule 3: Offsets must be contiguous (`off2 == off1 + 8`) and fit in `simm7 * 8` (`-512..=504`):
    if byte_off2 != byte_off1.saturating_add(8) {
        return None;
    }
    PairOffset7::try_new_64(byte_off1)
}

/// Validates whether two adjacent 64-bit stores (`STR rt1, [base, #off1]` and `STR rt2, [base, #off2]`)
/// can be legally fused into a single 128-bit `STP rt1, rt2, [base, #off1]` instruction.
///
/// Note: Unlike `LDP`, `STP x0, x0, [base]` with identical source registers (`rt1 == rt2`) IS
/// 100% legal in AArch64 silicon (commonly used to zero 16 bytes via `stp xzr, xzr, [sp]`)!
#[inline]
pub const fn can_fuse_into_stp64(byte_off1: i16, byte_off2: i16) -> Option<PairOffset7> {
    if byte_off2 != byte_off1.saturating_add(8) {
        return None;
    }
    PairOffset7::try_new_64(byte_off1)
}

/// Returns `true` if `alu_op` followed immediately by `B.cond(cc)` macro-fuses into
/// a single internal operation in the frontend of modern AArch64 cores.
#[inline]
pub const fn can_macro_fuse_bcond(alu_op: Opcode, _cc: CondCode) -> bool {
    alu_op.desc().macro_fuses_with_bcond
}

/// Returns `true` if an inline barrel-shift amount (`LSL #amt`) on `ADD`/`SUB` executes
/// on the fast 1-cycle latency path (`amt <= 4`, matching array element scaling `*1, *2, *4, *8, *16`).
///
/// On many Cortex / Neoverse microarchitectures, `ADD` with `LSL #0..4` executes in **1 cycle**,
/// whereas `LSL #5..63` takes an extra cycle in the barrel shifter.
#[inline]
pub const fn is_single_cycle_arith_shift(shift_amount: u8) -> bool {
    shift_amount <= 4
}
