//! Real AArch64 (`ARM64`) silicon opcodes (`Opcode`) and static hardware descriptors (`InstDesc`).
//!
//! **Strict Rule:** Zero compiler pseudo-instructions exist in this module.
//! Every variant in [`Opcode`] is a real 32-bit hardware instruction decoded by AArch64 silicon.

use crate::cond::NzcvFlags;

/// Static hardware instruction descriptor from the *Arm Architecture Reference Manual (DDI 0487)*.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstDesc {
    /// Standard assembly mnemonic (`"add"`, `"adds"`, `"ldp"`, `"tbz"`, `"madd"`, etc.).
    pub mnemonic: &'static str,
    /// `true` if `dst` is destructively tied to `src0` (only `MOVK` on AArch64 GPRs!).
    pub tied_dst_src0: bool,
    /// `NZCV` flags read by this instruction.
    pub flags_read: NzcvFlags,
    /// `NZCV` flags written by this instruction (`NONE` for almost all non-`S` AArch64 instructions!).
    pub flags_written: NzcvFlags,
    /// `true` if this instruction accepts a free inline barrel-shifter modifier (`LSL`, `LSR`, `ASR`, `ROR`) on `Rm`.
    pub supports_barrel_shift: bool,
    /// `true` if this instruction loads or stores a pair of registers (`LDP` / `STP`).
    pub is_pair_mem: bool,
    /// `true` if this instruction can macro-fuse with an immediately following `B.cond`
    /// on modern Apple Silicon (M1–M4) and Arm Cortex-X / Neoverse cores.
    pub macro_fuses_with_bcond: bool,
}

/// Real 32-bit AArch64 silicon instructions.
#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Opcode {
    // =========================================================================
    // 1. Immediate Materialization & PC-Relative Addressing (`MOVZ`, `MOVN`, `MOVK`, `ADR`, `ADRP`)
    // =========================================================================
    /// `movz rd, #imm16, lsl #(0/16/32/48)` (move 16-bit immediate and zero other 48 bits)
    Movz,
    /// `movn rd, #imm16, lsl #(0/16/32/48)` (move inverted 16-bit immediate)
    Movn,
    /// `movk rd, #imm16, lsl #(0/16/32/48)` (insert 16-bit immediate keeping other 48 bits intact — tied `dst`!)
    Movk,
    /// `adr xd, rel21` (PC-relative byte offset $\pm 1\text{ MiB}$)
    Adr,
    /// `adrp xd, rel21_page` (PC-relative 4 KiB page offset $\pm 4\text{ GiB}$)
    Adrp,

    // =========================================================================
    // 2. Integer Arithmetic — Immediate, Shifted Register & Extended Register
    // =========================================================================
    /// `add rd|sp, rn|sp, #uimm12, lsl #(0/12)` (0 `NZCV` clobbers!)
    AddImm,
    /// `adds rd, rn|sp, #uimm12, lsl #(0/12)` (sets `NZCV`; when `rd == XZR`, encodes `CMN`!)
    AddsImm,
    /// `sub rd|sp, rn|sp, #uimm12, lsl #(0/12)` (0 `NZCV` clobbers!)
    SubImm,
    /// `subs rd, rn|sp, #uimm12, lsl #(0/12)` (sets `NZCV`; when `rd == XZR`, encodes `CMP`!)
    SubsImm,
    /// `add rd, rn, rm, shift #amt` (3-op non-destructive + inline barrel shifter, 0 `NZCV` clobbers!)
    AddShifted,
    /// `adds rd, rn, rm, shift #amt` (3-op + barrel shifter, sets `NZCV`)
    AddsShifted,
    /// `sub rd, rn, rm, shift #amt` (3-op + barrel shifter, 0 `NZCV` clobbers!)
    SubShifted,
    /// `subs rd, rn, rm, shift #amt` (3-op + barrel shifter, sets `NZCV`; when `rd == XZR`, encodes `CMP`!)
    SubsShifted,
    /// `add rd|sp, rn|sp, rm, extend #0..4` (sign/zero-extend `rm` + shift `0..=4` + add)
    AddExtended,
    /// `sub rd|sp, rn|sp, rm, extend #0..4`
    SubExtended,
    /// `adc rd, rn, rm` (add with carry flag `C`)
    Adc,
    /// `sbc rd, rn, rm` (subtract with carry/not-borrow flag `C`)
    Sbc,

    // =========================================================================
    // 3. Logical Operations — Bitmask Immediate (`N:immr:imms`) & Shifted Register
    // =========================================================================
    /// `and rd, rn, #logical_imm` (13-bit rotating bitmask immediate `N:immr:imms`, 0 `NZCV` clobbers)
    AndImm,
    /// `orr rd, rn, #logical_imm` (when `rn == XZR`, encodes bitmask immediate `MOV`!)
    OrrImm,
    /// `eor rd, rn, #logical_imm`
    EorImm,
    /// `ands rd, rn, #logical_imm` (sets `NZCV`; when `rd == XZR`, encodes `TST`!)
    AndsImm,
    /// `and rd, rn, rm, shift #amt`
    AndShifted,
    /// `bic rd, rn, rm, shift #amt` (`rd = rn & ~(rm << amt)`)
    BicShifted,
    /// `orr rd, rn, rm, shift #amt` (when `rn == XZR` and `amt == 0`, encodes register `MOV`!)
    OrrShifted,
    /// `orn rd, rn, rm, shift #amt` (`rd = rn | ~(rm << amt)`)
    OrnShifted,
    /// `eor rd, rn, rm, shift #amt`
    EorShifted,
    /// `eon rd, rn, rm, shift #amt` (`rd = rn ^ ~(rm << amt)`)
    EonShifted,
    /// `ands rd, rn, rm, shift #amt` (sets `NZCV`; when `rd == XZR`, encodes `TST`!)
    AndsShifted,

    // =========================================================================
    // 4. Bitfields, Variable Shifts, Bit Manipulation & Multiply/Divide (`MADD`, `SDIV`, `UDIV`)
    // =========================================================================
    /// `ubfm rd, rn, #immr, #imms` (Unsigned Bitfield Move: hardware encoding for `LSL`/`LSR`/`UBFX`/`UXTB`/`UXTH`)
    Ubfm,
    /// `sbfm rd, rn, #immr, #imms` (Signed Bitfield Move: hardware encoding for `ASR`/`SBFX`/`SXTB`/`SXTH`/`SXTW`)
    Sbfm,
    /// `bfm rd, rn, #immr, #imms` (Bitfield Insert `BFI` / `BFXIL`, merges into `rd` — tied `dst`!)
    Bfm,
    /// `extr rd, rn, rm, #lsb` (extract register from concatenated `rn:rm >> lsb`; when `rn == rm`, encodes `ROR`!)
    Extr,
    Lslv,
    Lsrv,
    Asrv,
    Rorv,
    Clz,
    Rbit,
    Rev,
    /// `madd rd, rn, rm, ra` (`rd = ra + (rn * rm)`; when `ra == XZR`, encodes `MUL`!)
    Madd,
    /// `msub rd, rn, rm, ra` (`rd = ra - (rn * rm)`; when `ra == XZR`, encodes `MNEG`!)
    Msub,
    /// `smulh xd, xn, xm` (signed 64x64 -> high 64-bit product)
    Smulh,
    /// `umulh xd, xn, xm` (unsigned 64x64 -> high 64-bit product)
    Umulh,
    /// `sdiv rd, rn, rm` (signed integer division; divide-by-zero returns `0` without trapping in AArch64!)
    Sdiv,
    /// `udiv rd, rn, rm` (unsigned integer division; divide-by-zero returns `0` without trapping!)
    Udiv,

    // =========================================================================
    // 5. Conditional Select / Increment / Invert / Negate (`CSEL`, `CSINC`, `CSINV`, `CSNEG`)
    // =========================================================================
    /// `csel rd, rn, rm, cond` (`rd = cond ? rn : rm`)
    Csel,
    /// `csinc rd, rn, rm, cond` (`rd = cond ? rn : (rm + 1)` — encodes `CSET` when `rn = rm = XZR`!)
    Csinc,
    /// `csinv rd, rn, rm, cond` (`rd = cond ? rn : ~rm` — encodes `CSETM` when `rn = rm = XZR`!)
    Csinv,
    /// `csneg rd, rn, rm, cond` (`rd = cond ? rn : -rm` — encodes `CNEG` when `rn == rm`!)
    Csneg,

    // =========================================================================
    // 6. Single-Register & Paired 128-Bit Loads and Stores (`LDR`, `STR`, `LDP`, `STP`)
    // =========================================================================
    Ldr64,
    Ldr32,
    Ldr16,
    Ldr8,
    Ldrsw,
    Ldur64,
    Ldur32,
    Str64,
    Str32,
    Str16,
    Str8,
    Stur64,
    Stur32,
    /// `ldp rt1, rt2, [rn, #simm7 * 8]` (paired 64-bit register load — 128 bits in 1 instruction!)
    Ldp64,
    /// `ldp wt1, wt2, [rn, #simm7 * 4]` (paired 32-bit register load)
    Ldp32,
    /// `stp rt1, rt2, [rn, #simm7 * 8]` (paired 64-bit register store — 128 bits in 1 instruction!)
    Stp64,
    /// `stp wt1, wt2, [rn, #simm7 * 4]` (paired 32-bit register store)
    Stp32,

    // =========================================================================
    // 7. Control Flow & Flagless Branches (`B`, `BL`, `B.cond`, `CBZ`, `CBNZ`, `TBZ`, `TBNZ`, `RET`)
    // =========================================================================
    /// `b rel26_words` (unconditional branch $\pm 128\text{ MiB}$)
    B,
    /// `bl rel26_words` (branch with link to `X30`/`LR` $\pm 128\text{ MiB}$)
    Bl,
    /// `b.cond rel19_words` (conditional branch reading `NZCV` $\pm 1\text{ MiB}$)
    BCond,
    /// `cbz rt, rel19_words` (Compare and Branch on Zero — **does NOT read or write `NZCV`!**)
    Cbz,
    /// `cbnz rt, rel19_words` (Compare and Branch on Non-Zero — **does NOT read or write `NZCV`!**)
    Cbnz,
    /// `tbz rt, #bit6, rel14_words` (Test Single Bit and Branch if Zero $\pm 32\text{ KiB}$ — **0 `NZCV` use!**)
    Tbz,
    /// `tbnz rt, #bit6, rel14_words` (Test Single Bit and Branch if Non-Zero $\pm 32\text{ KiB}$ — **0 `NZCV` use!**)
    Tbnz,
    /// `br rn` (indirect branch to register)
    Br,
    /// `blr rn` (indirect call to register, writing return address to `X30`/`LR`)
    Blr,
    /// `ret rn` (function return, defaults to `X30`/`LR`)
    Ret,
    /// `svc #uimm16` (Supervisor Call / OS syscall)
    Svc,
    /// `brk #uimm16` ( debug breakpoint / trap)
    Brk,
    /// `nop` (`0xD503_201F`)
    Nop,

    // =========================================================================
    // 8. NEON Vector & Floating-Point Operations (`V0`..`V31`)
    // =========================================================================
    Fadd64,
    Fsub64,
    Fmul64,
    Fdiv64,
    Fsqrt64,
    Fmadd64,
    Fcmp64,
    VecEor128,
    VecAddI32x4,
}

impl Opcode {
    /// Returns the static hardware descriptor (`InstDesc`) for this AArch64 opcode.
    #[inline]
    pub const fn desc(self) -> InstDesc {
        const NONE: NzcvFlags = NzcvFlags::NONE;
        const NZCV: NzcvFlags = NzcvFlags::ALL;

        match self {
            // --- Immediate Moves & PC-Relative ---
            Self::Movz | Self::Movn | Self::Adr | Self::Adrp => InstDesc {
                mnemonic: match self {
                    Self::Movz => "movz",
                    Self::Movn => "movn",
                    Self::Adr  => "adr",
                    _          => "adrp",
                },
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::Movk => InstDesc {
                mnemonic: "movk",
                tied_dst_src0: true, // Preserves the other 48 bits of `rd`!
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- Non-Flag-Setting Shifted ALU ---
            Self::AddShifted | Self::SubShifted | Self::AndShifted | Self::BicShifted
            | Self::OrrShifted | Self::OrnShifted | Self::EorShifted | Self::EonShifted => InstDesc {
                mnemonic: "alu_shifted",
                tied_dst_src0: false, // 3-operand non-destructive!
                flags_read: NONE,
                flags_written: NONE,  // Never modifies NZCV!
                supports_barrel_shift: true,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- Flag-Setting ALU (`ADDS`, `SUBS`/`CMP`, `ANDS`/`TST`) ---
            Self::AddsImm | Self::SubsImm | Self::AndsImm => InstDesc {
                mnemonic: "alus_imm",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NZCV,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: true, // Macro-fuses with adjacent `B.cond`!
            },
            Self::AddsShifted | Self::SubsShifted | Self::AndsShifted => InstDesc {
                mnemonic: "alus_shifted",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NZCV,
                supports_barrel_shift: true,
                is_pair_mem: false,
                macro_fuses_with_bcond: true, // Macro-fuses with adjacent `B.cond`!
            },

            // --- Non-Flag-Setting Immediate & Extended ALU ---
            Self::AddImm | Self::SubImm | Self::AddExtended | Self::SubExtended
            | Self::AndImm | Self::OrrImm | Self::EorImm => InstDesc {
                mnemonic: "alu_imm_ext",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::Adc | Self::Sbc => InstDesc {
                mnemonic: if matches!(self, Self::Adc) { "adc" } else { "sbc" },
                tied_dst_src0: false,
                flags_read: NzcvFlags::C,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- Bitfields, Shifts & Multiply/Divide ---
            Self::Bfm => InstDesc {
                mnemonic: "bfm",
                tied_dst_src0: true, // Bitfield insert merges into `rd`!
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::Ubfm | Self::Sbfm | Self::Extr | Self::Lslv | Self::Lsrv | Self::Asrv | Self::Rorv
            | Self::Clz | Self::Rbit | Self::Rev
            | Self::Madd | Self::Msub | Self::Smulh | Self::Umulh | Self::Sdiv | Self::Udiv => InstDesc {
                mnemonic: "bit_mul_div",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- Conditional Selects (`CSEL`, `CSINC`, `CSINV`, `CSNEG`) ---
            Self::Csel | Self::Csinc | Self::Csinv | Self::Csneg => InstDesc {
                mnemonic: "csel_family",
                tied_dst_src0: false,
                flags_read: NZCV,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- Single-Register & Paired Loads/Stores ---
            Self::Ldr64 | Self::Ldr32 | Self::Ldr16 | Self::Ldr8 | Self::Ldrsw
            | Self::Ldur64 | Self::Ldur32
            | Self::Str64 | Self::Str32 | Self::Str16 | Self::Str8
            | Self::Stur64 | Self::Stur32 => InstDesc {
                mnemonic: "ldst_single",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::Ldp64 | Self::Ldp32 | Self::Stp64 | Self::Stp32 => InstDesc {
                mnemonic: if matches!(self, Self::Ldp64 | Self::Ldp32) { "ldp" } else { "stp" },
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: true,
                macro_fuses_with_bcond: false,
            },

            // --- Control Flow ---
            Self::BCond => InstDesc {
                mnemonic: "b.cond",
                tied_dst_src0: false,
                flags_read: NZCV,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::B | Self::Bl | Self::Cbz | Self::Cbnz | Self::Tbz | Self::Tbnz
            | Self::Br | Self::Blr | Self::Ret | Self::Svc | Self::Brk | Self::Nop => InstDesc {
                mnemonic: "branch_or_sys",
                tied_dst_src0: false,
                flags_read: NONE, // CBZ, CBNZ, TBZ, TBNZ do NOT read or write NZCV!
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },

            // --- NEON / FP ---
            Self::Fcmp64 => InstDesc {
                mnemonic: "fcmp",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NZCV,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
            Self::Fadd64 | Self::Fsub64 | Self::Fmul64 | Self::Fdiv64 | Self::Fsqrt64
            | Self::Fmadd64 | Self::VecEor128 | Self::VecAddI32x4 => InstDesc {
                mnemonic: "fp_vec",
                tied_dst_src0: false,
                flags_read: NONE,
                flags_written: NONE,
                supports_barrel_shift: false,
                is_pair_mem: false,
                macro_fuses_with_bcond: false,
            },
        }
    }
}
