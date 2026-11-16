//! Architectural condition codes (`CondCode`) and status flags (`FlagSet`) in `RFLAGS`.
//!
//! Every `CondCode` discriminant (`0..=15`) is the exact 4-bit hardware condition
//! field (`tttn`) encoded in the low nibble of `Jcc` (`0x70 + cc` / `0x0F 0x80 + cc`),
//! `SETcc` (`0x0F 0x90 + cc`), and `CMOVcc` (`0x0F 0x40 + cc`).

/// Architectural `EFLAGS` / `RFLAGS` condition and status bits.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct FlagSet(pub u8);

impl FlagSet {
    pub const NONE: Self = Self(0);
    /// Carry Flag (unsigned overflow / borrow)
    pub const CF: Self = Self(1 << 0);
    /// Parity Flag (even parity in low byte)
    pub const PF: Self = Self(1 << 1);
    /// Auxiliary Carry Flag (BCD nibble carry)
    pub const AF: Self = Self(1 << 2);
    /// Zero Flag (result is zero / operands equal)
    pub const ZF: Self = Self(1 << 3);
    /// Sign Flag (most significant bit is 1 / negative)
    pub const SF: Self = Self(1 << 4);
    /// Overflow Flag (signed two's-complement overflow)
    pub const OF: Self = Self(1 << 5);
    /// Direction Flag (string instruction auto-increment/decrement)
    pub const DF: Self = Self(1 << 6);

    /// Standard 6 arithmetic status flags written by `ADD`, `SUB`, `CMP`, `NEG`, etc.
    pub const ALL_ARITH: Self = Self(
        Self::CF.0 | Self::PF.0 | Self::AF.0 | Self::ZF.0 | Self::SF.0 | Self::OF.0,
    );

    /// Flags written by `INC` and `DEC` (notably does NOT modify `CF`!).
    pub const INC_DEC: Self = Self(
        Self::PF.0 | Self::AF.0 | Self::ZF.0 | Self::SF.0 | Self::OF.0,
    );

    /// Flags written by logical operations (`AND`, `OR`, `XOR`, `TEST` — clears `CF` and `OF`).
    pub const LOGICAL: Self = Self(
        Self::CF.0 | Self::PF.0 | Self::AF.0 | Self::ZF.0 | Self::SF.0 | Self::OF.0,
    );

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// 4-bit hardware condition code (`0..=15`) used by `Jcc`, `SETcc`, and `CMOVcc`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CondCode {
    /// Overflow (`OF == 1`)
    O  = 0x0,
    /// Not Overflow (`OF == 0`)
    NO = 0x1,
    /// Below / Carry (`CF == 1`, unsigned `<`)
    B  = 0x2,
    /// Above or Equal / Not Carry (`CF == 0`, unsigned `>=`)
    AE = 0x3,
    /// Equal / Zero (`ZF == 1`, `==`)
    E  = 0x4,
    /// Not Equal / Not Zero (`ZF == 0`, `!=`)
    NE = 0x5,
    /// Below or Equal (`CF == 1 || ZF == 1`, unsigned `<=`)
    BE = 0x6,
    /// Above (`CF == 0 && ZF == 0`, unsigned `>`)
    A  = 0x7,
    /// Sign / Negative (`SF == 1`)
    S  = 0x8,
    /// Not Sign / Non-Negative (`SF == 0`)
    NS = 0x9,
    /// Parity Even / Unordered FP (`PF == 1`)
    P  = 0xA,
    /// Parity Odd / Ordered FP (`PF == 0`)
    NP = 0xB,
    /// Less (`SF != OF`, signed `<`)
    L  = 0xC,
    /// Greater or Equal (`SF == OF`, signed `>=`)
    GE = 0xD,
    /// Less or Equal (`ZF == 1 || SF != OF`, signed `<=`)
    LE = 0xE,
    /// Greater (`ZF == 0 && SF == OF`, signed `>`)
    G  = 0xF,
}

impl CondCode {
    pub const Z: Self = Self::E;
    pub const NZ: Self = Self::NE;
    pub const C: Self = Self::B;
    pub const NC: Self = Self::AE;

    /// All 16 hardware condition codes in `0..=15` order.
    pub const ALL: [CondCode; 16] = [
        Self::O,  Self::NO, Self::B,  Self::AE,
        Self::E,  Self::NE, Self::BE, Self::A,
        Self::S,  Self::NS, Self::P,  Self::NP,
        Self::L,  Self::GE, Self::LE, Self::G,
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

    /// Returns the exact 4-bit hardware nibble (`0..=15`).
    #[inline]
    pub const fn hw_nibble(self) -> u8 {
        self as u8
    }

    /// Logical negation of the condition (`!(a cc b)`).
    ///
    /// Hardware invariant: in x86 silicon, flipping bit 0 (`cc ^ 1`) of ANY
    /// condition code produces its exact logical inverse (`E <-> NE`, `L <-> GE`, `B <-> AE`).
    #[inline]
    pub const fn invert(self) -> Self {
        Self::ALL[((self as u8) ^ 1) as usize]
    }

    /// Commutes the comparison operands (`a cc b` -> `b commuted_cc a`), if valid
    /// for relational/equality comparisons (`L <-> G`, `LE <-> GE`, `B <-> A`, `BE <-> AE`).
    #[inline]
    pub const fn commute(self) -> Option<Self> {
        match self {
            Self::E  => Some(Self::E),
            Self::NE => Some(Self::NE),
            Self::B  => Some(Self::A),
            Self::AE => Some(Self::BE),
            Self::BE => Some(Self::AE),
            Self::A  => Some(Self::B),
            Self::L  => Some(Self::G),
            Self::GE => Some(Self::LE),
            Self::LE => Some(Self::GE),
            Self::G  => Some(Self::L),
            _ => None,
        }
    }

    /// Returns the exact subset of `EFLAGS` bits read by the CPU when evaluating this condition.
    #[inline]
    pub const fn flags_read(self) -> FlagSet {
        match self {
            Self::O | Self::NO => FlagSet::OF,
            Self::B | Self::AE => FlagSet::CF,
            Self::E | Self::NE => FlagSet::ZF,
            Self::BE | Self::A => FlagSet::CF.union(FlagSet::ZF),
            Self::S | Self::NS => FlagSet::SF,
            Self::P | Self::NP => FlagSet::PF,
            Self::L | Self::GE => FlagSet::SF.union(FlagSet::OF),
            Self::LE | Self::G => FlagSet::ZF.union(FlagSet::SF).union(FlagSet::OF),
        }
    }

    /// Returns the assembly suffix (`"e"`, `"ne"`, `"l"`, `"ge"`, etc.).
    #[inline]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::O  => "o",  Self::NO => "no",
            Self::B  => "b",  Self::AE => "ae",
            Self::E  => "e",  Self::NE => "ne",
            Self::BE => "be", Self::A  => "a",
            Self::S  => "s",  Self::NS => "ns",
            Self::P  => "p",  Self::NP => "np",
            Self::L  => "l",  Self::GE => "ge",
            Self::LE => "le", Self::G  => "g",
        }
    }
}
