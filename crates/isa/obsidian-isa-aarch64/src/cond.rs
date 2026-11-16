//! AArch64 `NZCV` Condition Flags and 4-Bit Hardware `CondCode` (`0x0..=0xF`).
//!
//! ### How AArch64 Condition Flags Differ From x86
//! 1. **Opt-In Flag Updates:** Standard arithmetic/logical instructions (`ADD`, `SUB`, `AND`,
//!    `ORR`, `EOR`, `LSL`, `MADD`) **never modify `NZCV`**. Only explicit `S`-suffixed
//!    instructions (`ADDS`, `SUBS`/`CMP`, `ANDS`/`TST`, `CCMP`, `FCMP`) write `NZCV`.
//! 2. **Inverted Carry (`C`) Polarity on Subtract/Compare:**
//!    On AArch64, `SUBS`/`CMP` (`a - b`) computes `a + ~b + 1`, so:
//!    - `C = 1` means **No Borrow** (`a >= b` unsigned, tested by `CS` / `HS`).
//!    - `C = 0` means **Borrow Occurred** (`a < b` unsigned, tested by `CC` / `LO`).
//!    (The exact opposite of x86's `CF` on `sub`/`cmp`!)

/// Architectural 4-bit `NZCV` status register (`PSTATE.NZCV`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct NzcvFlags(pub u8);

impl NzcvFlags {
    pub const NONE: Self = Self(0);
    /// Overflow Flag (`V`, bit 0)
    pub const V: Self = Self(1 << 0);
    /// Carry / Not-Borrow Flag (`C`, bit 1)
    pub const C: Self = Self(1 << 1);
    /// Zero Flag (`Z`, bit 2)
    pub const Z: Self = Self(1 << 2);
    /// Negative / Sign Flag (`N`, bit 3)
    pub const N: Self = Self(1 << 3);

    /// All 4 flags (`N | Z | C | V`), written by `ADDS`, `SUBS`/`CMP`, `ANDS`/`TST`, `CCMP`, `FCMP`.
    pub const ALL: Self = Self(Self::N.0 | Self::Z.0 | Self::C.0 | Self::V.0);

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// 4-bit hardware condition code (`0x0..=0xF`) used in `B.cond`, `CSEL`, `CSINC`, `CSINV`, `CSNEG`, and `CCMP`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CondCode {
    /// Equal (`Z == 1`, `==`)
    EQ = 0x0,
    /// Not Equal (`Z == 0`, `!=`)
    NE = 0x1,
    /// Carry Set / Unsigned Higher or Same (`C == 1`, unsigned `>=`)
    HS = 0x2,
    /// Carry Clear / Unsigned Lower (`C == 0`, unsigned `<`)
    LO = 0x3,
    /// Minus / Negative (`N == 1`, `< 0`)
    MI = 0x4,
    /// Plus / Positive or Zero (`N == 0`, `>= 0`)
    PL = 0x5,
    /// Overflow Set (`V == 1`)
    VS = 0x6,
    /// Overflow Clear (`V == 0`)
    VC = 0x7,
    /// Unsigned Higher (`C == 1 && Z == 0`, unsigned `>`)
    HI = 0x8,
    /// Unsigned Lower or Same (`!(C == 1 && Z == 0)`, unsigned `<=`)
    LS = 0x9,
    /// Signed Greater or Equal (`N == V`, signed `>=`)
    GE = 0xA,
    /// Signed Less Than (`N != V`, signed `<`)
    LT = 0xB,
    /// Signed Greater Than (`Z == 0 && N == V`, signed `>`)
    GT = 0xC,
    /// Signed Less or Equal (`!(Z == 0 && N == V)`, signed `<=`)
    LE = 0xD,
    /// Always (`true`)
    AL = 0xE,
    /// Never (behaves identically to Always `0xE` in AArch64 instructions, reserved encoding `0xF`)
    NV = 0xF,
}

impl CondCode {
    pub const CS: Self = Self::HS;
    pub const CC: Self = Self::LO;

    pub const ALL: [CondCode; 16] = [
        Self::EQ, Self::NE, Self::HS, Self::LO,
        Self::MI, Self::PL, Self::VS, Self::VC,
        Self::HI, Self::LS, Self::GE, Self::LT,
        Self::GT, Self::LE, Self::AL, Self::NV,
    ];

    /// Construct from the 4-bit hardware nibble (`0..=15`).
    #[inline]
    pub const fn from_hw_nibble(nibble: u8) -> Option<Self> {
        if nibble < 16 {
            Some(Self::ALL[nibble as usize])
        } else {
            None
        }
    }

    #[inline]
    pub const fn hw_nibble(self) -> u8 {
        self as u8
    }

    /// Logical negation of the condition (`cc ^ 1`).
    ///
    /// Hardware invariant: in AArch64 silicon (even inside `CSINC`/`CSINV`/`CSNEG` hardware decoding!),
    /// flipping bit 0 of any condition code (`EQ <-> NE`, `HS <-> LO`, `HI <-> LS`, `GE <-> LT`, `GT <-> LE`)
    /// produces its exact logical inverse!
    #[inline]
    pub const fn invert(self) -> Self {
        Self::ALL[((self as u8) ^ 1) as usize]
    }

    /// Commutes the comparison operands (`a CMP b` -> `b CMP a`):
    /// - `HI (>)` <-> `LO (<)`
    /// - `HS (>=)` <-> `LS (<=)`
    /// - `GT (>)` <-> `LT (<)`
    /// - `GE (>=)` <-> `LE (<=)`
    #[inline]
    pub const fn commute(self) -> Option<Self> {
        match self {
            Self::EQ => Some(Self::EQ),
            Self::NE => Some(Self::NE),
            Self::HI => Some(Self::LO),
            Self::LO => Some(Self::HI),
            Self::HS => Some(Self::LS),
            Self::LS => Some(Self::HS),
            Self::GT => Some(Self::LT),
            Self::LT => Some(Self::GT),
            Self::GE => Some(Self::LE),
            Self::LE => Some(Self::GE),
            _ => None,
        }
    }

    /// Returns the exact subset of `NZCV` flags read by the CPU when evaluating this condition.
    #[inline]
    pub const fn flags_read(self) -> NzcvFlags {
        match self {
            Self::EQ | Self::NE => NzcvFlags::Z,
            Self::HS | Self::LO => NzcvFlags::C,
            Self::MI | Self::PL => NzcvFlags::N,
            Self::VS | Self::VC => NzcvFlags::V,
            Self::HI | Self::LS => NzcvFlags::C.union(NzcvFlags::Z),
            Self::GE | Self::LT => NzcvFlags::N.union(NzcvFlags::V),
            Self::GT | Self::LE => NzcvFlags::Z.union(NzcvFlags::N).union(NzcvFlags::V),
            Self::AL | Self::NV => NzcvFlags::NONE,
        }
    }
}
