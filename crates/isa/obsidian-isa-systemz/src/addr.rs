//! Hardware memory addressing modes for IBM SystemZ (`s390x`).
//!
//! SystemZ silicon provides four hardware memory addressing structures:
//! 1. **Short 12-Bit Displacement (`RX` / `RS` / `SS` formats):**
//!    Unsigned 12-bit offset `D12` (`0..=4095`) + Base `B` (`R1..R15`) + optional Index `X` (`R1..R15`).
//! 2. **Long 20-Bit Signed Displacement (`RXY` / `RSY` formats):**
//!    Signed 20-bit offset `D20` (`-524_288..=524_287`), encoded as `DL` (low 12 bits) and
//!    `DH` (high 8 bits) + Base `B` (`R1..R15`) + optional Index `X` (`R1..R15`).
//! 3. **Base + 12-Bit Displacement Without Index (`RS` / `SS` / `SIL` formats):**
//!    `D12(B)` used by storage-to-storage (`MVC`, `XC`, `CLC`) and multi-register (`LMG`/`STMG`) ops.
//! 4. **PC-Relative Long (`RIL` format):**
//!    Signed 32-bit halfword offset (`rel32_halfwords`, $\pm 4\text{ GiB}$ from `PSW`), used by `LARL`, `LGRL`, `BRASL`, `BRCL`.

use crate::regs::Gpr;

/// Unsigned 12-bit hardware displacement (`0..=4095`) used by 4-byte `RX` and 6-byte `SS` instructions.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Disp12(u16);

impl Disp12 {
    pub const ZERO: Self = Self(0);
    pub const MAX: u16 = 4095;

    /// Checks whether `offset` fits in an unsigned 12-bit SystemZ displacement (`0..=4095`).
    #[inline]
    pub const fn try_new(offset: i32) -> Option<Self> {
        if offset >= 0 && offset <= (Self::MAX as i32) {
            Some(Self(offset as u16))
        } else {
            None
        }
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0 & 0x0FFF
    }
}

/// Signed 20-bit hardware displacement (`-524_288..=524_287`) used by 6-byte `RXY` and `RSY` instructions.
///
/// In SystemZ silicon, a 20-bit signed displacement is split across two fields in bytes `2..=4`:
/// - `dl` (12 bits): low 12 bits `(disp & 0x0FFF)`
/// - `dh` (8 bits): high 8 bits `((disp >> 12) & 0xFF)`
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Disp20(i32);

impl Disp20 {
    pub const ZERO: Self = Self(0);
    pub const MIN: i32 = -(1 << 19); // -524,288
    pub const MAX: i32 = (1 << 19) - 1; // +524,287

    /// Checks whether `offset` fits in a signed 20-bit SystemZ displacement (`-524_288..=524_287`).
    #[inline]
    pub const fn try_new(offset: i32) -> Option<Self> {
        if offset >= Self::MIN && offset <= Self::MAX {
            Some(Self(offset))
        } else {
            None
        }
    }

    #[inline]
    pub const fn value(self) -> i32 {
        self.0
    }

    /// Returns the low 12 bits (`DL` field, `0..=0xFFF`) placed in bytes `2..=3` of `RXY`/`RSY`.
    #[inline]
    pub const fn dl_12(self) -> u16 {
        (self.0 as u32 & 0x0FFF) as u16
    }

    /// Returns the high 8 bits (`DH` field, `0..=0xFF`) placed in byte `4` of `RXY`/`RSY`.
    #[inline]
    pub const fn dh_8(self) -> u8 {
        ((self.0 as u32 >> 12) & 0xFF) as u8
    }

    /// Returns `true` if this 20-bit displacement also fits in an unsigned 12-bit `Disp12` (`0..=4095`),
    /// allowing a 6-byte `RXY` instruction (e.g., `LY`, `AY`) to use a shorter 4-byte `RX` form (`L`, `A`).
    #[inline]
    pub const fn fits_disp12(self) -> bool {
        self.0 >= 0 && self.0 <= (Disp12::MAX as i32)
    }
}

/// Hardware Base + Index + Displacement memory operand `D(X, B)` parameterized by register type `Reg`.
///
/// Note: SystemZ does NOT have an inline scale factor (`*2`, `*4`, `*8`) in `RX`/`RXY` memory operands;
/// address calculation is strictly `base + index + disp`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MemBx<Reg = Gpr> {
    pub base: Option<Reg>,
    pub index: Option<Reg>,
    pub disp: Disp20,
}

impl<Reg: Copy> MemBx<Reg> {
    /// `0(base)` — base register with zero displacement and no index.
    #[inline]
    pub const fn base(base: Reg) -> Self {
        Self {
            base: Some(base),
            index: None,
            disp: Disp20::ZERO,
        }
    }

    /// `disp(base)` — base register with 20-bit signed displacement.
    #[inline]
    pub const fn base_disp(base: Reg, disp: Disp20) -> Self {
        Self {
            base: Some(base),
            index: None,
            disp,
        }
    }

    /// `disp(index, base)` — base + index + 20-bit signed displacement.
    #[inline]
    pub const fn base_index_disp(base: Reg, index: Reg, disp: Disp20) -> Self {
        Self {
            base: Some(base),
            index: Some(index),
            disp,
        }
    }
}

impl MemBx<Gpr> {
    /// Validates whether this physical register combination is legal in SystemZ silicon.
    ///
    /// **Silicon Rule:** `Gpr::R0` (`0`) CANNOT be used as `Some(Gpr::R0)` in `base` or `index`
    /// because the hardware decodes register nibble `0` as the constant `0` (no register),
    /// ignoring the value inside `%r0`! Use `None` to represent an omitted base or index.
    #[inline]
    pub const fn is_legal_in_silicon(&self) -> bool {
        let base_ok = match self.base {
            Some(b) => b.can_be_base_or_index(),
            None => true,
        };
        let index_ok = match self.index {
            Some(x) => x.can_be_base_or_index(),
            None => true,
        };
        base_ok && index_ok
    }

    /// Returns the 4-bit `B` nibble (`0` if `base` is `None`, or `1..=15`).
    #[inline]
    pub const fn base_nibble(&self) -> u8 {
        match self.base {
            Some(b) => b.hw_id(),
            None => 0,
        }
    }

    /// Returns the 4-bit `X` nibble (`0` if `index` is `None`, or `1..=15`).
    #[inline]
    pub const fn index_nibble(&self) -> u8 {
        match self.index {
            Some(x) => x.hw_id(),
            None => 0,
        }
    }

    /// Converts to a 12-bit unsigned displacement if `0 <= disp <= 4095`.
    #[inline]
    pub const fn as_disp12(&self) -> Option<Disp12> {
        Disp12::try_new(self.disp.value())
    }
}
