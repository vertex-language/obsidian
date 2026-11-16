//! Real IBM SystemZ (`s390x`) silicon opcodes (`Opcode`) and static hardware descriptors (`InstDesc`).
//!
//! **Strict Rule:** Zero compiler pseudo-instructions exist in this module.
//! Every variant in [`Opcode`] is a real hardware instruction decoded by SystemZ silicon.

/// Hardware instruction encoding format from the *IBM z/Architecture Principles of Operation*.
///
/// Every format has a fixed hardware byte length (`2`, `4`, or `6` bytes), which is also
/// encoded in the top 2 bits of the first opcode byte (`00` = 2B, `01`/`10` = 4B, `11` = 6B).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum InstFormat {
    /// 2-byte Register-to-Register (`[op:8][r1:4][r2:4]`)
    RR  = 2,
    /// 4-byte Extended Register-to-Register (`[op:16][//:8][r1:4][r2:4]`)
    RRE = 4,
    /// 4-byte 3-Operand Register-to-Register (`[op:16][r3:4][m4:4][r1:4][r2:4]`)
    RRF = 5,
    /// 4-byte Register-and-Indexed-Storage (`[op:8][r1:4][x2:4][b2:4][d2:12]`)
    RX  = 6,
    /// 4-byte Register-and-Immediate (`[op1:8][r1:4][op2:4][i2:16]`)
    RI  = 7,
    /// 6-byte Long-Displacement Register-and-Indexed-Storage (`[op1:8][r1:4][x2:4][b2:4][dl2:12][dh2:8][op2:8]`)
    RXY = 8,
    /// 6-byte Long-Displacement Register-and-Storage / Shift (`[op1:8][r1:4][r3:4][b2:4][dl2:12][dh2:8][op2:8]`)
    RSY = 9,
    /// 6-byte Long-Immediate / PC-Relative (`[op1:8][r1:4][op2:4][i2:32]`)
    RIL = 10,
    /// 6-byte Register-Immediate-Extended (`RISBG`, `CGRJ`, `CGIJ`)
    RIE = 11,
    /// 6-byte Storage-to-Storage (`MVC`, `XC`, `CLC`: `[op:8][l:8][b1:4][d1:12][b2:4][d2:12]`)
    SS  = 12,
    /// 6-byte Vector Register-to-Register (`[op1:8][v1:4][v2:4][v3:4][//:4][m:4][rxb:4][op2:8]`)
    VRR = 13,
    /// 6-byte Vector Register-and-Indexed-Storage (`VL`, `VST`)
    VRX = 14,
}

impl InstFormat {
    /// Returns the exact hardware instruction length in bytes (`2`, `4`, or `6`).
    #[inline]
    pub const fn byte_len(self) -> u8 {
        match self {
            Self::RR => 2,
            Self::RRE | Self::RRF | Self::RX | Self::RI => 4,
            Self::RXY | Self::RSY | Self::RIL | Self::RIE | Self::SS | Self::VRR | Self::VRX => 6,
        }
    }
}

/// Static hardware instruction descriptor from the *IBM z/Architecture Principles of Operation*.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstDesc {
    /// Standard assembly mnemonic (`"lgr"`, `"agrk"`, `"risbg"`, `"cgrj"`, `"mvc"`, etc.).
    pub mnemonic: &'static str,
    /// Hardware encoding format (`RR`, `RRE`, `RRF`, `RX`, `RXY`, `RSY`, `RIL`, `RIE`, `SS`, `VRR`).
    pub format: InstFormat,
    /// Exact instruction byte length (`2`, `4`, or `6` bytes).
    pub byte_len: u8,
    /// `true` if `dst` is destructively tied to `src0` (`r1 = r1 op r2`).
    pub tied_dst_src0: bool,
    /// `true` if this instruction reads the 2-bit `PSW` Condition Code (`CC`).
    pub reads_cc: bool,
    /// `true` if this instruction modifies the 2-bit `PSW` Condition Code (`CC`).
    pub writes_cc: bool,
    /// `true` if this is a 32-bit operation that modifies only the low 32 bits of `dst`
    /// while leaving the upper 32 bits (IBM bits `0..31`) unmodified!
    pub preserves_high32: bool,
    /// `true` if `dst` must be an even-numbered register anchoring an `Even:Odd` `GprPair`.
    pub requires_even_pair: bool,
}

/// Real IBM SystemZ (`s390x`) silicon instructions.
#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Opcode {
    // =========================================================================
    // 1. Register Moves, Sign/Zero Extensions & Conditional Moves
    // =========================================================================
    /// `lgr r1, r2` (64-bit register copy, `B904`, 4 bytes, 0 CC change)
    Lgr,
    /// `lr r1, r2` (32-bit low register copy, `18`, 2 bytes, preserves high 32 bits!)
    Lr,
    /// `lgfr r1, r2` (sign-extend 32-bit `r2` to 64-bit `r1`, `B914`, 4 bytes)
    Lgfr,
    /// `llgfr r1, r2` (zero-extend 32-bit `r2` to 64-bit `r1`, `B916`, 4 bytes)
    Llgfr,
    /// `lgbr r1, r2` (sign-extend low 8 bits of `r2` to 64-bit `r1`, `B906`, 4 bytes)
    Lgbr,
    /// `llgcr r1, r2` (zero-extend low 8 bits of `r2` to 64-bit `r1`, `B984`, 4 bytes)
    Llgcr,
    /// `lghr r1, r2` (sign-extend low 16 bits of `r2` to 64-bit `r1`, `B907`, 4 bytes)
    Lghr,
    /// `llghr r1, r2` (zero-extend low 16 bits of `r2` to 64-bit `r1`, `B985`, 4 bytes)
    Llghr,
    /// `locgr r1, r2, m3` (load on condition 64-bit, `B9E2`, 4 bytes, reads `CC`)
    Locgr,
    /// `lrvgr r1, r2` (64-bit byte-swap / load reversed, `B90F`, 4 bytes)
    Lrvgr,

    // =========================================================================
    // 2. Immediate Materialization & PC-Relative Address Generation
    // =========================================================================
    /// `lghi r1, imm16` (sign-extend 16-bit imm to 64-bit, `A7x9`, 4 bytes)
    Lghi,
    /// `lhi r1, imm16` (sign-extend 16-bit imm to low 32-bit, `A7x8`, 4 bytes, preserves high 32!)
    Lhi,
    /// `lgfi r1, imm32` (sign-extend 32-bit imm to 64-bit, `C0x1`, 6 bytes)
    Lgfi,
    /// `llihf r1, imm32` (load 32-bit logical imm into high 32 bits, zero low 32 bits, `C0xE`, 6 bytes)
    Llihf,
    /// `llilf r1, imm32` (load 32-bit logical imm into low 32 bits, zero high 32 bits, `C0xF`, 6 bytes)
    Llilf,
    /// `iihf r1, imm32` (insert 32-bit imm into high 32 bits, preserving low 32 bits, `C0x8`, 6 bytes)
    Iihf,
    /// `iilf r1, imm32` (insert 32-bit imm into low 32 bits, preserving high 32 bits, `C0x9`, 6 bytes)
    Iilf,
    /// `larl r1, rel32_hw` (PC-relative address calculation, `C0x0`, 6 bytes, 0 CC change)
    Larl,

    // =========================================================================
    // 3. Memory Loads, Stores & Non-Destructive Address Generation (`LA`/`LAY`)
    // =========================================================================
    /// `lg r1, d20(x2, b2)` (64-bit load, `E3..04`, 6 bytes)
    Lg,
    /// `l r1, d12(x2, b2)` (32-bit load with 12-bit unsigned disp, `58`, 4 bytes, preserves high 32!)
    L,
    /// `ly r1, d20(x2, b2)` (32-bit load with 20-bit signed disp, `E3..58`, 6 bytes, preserves high 32!)
    Ly,
    /// `lgf r1, d20(x2, b2)` (32-to-64 sign-extending load, `E3..14`, 6 bytes)
    Lgf,
    /// `llgf r1, d20(x2, b2)` (32-to-64 zero-extending load, `E3..16`, 6 bytes)
    Llgf,
    /// `llgc r1, d20(x2, b2)` (8-to-64 zero-extending byte load, `E3..90`, 6 bytes)
    Llgc,
    /// `stg r1, d20(x2, b2)` (64-bit store, `E3..24`, 6 bytes)
    Stg,
    /// `st r1, d12(x2, b2)` (32-bit store with 12-bit disp, `50`, 4 bytes)
    St,
    /// `sty r1, d20(x2, b2)` (32-bit store with 20-bit disp, `E3..50`, 6 bytes)
    Sty,
    /// `stc r1, d12(x2, b2)` (8-bit store with 12-bit disp, `42`, 4 bytes)
    Stc,
    /// `lmg r1, r3, d20(b2)` (load multiple 64-bit registers `r1..=r3`, `EB..04`, 6 bytes)
    Lmg,
    /// `stmg r1, r3, d20(b2)` (store multiple 64-bit registers `r1..=r3`, `EB..24`, 6 bytes)
    Stmg,
    /// `la r1, d12(x2, b2)` (load address `base + index + d12`, `41`, 4 bytes, 0 CC change, avoids AGI!)
    La,
    /// `lay r1, d20(x2, b2)` (load address `base + index + d20`, `E3..71`, 6 bytes, 0 CC change)
    Lay,

    // =========================================================================
    // 4. 2-Operand Destructive Integer ALU (`RRE` / `RR` / `RI` / `RIL` / `RXY`)
    // =========================================================================
    Agr, Ar, Aghi, Ahi, Agfi, Ag,
    Sgr, Sr, Sgf, Sg,
    Ngr, Nr, Nilf, Ng,
    Ogr, Or, Oilf, Og,
    Xgr, Xr, Xilf, Xg,
    /// `lcgr r1, r2` (64-bit two's-complement negation `r1 = -r2`, `B903`, 4 bytes, sets `CC`)
    Lcgr,

    // =========================================================================
    // 5. 3-Operand Non-Destructive Integer ALU (Distinct-Operands Facility `RRF`)
    // =========================================================================
    /// `agrk r1, r2, r3` (`r1 = r2 + r3`, 64-bit non-destructive add, `B9E8`, 4 bytes)
    Agrk,
    /// `ark r1, r2, r3` (`r1 = r2 + r3`, 32-bit non-destructive add, `B9F8`, 4 bytes)
    Ark,
    /// `sgrk r1, r2, r3` (`r1 = r2 - r3`, 64-bit non-destructive sub, `B9E9`, 4 bytes)
    Sgrk,
    /// `srk r1, r2, r3` (`r1 = r2 - r3`, 32-bit non-destructive sub, `B9F9`, 4 bytes)
    Srk,
    /// `ngrk r1, r2, r3` (`r1 = r2 & r3`, 64-bit non-destructive AND, `B9E4`, 4 bytes)
    Ngrk,
    /// `ogrk r1, r2, r3` (`r1 = r2 | r3`, 64-bit non-destructive OR, `B9E6`, 4 bytes)
    Ogrk,
    /// `xgrk r1, r2, r3` (`r1 = r2 ^ r3`, 64-bit non-destructive XOR, `B9E7`, 4 bytes)
    Xgrk,

    // =========================================================================
    // 6. Multiply, Divide & 128-Bit Even:Odd Pair Operations
    // =========================================================================
    /// `msgr r1, r2` (64-bit signed single-register multiply `r1 = r1 * r2`, `B90C`, 4 bytes, 0 CC change!)
    Msgr,
    /// `msr r1, r2` (32-bit signed multiply, `B252`, 4 bytes, 0 CC change)
    Msr,
    /// `mghi r1, imm16` (64-bit multiply by 16-bit immediate, `A7xD`, 4 bytes, 0 CC change)
    Mghi,
    /// `mlgr r1_even, r2` (unsigned 64x64 -> 128-bit multiply into `r1:r1+1`, `B986`, 4 bytes)
    Mlgr,
    /// `dlgr r1_even, r2` (unsigned 128/64 divide `r1:r1+1 / r2` -> rem `r1`, quot `r1+1`, `B987`, 4 bytes)
    Dlgr,
    /// `dsgr r1_even, r2` (signed 64/64 divide `r1+1 / r2` -> rem `r1`, quot `r1+1`, `B90D`, 4 bytes)
    Dsgr,

    // =========================================================================
    // 7. Non-Destructive 3-Operand Shifts, Rotate-Insert (`RISBG`) & Bit Scans
    // =========================================================================
    /// `sllg r1, r3, d20(b2)` (64-bit shift left logical `r1 = r3 << (b2 + d20)`, `EB..0D`, 6 bytes, 0 CC change!)
    Sllg,
    /// `srlg r1, r3, d20(b2)` (64-bit shift right logical `r1 = r3 >> (b2 + d20)`, `EB..0C`, 6 bytes, 0 CC change!)
    Srlg,
    /// `srag r1, r3, d20(b2)` (64-bit shift right arithmetic, `EB..0A`, 6 bytes, sets `CC`!)
    Srag,
    /// `rllg r1, r3, d20(b2)` (64-bit rotate left logical, `EB..1C`, 6 bytes, 0 CC change!)
    Rllg,
    /// `risbg r1, r2, i3_start, i4_end, i5_rot` (Rotate Then Insert Selected Bits, `EC..55`, 6 bytes)
    Risbg,
    /// `risbgn r1, r2, i3_start, i4_end, i5_rot` (Non-CC-clobbering `RISBG`, zEC12+, `EC..59`, 6 bytes, 0 CC change!)
    Risbgn,
    /// `flogr r1_even, r2` (Find Leftmost One: leading zero count -> `r1`, bit-cleared val -> `r1+1`, `B983`, 4 bytes)
    Flogr,
    /// `popcnt r1, r2` (`B9E1`, 4 bytes)
    Popcnt,

    // =========================================================================
    // 8. Comparisons, Fused Compare-and-Branch (`CGRJ`/`CGIJ`) & Branches
    // =========================================================================
    Cgr, Cr, Clgr, Clr, Cghi, Chi, Cgfi, Clgfi, Cg, Tmll,
    /// `cgrj r1, r2, m3, rel16_hw` (64-bit signed compare & branch, `EC..64`, 6 bytes, 0 CC clobber!)
    Cgrj,
    /// `crj r1, r2, m3, rel16_hw` (32-bit signed compare & branch, `EC..76`, 6 bytes, 0 CC clobber!)
    Crj,
    /// `clgrj r1, r2, m3, rel16_hw` (64-bit unsigned compare & branch, `EC..65`, 6 bytes, 0 CC clobber!)
    Clgrj,
    /// `cgij r1, imm8, m3, rel16_hw` (64-bit signed compare imm8 & branch, `EC..7C`, 6 bytes, 0 CC clobber!)
    Cgij,
    /// `cij r1, imm8, m3, rel16_hw` (32-bit signed compare imm8 & branch, `EC..7E`, 6 bytes, 0 CC clobber!)
    Cij,
    /// `bcr m1, r2` (branch on condition to register, `07`, 2 bytes; `bcr 15, %r14` is `br %r14` return!)
    Bcr,
    /// `brc m1, rel16_hw` (branch relative on condition, `A7x4`, 4 bytes)
    Brc,
    /// `brcl m1, rel32_hw` (branch relative long on condition, `C0x4`, 6 bytes)
    Brcl,
    /// `brasl r1, rel32_hw` (branch relative and save long — direct function call, `C0x5`, 6 bytes)
    Brasl,
    /// `basr r1, r2` (branch and save register — indirect function call, `0D`, 2 bytes)
    Basr,
    /// `svc imm8` (Supervisor Call / Linux `s390x` syscall, `0A`, 2 bytes)
    Svc,

    // =========================================================================
    // 9. Storage-to-Storage (`SS`) & Vector (`VRR` / `VRX`) Operations
    // =========================================================================
    /// `mvc d1(l, b1), d2(b2)` (Move Characters: 1..=256 byte memory-to-memory copy, `D2`, 6 bytes, 0 CC change!)
    Mvc,
    /// `xc d1(l, b1), d2(b2)` (Exclusive OR Characters: 1..=256 byte memory XOR / zeroing, `D7`, 6 bytes, sets `CC`!)
    Xc,
    /// `clc d1(l, b1), d2(b2)` (Compare Logical Characters: 1..=256 byte `memcmp`, `D5`, 6 bytes, sets `CC`!)
    Clc,
    /// `vl v1, d12(x2, b2)` (128-bit vector load, `E7..06`, 6 bytes)
    Vl,
    /// `vst v1, d12(x2, b2)` (128-bit vector store, `E7..0E`, 6 bytes)
    Vst,
    /// `vx v1, v2, v3` (128-bit vector XOR, `E7..6D`, 6 bytes)
    Vx,
    /// `vfadb v1, v2, v3` (vector/scalar FP64 add, `E7..E3`, 6 bytes)
    Vfadb,
    /// `vfmadb v1, v2, v3, v4` (vector/scalar FP64 fused multiply-add, `E7..8F`, 6 bytes)
    Vfmadb,
}

impl Opcode {
    /// Returns the static hardware descriptor (`InstDesc`) for this SystemZ opcode.
    #[inline]
    pub const fn desc(self) -> InstDesc {
        match self {
            // --- 2-Byte RR Instructions ---
            Self::Lr => InstDesc {
                mnemonic: "lr",
                format: InstFormat::RR,
                byte_len: 2,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: false,
                preserves_high32: true,
                requires_even_pair: false,
            },
            Self::Ar | Self::Sr | Self::Nr | Self::Or | Self::Xr => InstDesc {
                mnemonic: match self {
                    Self::Ar => "ar",
                    Self::Sr => "sr",
                    Self::Nr => "nr",
                    Self::Or => "or",
                    _ => "xr",
                },
                format: InstFormat::RR,
                byte_len: 2,
                tied_dst_src0: true,
                reads_cc: false,
                writes_cc: true,
                preserves_high32: true,
                requires_even_pair: false,
            },
            Self::Cr | Self::Clr => InstDesc {
                mnemonic: if matches!(self, Self::Cr) { "cr" } else { "clr" },
                format: InstFormat::RR,
                byte_len: 2,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: true,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Bcr | Self::Basr | Self::Svc => InstDesc {
                mnemonic: match self {
                    Self::Bcr => "bcr",
                    Self::Basr => "basr",
                    _ => "svc",
                },
                format: InstFormat::RR,
                byte_len: 2,
                tied_dst_src0: false,
                reads_cc: matches!(self, Self::Bcr),
                writes_cc: false,
                preserves_high32: false,
                requires_even_pair: false,
            },

            // --- 4-Byte RRE / RRF / RI / RX Instructions ---
            Self::Lgr | Self::Lgfr | Self::Llgfr | Self::Lgbr | Self::Llgcr | Self::Lghr | Self::Llghr | Self::Lrvgr | Self::Popcnt => InstDesc {
                mnemonic: "mov_ext64",
                format: InstFormat::RRE,
                byte_len: 4,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: matches!(self, Self::Popcnt),
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Locgr => InstDesc {
                mnemonic: "locgr",
                format: InstFormat::RRF,
                byte_len: 4,
                tied_dst_src0: true,
                reads_cc: true,
                writes_cc: false,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Agr | Self::Sgr | Self::Ngr | Self::Ogr | Self::Xgr | Self::Lcgr => InstDesc {
                mnemonic: "alu64_2op",
                format: InstFormat::RRE,
                byte_len: 4,
                tied_dst_src0: !matches!(self, Self::Lcgr),
                reads_cc: false,
                writes_cc: true,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Agrk | Self::Sgrk | Self::Ngrk | Self::Ogrk | Self::Xgrk => InstDesc {
                mnemonic: "alu64_3op",
                format: InstFormat::RRF,
                byte_len: 4,
                tied_dst_src0: false, // Distinct-Operands Facility: non-destructive 3-operand!
                reads_cc: false,
                writes_cc: true,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Ark | Self::Srk => InstDesc {
                mnemonic: "alu32_3op",
                format: InstFormat::RRF,
                byte_len: 4,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: true,
                preserves_high32: true,
                requires_even_pair: false,
            },
            Self::Msgr | Self::Msr => InstDesc {
                mnemonic: if matches!(self, Self::Msgr) { "msgr" } else { "msr" },
                format: InstFormat::RRE,
                byte_len: 4,
                tied_dst_src0: true,
                reads_cc: false,
                writes_cc: false, // Single-register multiply on SystemZ does NOT touch CC!
                preserves_high32: matches!(self, Self::Msr),
                requires_even_pair: false,
            },
            Self::Mlgr | Self::Dlgr | Self::Dsgr | Self::Flogr => InstDesc {
                mnemonic: "pair128_op",
                format: InstFormat::RRE,
                byte_len: 4,
                tied_dst_src0: !matches!(self, Self::Flogr),
                reads_cc: false,
                writes_cc: matches!(self, Self::Flogr),
                preserves_high32: false,
                requires_even_pair: true, // Destination MUST be an even-numbered GprPair!
            },
            Self::Cgr | Self::Clgr => InstDesc {
                mnemonic: if matches!(self, Self::Cgr) { "cgr" } else { "clgr" },
                format: InstFormat::RRE,
                byte_len: 4,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: true,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Lghi | Self::Lhi | Self::Aghi | Self::Ahi | Self::Mghi | Self::Cghi | Self::Chi | Self::Tmll | Self::Brc => InstDesc {
                mnemonic: "ri16_op",
                format: InstFormat::RI,
                byte_len: 4,
                tied_dst_src0: matches!(self, Self::Aghi | Self::Ahi | Self::Mghi),
                reads_cc: matches!(self, Self::Brc),
                writes_cc: matches!(self, Self::Aghi | Self::Ahi | Self::Cghi | Self::Chi | Self::Tmll),
                preserves_high32: matches!(self, Self::Lhi | Self::Ahi),
                requires_even_pair: false,
            },
            Self::L | Self::St | Self::Stc | Self::La => InstDesc {
                mnemonic: match self {
                    Self::L => "l",
                    Self::St => "st",
                    Self::Stc => "stc",
                    _ => "la",
                },
                format: InstFormat::RX,
                byte_len: 4,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: false,
                preserves_high32: matches!(self, Self::L),
                requires_even_pair: false,
            },

            // --- 6-Byte RIL / RXY / RSY / RIE / SS / VRR Instructions ---
            Self::Lgfi | Self::Llihf | Self::Llilf | Self::Iihf | Self::Iilf | Self::Larl
            | Self::Agfi | Self::Nilf | Self::Oilf | Self::Xilf | Self::Cgfi | Self::Clgfi
            | Self::Brcl | Self::Brasl => InstDesc {
                mnemonic: "ril32_op",
                format: InstFormat::RIL,
                byte_len: 6,
                tied_dst_src0: matches!(self, Self::Iihf | Self::Iilf | Self::Agfi | Self::Nilf | Self::Oilf | Self::Xilf),
                reads_cc: matches!(self, Self::Brcl),
                writes_cc: matches!(self, Self::Agfi | Self::Nilf | Self::Oilf | Self::Xilf | Self::Cgfi | Self::Clgfi),
                preserves_high32: matches!(self, Self::Iilf | Self::Nilf | Self::Oilf | Self::Xilf),
                requires_even_pair: false,
            },
            Self::Lg | Self::Ly | Self::Lgf | Self::Llgf | Self::Llgc
            | Self::Stg | Self::Sty | Self::Lay
            | Self::Ag | Self::Sg | Self::Sgf | Self::Ng | Self::Og | Self::Xg | Self::Cg => InstDesc {
                mnemonic: "rxy20_op",
                format: InstFormat::RXY,
                byte_len: 6,
                tied_dst_src0: matches!(self, Self::Ag | Self::Sg | Self::Sgf | Self::Ng | Self::Og | Self::Xg),
                reads_cc: false,
                writes_cc: matches!(self, Self::Ag | Self::Sg | Self::Sgf | Self::Ng | Self::Og | Self::Xg | Self::Cg),
                preserves_high32: matches!(self, Self::Ly),
                requires_even_pair: false,
            },
            Self::Lmg | Self::Stmg | Self::Sllg | Self::Srlg | Self::Srag | Self::Rllg => InstDesc {
                mnemonic: "rsy_op",
                format: InstFormat::RSY,
                byte_len: 6,
                tied_dst_src0: false, // 64-bit SystemZ shifts are non-destructive 3-operand!
                reads_cc: false,
                writes_cc: matches!(self, Self::Srag), // Only arithmetic shift sets CC; SLLG/SRLG/RLLG do NOT!
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Risbg | Self::Risbgn => InstDesc {
                mnemonic: if matches!(self, Self::Risbg) { "risbg" } else { "risbgn" },
                format: InstFormat::RIE,
                byte_len: 6,
                tied_dst_src0: false, // When zero-bit bit 0 of i4 is set, does not merge; otherwise merges into r1
                reads_cc: false,
                writes_cc: matches!(self, Self::Risbg), // `risbgn` never modifies CC!
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Cgrj | Self::Crj | Self::Clgrj | Self::Cgij | Self::Cij => InstDesc {
                mnemonic: "cmp_branch_rie",
                format: InstFormat::RIE,
                byte_len: 6,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: false, // Fused compare-and-branch in SystemZ does NOT modify PSW.CC!
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Mvc | Self::Xc | Self::Clc => InstDesc {
                mnemonic: match self {
                    Self::Mvc => "mvc",
                    Self::Xc => "xc",
                    _ => "clc",
                },
                format: InstFormat::SS,
                byte_len: 6,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: !matches!(self, Self::Mvc),
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Vl | Self::Vst => InstDesc {
                mnemonic: if matches!(self, Self::Vl) { "vl" } else { "vst" },
                format: InstFormat::VRX,
                byte_len: 6,
                tied_dst_src0: false,
                reads_cc: false,
                writes_cc: false,
                preserves_high32: false,
                requires_even_pair: false,
            },
            Self::Vx | Self::Vfadb | Self::Vfmadb => InstDesc {
                mnemonic: "vec_alu",
                format: InstFormat::VRR,
                byte_len: 6,
                tied_dst_src0: false, // 3/4-operand non-destructive vector ops!
                reads_cc: false,
                writes_cc: false,
                preserves_high32: false,
                requires_even_pair: false,
            },
        }
    }
}
