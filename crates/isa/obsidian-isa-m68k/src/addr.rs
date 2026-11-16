//! Hardware 6-Bit Effective Address (`EffAddr`) and Brief Extension Word for Motorola 680x0 (`M68k`).
//!
//! Every M68k operand is described by a 6-bit Effective Address (`EA`) field:
//! - `mode` (3 bits, `0..=7`)
//! - `reg`  (3 bits, `0..=7`)
//! followed by `0`, `1`, or `2` 16-bit Big-Endian extension words (`0`, `2`, or `4` bytes).

use crate::regs::{AddrReg, DataReg, OpSize, Reg};

/// 2-bit index scale factor (`*1`, `*2`, `*4`, `*8`) in the M68k Brief Extension Word
/// (`bits 9..=10`; `*1` on baseline 68000, `*1`/`*2`/`*4`/`*8` on 68020+).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum IndexScale {
    S1 = 0, // * 1
    S2 = 1, // * 2
    S4 = 2, // * 4
    S8 = 3, // * 8
}

impl IndexScale {
    #[inline]
    pub const fn factor(self) -> u8 {
        1u8 << (self as u8)
    }

    #[inline]
    pub const fn ext_bits(self) -> u8 {
        self as u8
    }
}

/// Indexed addressing displacement + index register (`(d8, An, Xn.size * scale)` or `(d8, PC, Xn.size * scale)`),
/// which packs into a single 16-bit **Brief Extension Word**:
/// `[D/A:1][Xn:3][W/L:1][Scale:2][0:1][d8:8]`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct BriefIndex {
    /// Index register (`D0..D7` or `A0..A7`).
    pub index_reg: Reg,
    /// `true` if the full 32-bit index register is added (`.l`); `false` if sign-extended low 16-bit word (`.w`).
    pub index_is_long: bool,
    /// Scale multiplier (`*1`, `*2`, `*4`, `*8`).
    pub scale: IndexScale,
    /// Signed 8-bit displacement (`-128..=127`).
    pub disp8: i8,
}

impl BriefIndex {
    /// Packs this indexed operand into its exact 16-bit M68k hardware Brief Extension Word.
    #[inline]
    pub const fn to_extension_word(self) -> u16 {
        let da = (self.index_reg.da_bit() as u16) << 15;
        let reg = (self.index_reg.low3() as u16) << 12;
        let wl = (if self.index_is_long { 1u16 } else { 0u16 }) << 11;
        let sc = (self.scale.ext_bits() as u16) << 9;
        let d8 = (self.disp8 as u8) as u16;
        da | reg | wl | sc | d8
    }
}

/// Hardware M68k 6-bit Effective Address (`EA`) modes.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EffAddr {
    /// `Dn` — Data Register Direct (`mode = 0b000, reg = Dn`, 0 ext bytes).
    DataDirect(DataReg),
    /// `An` — Address Register Direct (`mode = 0b001, reg = An`, 0 ext bytes).
    AddrDirect(AddrReg),
    /// `(An)` — Address Register Indirect (`mode = 0b010, reg = An`, 0 ext bytes).
    AddrIndirect(AddrReg),
    /// `(An)+` — Address Register Indirect with Post-Increment (`mode = 0b011, reg = An`, 0 ext bytes).
    AddrPostInc(AddrReg),
    /// `-(An)` — Address Register Indirect with Pre-Decrement (`mode = 0b100, reg = An`, 0 ext bytes).
    AddrPreDec(AddrReg),
    /// `(d16, An)` — Address Register Indirect with 16-bit Signed Displacement (`mode = 0b101, reg = An`, 2 ext bytes).
    AddrDisp16(AddrReg, i16),
    /// `(d8, An, Xn.size * scale)` — Address Register Indirect with Index (`mode = 0b110, reg = An`, 2 ext bytes).
    AddrIndex(AddrReg, BriefIndex),
    /// `(xxx).W` — Absolute Short 16-bit sign-extended address (`mode = 0b111, reg = 0b000`, 2 ext bytes).
    AbsShort(i16),
    /// `(xxx).L` — Absolute Long 32-bit address (`mode = 0b111, reg = 0b001`, 4 ext bytes).
    AbsLong(u32),
    /// `(d16, PC)` — PC-Relative with 16-bit Signed Displacement (`mode = 0b111, reg = 0b010`, 2 ext bytes).
    PcDisp16(i16),
    /// `(d8, PC, Xn.size * scale)` — PC-Relative with Index (`mode = 0b111, reg = 0b011`, 2 ext bytes).
    PcIndex(BriefIndex),
    /// `#imm` — Immediate Data (`mode = 0b111, reg = 0b100`; 2 ext bytes for `Byte`/`Word`, 4 ext bytes for `Long`).
    Immediate(u32),
}

impl EffAddr {
    /// Returns the 3-bit hardware `mode` field (`0..=7`).
    #[inline]
    pub const fn mode_bits(&self) -> u8 {
        match *self {
            Self::DataDirect(_)    => 0b000,
            Self::AddrDirect(_)    => 0b001,
            Self::AddrIndirect(_)  => 0b010,
            Self::AddrPostInc(_)   => 0b011,
            Self::AddrPreDec(_)    => 0b100,
            Self::AddrDisp16(_, _) => 0b101,
            Self::AddrIndex(_, _)  => 0b110,
            Self::AbsShort(_)
            | Self::AbsLong(_)
            | Self::PcDisp16(_)
            | Self::PcIndex(_)
            | Self::Immediate(_)   => 0b111,
        }
    }

    /// Returns the 3-bit hardware `reg` field (`0..=7`).
    #[inline]
    pub const fn reg_bits(&self) -> u8 {
        match *self {
            Self::DataDirect(d)       => d.hw_id(),
            Self::AddrDirect(a)
            | Self::AddrIndirect(a)
            | Self::AddrPostInc(a)
            | Self::AddrPreDec(a)
            | Self::AddrDisp16(a, _)
            | Self::AddrIndex(a, _)   => a.hw_id(),
            Self::AbsShort(_)         => 0b000,
            Self::AbsLong(_)          => 0b001,
            Self::PcDisp16(_)         => 0b010,
            Self::PcIndex(_)          => 0b011,
            Self::Immediate(_)        => 0b100,
        }
    }

    /// Returns the standard 6-bit Effective Address field `(mode << 3) | reg` (`0..=63`)
    /// placed in bits `0..=5` of most M68k instruction words.
    #[inline]
    pub const fn ea6_mode_reg(&self) -> u8 {
        (self.mode_bits() << 3) | self.reg_bits()
    }

    /// Returns the reversed 6-bit Effective Address field `(reg << 3) | mode` (`0..=63`)
    /// required by the **destination** operand of the M68k `MOVE` instruction (`bits 6..=11`)!
    #[inline]
    pub const fn move_dst_ea6_reg_mode(&self) -> u8 {
        (self.reg_bits() << 3) | self.mode_bits()
    }

    /// Returns the exact number of extension bytes (`0`, `2`, or `4`) appended after the opcode word
    /// for this Effective Address mode and operand size (`op_size`).
    #[inline]
    pub const fn extension_byte_len(&self, op_size: OpSize) -> u8 {
        match *self {
            Self::DataDirect(_)
            | Self::AddrDirect(_)
            | Self::AddrIndirect(_)
            | Self::AddrPostInc(_)
            | Self::AddrPreDec(_) => 0,
            Self::AddrDisp16(_, _)
            | Self::AddrIndex(_, _)
            | Self::AbsShort(_)
            | Self::PcDisp16(_)
            | Self::PcIndex(_) => 2,
            Self::AbsLong(_) => 4,
            Self::Immediate(_) => match op_size {
                // Note: Byte immediates on M68k occupy a full 2-byte extension word (low byte holds the imm8)!
                OpSize::Byte | OpSize::Word => 2,
                OpSize::Long => 4,
            },
        }
    }

    /// Returns `true` if this `EffAddr` is a legal **Control Addressing Mode**
    /// (required by `LEA`, `PEA`, `JSR`, and `JMP`, which compute an address without dereferencing it).
    ///
    /// Control modes are: `(An)`, `(d16, An)`, `(d8, An, Xn)`, `(xxx).W`, `(xxx).L`, `(d16, PC)`, `(d8, PC, Xn)`.
    /// Register direct (`Dn`/`An`), `(An)+`, `-(An)`, and `#imm` are illegal in `LEA`/`PEA`/`JMP`/`JSR`!
    #[inline]
    pub const fn is_control_addressing_mode(&self) -> bool {
        matches!(
            *self,
            Self::AddrIndirect(_)
                | Self::AddrDisp16(_, _)
                | Self::AddrIndex(_, _)
                | Self::AbsShort(_)
                | Self::AbsLong(_)
                | Self::PcDisp16(_)
                | Self::PcIndex(_)
        )
    }

    /// Returns `true` if this `EffAddr` is a legal **Alterable Destination** (writable by `MOVE`, `ADD`, `CLR`, etc.).
    ///
    /// PC-relative modes and `#imm` are read-only in M68k silicon!
    #[inline]
    pub const fn is_alterable(&self) -> bool {
        !matches!(
            *self,
            Self::PcDisp16(_) | Self::PcIndex(_) | Self::Immediate(_)
        )
    }
}
