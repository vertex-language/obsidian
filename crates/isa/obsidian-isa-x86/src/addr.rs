//! Hardware memory addressing (`ModR/M` + `SIB` + Displacement) for x86-64.
//!
//! Represents only legal silicon addressing forms:
//! 1. `[base + index * scale + disp]`
//! 2. `[rip + disp32]` (64-bit PC-relative addressing)
//! 3. `[disp32]` (absolute 32-bit sign-extended SIB addressing)

use crate::regs::Gpr;

/// 2-bit `SIB.scale` field (`0..=3`), encoding index multipliers `1`, `2`, `4`, or `8`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Scale {
    /// `* 1` (`SIB.ss = 0b00`)
    S1 = 0,
    /// `* 2` (`SIB.ss = 0b01`)
    S2 = 1,
    /// `* 4` (`SIB.ss = 0b10`)
    S4 = 2,
    /// `* 8` (`SIB.ss = 0b11`)
    S8 = 3,
}

impl Scale {
    /// Convert a byte multiplier (`1`, `2`, `4`, `8`) to its 2-bit `Scale`.
    #[inline]
    pub const fn from_factor(factor: u8) -> Option<Self> {
        match factor {
            1 => Some(Self::S1),
            2 => Some(Self::S2),
            4 => Some(Self::S4),
            8 => Some(Self::S8),
            _ => None,
        }
    }

    /// Returns the numeric multiplier (`1`, `2`, `4`, or `8`).
    #[inline]
    pub const fn factor(self) -> u8 {
        1u8 << (self as u8)
    }

    /// Returns the 2-bit `SIB.ss` field (`0..=3`).
    #[inline]
    pub const fn sib_bits(self) -> u8 {
        self as u8
    }
}

/// Displacement encoding control for a memory operand.
///
/// While `Disp::Auto(offset)` selects the shortest legal displacement (`0`, `1`, or `4` bytes),
/// `Disp::ForceDisp8` and `Disp::ForceDisp32` allow later SSA packing passes (such as the
/// 32-byte $\mu\text{op}$ DSB window aligner) to request the standard hardware `disp8` (`mod=01`)
/// or `disp32` (`mod=10`) encoding to adjust instruction byte length without emitting `NOP`s.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Disp {
    /// Automatically pick the shortest hardware-legal displacement (`0`, `1`, or `4` bytes).
    Auto(i32),
    /// Explicitly encode a 1-byte sign-extended displacement (`ModR/M.mod = 0b01`).
    ForceDisp8(i8),
    /// Explicitly encode a 4-byte displacement (`ModR/M.mod = 0b10`).
    ForceDisp32(i32),
}

impl Disp {
    pub const ZERO: Self = Self::Auto(0);

    #[inline]
    pub const fn value(self) -> i32 {
        match self {
            Self::Auto(v) => v,
            Self::ForceDisp8(v) => v as i32,
            Self::ForceDisp32(v) => v,
        }
    }
}

/// Hardware x86-64 memory operand parameterized by register type `Reg`
/// (use `Gpr` for physical registers or `ValueId` in SSA before register allocation).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MemoryOperand<Reg = Gpr> {
    /// General base/index/scale/displacement `[base + index * scale + disp]`.
    BaseIndex {
        base: Option<Reg>,
        index: Option<(Reg, Scale)>,
        disp: Disp,
    },
    /// 64-bit RIP-relative addressing `[rip + disp32]` (`ModR/M.mod = 0b00, rm = 0b101`).
    RipRelative {
        disp32: i32,
    },
}

impl<Reg: Copy> MemoryOperand<Reg> {
    /// `[base]` (0 displacement, no index).
    #[inline]
    pub const fn base(base: Reg) -> Self {
        Self::BaseIndex {
            base: Some(base),
            index: None,
            disp: Disp::ZERO,
        }
    }

    /// `[base + disp]` (base register + auto-sized signed offset).
    #[inline]
    pub const fn base_disp(base: Reg, disp: i32) -> Self {
        Self::BaseIndex {
            base: Some(base),
            index: None,
            disp: Disp::Auto(disp),
        }
    }

    /// `[base + index * scale + disp]` (full 3-component indexed address).
    #[inline]
    pub const fn base_index_disp(base: Reg, index: Reg, scale: Scale, disp: i32) -> Self {
        Self::BaseIndex {
            base: Some(base),
            index: Some((index, scale)),
            disp: Disp::Auto(disp),
        }
    }

    /// `[rip + disp32]` (PC-relative memory operand).
    #[inline]
    pub const fn rip_rel(disp32: i32) -> Self {
        Self::RipRelative { disp32 }
    }

    /// Returns `true` if this memory operand includes a scaled index register (`[... + index * scale]`).
    #[inline]
    pub const fn is_indexed(&self) -> bool {
        matches!(self, Self::BaseIndex { index: Some(_), .. })
    }
}

impl MemoryOperand<Gpr> {
    /// Validates whether this physical register combination is legal in x86-64 silicon.
    ///
    /// Specifically, `RSP` (`hw_id == 4`) cannot be used as an index register in a `SIB` byte.
    #[inline]
    pub const fn is_legal_in_silicon(&self) -> bool {
        match *self {
            Self::BaseIndex {
                index: Some((idx, _)),
                ..
            } => idx.can_be_index(),
            _ => true,
        }
    }

    /// Returns `true` if encoding this memory operand requires a `SIB` byte after `ModR/M`.
    ///
    /// Silicon rules requiring a `SIB` byte:
    /// 1. Any index register is present (`[base + index * scale]`), OR
    /// 2. No base register is present (`[disp32]` absolute addressing), OR
    /// 3. The base register is `RSP` (`4`) or `R12` (`12`) (`base.low3() == 0b100`).
    #[inline]
    pub const fn requires_sib(&self) -> bool {
        match *self {
            Self::RipRelative { .. } => false,
            Self::BaseIndex { base, index, .. } => {
                if index.is_some() {
                    return true;
                }
                match base {
                    None => true,
                    Some(b) => b.base_requires_sib(),
                }
            }
        }
    }

    /// Returns the exact number of displacement bytes (`0`, `1`, or `4`) emitted in silicon.
    #[inline]
    pub const fn disp_byte_len(&self) -> u8 {
        match *self {
            Self::RipRelative { .. } => 4,
            Self::BaseIndex { base: None, .. } => 4, // Baseless SIB always uses disp32 (mod=00, base=101)
            Self::BaseIndex {
                base: Some(b),
                disp,
                ..
            } => match disp {
                Disp::ForceDisp8(_) => 1,
                Disp::ForceDisp32(_) => 4,
                Disp::Auto(0) => {
                    // Hardware quirk: [rbp] and [r13] cannot use 0-byte disp; silicon forces disp8=0!
                    if b.base_requires_disp8_zero() {
                        1
                    } else {
                        0
                    }
                }
                Disp::Auto(d) => {
                    if d >= (i8::MIN as i32) && d <= (i8::MAX as i32) {
                        1
                    } else {
                        4
                    }
                }
            },
        }
    }

    /// Returns the total byte length of `ModR/M + optional SIB + displacement` (`1..=6` bytes).
    #[inline]
    pub const fn addressing_byte_len(&self) -> u8 {
        1 + (if self.requires_sib() { 1 } else { 0 }) + self.disp_byte_len()
    }
}
