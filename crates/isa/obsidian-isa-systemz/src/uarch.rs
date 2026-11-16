//! Static micro-architectural rules for IBM SystemZ (`z15` / `z16` / `Telum`) silicon.
//!
//! This module does not perform optimization passes; it provides the hardware rules
//! consumed by `obsidian-ssa-systemz` and `obsidian-regalloc`.

use crate::addr::MemBx;
use crate::opcode::Opcode;
use crate::regs::Gpr;

/// Maximum number of bytes (`256`) that a single Storage-to-Storage (`SS`) instruction
/// (`MVC`, `XC`, `CLC`) can process in hardware (`L` field = `len - 1`, `0..=255`).
pub const SS_MAX_BYTES_PER_INST: usize = 256;

/// Decodes the exact byte length (`2`, `4`, or `6` bytes) of any SystemZ instruction
/// from the top 2 bits of its very first byte (`byte0 >> 6`), exactly as the CPU
/// instruction fetcher does in silicon:
/// - `0b00`: 2 bytes (`1` halfword)
/// - `0b01` or `0b10`: 4 bytes (`2` halfwords)
/// - `0b11`: 6 bytes (`3` halfwords)
#[inline]
pub const fn inst_byte_len_from_first_byte(first_byte: u8) -> u8 {
    match first_byte >> 6 {
        0b00 => 2,
        0b01 | 0b10 => 4,
        _ => 6,
    }
}

/// Returns `true` if `producer_op` writing `reg` followed closely by a memory instruction
/// using `reg` as a `base` or `index` in `mem` triggers an **Address Generation Interlock (AGI)**
/// pipeline stall on SystemZ cores.
///
/// ### Silicon Rule
/// On z/Architecture cores, address generation (`AGEN`) happens early in the pipeline before
/// the integer ALU execution stage. If an ALU instruction (`AGR`, `SGR`, `SLLG`) modifies a
/// register that is immediately dereferenced as a base or index pointer (`LG`, `STG`), the
/// frontend stalls for ~3–4 cycles waiting for the ALU result.
///
/// **Exception:** Using `LA` (`Load Address` 12-bit disp) or `LAY` (20-bit disp) executes
/// inside the `AGEN` unit itself and **bypasses the AGI stall**!
#[inline]
pub const fn causes_agi_hazard(producer_op: Opcode, written_reg: Gpr, mem: &MemBx<Gpr>) -> bool {
    if !written_reg.can_be_base_or_index() {
        return false;
    }
    // `LA` and `LAY` execute in the AGEN unit and can be forwarded without an AGI stall:
    if matches!(producer_op, Opcode::La | Opcode::Lay) {
        return false;
    }
    let used_as_base = match mem.base {
        Some(b) => b as u8 == written_reg as u8,
        None => false,
    };
    let used_as_index = match mem.index {
        Some(x) => x as u8 == written_reg as u8,
        None => false,
    };
    used_as_base || used_as_index
}

/// Returns `true` if `op` is a single-instruction fused Compare-and-Branch (`CGRJ`, `CRJ`,
/// `CLGRJ`, `CGIJ`, `CIJ`) that compares operands and branches without modifying `PSW.CC`.
#[inline]
pub const fn is_cc_preserving_compare_branch(op: Opcode) -> bool {
    matches!(
        op,
        Opcode::Cgrj | Opcode::Crj | Opcode::Clgrj | Opcode::Cgij | Opcode::Cij
    )
}

/// Returns `true` if `xc d(len, b), d(b)` (XOR of a memory region with itself) is recognized
/// by SystemZ silicon as a fast hardware zeroing block store (`memset(ptr, 0, len)` for `1..=256` bytes)
/// without reading the old cache line values first.
#[inline]
pub const fn is_fast_xc_self_zero(
    dst_base: Gpr,
    dst_disp12: u16,
    src_base: Gpr,
    src_disp12: u16,
    byte_len: usize,
) -> bool {
    dst_base.can_be_base_or_index()
        && (dst_base as u8 == src_base as u8)
        && (dst_disp12 == src_disp12)
        && byte_len >= 1
        && byte_len <= SS_MAX_BYTES_PER_INST
}
