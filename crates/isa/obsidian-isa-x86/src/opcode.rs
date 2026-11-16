//! Real x86-64 silicon opcodes (`Opcode`) and static hardware descriptors (`InstDesc`).
//!
//! **Strict Rule:** Zero compiler pseudo-instructions exist in this module.
//! Every variant in [`Opcode`] is a real hardware instruction decoded by x86-64 silicon.

use crate::cond::FlagSet;
use crate::regs::{Gpr, GprMask};

/// Hardware macro-fusion classification for an ALU/compare instruction when immediately
/// followed by a conditional jump (`Jcc`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MacroFusionGroup {
    /// Cannot macro-fuse with a following `Jcc`.
    None = 0,
    /// `CMP` and `TEST`: macro-fuses with ALL 16 conditional jumps (`Jcc`) on modern Intel/AMD cores
    /// (unless it is a `CMP [base + index], imm` which un-laminates!).
    CmpOrTest = 1,
    /// `ADD` and `SUB`: macro-fuses with unsigned (`JB`/`JAE`/`JBE`/`JA`), equality (`JE`/`JNE`),
    /// and signed (`JL`/`JGE`/`JLE`/`JG`) jumps.
    AddOrSub = 2,
    /// `AND`: macro-fuses with equality (`JE`/`JNE`), sign (`JS`/`JNS`), and parity (`JP`/`JNP`).
    And = 3,
    /// `INC` and `DEC`: macro-fuses only with equality (`JE`/`JNE`) and signed (`JL`/`JGE`/`JLE`/`JG`)
    /// jumps because `INC`/`DEC` do NOT update `CF`.
    IncOrDec = 4,
}

/// Static hardware instruction descriptor from the Intel SDM / AMD64 manuals.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstDesc {
    /// Standard assembly mnemonic (`"add"`, `"lea"`, `"tzcnt"`, `"vfmadd213pd"`, etc.).
    pub mnemonic: &'static str,
    /// `true` if `dst` is destructively tied to `src0` in hardware (`dst = dst op src1`).
    pub tied_dst_src0: bool,
    /// `true` if the operation is commutative across its primary inputs (`a + b == b + a`).
    pub is_commutative: bool,
    /// `EFLAGS` status bits read by this instruction.
    pub flags_read: FlagSet,
    /// `EFLAGS` status bits written or clobbered by this instruction.
    pub flags_written: FlagSet,
    /// Fixed physical GPRs implicitly read by silicon (e.g., `RCX` for variable shifts, `RAX` for `MUL`/`DIV`).
    pub fixed_uses: GprMask,
    /// Fixed physical GPRs implicitly written by silicon (e.g., `RAX` and `RDX` for `MUL`/`DIV`/`CQO`).
    pub fixed_defs: GprMask,
    /// Number of internal RISC micro-operations ($\mu\text{ops}$) emitted into the CPU's DSB ($\mu\text{op}$ cache).
    pub uops: u8,
    /// `true` if the instruction carries a false hardware read dependency on its destination register
    /// (`TZCNT`, `LZCNT`, `POPCNT`, `SQRTSD`, `SQRTSS`, `CVTSI2SD`, `CVTSI2SS`).
    pub has_false_dep: bool,
    /// `true` if a reg-to-reg execution can be eliminated in 0 cycles in the CPU's Register Alias Table (RAT).
    pub rat_move_elim: bool,
    /// Hardware macro-fusion eligibility when paired with an adjacent `Jcc`.
    pub macro_fusion: MacroFusionGroup,
}

/// Real x86-64 silicon instructions.
///
/// Naming suffixes follow hardware operand shapes:
/// - `rr`: register, register
/// - `rm`: register, `[memory]`
/// - `mr`: `[memory]`, register
/// - `ri`: register, immediate
/// - `mi`: `[memory]`, immediate
#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Opcode {
    // =========================================================================
    // 1. Data Movement, Zero/Sign Extension & Conditional Moves
    // =========================================================================
    Mov64rr,
    Mov32rr,
    Mov64rm,
    Mov32rm,
    Mov16rm,
    Mov8rm,
    Mov64mr,
    Mov32mr,
    Mov16mr,
    Mov8mr,
    /// `mov r64, imm32` (sign-extended 32-bit immediate, `C7 /0`, 7 bytes, 1 DSB slot)
    Mov64ri32,
    /// `movabs r64, imm64` (full 64-bit immediate, `REX.W + B8+rd`, 10 bytes, consumes 2 DSB slots!)
    Movabs64ri64,
    /// `mov r32, imm32` (`B8+rd`, 5–6 bytes, implicitly zeroes bits `32..64`)
    Mov32ri,
    Movzx64r8,
    Movzx64r16,
    Movzx32r8,
    Movzx32r16,
    Movsx64r8,
    Movsx64r16,
    /// `movsxd r64, r/m32` (`REX.W + 63 /r`)
    Movsxd64r32,
    Cmovcc64rr,
    Cmovcc32rr,
    Cmovcc64rm,
    Xchg64rr,
    Bswap64,
    Bswap32,
    Push64r,
    Pop64r,

    // =========================================================================
    // 2. Non-Destructive 3-Way Address Math (`LEA`)
    // =========================================================================
    /// `lea r64, [base + index * scale + disp]` (non-destructive, 0 flag clobbers, 1 cycle)
    Lea64,
    /// `lea r32, [base + index * scale + disp]` (32-bit result zero-extended to 64 bits)
    Lea32,

    // =========================================================================
    // 3. Binary Integer ALU (64-bit & 32-bit)
    // =========================================================================
    Add64rr, Add32rr, Add64rm, Add32rm, Add64mr, Add64ri, Add32ri,
    Sub64rr, Sub32rr, Sub64rm, Sub32rm, Sub64mr, Sub64ri, Sub32ri,
    Adc64rr, Sbb64rr,
    And64rr, And32rr, And64rm, And32rm, And64ri, And32ri,
    Or64rr,  Or32rr,  Or64rm,  Or32rm,  Or64ri,  Or32ri,
    Xor64rr, Xor32rr, Xor64rm, Xor32rm, Xor64ri, Xor32ri,
    Cmp64rr, Cmp32rr, Cmp8rr, Cmp64rm, Cmp32rm, Cmp8rm, Cmp64mr, Cmp32mr, Cmp8mr, Cmp64ri, Cmp32ri, Cmp8ri, Cmp64mi, Cmp32mi, Cmp8mi,
    Test64rr, Test32rr, Test8rr, Test64mr, Test32mr, Test8mr, Test64ri, Test32ri, Test8ri, Test64mi, Test32mi, Test8mi,

    // =========================================================================
    // 4. Unary Integer ALU, Multiply & Divide
    // =========================================================================
    Inc64r, Dec64r, Neg64r, Neg32r, Not64r, Not32r,
    Imul64rr, Imul32rr, Imul64rri, Imul32rri,
    /// Unsigned `mul r64`: computes `RDX:RAX = RAX * r64`
    Mul64r,
    /// Signed 1-operand `imul r64`: computes `RDX:RAX = RAX * r64`
    Imul1Op64r,
    /// Unsigned `div r64`: divides `RDX:RAX` by `r64` -> quotient `RAX`, remainder `RDX`
    Div64r,
    /// Signed `idiv r64`: divides `RDX:RAX` by `r64` -> quotient `RAX`, remainder `RDX`
    Idiv64r,
    /// `cqo` (`cqto`): sign-extends `RAX` into `RDX:RAX`
    Cqo,
    /// `cdq`: sign-extends `EAX` into `EDX:EAX`
    Cdq,

    // =========================================================================
    // 5. Shifts & Rotates
    // =========================================================================
    Shl64ri, Shl32ri, Shl64rcl, Shl32rcl,
    Shr64ri, Shr32ri, Shr64rcl, Shr32rcl,
    Sar64ri, Sar32ri, Sar64rcl, Sar32rcl,
    Rol64ri, Ror64ri,

    // =========================================================================
    // 6. Bit Manipulation (`ABM`, `BMI1`, `BMI2`)
    // =========================================================================
    /// `popcnt r64, r/m64` (carries false destination dependency on many Intel cores!)
    Popcnt64rr,
    Popcnt32rr,
    /// `tzcnt r64, r/m64` (carries false destination dependency prior to Zen4 / Cannon Lake!)
    Tzcnt64rr,
    Tzcnt32rr,
    /// `lzcnt r64, r/m64` (carries false destination dependency!)
    Lzcnt64rr,
    Lzcnt32rr,
    Bsf64rr,
    Bsr64rr,
    Bt64rr,
    Bt64ri,
    Bts64rr,
    Btr64rr,
    Btc64rr,
    /// BMI1 `andn r64, src1, src2`: non-destructive `dst = !src1 & src2` (VEX-encoded)
    Andn64rrr,
    Andn32rrr,
    /// BMI1 `blsi r64, src`: extract lowest set bit (`dst = src & -src`, VEX-encoded)
    Blsi64rr,
    /// BMI1 `blsmsk r64, src`: mask up to lowest set bit (`dst = src ^ (src - 1)`, VEX-encoded)
    Blsmsk64rr,
    /// BMI1 `blsr r64, src`: reset lowest set bit (`dst = src & (src - 1)`, VEX-encoded)
    Blsr64rr,
    /// BMI2 `bzhi r64, src, idx`: zero high bits starting at bit `idx` (VEX-encoded, 0 CF/OF clobber)
    Bzhi64rrr,
    /// BMI2 `pdep r64, src, mask`: parallel bit deposit (VEX-encoded)
    Pdep64rrr,
    /// BMI2 `pext r64, src, mask`: parallel bit extract (VEX-encoded)
    Pext64rrr,
    /// BMI2 `rorx r64, src, imm8`: flagless non-destructive rotate right (VEX-encoded!)
    Rorx64rri,
    /// BMI2 `shlx r64, src, cnt`: flagless non-destructive shift left (VEX-encoded, no `RCX` tie!)
    Shlx64rrr,
    /// BMI2 `shrx r64, src, cnt`: flagless non-destructive logical shift right (VEX-encoded)
    Shrx64rrr,
    /// BMI2 `sarx r64, src, cnt`: flagless non-destructive arithmetic shift right (VEX-encoded)
    Sarx64rrr,
    /// BMI2 `mulx hi, lo, src`: flagless unsigned multiply with explicit `RDX` source (VEX-encoded)
    Mulx64rrr,

    // =========================================================================
    // 7. Control Flow, Flag Materialization & System Instructions
    // =========================================================================
    JmpRel8,
    JmpRel32,
    Jmp64r,
    Jmp64m,
    JccRel8,
    JccRel32,
    Setcc8r,
    CallRel32,
    Call64r,
    Call64m,
    Ret,
    Ud2,
    Nop,
    Int3,
    Syscall,
    Pause,
    Mfence,
    Lfence,
    Sfence,

    // =========================================================================
    // 8. SSE / AVX / FMA Scalar & Vector Operations (`XMM` / `YMM`)
    // =========================================================================
    MovapsRr, MovupsRr, MovapdRr, MovupdRr, MovdqaRr, MovdquRr,
    MovssRr, MovsdRr,
    MovqXmmGpr, MovqGprXmm,
    AddpsRr, AddpdRr, AddssRr, AddsdRr,
    SubpsRr, SubpdRr, SubssRr, SubsdRr,
    MulpsRr, MulpdRr, MulssRr, MulsdRr,
    DivpsRr, DivpdRr, DivssRr, DivsdRr,
    /// Scalar single-precision square root (`sqrtss xmm1, xmm2` — false dependency on `xmm1`!)
    SqrtssRr,
    /// Scalar double-precision square root (`sqrtsd xmm1, xmm2` — false dependency on `xmm1`!)
    SqrtsdRr,
    XorpsRr, XorpdRr, PxorRr,
    PadddRr, PaddqRr, PsubdRr, PsubqRr, PcmpeqbRr, PmovmskbGprXmm,
    UcomissRr, UcomisdRr,
    /// Convert 64-bit integer to scalar double (`cvtsi2sd xmm1, r64` — false dependency on high bits of `xmm1`!)
    Cvtsi2sd64Rr,
    Cvtsd2si64Rr,
    /// AVX non-destructive 3-operand instructions (VEX-encoded)
    VaddpsRrr, VaddpdRrr, VmulpsRrr, VmulpdRrr, VxorpsRrr, VpxorRrr,
    /// FMA3 fused multiply-add: `dst = (dst * src1) + src2` (VEX-encoded)
    Vfmadd213psRrr, Vfmadd213pdRrr, Vfmadd213ssRrr, Vfmadd213sdRrr,
}

impl Opcode {
    /// Returns the static hardware descriptor (`InstDesc`) for this silicon opcode.
    #[inline]
    pub const fn desc(self) -> InstDesc {
        const NONE: FlagSet = FlagSet::NONE;
        const ARITH: FlagSet = FlagSet::ALL_ARITH;
        const LOG: FlagSet = FlagSet::LOGICAL;
        const INCDEC: FlagSet = FlagSet::INC_DEC;
        const E_GPR: GprMask = GprMask::EMPTY;
        const RAX_RDX: GprMask = GprMask::from_slice(&[Gpr::Rax, Gpr::Rdx]);

        match self {
            // --- Moves ---
            Self::Mov64rr => InstDesc {
                mnemonic: "mov",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: true, // 64-bit GPR-to-GPR move is 0-cycle RAT eliminated!
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mov32rr => InstDesc {
                mnemonic: "mov",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: true, // Valid when dst != src; self-move `mov eax, eax` fails RAT!
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mov64rm | Self::Mov32rm | Self::Mov16rm | Self::Mov8rm => InstDesc {
                mnemonic: "mov",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: matches!(self, Self::Mov16rm | Self::Mov8rm), // Partial reg merge!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mov64mr | Self::Mov32mr | Self::Mov16mr | Self::Mov8mr => InstDesc {
                mnemonic: "mov",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mov64ri32 | Self::Mov32ri => InstDesc {
                mnemonic: "mov",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Movabs64ri64 => InstDesc {
                mnemonic: "movabs",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 2, // 64-bit immediate consumes 2 slots in the CPU's DSB uop cache!
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Movzx64r8 | Self::Movzx64r16 | Self::Movzx32r8 | Self::Movzx32r16 => InstDesc {
                mnemonic: "movzx",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Movsx64r8 | Self::Movsx64r16 => InstDesc {
                mnemonic: "movsx",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Movsxd64r32 => InstDesc {
                mnemonic: "movsxd",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Cmovcc64rr | Self::Cmovcc32rr | Self::Cmovcc64rm => InstDesc {
                mnemonic: "cmovcc",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: ARITH,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Xchg64rr => InstDesc {
                mnemonic: "xchg",
                tied_dst_src0: true,
                is_commutative: true,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 3,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Bswap64 | Self::Bswap32 => InstDesc {
                mnemonic: "bswap",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Push64r => InstDesc {
                mnemonic: "push",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: GprMask::single(Gpr::Rsp),
                fixed_defs: GprMask::single(Gpr::Rsp),
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Pop64r => InstDesc {
                mnemonic: "pop",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: GprMask::single(Gpr::Rsp),
                fixed_defs: GprMask::single(Gpr::Rsp),
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- LEA ---
            Self::Lea64 | Self::Lea32 => InstDesc {
                mnemonic: "lea",
                tied_dst_src0: false, // Non-destructive 3-operand address calculation!
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,  // Never touches EFLAGS!
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- ADD / SUB ---
            Self::Add64rr | Self::Add32rr | Self::Add64rm | Self::Add32rm | Self::Add64mr | Self::Add64ri | Self::Add32ri => InstDesc {
                mnemonic: "add",
                tied_dst_src0: true,
                is_commutative: matches!(self, Self::Add64rr | Self::Add32rr),
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: if matches!(self, Self::Add64mr) { 2 } else { 1 },
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::AddOrSub,
            },
            Self::Sub64rr | Self::Sub32rr | Self::Sub64rm | Self::Sub32rm | Self::Sub64mr | Self::Sub64ri | Self::Sub32ri => InstDesc {
                mnemonic: "sub",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: if matches!(self, Self::Sub64mr) { 2 } else { 1 },
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::AddOrSub,
            },
            Self::Adc64rr | Self::Sbb64rr => InstDesc {
                mnemonic: if matches!(self, Self::Adc64rr) { "adc" } else { "sbb" },
                tied_dst_src0: true,
                is_commutative: matches!(self, Self::Adc64rr),
                flags_read: FlagSet::CF,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- Logical ALU (AND / OR / XOR) ---
            Self::And64rr | Self::And32rr | Self::And64rm | Self::And32rm | Self::And64ri | Self::And32ri => InstDesc {
                mnemonic: "and",
                tied_dst_src0: true,
                is_commutative: matches!(self, Self::And64rr | Self::And32rr),
                flags_read: NONE,
                flags_written: LOG,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::And,
            },
            Self::Or64rr | Self::Or32rr | Self::Or64rm | Self::Or32rm | Self::Or64ri | Self::Or32ri => InstDesc {
                mnemonic: "or",
                tied_dst_src0: true,
                is_commutative: matches!(self, Self::Or64rr | Self::Or32rr),
                flags_read: NONE,
                flags_written: LOG,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Xor64rr | Self::Xor32rr | Self::Xor64rm | Self::Xor32rm | Self::Xor64ri | Self::Xor32ri => InstDesc {
                mnemonic: "xor",
                tied_dst_src0: true,
                is_commutative: matches!(self, Self::Xor64rr | Self::Xor32rr),
                flags_read: NONE,
                flags_written: LOG,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- Compare & Test ---
            Self::Cmp64rr | Self::Cmp32rr | Self::Cmp8rr
            | Self::Cmp64rm | Self::Cmp32rm | Self::Cmp8rm
            | Self::Cmp64mr | Self::Cmp32mr | Self::Cmp8mr
            | Self::Cmp64ri | Self::Cmp32ri | Self::Cmp8ri
            | Self::Cmp64mi | Self::Cmp32mi | Self::Cmp8mi => InstDesc {
                mnemonic: "cmp",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::CmpOrTest,
            },
            Self::Test64rr | Self::Test32rr | Self::Test8rr
            | Self::Test64mr | Self::Test32mr | Self::Test8mr
            | Self::Test64ri | Self::Test32ri | Self::Test8ri
            | Self::Test64mi | Self::Test32mi | Self::Test8mi => InstDesc {
                mnemonic: "test",
                tied_dst_src0: false,
                is_commutative: matches!(self, Self::Test64rr | Self::Test32rr),
                flags_read: NONE,
                flags_written: LOG,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::CmpOrTest,
            },

            // --- Unary & Mul/Div ---
            Self::Inc64r | Self::Dec64r => InstDesc {
                mnemonic: if matches!(self, Self::Inc64r) { "inc" } else { "dec" },
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: INCDEC, // Preserves CF!
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::IncOrDec,
            },
            Self::Neg64r | Self::Neg32r => InstDesc {
                mnemonic: "neg",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Not64r | Self::Not32r => InstDesc {
                mnemonic: "not",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Imul64rr | Self::Imul32rr => InstDesc {
                mnemonic: "imul",
                tied_dst_src0: true,
                is_commutative: true,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Imul64rri | Self::Imul32rri => InstDesc {
                mnemonic: "imul",
                tied_dst_src0: false, // 3-operand `imul dst, src, imm` is non-destructive!
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mul64r | Self::Imul1Op64r => InstDesc {
                mnemonic: if matches!(self, Self::Mul64r) { "mul" } else { "imul" },
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: GprMask::single(Gpr::Rax),
                fixed_defs: RAX_RDX,
                uops: 2,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Div64r | Self::Idiv64r => InstDesc {
                mnemonic: if matches!(self, Self::Div64r) { "div" } else { "idiv" },
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: RAX_RDX,
                fixed_defs: RAX_RDX,
                uops: 4,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Cqo | Self::Cdq => InstDesc {
                mnemonic: if matches!(self, Self::Cqo) { "cqo" } else { "cdq" },
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: GprMask::single(Gpr::Rax),
                fixed_defs: GprMask::single(Gpr::Rdx),
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- Shifts & Rotates ---
            Self::Shl64ri | Self::Shl32ri | Self::Shr64ri | Self::Shr32ri | Self::Sar64ri | Self::Sar32ri | Self::Rol64ri | Self::Ror64ri => InstDesc {
                mnemonic: "shift_imm",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Shl64rcl | Self::Shl32rcl | Self::Shr64rcl | Self::Shr32rcl | Self::Sar64rcl | Self::Sar32rcl => InstDesc {
                mnemonic: "shift_cl",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: GprMask::single(Gpr::Rcx), // Legacy variable shifts require CL!
                fixed_defs: E_GPR,
                uops: 2, // 2-3 uops because of conditional flag update when CL == 0
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- Bit Manipulation (ABM / BMI1 / BMI2) ---
            Self::Popcnt64rr | Self::Popcnt32rr => InstDesc {
                mnemonic: "popcnt",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Hardware false read dependency on dst!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Tzcnt64rr | Self::Tzcnt32rr => InstDesc {
                mnemonic: "tzcnt",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Hardware false read dependency on dst!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Lzcnt64rr | Self::Lzcnt32rr => InstDesc {
                mnemonic: "lzcnt",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Hardware false read dependency on dst!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Bsf64rr | Self::Bsr64rr => InstDesc {
                mnemonic: if matches!(self, Self::Bsf64rr) { "bsf" } else { "bsr" },
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Undefined/unchanged dst when src == 0 creates hardware dependency!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Bt64rr | Self::Bt64ri | Self::Bts64rr | Self::Btr64rr | Self::Btc64rr => InstDesc {
                mnemonic: "bt",
                tied_dst_src0: !matches!(self, Self::Bt64rr | Self::Bt64ri),
                is_commutative: false,
                flags_read: NONE,
                flags_written: FlagSet::CF,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Andn64rrr | Self::Andn32rrr | Self::Blsi64rr | Self::Blsmsk64rr | Self::Blsr64rr | Self::Bzhi64rrr | Self::Pdep64rrr | Self::Pext64rrr => InstDesc {
                mnemonic: "bmi",
                tied_dst_src0: false, // VEX 3-operand non-destructive!
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Rorx64rri | Self::Shlx64rrr | Self::Shrx64rrr | Self::Sarx64rrr => InstDesc {
                mnemonic: "bmi2_shift",
                tied_dst_src0: false, // VEX 3-operand non-destructive, NO RCX tie!
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,  // Flagless shift/rotate!
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Mulx64rrr => InstDesc {
                mnemonic: "mulx",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE, // Flagless 128-bit multiply!
                fixed_uses: GprMask::single(Gpr::Rdx),
                fixed_defs: E_GPR,
                uops: 2,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- Control Flow & System ---
            Self::JmpRel8 | Self::JmpRel32 | Self::Jmp64r | Self::Jmp64m => InstDesc {
                mnemonic: "jmp",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::JccRel8 | Self::JccRel32 => InstDesc {
                mnemonic: "jcc",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: ARITH,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Setcc8r => InstDesc {
                mnemonic: "setcc",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: ARITH,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Writes only low 8 bits -> partial register false dependency!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::CallRel32 | Self::Call64r | Self::Call64m => InstDesc {
                mnemonic: "call",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: ARITH,
                fixed_uses: GprMask::single(Gpr::Rsp),
                fixed_defs: GprMask::single(Gpr::Rsp),
                uops: 2,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Ret => InstDesc {
                mnemonic: "ret",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: GprMask::single(Gpr::Rsp),
                fixed_defs: GprMask::single(Gpr::Rsp),
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Ud2 | Self::Nop | Self::Int3 | Self::Syscall | Self::Pause | Self::Mfence | Self::Lfence | Self::Sfence => InstDesc {
                mnemonic: "sys",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: if matches!(self, Self::Syscall) { ARITH } else { NONE },
                fixed_uses: E_GPR,
                fixed_defs: if matches!(self, Self::Syscall) {
                    GprMask::from_slice(&[Gpr::Rax, Gpr::Rcx, Gpr::R11])
                } else {
                    E_GPR
                },
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },

            // --- SSE / AVX / FMA ---
            Self::MovapsRr | Self::MovupsRr | Self::MovapdRr | Self::MovupdRr | Self::MovdqaRr | Self::MovdquRr => InstDesc {
                mnemonic: "mov_vec",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: true, // Full 128-bit XMM reg-to-reg moves are RAT move-eliminated!
                macro_fusion: MacroFusionGroup::None,
            },
            Self::MovssRr | Self::MovsdRr => InstDesc {
                mnemonic: "movs_scalar",
                tied_dst_src0: true, // Reg-to-reg `movss`/`movsd` merges upper lanes of dst!
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Merges into upper bits of XMM dst!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::MovqXmmGpr | Self::MovqGprXmm | Self::Cvtsd2si64Rr | Self::PmovmskbGprXmm => InstDesc {
                mnemonic: "movq_cvt",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::SqrtssRr | Self::SqrtsdRr | Self::Cvtsi2sd64Rr => InstDesc {
                mnemonic: "scalar_sse_unary",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: true, // Preserves bits 64..128 of dst XMM -> hardware false dependency!
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::AddpsRr | Self::AddpdRr | Self::AddssRr | Self::AddsdRr
            | Self::SubpsRr | Self::SubpdRr | Self::SubssRr | Self::SubsdRr
            | Self::MulpsRr | Self::MulpdRr | Self::MulssRr | Self::MulsdRr
            | Self::DivpsRr | Self::DivpdRr | Self::DivssRr | Self::DivsdRr
            | Self::XorpsRr | Self::XorpdRr | Self::PxorRr
            | Self::PadddRr | Self::PaddqRr | Self::PsubdRr | Self::PsubqRr | Self::PcmpeqbRr => InstDesc {
                mnemonic: "sse_alu",
                tied_dst_src0: true,
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::UcomissRr | Self::UcomisdRr => InstDesc {
                mnemonic: "ucomis",
                tied_dst_src0: false,
                is_commutative: false,
                flags_read: NONE,
                flags_written: FlagSet::CF.union(FlagSet::PF).union(FlagSet::ZF),
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::VaddpsRrr | Self::VaddpdRrr | Self::VmulpsRrr | Self::VmulpdRrr | Self::VxorpsRrr | Self::VpxorRrr => InstDesc {
                mnemonic: "avx_alu",
                tied_dst_src0: false, // VEX 3-operand non-destructive!
                is_commutative: true,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
            Self::Vfmadd213psRrr | Self::Vfmadd213pdRrr | Self::Vfmadd213ssRrr | Self::Vfmadd213sdRrr => InstDesc {
                mnemonic: "vfmadd213",
                tied_dst_src0: true, // `dst = (dst * src1) + src2`
                is_commutative: false,
                flags_read: NONE,
                flags_written: NONE,
                fixed_uses: E_GPR,
                fixed_defs: E_GPR,
                uops: 1,
                has_false_dep: false,
                rat_move_elim: false,
                macro_fusion: MacroFusionGroup::None,
            },
        }
    }
}
