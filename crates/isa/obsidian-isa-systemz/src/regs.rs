//! Physical IBM SystemZ (`s390x` / z/Architecture) registers and hardware encoding rules.
//!
//! Every discriminant in this module matches the exact 4-bit (`0..=15`) or
//! 5-bit (`0..=31`, via `RXB`) hardware register number decoded by SystemZ silicon
//! as defined in the *IBM z/Architecture Principles of Operation (SA22-7832)*.

/// 64-bit General Purpose Registers (`%r0`..`%r15`) in exact 4-bit hardware encoding order (`0..=15`).
///
/// ### Critical Silicon Rules
/// 1. **`R0` Base / Index Zero Substitution (`hw_id == 0`):**
///    Whenever `R0` is encoded in a memory base (`B`), memory index (`X`), or register-branch
///    target (`BCR R2`) field, the CPU **does not read `%r0`**—it substitutes the constant `0`
///    (meaning "no base register", "no index register", or "no-op branch").
///    Therefore, only `R1..=R15` can hold memory base or index pointers (`reg.can_be_base_or_index()`).
/// 2. **32-Bit Low vs. High-Word Preservation:**
///    Standard 32-bit instructions (`LR`, `AR`, `L`, `LHI`) operate on the **low 32 bits**
///    (IBM bits `32..63`) and leave the **upper 32 bits** (IBM bits `0..31`) untouched!
///    With the z196 High-Word Facility, the upper 32 bits can also be addressed independently.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Gpr {
    R0  = 0,
    R1  = 1,
    R2  = 2,
    R3  = 3,
    R4  = 4,
    R5  = 5,
    R6  = 6,
    R7  = 7,
    R8  = 8,
    R9  = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    /// Standard return address register (`%r14`) in the Linux `s390x` ELF ABI.
    R14 = 14,
    /// Standard stack pointer (`%r15`) in the Linux `s390x` ELF ABI.
    R15 = 15,
}

impl Gpr {
    pub const COUNT: usize = 16;

    pub const ALL: [Gpr; 16] = [
        Self::R0,  Self::R1,  Self::R2,  Self::R3,
        Self::R4,  Self::R5,  Self::R6,  Self::R7,
        Self::R8,  Self::R9,  Self::R10, Self::R11,
        Self::R12, Self::R13, Self::R14, Self::R15,
    ];

    /// Construct a `Gpr` from its 4-bit hardware nibble (`0..=15`).
    #[inline]
    pub const fn from_hw_id(id: u8) -> Option<Self> {
        if id < 16 {
            Some(Self::ALL[id as usize])
        } else {
            None
        }
    }

    /// Returns the 4-bit hardware register number (`0..=15`).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Hardware rule: `R0` (`0`) in a `B` (base) or `X` (index) field is decoded by SystemZ
    /// silicon as the constant `0` (no register), NEVER the value stored in `%r0`.
    /// Only `R1..=R15` can be used as base or index address registers.
    #[inline]
    pub const fn can_be_base_or_index(self) -> bool {
        (self as u8) != 0
    }

    /// Returns `true` if this register is an even-numbered register (`R0`, `R2`, ..., `R14`)
    /// eligible to anchor a 128-bit `Even:Odd` register pair (`GprPair`).
    #[inline]
    pub const fn is_even(self) -> bool {
        ((self as u8) & 1) == 0
    }

    /// Returns the architectural assembly name (`"r0"`..`"r15"`).
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::R0  => "r0",  Self::R1  => "r1",  Self::R2  => "r2",  Self::R3  => "r3",
            Self::R4  => "r4",  Self::R5  => "r5",  Self::R6  => "r6",  Self::R7  => "r7",
            Self::R8  => "r8",  Self::R9  => "r9",  Self::R10 => "r10", Self::R11 => "r11",
            Self::R12 => "r12", Self::R13 => "r13", Self::R14 => "r14", Self::R15 => "r15",
        }
    }
}

/// Operand sub-register view for SystemZ General Purpose Registers.
///
/// Note: IBM z/Architecture numbers bits from `0` (MSB) to `63` (LSB):
/// - `B64`: Full 64-bit register (IBM bits `0..=63`).
/// - `B32Low`: Low 32-bit word (IBM bits `32..=63`). Writing `B32Low` preserves `B32High`!
/// - `B32High`: High 32-bit word (IBM bits `0..=31`, accessed via the High-Word Facility).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprPart {
    B64     = 8,
    B32Low  = 4,
    B32High = 5,
}

impl GprPart {
    /// Returns `true` if writing to this register view leaves the other 32-bit half unmodified.
    #[inline]
    pub const fn preserves_other_half(self) -> bool {
        matches!(self, Self::B32Low | Self::B32High)
    }
}

/// Hardware 128-bit `Even:Odd` General Purpose Register pair (`R0:R1` .. `R14:R15`).
///
/// Required by 128-bit multiply (`MLGR`), 128-bit divide (`DLGR`, `DSGR`), and `FLOGR`
/// (Find Leftmost One), which always encode the **even** register number in the instruction
/// and implicitly read/write `even` (high 64 bits) and `even + 1` (low 64 bits).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprPair {
    R0R1   = 0,
    R2R3   = 2,
    R4R5   = 4,
    R6R7   = 6,
    R8R9   = 8,
    R10R11 = 10,
    R12R13 = 12,
    R14R15 = 14,
}

impl GprPair {
    /// Construct an `Even:Odd` register pair from an even `Gpr`.
    /// Returns `None` if `reg` is odd (`R1`, `R3`, ..., `R15`).
    #[inline]
    pub const fn from_even(reg: Gpr) -> Option<Self> {
        match reg {
            Gpr::R0  => Some(Self::R0R1),
            Gpr::R2  => Some(Self::R2R3),
            Gpr::R4  => Some(Self::R4R5),
            Gpr::R6  => Some(Self::R6R7),
            Gpr::R8  => Some(Self::R8R9),
            Gpr::R10 => Some(Self::R10R11),
            Gpr::R12 => Some(Self::R12R13),
            Gpr::R14 => Some(Self::R14R15),
            _ => None,
        }
    }

    /// Returns the even (high 64-bit / remainder) register of the pair.
    #[inline]
    pub const fn even(self) -> Gpr {
        Gpr::ALL[self as usize]
    }

    /// Returns the odd (low 64-bit / quotient) register of the pair.
    #[inline]
    pub const fn odd(self) -> Gpr {
        Gpr::ALL[(self as usize) + 1]
    }

    /// Returns the 4-bit hardware encoding nibble (the even register's ID).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Returns the `GprMask` containing both the even and odd registers.
    #[inline]
    pub const fn mask(self) -> GprMask {
        GprMask((0b11u16) << (self as u8))
    }
}

/// Compact 16-bit bitmask representing a set of SystemZ `Gpr`s (`R0`..`R15`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct GprMask(pub u16);

impl GprMask {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self(0xFFFF);
    /// All registers valid as base or index pointers (`R1..=R15`, excluding `R0`).
    pub const ADDR_REGS: Self = Self(0xFFFE);

    #[inline]
    pub const fn single(reg: Gpr) -> Self {
        Self(1u16 << (reg as u8))
    }

    #[inline]
    pub const fn from_slice(regs: &[Gpr]) -> Self {
        let mut mask = 0u16;
        let mut i = 0;
        while i < regs.len() {
            mask |= 1u16 << (regs[i] as u8);
            i += 1;
        }
        Self(mask)
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn contains(self, reg: Gpr) -> bool {
        (self.0 & (1u16 << (reg as u8))) != 0
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// 128-bit Vector & Floating-Point Registers (`%v0`..`%v31`).
///
/// ### Hardware Aliasing Rule
/// On z13+ silicon, the 16 architectural 64-bit Floating-Point Registers (`%f0`..`%f15`)
/// reside in the **high 64 bits (IBM bits `0..=63`) of `%v0`..`%v15`**.
/// Registers `%v16`..`%v31` require setting the corresponding extension bit in the
/// 4-bit `RXB` (Register Extension Bit) field of 6-byte `VRR`/`VRX` instructions.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum VecReg {
    V0 = 0,   V1 = 1,   V2 = 2,   V3 = 3,   V4 = 4,   V5 = 5,   V6 = 6,   V7 = 7,
    V8 = 8,   V9 = 9,   V10 = 10, V11 = 11, V12 = 12, V13 = 13, V14 = 14, V15 = 15,
    V16 = 16, V17 = 17, V18 = 18, V19 = 19, V20 = 20, V21 = 21, V22 = 22, V23 = 23,
    V24 = 24, V25 = 25, V26 = 26, V27 = 27, V28 = 28, V29 = 29, V30 = 30, V31 = 31,
}

impl VecReg {
    pub const F0: Self = Self::V0;
    pub const F1: Self = Self::V1;
    pub const F2: Self = Self::V2;
    pub const F3: Self = Self::V3;
    pub const F4: Self = Self::V4;
    pub const F5: Self = Self::V5;
    pub const F6: Self = Self::V6;
    pub const F7: Self = Self::V7;
    pub const F8: Self = Self::V8;
    pub const F9: Self = Self::V9;
    pub const F10: Self = Self::V10;
    pub const F11: Self = Self::V11;
    pub const F12: Self = Self::V12;
    pub const F13: Self = Self::V13;
    pub const F14: Self = Self::V14;
    pub const F15: Self = Self::V15;

    /// Returns the full 5-bit hardware register number (`0..=31`).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Returns the low 4 bits (`0..=15`) placed into a standard `V1`..`V4` or `R1`/`R2` nibble.
    #[inline]
    pub const fn low4(self) -> u8 {
        (self as u8) & 0x0F
    }

    /// Returns the 5th bit (`0` or `1`) placed into the `RXB` (Register Extension Bit) nibble.
    #[inline]
    pub const fn rxb_bit(self) -> u8 {
        ((self as u8) >> 4) & 1
    }

    /// Returns `true` if this vector register overlaps architectural floating-point register `%f0..%f15`.
    #[inline]
    pub const fn aliases_fpr(self) -> bool {
        (self as u8) < 16
    }
}

/// 32-bit Access Registers (`%a0`..`%a15`).
///
/// In the Linux `s390x` ABI, `%a0` and `%a1` hold the 64-bit Thread-Local Storage (`TLS`) pointer.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum AccessReg {
    A0 = 0,   A1 = 1,   A2 = 2,   A3 = 3,
    A4 = 4,   A5 = 5,   A6 = 6,   A7 = 7,
    A8 = 8,   A9 = 9,   A10 = 10, A11 = 11,
    A12 = 12, A13 = 13, A14 = 14, A15 = 15,
}
