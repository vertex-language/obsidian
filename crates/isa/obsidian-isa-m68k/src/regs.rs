//! Physical Motorola 680x0 (`M68k`) registers and split Data (`D0..D7`) / Address (`A0..A7`) rules.
//!
//! Every discriminant matches the exact 3-bit (`0..=7`) or 4-bit (`0..=15`) hardware register
//! number decoded by M68k silicon as defined in the *Motorola M68000 Family Programmer's
//! Reference Manual (M68000PM/AD)*.
//!
//! ### Critical Silicon Rules for Split Register Banks
//! 1. **Data Registers (`DataReg`: `D0`..`D7`):**
//!    - Support `Byte` (8-bit), `Word` (16-bit), and `Long` (32-bit) operations.
//!    - Writing `Byte` (`bits 0..8`) or `Word` (`bits 0..16`) **preserves the upper bits** of `Dn`!
//!    - Arithmetic/moves targeting `Dn` update the Condition Code Register (`CCR`).
//! 2. **Address Registers (`AddrReg`: `A0`..`A7`, where `A7` is `SP`):**
//!    - **Forbid 8-bit `Byte` operations** (`OpSize::Byte` on `An` is illegal in silicon!).
//!    - Writing a 16-bit `Word` to `An` (`MOVEA.W`, `ADDA.W`, `SUBA.W`) **always sign-extends
//!      to 32 bits** across the entire address register!
//!    - Operations targeting `An` (`MOVEA`, `ADDA`, `SUBA`, `LEA`) **never modify `CCR` flags**!
//!    - Using `A7` (`SP`) with byte-sized `(A7)+` or `-(A7)` steps `A7` by **2 bytes** instead of 1
//!      to maintain hardware 16-bit stack alignment!

/// 32-bit Data Registers (`d0`..`d7`) in exact 3-bit hardware encoding order (`0..=7`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum DataReg {
    D0 = 0,
    D1 = 1,
    D2 = 2,
    D3 = 3,
    D4 = 4,
    D5 = 5,
    D6 = 6,
    D7 = 7,
}

impl DataReg {
    pub const COUNT: usize = 8;

    pub const ALL: [DataReg; 8] = [
        Self::D0, Self::D1, Self::D2, Self::D3,
        Self::D4, Self::D5, Self::D6, Self::D7,
    ];

    /// Construct a `DataReg` from its 3-bit hardware index (`0..=7`).
    #[inline]
    pub const fn from_hw_id(id: u8) -> Option<Self> {
        if id < 8 {
            Some(Self::ALL[id as usize])
        } else {
            None
        }
    }

    /// Returns the 3-bit hardware register number (`0..=7`).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Converts to the unified 4-bit `Reg` (`0..=7`).
    #[inline]
    pub const fn to_reg(self) -> Reg {
        Reg::Data(self)
    }

    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::D0 => "d0", Self::D1 => "d1", Self::D2 => "d2", Self::D3 => "d3",
            Self::D4 => "d4", Self::D5 => "d5", Self::D6 => "d6", Self::D7 => "d7",
        }
    }
}

/// 32-bit Address Registers (`a0`..`a7`) in exact 3-bit hardware encoding order (`0..=7`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AddrReg {
    A0 = 0,
    A1 = 1,
    A2 = 2,
    A3 = 3,
    A4 = 4,
    A5 = 5,
    /// Conventional Frame Pointer (`%a6` / `%fp`).
    A6 = 6,
    /// Hardware Stack Pointer (`%a7` / `%sp`).
    A7 = 7,
}

impl AddrReg {
    pub const COUNT: usize = 8;
    pub const FP: Self = Self::A6;
    pub const SP: Self = Self::A7;

    pub const ALL: [AddrReg; 8] = [
        Self::A0, Self::A1, Self::A2, Self::A3,
        Self::A4, Self::A5, Self::A6, Self::A7,
    ];

    /// Construct an `AddrReg` from its 3-bit hardware index (`0..=7`).
    #[inline]
    pub const fn from_hw_id(id: u8) -> Option<Self> {
        if id < 8 {
            Some(Self::ALL[id as usize])
        } else {
            None
        }
    }

    /// Returns the 3-bit hardware register number (`0..=7`).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Converts to the unified 4-bit `Reg` (`8..=15`).
    #[inline]
    pub const fn to_reg(self) -> Reg {
        Reg::Addr(self)
    }

    /// Returns the exact byte step applied to this address register by `(An)+` or `-(An)`
    /// for a given `OpSize`.
    ///
    /// **Silicon Rule:** When `A7` (`SP`) is stepped with `OpSize::Byte` (`1` byte), the CPU
    /// steps `A7` by **2 bytes** instead of 1 so the hardware stack pointer always stays
    /// aligned to a 16-bit word boundary!
    #[inline]
    pub const fn auto_inc_dec_step(self, size: OpSize) -> u8 {
        match (self, size) {
            (Self::A7, OpSize::Byte) => 2,
            (_, OpSize::Byte) => 1,
            (_, OpSize::Word) => 2,
            (_, OpSize::Long) => 4,
        }
    }

    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            Self::A0 => "a0", Self::A1 => "a1", Self::A2 => "a2", Self::A3 => "a3",
            Self::A4 => "a4", Self::A5 => "a5", Self::A6 => "a6", Self::A7 => "sp",
        }
    }
}

/// Unified 4-bit M68k general register (`0..=7` = `D0..D7`, `8..=15` = `A0..A7`),
/// used in brief extension word index fields (`D/A` bit + 3-bit reg) and `MOVEM` masks.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Reg {
    Data(DataReg),
    Addr(AddrReg),
}

impl Reg {
    /// Returns the 4-bit hardware ID (`0..=7` for `D0..D7`, `8..=15` for `A0..A7`).
    #[inline]
    pub const fn hw_id4(self) -> u8 {
        match self {
            Self::Data(d) => d as u8,
            Self::Addr(a) => 8 + (a as u8),
        }
    }

    /// Returns the 3-bit bank-local register number (`0..=7`).
    #[inline]
    pub const fn low3(self) -> u8 {
        match self {
            Self::Data(d) => d as u8,
            Self::Addr(a) => a as u8,
        }
    }

    /// Returns `1` if this is an `AddrReg` (`A0..A7`) or `0` if `DataReg` (`D0..D7`).
    #[inline]
    pub const fn da_bit(self) -> u8 {
        match self {
            Self::Data(_) => 0,
            Self::Addr(_) => 1,
        }
    }

    /// Returns `true` if `size` is legal on this register in M68k silicon.
    ///
    /// Specifically, `OpSize::Byte` is legal on `DataReg`, but **illegal on `AddrReg`**.
    #[inline]
    pub const fn supports_size(self, size: OpSize) -> bool {
        match (self, size) {
            (Self::Addr(_), OpSize::Byte) => false,
            _ => true,
        }
    }
}

/// Hardware operand width (`Byte = .b`, `Word = .w`, `Long = .l`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum OpSize {
    /// 8-bit byte (`.b`)
    Byte = 1,
    /// 16-bit word (`.w`)
    Word = 2,
    /// 32-bit longword (`.l`)
    Long = 4,
}

impl OpSize {
    #[inline]
    pub const fn byte_len(self) -> u8 {
        self as u8
    }

    /// Returns the standard 2-bit size field (`0b00 = Byte`, `0b01 = Word`, `0b10 = Long`)
    /// used by `ADD`, `SUB`, `CMP`, `AND`, `OR`, `EOR`, `CLR`, `NEG`, `NOT`, `TST`, and shifts.
    #[inline]
    pub const fn std_size_bits(self) -> u8 {
        match self {
            Self::Byte => 0b00,
            Self::Word => 0b01,
            Self::Long => 0b10,
        }
    }

    /// Returns the `MOVE`-specific 2-bit size field (`0b01 = Byte`, `0b11 = Word`, `0b10 = Long`)
    /// placed in bits `12..=13` of the `MOVE` / `MOVEA` opcode word.
    #[inline]
    pub const fn move_size_bits(self) -> u8 {
        match self {
            Self::Byte => 0b01,
            Self::Word => 0b11,
            Self::Long => 0b10,
        }
    }

    /// Returns `true` if writing a result of this size to a `DataReg` (`D0..D7`) preserves
    /// the upper bits of that `DataReg` (`Byte` preserves bits `8..32`; `Word` preserves `16..32`).
    #[inline]
    pub const fn preserves_upper_data_bits(self) -> bool {
        matches!(self, Self::Byte | Self::Word)
    }
}

/// 16-bit register list mask (`D0..D7`, `A0..A7`) used by `MOVEM` (Move Multiple Registers).
///
/// ### Silicon Bit-Order Quirk for `MOVEM`
/// - For all normal addressing modes (`(An)`, `(An)+`, `(d16, An)`, etc.), bit `0` is `D0`..bit `7` is `D7`,
///   and bit `8` is `A0`..bit `15` is `A7`.
/// - For **Pre-Decrement mode `-(An)` ONLY**, M68k silicon **reverses the 16-bit mask**
///   (`mask.reverse_bits()`): bit `0` is `A7`..bit `7` is `A0`, and bit `8` is `D7`..bit `15` is `D0`!
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct RegListMask(pub u16);

impl RegListMask {
    pub const EMPTY: Self = Self(0);

    #[inline]
    pub const fn from_regs(regs: &[Reg]) -> Self {
        let mut mask = 0u16;
        let mut i = 0;
        while i < regs.len() {
            mask |= 1u16 << regs[i].hw_id4();
            i += 1;
        }
        Self(mask)
    }

    #[inline]
    pub const fn contains(self, reg: Reg) -> bool {
        (self.0 & (1u16 << reg.hw_id4())) != 0
    }

    #[inline]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Returns the exact 16-bit extension word required by the `MOVEM` instruction:
    /// normal order (`D0=bit0 .. A7=bit15`) for standard modes, or bit-reversed
    /// (`A7=bit0 .. D0=bit15`) when `is_predecrement` (`-(An)`) is `true`!
    #[inline]
    pub const fn movem_extension_word(self, is_predecrement: bool) -> u16 {
        if is_predecrement {
            self.0.reverse_bits()
        } else {
            self.0
        }
    }
}

/// 80-bit Extended-Precision Floating-Point Registers (`FP0`..`FP7`, MC68881/68882/68040+).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FpReg {
    Fp0 = 0, Fp1 = 1, Fp2 = 2, Fp3 = 3,
    Fp4 = 4, Fp5 = 5, Fp6 = 6, Fp7 = 7,
}
