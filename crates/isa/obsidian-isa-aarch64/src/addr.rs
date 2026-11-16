//! AArch64 Inline Barrel Shifters (`Shift`), Register Extenders (`Extend`),
//! 13-Bit Rotating Bitmask Immediates (`LogicalImm`: `N:immr:imms`), and Memory Addressing (`MemAddr`).

use crate::regs::{GprOrSp, GprOrXzr};

// =============================================================================
// 1. Inline Barrel Shifter (`LSL`, `LSR`, `ASR`, `ROR`) & Register Extender (`UXTW`, `SXTW`, ...)
// =============================================================================

/// 2-bit hardware shift kind (`0b00 = LSL`, `0b01 = LSR`, `0b10 = ASR`, `0b11 = ROR`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ShiftKind {
    /// Logical Shift Left (`0b00`)
    Lsl = 0b00,
    /// Logical Shift Right (`0b01`)
    Lsr = 0b01,
    /// Arithmetic Shift Right (`0b10`)
    Asr = 0b10,
    /// Rotate Right (`0b11` — legal in logical ops `AND`/`ORR`/`EOR`, illegal in `ADD`/`SUB`!)
    Ror = 0b11,
}

/// Inline barrel-shifter modifier (`kind` + 6-bit shift amount `0..=63`) attached to the
/// second source register (`Rm`) of AArch64 data-processing instructions.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Shift {
    pub kind: ShiftKind,
    pub amount: u8,
}

impl Shift {
    pub const NONE: Self = Self {
        kind: ShiftKind::Lsl,
        amount: 0,
    };

    #[inline]
    pub const fn lsl(amount: u8) -> Self {
        Self {
            kind: ShiftKind::Lsl,
            amount: amount & 0x3F,
        }
    }

    #[inline]
    pub const fn lsr(amount: u8) -> Self {
        Self {
            kind: ShiftKind::Lsr,
            amount: amount & 0x3F,
        }
    }

    #[inline]
    pub const fn asr(amount: u8) -> Self {
        Self {
            kind: ShiftKind::Asr,
            amount: amount & 0x3F,
        }
    }

    #[inline]
    pub const fn ror(amount: u8) -> Self {
        Self {
            kind: ShiftKind::Ror,
            amount: amount & 0x3F,
        }
    }

    /// Hardware rule: `ADD`/`ADDS`/`SUB`/`SUBS` (shifted register) support `LSL`, `LSR`, and `ASR`,
    /// but encoding `0b11` (`ROR`) is `RESERVED` / illegal in arithmetic shifted-register ops!
    #[inline]
    pub const fn is_valid_for_add_sub(self, is_64bit: bool) -> bool {
        let max_amt = if is_64bit { 63 } else { 31 };
        !matches!(self.kind, ShiftKind::Ror) && self.amount <= max_amt
    }
}

/// 3-bit hardware register extension kind (`UXTB`..`SXTX`) for Extended-Register `ADD`/`SUB` and loads/stores.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ExtendKind {
    Uxtb = 0b000,
    Uxth = 0b001,
    Uxtw = 0b010,
    Uxtx = 0b011,
    Sxtb = 0b100,
    Sxth = 0b101,
    Sxtw = 0b110,
    Sxtx = 0b111,
}

/// Register extension + optional left shift (`0..=4`) for Extended-Register instructions.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Extend {
    pub kind: ExtendKind,
    /// Left shift applied after sign/zero extension (`0..=4` only in hardware!).
    pub left_shift: u8,
}

impl Extend {
    #[inline]
    pub const fn is_legal_in_silicon(self) -> bool {
        self.left_shift <= 4
    }
}

// =============================================================================
// 2. 13-Bit Rotating Bitmask Immediate (`LogicalImm` — `N:1 | immr:6 | imms:6`)
// =============================================================================

/// Hardware 13-bit AArch64 Logical Bitmask Immediate (`N:1 | immr:6 | imms:6`)
/// used by `AND`, `ORR`, `EOR`, and `ANDS` (`TST`).
///
/// ### Silicon Rule
/// An AArch64 32-bit instruction word cannot hold an arbitrary 64-bit constant.
/// Instead, AArch64 encodes any **repeating power-of-2 bit pattern** (`2, 4, 8, 16, 32, or 64` bits wide)
/// consisting of a single contiguous run of `1` bits rotated right by `immr`:
/// - All-zeros (`0`) and all-ones (`!0`) are **illegal** as `LogicalImm`.
/// - `0x5555_5555_5555_5555` (alternating 2-bit pattern `0b01`) is **legal**!
/// - `0x0000_0000_FFFF_FF00` (contiguous run of 16 ones) is **legal**!
/// - `0x1234_5678` (non-contiguous bits in a 64-bit period) is **illegal** and must be
///   materialized with `MOVZ` / `MOVK` in SSA!
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LogicalImm {
    /// 1-bit `N` field (bit 22 of the instruction word; `1` if element size is 64 bits, `0` otherwise).
    pub n: u8,
    /// 6-bit `immr` field (bits `16..=21`: right-rotation count `0..=63`).
    pub immr: u8,
    /// 6-bit `imms` field (bits `10..=15`: encodes both element size and `(ones_count - 1)`).
    pub imms: u8,
}

impl LogicalImm {
    /// Packs `(N, immr, imms)` into the 13-bit hardware bitfield `(N << 12) | (immr << 6) | imms`
    /// placed at bits `10..=22` of `AND`/`ORR`/`EOR`/`ANDS` (immediate).
    #[inline]
    pub const fn hw_bits13(self) -> u32 {
        (((self.n & 1) as u32) << 12)
            | (((self.immr & 0x3F) as u32) << 6)
            | ((self.imms & 0x3F) as u32)
    }

    /// Attempts to encode a 64-bit constant `val` into a 13-bit AArch64 `LogicalImm`.
    /// Returns `None` if `val` cannot be represented by AArch64's bitmask immediate hardware.
    pub const fn try_encode64(val: u64) -> Option<Self> {
        // All-0s and all-1s cannot be encoded in `N:immr:imms`:
        if val == 0 || val == u64::MAX {
            return None;
        }

        // Find the smallest repeating element size `size` in {2, 4, 8, 16, 32, 64}:
        let mut size: u32 = 64;
        while size > 2 {
            let half = size / 2;
            let mask = (1u64 << half) - 1;
            if (val & mask) != ((val >> half) & mask) {
                break;
            }
            size = half;
        }

        // Extract one repeating element of width `size`:
        let elem_mask = if size == 64 { u64::MAX } else { (1u64 << size) - 1 };
        let elem = val & elem_mask;

        // Check if `elem` (within `size` bits) is a single circularly contiguous run of 1s.
        // A circular bit-pattern in `size` bits has a single run of 1s iff `elem ^ ROR(elem, 1)`
        // has exactly 2 bit-transitions!
        let ror1 = ((elem >> 1) | ((elem & 1) << (size - 1))) & elem_mask;
        let transitions = (elem ^ ror1).count_ones();
        if transitions != 2 {
            return None;
        }

        let ones = elem.count_ones();
        // Count how many right-rotations of `(1 << ones) - 1` (within `size` bits) produce `elem`:
        let unrotated = (1u64 << ones) - 1;
        let mut rot: u32 = 0;
        let mut cur = unrotated;
        while rot < size {
            if cur == elem {
                break;
            }
            // Rotate right by 1 within `size`-bit element:
            cur = ((cur >> 1) | ((cur & 1) << (size - 1))) & elem_mask;
            rot += 1;
        }

        let n = if size == 64 { 1u8 } else { 0u8 };
        let immr = (rot & (size - 1)) as u8;
        // `imms` encodes `(!(size * 2 - 1) | (ones - 1)) & 0x3F`:
        let nimms = !((size * 2) - 1) & 0x3F;
        let imms = (nimms | (ones - 1)) as u8;

        Some(Self { n, immr, imms })
    }

    /// Attempts to encode a 32-bit constant `val` into a 13-bit AArch64 `LogicalImm` (`N = 0`).
    #[inline]
    pub const fn try_encode32(val: u32) -> Option<Self> {
        if val == 0 || val == u32::MAX {
            return None;
        }
        // Duplicate the 32-bit pattern into both halves of a 64-bit word so element size <= 32 (`N = 0`):
        let dup64 = (val as u64) | ((val as u64) << 32);
        Self::try_encode64(dup64)
    }
}

// =============================================================================
// 3. Memory Addressing Modes (`MemAddr` & `PairOffset` for `LDP` / `STP`)
// =============================================================================

/// Hardware AArch64 memory addressing modes for single-register loads and stores (`LDR`, `STR`, `LDUR`, `STUR`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MemAddr {
    /// `[base, #uimm12_scaled]` — Unsigned 12-bit scaled offset (`0..=4095 * access_size`, e.g., `0..=32760` for 64-bit).
    UnsignedScaled {
        base: GprOrSp,
        uimm12: u16,
    },
    /// `[base, #simm9]` — Unscaled 9-bit signed byte offset (`-256..=255`, `LDUR` / `STUR`).
    UnscaledSimm9 {
        base: GprOrSp,
        simm9: i16,
    },
    /// `[base, #simm9]!` — Pre-indexed 9-bit signed byte offset with base writeback (`-256..=255`).
    PreIndex {
        base: GprOrSp,
        simm9: i16,
    },
    /// `[base], #simm9` — Post-indexed 9-bit signed byte offset with base writeback (`-256..=255`).
    PostIndex {
        base: GprOrSp,
        simm9: i16,
    },
    /// `[base, index, extend #shift]` — Register offset (`index` is `GprOrXzr`, `extend` is `UXTW`/`LSL`/`SXTW`/`SXTX`).
    RegOffset {
        base: GprOrSp,
        index: GprOrXzr,
        extend: ExtendKind,
        scale_by_access_size: bool,
    },
}

impl MemAddr {
    /// `[base]` (`uimm12 = 0`).
    #[inline]
    pub const fn base(base: GprOrSp) -> Self {
        Self::UnsignedScaled { base, uimm12: 0 }
    }

    /// Attempts to construct a scaled 12-bit unsigned offset `[base, #byte_offset]` for an access
    /// of `access_bytes` (`1`, `2`, `4`, `8`, or `16`).
    ///
    /// Returns `None` if `byte_offset` is negative, not a multiple of `access_bytes`, or `> 4095 * access_bytes`.
    #[inline]
    pub const fn try_unsigned_scaled(base: GprOrSp, byte_offset: i32, access_bytes: u8) -> Option<Self> {
        if byte_offset < 0 || access_bytes == 0 {
            return None;
        }
        let scale = access_bytes as i32;
        if (byte_offset % scale) != 0 {
            return None;
        }
        let uimm12 = byte_offset / scale;
        if uimm12 <= 4095 {
            Some(Self::UnsignedScaled {
                base,
                uimm12: uimm12 as u16,
            })
        } else {
            None
        }
    }

    /// Attempts to construct an unscaled 9-bit signed offset `[base, #simm9]` (`-256..=255`) for `LDUR`/`STUR`.
    #[inline]
    pub const fn try_unscaled_simm9(base: GprOrSp, byte_offset: i32) -> Option<Self> {
        if byte_offset >= -256 && byte_offset <= 255 {
            Some(Self::UnscaledSimm9 {
                base,
                simm9: byte_offset as i16,
            })
        } else {
            None
        }
    }
}

/// Signed 7-bit scaled offset (`simm7` = `-64..=63`) used by AArch64 paired load/store (`LDP` / `STP`):
/// - For 32-bit `W` registers (`scale = 4`): byte range `-256..=252` (multiples of 4).
/// - For 64-bit `X` registers (`scale = 8`): byte range `-512..=504` (multiples of 8).
/// - For 128-bit `Q` vector registers (`scale = 16`): byte range `-1024..=1008` (multiples of 16).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct PairOffset7(i8);

impl PairOffset7 {
    pub const ZERO: Self = Self(0);

    /// Validates and encodes a byte offset for a 64-bit `LDP`/`STP` (`-512..=504`, multiple of 8).
    #[inline]
    pub const fn try_new_64(byte_offset: i16) -> Option<Self> {
        if (byte_offset % 8) != 0 {
            return None;
        }
        let scaled = byte_offset / 8;
        if scaled >= -64 && scaled <= 63 {
            Some(Self(scaled as i8))
        } else {
            None
        }
    }

    /// Validates and encodes a byte offset for a 32-bit `LDP`/`STP` (`-256..=252`, multiple of 4).
    #[inline]
    pub const fn try_new_32(byte_offset: i16) -> Option<Self> {
        if (byte_offset % 4) != 0 {
            return None;
        }
        let scaled = byte_offset / 4;
        if scaled >= -64 && scaled <= 63 {
            Some(Self(scaled as i8))
        } else {
            None
        }
    }

    /// Returns the 7-bit two's-complement hardware encoding (`0..=0x7F`) placed at bits `15..=21` of `LDP`/`STP`.
    #[inline]
    pub const fn hw_bits7(self) -> u32 {
        (self.0 as u8 & 0x7F) as u32
    }

    /// Returns the signed byte offset for a 64-bit `LDP`/`STP` (`simm7 * 8`, `-512..=504`).
    #[inline]
    pub const fn byte_offset_64(self) -> i16 {
        (self.0 as i16) * 8
    }
}
