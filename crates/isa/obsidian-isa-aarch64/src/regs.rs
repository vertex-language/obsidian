//! Physical AArch64 (`ARM64`) registers and the hardware encoding `31` (`SP` vs. `XZR`) split.
//!
//! Every discriminant matches the exact 5-bit (`0..=31`) hardware register number
//! decoded by AArch64 silicon as defined in the *Arm Architecture Reference Manual
//! for A-profile architecture (DDI 0487)*.
//!
//! ### Critical Silicon Rule: 5-Bit Encoding `31` (`0b11111`)
//! In AArch64 instruction words, the 5-bit register number `31` (`0b11111`) does NOT
//! refer to a single register—its meaning depends on the instruction field:
//! 1. **Data / ALU / Branch / Load-Target fields (`Rd`, `Rm`, `Ra`, `Rt`, `Rt2`):**
//!    Encoding `31` is the **Zero Register (`XZR` / `WZR`)** (reads as `0`, discards writes).
//! 2. **Memory Base Address & Immediate Add/Sub (`Rn` of loads/stores, `Rd`/`Rn` of `ADD`/`SUB` imm):**
//!    Encoding `31` is the **Stack Pointer (`SP` / `WSP`)**!
//!
//! To make `SP` vs. `XZR` confusion a compile-time error, this module provides:
//! - [`Gpr`]: The 31 general-purpose registers (`X0`..`X30`, IDs `0..=30`) OR explicit
//!   [`Reg31::Xzr`] / [`Reg31::Sp`] via [`GprOrXzr`] and [`GprOrSp`].

/// The 31 numbered 64-bit General Purpose Registers (`X0`..`X30` / 32-bit `W0`..`W30`, IDs `0..=30`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Gpr {
    X0  = 0,  X1  = 1,  X2  = 2,  X3  = 3,  X4  = 4,  X5  = 5,  X6  = 6,  X7  = 7,
    X8  = 8,  X9  = 9,  X10 = 10, X11 = 11, X12 = 12, X13 = 13, X14 = 14, X15 = 15,
    /// Intra-Procedure-Call scratch register 0 (`IP0` / `X16`).
    X16 = 16,
    /// Intra-Procedure-Call scratch register 1 (`IP1` / `X17`).
    X17 = 17,
    /// Platform register (`PR` / `X18` — reserved on Apple Darwin and Windows ARM64!).
    X18 = 18,
    X19 = 19, X20 = 20, X21 = 21, X22 = 22, X23 = 23, X24 = 24, X25 = 25, X26 = 26,
    X27 = 27, X28 = 28,
    /// Frame Pointer (`FP` / `X29`).
    X29 = 29,
    /// Procedure Link Register (`LR` / `X30`, written by `BL` / `BLR`, read by `RET`).
    X30 = 30,
}

impl Gpr {
    pub const COUNT: usize = 31;
    pub const IP0: Self = Self::X16;
    pub const IP1: Self = Self::X17;
    pub const FP: Self  = Self::X29;
    pub const LR: Self  = Self::X30;

    pub const ALL: [Gpr; 31] = [
        Self::X0,  Self::X1,  Self::X2,  Self::X3,  Self::X4,  Self::X5,  Self::X6,  Self::X7,
        Self::X8,  Self::X9,  Self::X10, Self::X11, Self::X12, Self::X13, Self::X14, Self::X15,
        Self::X16, Self::X17, Self::X18, Self::X19, Self::X20, Self::X21, Self::X22, Self::X23,
        Self::X24, Self::X25, Self::X26, Self::X27, Self::X28, Self::X29, Self::X30,
    ];

    /// Construct a `Gpr` (`X0..X30`) from a 5-bit hardware index (`0..=30`).
    /// Returns `None` if `id >= 31` (since `31` is either `SP` or `XZR` depending on context).
    #[inline]
    pub const fn from_hw_id(id: u8) -> Option<Self> {
        if id < 31 {
            Some(Self::ALL[id as usize])
        } else {
            None
        }
    }

    /// Returns the 5-bit hardware register index (`0..=30`).
    #[inline]
    pub const fn hw_id5(self) -> u8 {
        self as u8
    }

    /// Converts to a [`GprOrXzr`] (`X0..X30` in data/zero-register contexts).
    #[inline]
    pub const fn or_xzr(self) -> GprOrXzr {
        GprOrXzr::Gpr(self)
    }

    /// Converts to a [`GprOrSp`] (`X0..X30` in base-pointer/stack-pointer contexts).
    #[inline]
    pub const fn or_sp(self) -> GprOrSp {
        GprOrSp::Gpr(self)
    }
}

/// 5-bit register field (`0..=31`) in **Zero-Register (`XZR`/`WZR`) context**
/// (used by shifted-register ALU ops, multiplies, branches, and load/store data targets `Rt`).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprOrXzr {
    /// General purpose register `X0..X30` (`hw_id = 0..=30`).
    Gpr(Gpr),
    /// Architectural Zero Register `XZR` / `WZR` (`hw_id = 31`).
    Xzr,
}

impl GprOrXzr {
    /// Returns the exact 5-bit hardware encoding (`0..=30` for `X0..X30`, `31` for `XZR`).
    #[inline]
    pub const fn hw_id5(self) -> u8 {
        match self {
            Self::Gpr(r) => r.hw_id5(),
            Self::Xzr => 31,
        }
    }

    #[inline]
    pub const fn is_xzr(self) -> bool {
        matches!(self, Self::Xzr)
    }
}

/// 5-bit register field (`0..=31`) in **Stack-Pointer (`SP`/`WSP`) context**
/// (used by memory base registers `[Rn, ...]` and immediate/extended-register `ADD`/`SUB` without flags).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprOrSp {
    /// General purpose register `X0..X30` (`hw_id = 0..=30`).
    Gpr(Gpr),
    /// Architectural Stack Pointer `SP` / `WSP` (`hw_id = 31`).
    Sp,
}

impl GprOrSp {
    /// Returns the exact 5-bit hardware encoding (`0..=30` for `X0..X30`, `31` for `SP`).
    #[inline]
    pub const fn hw_id5(self) -> u8 {
        match self {
            Self::Gpr(r) => r.hw_id5(),
            Self::Sp => 31,
        }
    }

    #[inline]
    pub const fn is_sp(self) -> bool {
        matches!(self, Self::Sp)
    }
}

/// Integer operand width (`W32` = 32-bit `W0..W30`, `X64` = 64-bit `X0..X30`).
///
/// Hardware rule: every 32-bit (`W32`, `sf = 0`) operation in AArch64 implicitly
/// zeroes the upper 32 bits (`bits 32..64`) of the destination 64-bit register.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprWidth {
    /// 32-bit (`W` register view, `sf = 0` bit in bit 31 of the instruction word).
    W32 = 0,
    /// 64-bit (`X` register view, `sf = 1` bit in bit 31 of the instruction word).
    X64 = 1,
}

impl GprWidth {
    /// Returns the 1-bit `sf` (sixty-four) bit (`0` for 32-bit, `1` for 64-bit) placed at bit 31.
    #[inline]
    pub const fn sf_bit(self) -> u32 {
        self as u32
    }

    #[inline]
    pub const fn bit_width(self) -> u8 {
        match self {
            Self::W32 => 32,
            Self::X64 => 64,
        }
    }
}

/// 32-bit bitmask representing a subset of `Gpr`s (`X0..X30` in bits `0..=30`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct GprMask(pub u32);

impl GprMask {
    pub const EMPTY: Self = Self(0);
    pub const ALL_31: Self = Self(0x7FFF_FFFF);

    /// AAPCS64 caller-saved (volatile) registers (`X0..X17`).
    pub const AAPCS64_CALLER_SAVED: Self = Self(0x0003_FFFF);

    /// AAPCS64 callee-saved (non-volatile) registers (`X19..X28`).
    pub const AAPCS64_CALLEE_SAVED: Self = Self(0x1FF8_0000);

    #[inline]
    pub const fn single(reg: Gpr) -> Self {
        Self(1u32 << (reg as u8))
    }

    #[inline]
    pub const fn contains(self, reg: Gpr) -> bool {
        (self.0 & (1u32 << (reg as u8))) != 0
    }

    #[inline]
    pub const fn insert(self, reg: Gpr) -> Self {
        Self(self.0 | (1u32 << (reg as u8)))
    }

    #[inline]
    pub const fn remove(self, reg: Gpr) -> Self {
        Self(self.0 & !(1u32 << (reg as u8)))
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[inline]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn first(self) -> Option<Gpr> {
        if self.0 == 0 {
            None
        } else {
            Gpr::from_hw_id(self.0.trailing_zeros() as u8)
        }
    }
}

/// 128-bit NEON / Floating-Point / SVE Vector Registers (`V0`..`V31` / `Q0..Q31` / `D0..D31` / `S0..S31`).
///
/// Hardware rule: Unlike ARM32 (where `S0` and `S1` packed into `D0`), on AArch64
/// `B0`, `H0`, `S0`, `D0`, `Q0`, and `Z0` are all the bottom bits of the **same** register `V0` (`0..=31`),
/// and writing a scalar/smaller width zeroes all upper bits of `Vn`!
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum VecReg {
    V0 = 0,   V1 = 1,   V2 = 2,   V3 = 3,   V4 = 4,   V5 = 5,   V6 = 6,   V7 = 7,
    V8 = 8,   V9 = 9,   V10 = 10, V11 = 11, V12 = 12, V13 = 13, V14 = 14, V15 = 15,
    V16 = 16, V17 = 17, V18 = 18, V19 = 19, V20 = 20, V21 = 21, V22 = 22, V23 = 23,
    V24 = 24, V25 = 25, V26 = 26, V27 = 27, V28 = 28, V29 = 29, V30 = 30, V31 = 31,
}

impl VecReg {
    #[inline]
    pub const fn hw_id5(self) -> u8 {
        self as u8
    }
}

/// SVE / SME Predicate Registers (`P0`..`P15`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PredReg {
    P0 = 0,   P1 = 1,   P2 = 2,   P3 = 3,
    P4 = 4,   P5 = 5,   P6 = 6,   P7 = 7,
    P8 = 8,   P9 = 9,   P10 = 10, P11 = 11,
    P12 = 12, P13 = 13, P14 = 14, P15 = 15,
}
