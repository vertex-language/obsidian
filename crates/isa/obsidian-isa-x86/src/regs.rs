//! Physical x86-64 register definitions and hardware encoding numbers.
//!
//! Every discriminant in this module matches the exact 3-bit (`0..=7`) or
//! 4-bit (`0..=15`) / 5-bit (`0..=31`) hardware register number decoded by
//! x86-64 silicon in `ModR/M`, `SIB`, `REX`, `VEX`, and `EVEX` fields.
//! Zero virtual or compiler-invented registers exist here.

/// 64-bit General Purpose Registers (`RAX`..`R15`) in exact hardware encoding order.
///
/// - Low 3 bits (`reg.low3()`): encoded into `ModR/M.reg`, `ModR/M.rm`, `SIB.base`, or `SIB.index`.
/// - Bit 3 (`reg.needs_rex_ext()`): encoded into `REX.R`, `REX.B`, or `REX.X` (and inverted in `VEX`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Gpr {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8  = 8,
    R9  = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl Gpr {
    /// Total number of architectural GPRs in x86-64 mode.
    pub const COUNT: usize = 16;

    /// All 16 architectural GPRs in hardware encoding order (`0..=15`).
    pub const ALL: [Gpr; 16] = [
        Self::Rax, Self::Rcx, Self::Rdx, Self::Rbx,
        Self::Rsp, Self::Rbp, Self::Rsi, Self::Rdi,
        Self::R8,  Self::R9,  Self::R10, Self::R11,
        Self::R12, Self::R13, Self::R14, Self::R15,
    ];

    /// Construct a `Gpr` from its 4-bit hardware index (`0..=15`).
    #[inline]
    pub const fn from_hw_id(id: u8) -> Option<Self> {
        if id < 16 {
            Some(Self::ALL[id as usize])
        } else {
            None
        }
    }

    /// Returns the full 4-bit hardware index (`0..=15`).
    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    /// Returns the low 3 bits (`0..=7`) placed into `ModR/M` or `SIB` fields.
    #[inline]
    pub const fn low3(self) -> u8 {
        (self as u8) & 0b0111
    }

    /// Returns the 4th bit (`0` or `1`) placed into `REX.R`, `REX.X`, or `REX.B`.
    #[inline]
    pub const fn rex_bit(self) -> u8 {
        (self as u8) >> 3
    }

    /// Returns `true` if this register is in `R8..=R15` (requires a `REX` or `VEX` prefix).
    #[inline]
    pub const fn needs_rex_ext(self) -> bool {
        (self as u8) >= 8
    }

    /// Returns `true` if using this register as an 8-bit low sub-register (`SPL`, `BPL`, `SIL`, `DIL`)
    /// requires a `REX` prefix so the CPU does not decode it as `AH`, `CH`, `DH`, or `BH`.
    #[inline]
    pub const fn needs_rex_for_byte_low(self) -> bool {
        (self as u8) >= 4
    }

    /// Hardware quirk: `RSP` (`4`) and `R12` (`12`) share `low3() == 0b100`, which is the
    /// `ModR/M.rm` escape code for a `SIB` byte. Therefore `[rsp]` and `[r12]` ALWAYS
    /// require an extra `SIB` byte (`0x24`) in machine code.
    #[inline]
    pub const fn base_requires_sib(self) -> bool {
        self.low3() == 0b100
    }

    /// Hardware quirk: `RBP` (`5`) and `R13` (`13`) share `low3() == 0b101`, which in
    /// `ModR/M.mod == 0b00` encodes RIP-relative addressing (`[rip + disp32]`).
    /// Therefore `[rbp]` and `[r13]` CANNOT be encoded with a 0-byte displacement;
    /// the CPU requires at least a 1-byte `disp8 = 0x00` (`mod = 0b01`).
    #[inline]
    pub const fn base_requires_disp8_zero(self) -> bool {
        self.low3() == 0b101
    }

    /// Hardware rule: `RSP` (`4`) cannot be used as a scaled index register in a `SIB` byte
    /// (because `SIB.index == 0b100` with `REX.X == 0` means "no index register").
    /// Note: `R12` (`12`) *can* be used as an index register because `REX.X == 1`.
    #[inline]
    pub const fn can_be_index(self) -> bool {
        !matches!(self, Self::Rsp)
    }

    /// Returns the architectural assembly name for the given operand width.
    #[inline]
    pub const fn name(self, size: GprSize) -> &'static str {
        match size {
            GprSize::B64 => match self {
                Self::Rax => "rax", Self::Rcx => "rcx", Self::Rdx => "rdx", Self::Rbx => "rbx",
                Self::Rsp => "rsp", Self::Rbp => "rbp", Self::Rsi => "rsi", Self::Rdi => "rdi",
                Self::R8  => "r8",  Self::R9  => "r9",  Self::R10 => "r10", Self::R11 => "r11",
                Self::R12 => "r12", Self::R13 => "r13", Self::R14 => "r14", Self::R15 => "r15",
            },
            GprSize::B32 => match self {
                Self::Rax => "eax",  Self::Rcx => "ecx",  Self::Rdx => "edx",  Self::Rbx => "ebx",
                Self::Rsp => "esp",  Self::Rbp => "ebp",  Self::Rsi => "esi",  Self::Rdi => "edi",
                Self::R8  => "r8d",  Self::R9  => "r9d",  Self::R10 => "r10d", Self::R11 => "r11d",
                Self::R12 => "r12d", Self::R13 => "r13d", Self::R14 => "r14d", Self::R15 => "r15d",
            },
            GprSize::B16 => match self {
                Self::Rax => "ax",   Self::Rcx => "cx",   Self::Rdx => "dx",   Self::Rbx => "bx",
                Self::Rsp => "sp",   Self::Rbp => "bp",   Self::Rsi => "si",   Self::Rdi => "di",
                Self::R8  => "r8w",  Self::R9  => "r9w",  Self::R10 => "r10w", Self::R11 => "r11w",
                Self::R12 => "r12w", Self::R13 => "r13w", Self::R14 => "r14w", Self::R15 => "r15w",
            },
            GprSize::B8 => match self {
                Self::Rax => "al",   Self::Rcx => "cl",   Self::Rdx => "dl",   Self::Rbx => "bl",
                Self::Rsp => "spl",  Self::Rbp => "bpl",  Self::Rsi => "sil",  Self::Rdi => "dil",
                Self::R8  => "r8b",  Self::R9  => "r9b",  Self::R10 => "r10b", Self::R11 => "r11b",
                Self::R12 => "r12b", Self::R13 => "r13b", Self::R14 => "r14b", Self::R15 => "r15b",
            },
        }
    }
}

/// Operand width for General Purpose Registers.
///
/// Hardware rule: writing a 32-bit GPR (`GprSize::B32`) implicitly zero-extends into
/// the upper 32 bits of the 64-bit register, whereas writing an 8-bit (`B8`) or 16-bit (`B16`)
/// sub-register preserves the upper bits (creating a partial-register merge or false dependency).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum GprSize {
    B8  = 1,
    B16 = 2,
    B32 = 4,
    B64 = 8,
}

impl GprSize {
    /// Returns `true` if writing a register of this size zeroes bits `32..64` in hardware.
    #[inline]
    pub const fn zero_extends_to_64(self) -> bool {
        matches!(self, Self::B32)
    }

    /// Returns `true` if writing a register of this size leaves upper bits unmodified.
    #[inline]
    pub const fn merges_upper_bits(self) -> bool {
        matches!(self, Self::B8 | Self::B16)
    }
}

/// Legacy high-byte 8-bit sub-registers (`AH`, `CH`, `DH`, `BH`).
///
/// Hardware rule: These 4 registers encode as IDs `4..=7` *only* when NO `REX` prefix is present.
/// They cannot be encoded alongside any `REX` prefix in x86-64 silicon.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum HighByteReg {
    Ah = 4,
    Ch = 5,
    Dh = 6,
    Bh = 7,
}

impl HighByteReg {
    /// Returns the 64-bit architectural parent register that aliases this high byte (`bits 8..16`).
    #[inline]
    pub const fn parent_gpr(self) -> Gpr {
        match self {
            Self::Ah => Gpr::Rax,
            Self::Ch => Gpr::Rcx,
            Self::Dh => Gpr::Rdx,
            Self::Bh => Gpr::Rbx,
        }
    }
}

/// Compact 16-bit bitmask representing a set of `Gpr` registers (`Rax`..`R15`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct GprMask(pub u16);

impl GprMask {
    pub const EMPTY: Self = Self(0);
    pub const ALL: Self = Self(0xFFFF);

    /// Creates a mask containing a single `Gpr`.
    #[inline]
    pub const fn single(reg: Gpr) -> Self {
        Self(1u16 << (reg as u8))
    }

    /// Creates a mask from a slice of `Gpr`s at compile time.
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

    #[inline]
    pub const fn without(self, reg: Gpr) -> Self {
        Self(self.0 & !(1u16 << (reg as u8)))
    }

    #[inline]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }
}

/// Vector / SIMD register index (`0..=31`), shared by `XMM`, `YMM`, and `ZMM` banks.
///
/// - `0..=15` (`Xmm0..Xmm15` / `Ymm0..Ymm15`): available in baseline SSE/AVX.
/// - `16..=31` (`Xmm16..Xmm31` / `Ymm16..Ymm31` / `Zmm0..Zmm31`): require AVX-512 `EVEX`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SimdReg {
    V0 = 0,   V1 = 1,   V2 = 2,   V3 = 3,   V4 = 4,   V5 = 5,   V6 = 6,   V7 = 7,
    V8 = 8,   V9 = 9,   V10 = 10, V11 = 11, V12 = 12, V13 = 13, V14 = 14, V15 = 15,
    V16 = 16, V17 = 17, V18 = 18, V19 = 19, V20 = 20, V21 = 21, V22 = 22, V23 = 23,
    V24 = 24, V25 = 25, V26 = 26, V27 = 27, V28 = 28, V29 = 29, V30 = 30, V31 = 31,
}

impl SimdReg {
    pub const XMM0: Self = Self::V0;
    pub const XMM1: Self = Self::V1;
    pub const XMM2: Self = Self::V2;
    pub const XMM3: Self = Self::V3;
    pub const XMM4: Self = Self::V4;
    pub const XMM5: Self = Self::V5;
    pub const XMM6: Self = Self::V6;
    pub const XMM7: Self = Self::V7;
    pub const XMM8: Self = Self::V8;
    pub const XMM9: Self = Self::V9;
    pub const XMM10: Self = Self::V10;
    pub const XMM11: Self = Self::V11;
    pub const XMM12: Self = Self::V12;
    pub const XMM13: Self = Self::V13;
    pub const XMM14: Self = Self::V14;
    pub const XMM15: Self = Self::V15;

    #[inline]
    pub const fn hw_id(self) -> u8 {
        self as u8
    }

    #[inline]
    pub const fn low3(self) -> u8 {
        (self as u8) & 0b0111
    }

    #[inline]
    pub const fn rex_bit(self) -> u8 {
        ((self as u8) >> 3) & 1
    }

    #[inline]
    pub const fn needs_rex_ext(self) -> bool {
        (self as u8) >= 8
    }

    #[inline]
    pub const fn needs_evex(self) -> bool {
        (self as u8) >= 16
    }
}

/// Vector register width (`128-bit XMM`, `256-bit YMM`, `512-bit ZMM`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum VecWidth {
    Xmm128 = 16,
    Ymm256 = 32,
    Zmm512 = 64,
}

/// AVX-512 Opmask Registers (`k0`..`k7`).
/// Note: `k0` when used as a predicate mask means "unmasked" in hardware.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum KReg {
    K0 = 0, K1 = 1, K2 = 2, K3 = 3,
    K4 = 4, K5 = 5, K6 = 6, K7 = 7,
}

/// Physical register bank classes on x86-64 silicon.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RegClass {
    /// General Purpose Integer Registers (`RAX`..`R15`)
    Gpr = 0,
    /// Vector / Floating-Point Registers (`XMM` / `YMM` / `ZMM`)
    Simd = 1,
    /// AVX-512 Predicate Mask Registers (`K0`..`K7`)
    Mask = 2,
    /// Architectural Status Flags (`EFLAGS` / `RFLAGS`)
    Flags = 3,
}
