//! Real Motorola 680x0 (`M68k`) silicon opcodes (`Opcode`) and static hardware descriptors (`InstDesc`).
//!
//! **Strict Rule:** Zero compiler pseudo-instructions exist in this module.
//! Every variant in [`Opcode`] is a real hardware instruction decoded by M68k silicon.

use crate::cond::CcrFlags;

/// Static hardware instruction descriptor from the *Motorola M68000 Family Programmer's Reference Manual*.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstDesc {
    /// Standard assembly mnemonic (`"moveq"`, `"lea"`, `"addq"`, `"movem"`, `"dbcc"`, etc.).
    pub mnemonic: &'static str,
    /// Base number of 16-bit Big-Endian opcode words (`1` = 2 bytes, `2` = 4 bytes) before any EA extension words.
    pub base_words: u8,
    /// `true` if `dst` is destructively tied to `src0` (`dst = dst op src`).
    pub tied_dst_src0: bool,
    /// `CCR` status flags (`X`, `N`, `Z`, `V`, `C`) read by this instruction.
    pub flags_read: CcrFlags,
    /// `CCR` status flags (`X`, `N`, `Z`, `V`, `C`) modified or clobbered by this instruction.
    pub flags_written: CcrFlags,
    /// `true` if this instruction writes exclusively to an Address Register (`A0..A7`),
    /// which in M68k silicon sign-extends 16-bit `.w` results to 32 bits and never modifies `CCR`.
    pub targets_addr_reg: bool,
}

/// Real Motorola 680x0 (`M68k`) silicon instructions.
#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Opcode {
    // =========================================================================
    // 1. Data & Address Movement (`MOVE`, `MOVEA`, `MOVEQ`, `MOVEM`, `LEA`, `PEA`, `EXG`, `SWAP`)
    // =========================================================================
    /// `move.<b/w/l> <ea_src>, <ea_dst>` (general memory/register move; updates `N,Z,V,C`, preserves `X`!)
    Move,
    /// `movea.<w/l> <ea_src>, An` (move into Address Register `An`; sign-extends `.w` to 32 bits, 0 `CCR` change!)
    Movea,
    /// `moveq #imm8, Dn` (2-byte quick move: sign-extends `-128..=127` to 32-bit `Dn`, updates `N,Z,V,C`)
    Moveq,
    /// `movem.<w/l> <reg_list>, <ea>` (store multiple registers via 16-bit mask, 0 `CCR` change)
    MovemToMem,
    /// `movem.<w/l> <ea>, <reg_list>` (load multiple registers via 16-bit mask, 0 `CCR` change)
    MovemFromMem,
    /// `lea <ea_ctrl>, An` (load 32-bit effective address into `An`, 0 `CCR` change)
    Lea,
    /// `pea <ea_ctrl>` (push 32-bit effective address onto stack `-(sp)`, 0 `CCR` change)
    Pea,
    /// `exg Rx, Ry` (exchange two 32-bit registers in 2 bytes, 0 `CCR` change)
    Exg,
    /// `swap Dn` (swap upper and lower 16-bit halves of `Dn`, updates `N,Z,V,C`)
    Swap,

    // =========================================================================
    // 2. Integer Arithmetic (`ADD`, `ADDA`, `ADDQ`, `ADDX`, `SUB`, `SUBA`, `SUBQ`, `SUBX`, `CMP`, `CMPA`)
    // =========================================================================
    /// `add.<b/w/l> <ea>, Dn` or `Dn, <ea>` (writes all 5 `X,N,Z,V,C` flags)
    Add,
    /// `adda.<w/l> <ea>, An` (address addition into `An`; sign-extends `.w` to 32 bits, 0 `CCR` change!)
    Adda,
    /// `addq.<b/w/l> #1..8, <ea>` (2-byte quick add of immediate `1..=8`; 0 `CCR` change when `<ea>` is `An`!)
    Addq,
    /// `addx.<b/w/l> Dy, Dx` (multi-precision add with Extend flag `X`)
    Addx,
    /// `sub.<b/w/l> <ea>, Dn` or `Dn, <ea>` (writes all 5 `X,N,Z,V,C` flags)
    Sub,
    /// `suba.<w/l> <ea>, An` (address subtraction into `An`; sign-extends `.w` to 32 bits, 0 `CCR` change!)
    Suba,
    /// `subq.<b/w/l> #1..8, <ea>` (2-byte quick subtract of immediate `1..=8`; 0 `CCR` change when `<ea>` is `An`!)
    Subq,
    /// `subx.<b/w/l> Dy, Dx` (multi-precision subtract with Extend flag `X`)
    Subx,
    /// `cmp.<b/w/l> <ea>, Dn` (compares `Dn - <ea>`, updates `N,Z,V,C`, preserves `X`!)
    Cmp,
    /// `cmpa.<w/l> <ea>, An` (compares `An - sign_ext(<ea>)`, updates `N,Z,V,C`, preserves `X`!)
    Cmpa,
    /// `tst.<b/w/l> <ea>` (compares `<ea>` with 0, updates `N,Z,V,C`, preserves `X`!)
    Tst,

    // =========================================================================
    // 3. Logical, Unary, Sign-Extension & Multiply/Divide Operations
    // =========================================================================
    And,
    Or,
    Eor,
    Not,
    Neg,
    Negx,
    Clr,
    /// `ext.w Dn` (sign-extend byte `bits 0..7` to 16-bit word `bits 0..15` in `Dn`)
    ExtW,
    /// `ext.l Dn` (sign-extend 16-bit word `bits 0..15` to 32-bit long `bits 0..31` in `Dn`)
    ExtL,
    /// `extb.l Dn` (68020+: sign-extend byte `bits 0..7` directly to 32-bit long in `Dn`)
    ExtbL,
    /// `muls.w <ea>, Dn` (signed 16x16 -> 32-bit multiply into `Dn`)
    MulsW,
    /// `mulu.w <ea>, Dn` (unsigned 16x16 -> 32-bit multiply into `Dn`)
    MuluW,
    /// `muls.l <ea>, Dl` (68020+: signed 32x32 -> 32/64-bit multiply, 2 base words)
    MulsL,
    /// `mulu.l <ea>, Dl` (68020+: unsigned 32x32 -> 32/64-bit multiply, 2 base words)
    MuluL,
    /// `divs.w <ea>, Dn` (signed 32/16 divide -> 16-bit rem in high word, 16-bit quot in low word of `Dn`)
    DivsW,
    /// `divu.w <ea>, Dn` (unsigned 32/16 divide -> 16-bit rem in high word, 16-bit quot in low word of `Dn`)
    DivuW,

    // =========================================================================
    // 4. Shifts, Rotates & Bit Manipulation (`BTST`, `BSET`, `BCLR`, `BCHG`)
    // =========================================================================
    Lsl,
    Lsr,
    Asl,
    Asr,
    Rol,
    Ror,
    /// `btst Dn/#imm, <ea>` (tests a single bit and sets `CCR.Z = !bit`; leaves `X,N,V,C` untouched!)
    Btst,
    /// `bset Dn/#imm, <ea>` (tests a bit into `CCR.Z` and sets that bit to 1)
    Bset,
    /// `bclr Dn/#imm, <ea>` (tests a bit into `CCR.Z` and clears that bit to 0)
    Bclr,
    /// `bchg Dn/#imm, <ea>` (tests a bit into `CCR.Z` and inverts that bit)
    Bchg,

    // =========================================================================
    // 5. Control Flow, Loop Primitives (`DBcc`) & Stack Frame (`LINK` / `UNLK`)
    // =========================================================================
    /// `bcc.<b/w/l> label` (conditional branch on 4-bit `CondCode`, 0 `CCR` change)
    Bcc,
    /// `bra.<b/w/l> label` (unconditional PC-relative branch, `CondCode::T`, 0 `CCR` change)
    Bra,
    /// `bsr.<b/w/l> label` (PC-relative subroutine call pushing return address to `-(sp)`)
    Bsr,
    /// `dbcc Dn, disp16` (test condition, decrement 16-bit `Dn.w`, and branch if `Dn.w != -1`)
    Dbcc,
    /// `scc <ea>` (set byte at `<ea>` to `0xFF` if `CondCode` is true, or `0x00` if false)
    Scc,
    /// `jsr <ea_ctrl>` (jump to subroutine pushing return address to `-(sp)`)
    Jsr,
    /// `jmp <ea_ctrl>` (unconditional jump to effective address)
    Jmp,
    /// `rts` (return from subroutine popping PC from `(sp)+`, `0x4E75`, 2 bytes)
    Rts,
    /// `link.w An, #disp16` (push `An` to `-(sp)`, copy `sp` to `An`, add `disp16` to `sp`, 4 bytes)
    LinkW,
    /// `unlk An` (copy `An` to `sp` and pop saved frame pointer into `An`, `0x4E58 + An`, 2 bytes)
    Unlk,
    /// `nop` (`0x4E71`, 2 bytes)
    Nop,
    /// `trap #vector4` (software exception / system call `0x4E40 + vec4`, 2 bytes)
    Trap,
}

impl Opcode {
    /// Returns the static hardware descriptor (`InstDesc`) for this M68k opcode.
    #[inline]
    pub const fn desc(self) -> InstDesc {
        const NONE: CcrFlags = CcrFlags::NONE;
        const NZVC: CcrFlags = CcrFlags::NZVC;
        const XNZVC: CcrFlags = CcrFlags::XNZVC;

        match self {
            // --- Moves & Address Generation ---
            Self::Move | Self::Moveq | Self::Swap | Self::Clr => InstDesc {
                mnemonic: match self {
                    Self::Move => "move",
                    Self::Moveq => "moveq",
                    Self::Swap => "swap",
                    _ => "clr",
                },
                base_words: 1,
                tied_dst_src0: matches!(self, Self::Swap),
                flags_read: NONE,
                flags_written: NZVC, // Preserves Extend (X) flag!
                targets_addr_reg: false,
            },
            Self::Movea | Self::Lea => InstDesc {
                mnemonic: if matches!(self, Self::Movea) { "movea" } else { "lea" },
                base_words: 1,
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE, // Targeting An NEVER touches CCR!
                targets_addr_reg: true,
            },
            Self::MovemToMem | Self::MovemFromMem => InstDesc {
                mnemonic: "movem",
                base_words: 2, // Opcode word + 16-bit register list mask word
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                targets_addr_reg: false,
            },
            Self::Pea | Self::Exg => InstDesc {
                mnemonic: if matches!(self, Self::Pea) { "pea" } else { "exg" },
                base_words: 1,
                tied_dst_src0: matches!(self, Self::Exg),
                flags_read: NONE,
                flags_written: NONE,
                targets_addr_reg: false,
            },

            // --- Integer Arithmetic ---
            Self::Add | Self::Sub | Self::Addq | Self::Subq | Self::Neg => InstDesc {
                mnemonic: match self {
                    Self::Add => "add",
                    Self::Sub => "sub",
                    Self::Addq => "addq",
                    Self::Subq => "subq",
                    _ => "neg",
                },
                base_words: 1,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: XNZVC, // Updates all 5 flags including Extend (X)
                targets_addr_reg: false,
            },
            Self::Adda | Self::Suba => InstDesc {
                mnemonic: if matches!(self, Self::Adda) { "adda" } else { "suba" },
                base_words: 1,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: NONE, // Address register arithmetic NEVER modifies CCR!
                targets_addr_reg: true,
            },
            Self::Addx | Self::Subx | Self::Negx => InstDesc {
                mnemonic: match self {
                    Self::Addx => "addx",
                    Self::Subx => "subx",
                    _ => "negx",
                },
                base_words: 1,
                tied_dst_src0: true,
                flags_read: CcrFlags::X.union(CcrFlags::Z),
                flags_written: XNZVC,
                targets_addr_reg: false,
            },
            Self::Cmp | Self::Cmpa | Self::Tst => InstDesc {
                mnemonic: match self {
                    Self::Cmp => "cmp",
                    Self::Cmpa => "cmpa",
                    _ => "tst",
                },
                base_words: 1,
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NZVC, // Comparisons never modify Extend (X)!
                targets_addr_reg: false,
            },

            // --- Logical, Sign Extension & Mul/Div ---
            Self::And | Self::Or | Self::Eor | Self::Not | Self::ExtW | Self::ExtL | Self::ExtbL
            | Self::MulsW | Self::MuluW | Self::DivsW | Self::DivuW => InstDesc {
                mnemonic: "logic_mul_ext",
                base_words: 1,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: NZVC,
                targets_addr_reg: false,
            },
            Self::MulsL | Self::MuluL => InstDesc {
                mnemonic: if matches!(self, Self::MulsL) { "muls.l" } else { "mulu.l" },
                base_words: 2,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: NZVC,
                targets_addr_reg: false,
            },

            // --- Shifts, Rotates & Bit Manipulation ---
            Self::Lsl | Self::Lsr | Self::Asl | Self::Asr => InstDesc {
                mnemonic: "shift",
                base_words: 1,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: XNZVC,
                targets_addr_reg: false,
            },
            Self::Rol | Self::Ror => InstDesc {
                mnemonic: "rotate",
                base_words: 1,
                tied_dst_src0: true,
                flags_read: NONE,
                flags_written: NZVC, // ROL/ROR do NOT modify Extend (X)!
                targets_addr_reg: false,
            },
            Self::Btst | Self::Bset | Self::Bclr | Self::Bchg => InstDesc {
                mnemonic: "bit_op",
                base_words: 1,
                tied_dst_src0: !matches!(self, Self::Btst),
                flags_read: NONE,
                flags_written: CcrFlags::Z, // Only modifies the Zero (Z) flag!
                targets_addr_reg: false,
            },

            // --- Control Flow & Stack Frame ---
            Self::Bcc | Self::Dbcc | Self::Scc => InstDesc {
                mnemonic: match self {
                    Self::Bcc => "bcc",
                    Self::Dbcc => "dbcc",
                    _ => "scc",
                },
                base_words: if matches!(self, Self::Dbcc) { 2 } else { 1 },
                tied_dst_src0: matches!(self, Self::Dbcc),
                flags_read: NZVC,
                flags_written: NONE,
                targets_addr_reg: false,
            },
            Self::Bra | Self::Bsr | Self::Jsr | Self::Jmp | Self::Rts | Self::LinkW | Self::Unlk | Self::Nop | Self::Trap => InstDesc {
                mnemonic: "ctrl",
                base_words: if matches!(self, Self::LinkW) { 2 } else { 1 },
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                targets_addr_reg: false,
            },
        }
    }
}
