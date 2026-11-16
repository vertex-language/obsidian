//! Motorola 680x0 Condition Code Register (`CCR`) flags (`X`, `N`, `Z`, `V`, `C`) and 4-bit `CondCode`.
//!
//! ### How M68k `CCR` Flags Differ From x86
//! M68k separates the **Carry (`C`)** flag (used by unsigned branches/borrows) from the
//! **Extend (`X`)** flag (used by multi-precision arithmetic `ADDX`/`SUBX`/`ROXL`/`ROXR`):
//! - `MOVE`, `AND`, `OR`, `EOR`, `CMP`, `TST`, `CLR` update `N`, `Z`, `V`, `C`, and
//!   **leave `X` completely unchanged**!
//! - `ADDA`, `SUBA`, `MOVEA`, and `LEA` targeting an Address Register (`A0..A7`)
//!   **leave all 5 flags (`X`, `N`, `Z`, `V`, `C`) completely unchanged**!

/// Architectural 5-bit Condition Code Register (`CCR`, low byte of the 16-bit `SR`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct CcrFlags(pub u8);

impl CcrFlags {
    pub const NONE: Self = Self(0);
    /// Carry / Borrow Flag (bit 0)
    pub const C: Self = Self(1 << 0);
    /// Overflow Flag (bit 1)
    pub const V: Self = Self(1 << 1);
    /// Zero Flag (bit 2)
    pub const Z: Self = Self(1 << 2);
    /// Negative Flag (bit 3)
    pub const N: Self = Self(1 << 3);
    /// Extend Flag (bit 4 — multi-precision carry, preserved by `MOVE`/`CMP`/`TST`/logical ops!)
    pub const X: Self = Self(1 << 4);

    /// Flags written by `CMP`, `TST`, `MOVE` (to `Dn`/mem), `AND`, `OR`, `EOR`, `NOT`, `CLR`
    /// (`N`, `Z`, `V`, `C` modified; `X` preserved!).
    pub const NZVC: Self = Self(Self::N.0 | Self::Z.0 | Self::V.0 | Self::C.0);

    /// All 5 flags (`X`, `N`, `Z`, `V`, `C`) written by `ADD`, `SUB`, `NEG`, `ASL`, `ASR`, `LSL`, `LSR`.
    pub const XNZVC: Self = Self(Self::X.0 | Self::N.0 | Self::Z.0 | Self::V.0 | Self::C.0);

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

/// 4-bit hardware condition code (`0x0..=0xF`) used in `Bcc`, `Scc`, and `DBcc`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CondCode {
    /// True (`1` — in `Bcc` encodes `BRA`; in `Scc` encodes `ST`)
    T  = 0x0,
    /// False (`0` — in `Bcc` encodes `BSR`; in `DBcc` encodes `DBF`/`DBRA`; in `Scc` encodes `SF`)
    F  = 0x1,
    /// High (`!C && !Z`, unsigned `>`)
    HI = 0x2,
    /// Low or Same (`C || Z`, unsigned `<=`)
    LS = 0x3,
    /// Carry Clear / High or Same (`!C`, unsigned `>=`)
    CC = 0x4,
    /// Carry Set / Low (`C`, unsigned `<`)
    CS = 0x5,
    /// Not Equal (`!Z`, `!=`)
    NE = 0x6,
    /// Equal (`Z`, `==`)
    EQ = 0x7,
    /// Overflow Clear (`!V`)
    VC = 0x8,
    /// Overflow Set (`V`)
    VS = 0x9,
    /// Plus (`!N`, signed `>= 0`)
    PL = 0xA,
    /// Minus (`N`, signed `< 0`)
    MI = 0xB,
    /// Greater or Equal (`N == V`, signed `>=`)
    GE = 0xC,
    /// Less Than (`N != V`, signed `<`)
    LT = 0xD,
    /// Greater Than (`!Z && (N == V)`, signed `>`)
    GT = 0xE,
    /// Less or Equal (`Z || (N != V)`, signed `<=`)
    LE = 0xF,
}

impl CondCode {
    pub const HS: Self = Self::CC;
    pub const LO: Self = Self::CS;

    pub const ALL: [CondCode; 16] = [
        Self::T,  Self::F,  Self::HI, Self::LS,
        Self::CC, Self::CS, Self::NE, Self::EQ,
        Self::VC, Self::VS, Self::PL, Self::MI,
        Self::GE, Self::LT, Self::GT, Self::LE,
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
    /// Hardware invariant: in M68k silicon, flipping bit 0 of any condition code
    /// (`T <-> F`, `HI <-> LS`, `CC <-> CS`, `NE <-> EQ`, `GE <-> LT`, `GT <-> LE`)
    /// produces its exact logical inverse!
    #[inline]
    pub const fn invert(self) -> Self {
        Self::ALL[((self as u8) ^ 1) as usize]
    }

    /// Commutes comparison operands (`a CMP b` -> `b CMP a`) for relational/equality codes:
    /// - `HI (>)` <-> `CS (<)`
    /// - `LS (<=)` <-> `CC (>=)`
    /// - `GT (>)` <-> `LT (<)`
    /// - `LE (<=)` <-> `GE (>=)`
    #[inline]
    pub const fn commute(self) -> Option<Self> {
        match self {
            Self::EQ => Some(Self::EQ),
            Self::NE => Some(Self::NE),
            Self::HI => Some(Self::CS),
            Self::CS => Some(Self::HI),
            Self::LS => Some(Self::CC),
            Self::CC => Some(Self::LS),
            Self::GT => Some(Self::LT),
            Self::LT => Some(Self::GT),
            Self::LE => Some(Self::GE),
            Self::GE => Some(Self::LE),
            _ => None,
        }
    }

    /// Returns the exact subset of `CCR` bits read by the CPU when evaluating this condition.
    #[inline]
    pub const fn flags_read(self) -> CcrFlags {
        match self {
            Self::T | Self::F => CcrFlags::NONE,
            Self::HI | Self::LS => CcrFlags::C.union(CcrFlags::Z),
            Self::CC | Self::CS => CcrFlags::C,
            Self::NE | Self::EQ => CcrFlags::Z,
            Self::VC | Self::VS => CcrFlags::V,
            Self::PL | Self::MI => CcrFlags::N,
            Self::GE | Self::LT => CcrFlags::N.union(CcrFlags::V),
            Self::GT | Self::LE => CcrFlags::Z.union(CcrFlags::N).union(CcrFlags::V),
        }
    }
}
