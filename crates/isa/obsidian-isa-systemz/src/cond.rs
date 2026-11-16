//! SystemZ Program Status Word (`PSW`) 2-bit Condition Code (`CC`) and 4-bit Branch Mask (`CondMask`).
//!
//! ### How SystemZ Condition Codes Differ From x86
//! Instead of 6 independent status flags (`CF`, `ZF`, `SF`, `OF`, `PF`, `AF`), SystemZ silicon has
//! a **single 2-bit Condition Code (`CC`)** in the `PSW` that holds one of 4 values (`0`, `1`, `2`, `3`):
//! - `CC = 0`: Equal / Zero (`==`)
//! - `CC = 1`: Low / Less (`<`)
//! - `CC = 2`: High / Greater (`>`)
//! - `CC = 3`: Overflow (signed ALU) or Unordered / NaN (floating-point compare)
//!
//! Conditional branches (`BRC`, `BRCL`, `CGRJ`), conditional moves (`LOCGR`), and traps specify a
//! **4-bit Condition Mask (`M` = `0..=15`)** where each bit selects one of the 4 `CC` states:
//! - Bit `0b1000` (`8`): matches `CC == 0`
//! - Bit `0b0100` (`4`): matches `CC == 1`
//! - Bit `0b0010` (`2`): matches `CC == 2`
//! - Bit `0b0001` (`1`): matches `CC == 3`

/// The 4 mutually exclusive states (`0..=3`) of the SystemZ 2-bit `PSW` Condition Code (`CC`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CcValue {
    /// `CC == 0`: Equal (`a == b`) or Zero result (`res == 0`). Matches mask bit `8` (`0b1000`).
    Cc0 = 0,
    /// `CC == 1`: First operand Low (`a < b`) or Negative result (`res < 0`). Matches mask bit `4` (`0b0100`).
    Cc1 = 1,
    /// `CC == 2`: First operand High (`a > b`) or Positive result (`res > 0`). Matches mask bit `2` (`0b0010`).
    Cc2 = 2,
    /// `CC == 3`: Overflow (`ALU`) or Unordered/NaN (`FP`) or All-Ones (`TM`). Matches mask bit `1` (`0b0001`).
    Cc3 = 3,
}

impl CcValue {
    /// Converts this 2-bit `CC` state (`0..=3`) into its single-bit `CondMask` (`8`, `4`, `2`, or `1`).
    #[inline]
    pub const fn to_mask(self) -> CondMask {
        CondMask(1u8 << (3 - (self as u8)))
    }
}

/// 4-bit hardware Condition Mask (`0x0..=0xF`) encoded in the `M1`/`M3` nibble of
/// `BRC`, `BRCL`, `BCR`, `LOCGR`, `CGRJ`, `CRJ`, `CGIJ`, and `CIJ`.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct CondMask(pub u8);

impl CondMask {
    /// `0b0000` (`0`): Never branch (`NOP`).
    pub const NEVER: Self = Self(0b0000);
    /// `0b0001` (`1`): Branch on Overflow / Unordered (`CC == 3`, `jo` / `brc 1`).
    pub const O: Self = Self(0b0001);
    /// `0b0010` (`2`): Branch on High / Greater (`CC == 2`, `jh` / `jg` / `brc 2`).
    pub const H: Self = Self(0b0010);
    /// `0b0100` (`4`): Branch on Low / Less (`CC == 1`, `jl` / `brc 4`).
    pub const L: Self = Self(0b0100);
    /// `0b0110` (`6`): Branch on Not Equal for integer compare (`CC == 1 || CC == 2`, `jne` in `CRJ`/`CGRJ`).
    pub const NE_CMP: Self = Self(0b0110);
    /// `0b0111` (`7`): Branch on Not Zero / Not Equal (`CC != 0`, `jnz` / `jne` / `brc 7`).
    pub const NE: Self = Self(0b0111);
    /// `0b1000` (`8`): Branch on Equal / Zero (`CC == 0`, `je` / `jz` / `brc 8`).
    pub const E: Self = Self(0b1000);
    /// `0b1010` (`10`): Branch on High or Equal (`CC == 0 || CC == 2`, `jhe` / `jge` / `brc 10`).
    pub const HE: Self = Self(0b1010);
    /// `0b1100` (`12`): Branch on Low or Equal (`CC == 0 || CC == 1`, `jle` / `brc 12`).
    pub const LE: Self = Self(0b1100);
    /// `0b1110` (`14`): Branch on No Overflow / Ordered (`CC != 3`, `jno` / `brc 14`).
    pub const NO: Self = Self(0b1110);
    /// `0b1111` (`15`): Unconditional branch (`CC` ignored, `j` / `br` / `brc 15`).
    pub const ALWAYS: Self = Self(0b1111);

    /// Construct a `CondMask` from a 4-bit hardware nibble (`0..=15`).
    #[inline]
    pub const fn from_hw_nibble(nibble: u8) -> Option<Self> {
        if nibble < 16 {
            Some(Self(nibble))
        } else {
            None
        }
    }

    /// Returns the 4-bit hardware nibble (`0..=15`).
    #[inline]
    pub const fn hw_nibble(self) -> u8 {
        self.0 & 0x0F
    }

    /// Returns `true` if this mask matches the given `CcValue`.
    #[inline]
    pub const fn matches(self, cc: CcValue) -> bool {
        (self.0 & cc.to_mask().0) != 0
    }

    /// Inverts all 4 condition code bits (`mask ^ 0b1111`).
    ///
    /// For example, `CondMask::E (8) <-> CondMask::NE (7)`, `CondMask::O (1) <-> CondMask::NO (14)`.
    #[inline]
    pub const fn invert_all(self) -> Self {
        Self((self.0 ^ 0b1111) & 0x0F)
    }

    /// Inverts the 3 comparison bits (`CC0`, `CC1`, `CC2`: `mask ^ 0b1110`), used by
    /// integer compare-and-branch instructions (`CRJ`, `CGRJ`, `CIJ`, `CGIJ`) where `CC3` cannot occur:
    /// - `E (8)` <-> `NE_CMP (6)`
    /// - `L (4)` <-> `HE (10)`
    /// - `H (2)` <-> `LE (12)`
    #[inline]
    pub const fn invert_cmp(self) -> Self {
        Self((self.0 ^ 0b1110) & 0b1110)
    }

    /// Commutes the comparison operands (`a CMP b` -> `b CMP a`) by swapping
    /// `CC == 1` (`Low`, bit 2) and `CC == 2` (`High`, bit 1):
    /// - `L (4)` <-> `H (2)`
    /// - `LE (12)` <-> `HE (10)`
    /// - `E (8)` <-> `E (8)`
    #[inline]
    pub const fn commute_cmp(self) -> Self {
        let cc0 = self.0 & 0b1000;
        let cc1 = (self.0 & 0b0100) >> 1; // Low -> High
        let cc2 = (self.0 & 0b0010) << 1; // High -> Low
        let cc3 = self.0 & 0b0001;
        Self(cc0 | cc1 | cc2 | cc3)
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self((self.0 | other.0) & 0x0F)
    }
}
