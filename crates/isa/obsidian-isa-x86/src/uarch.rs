//! Static micro-architectural rules for modern x86-64 silicon (Intel Core / AMD Zen).
//!
//! This module does NOT perform any optimization passes itself; it provides the
//! hardware constants and boolean legality queries consumed by `obsidian-ssa-x86`
//! and `obsidian-regalloc`.

use crate::addr::MemoryOperand;
use crate::cond::CondCode;
use crate::opcode::{MacroFusionGroup, Opcode};
use crate::regs::Gpr;

/// Modern Intel/AMD Decoded Stream Buffer (DSB / $\mu\text{op}$ cache) window size in bytes.
pub const DSB_WINDOW_BYTES: u8 = 32;

/// Maximum number of $\mu\text{op}$ cache lines (up to 6 $\mu\text{ops}$ each = 18 $\mu\text{ops}$)
/// that can be allocated to a single 32-byte aligned machine-code window before
/// overflowing to the slow legacy MITE decoders.
pub const DSB_MAX_UOPS_PER_32B_WINDOW: u8 = 18;

/// Maximum number of conditional/unconditional branches recommended per 32-byte DSB window
/// to avoid BPU / DSB way contention.
pub const DSB_MAX_BRANCHES_PER_32B_WINDOW: u8 = 2;

/// 4-register caller-saved GPR rotation ring (`RAX`, `RDX`, `R8`, `R9`) used by
/// `obsidian-regalloc` to break hardware false destination dependencies on unary
/// instructions (`TZCNT`, `LZCNT`, `POPCNT`) across loop iterations in 0 extra bytes.
pub const FALSE_DEP_GPR_RING: [Gpr; 4] = [Gpr::Rax, Gpr::Rdx, Gpr::R8, Gpr::R9];

/// Returns `true` if `alu_op` followed immediately by `Jcc(cc)` can macro-fuse into
/// a single internal $\mu\text{op}$ in the CPU frontend.
#[inline]
pub const fn can_macro_fuse(alu_op: Opcode, cc: CondCode) -> bool {
    match alu_op.desc().macro_fusion {
        MacroFusionGroup::None => false,
        MacroFusionGroup::CmpOrTest => true, // Fuses with all 16 condition codes
        MacroFusionGroup::AddOrSub => matches!(
            cc,
            CondCode::B
                | CondCode::AE
                | CondCode::E
                | CondCode::NE
                | CondCode::BE
                | CondCode::A
                | CondCode::L
                | CondCode::GE
                | CondCode::LE
                | CondCode::G
        ),
        MacroFusionGroup::And => matches!(
            cc,
            CondCode::E
                | CondCode::NE
                | CondCode::S
                | CondCode::NS
                | CondCode::P
                | CondCode::NP
        ),
        MacroFusionGroup::IncOrDec => matches!(
            cc,
            CondCode::E
                | CondCode::NE
                | CondCode::L
                | CondCode::GE
                | CondCode::LE
                | CondCode::G
        ),
    }
}

/// Returns `true` if a memory-plus-immediate instruction (such as `cmp [base + index * scale], imm`
/// or `cmp [rip + disp32], imm`) triggers hardware **un-lamination** and fails to macro-fuse
/// with a following `Jcc`.
///
/// On Intel/AMD hardware:
/// 1. Combining an **indexed memory operand** (`[base + index * scale]`) with an **immediate**
///    cannot stay micro-fused in the DSB and un-laminates into multiple $\mu\text{ops}$.
/// 2. Combining a **RIP-relative memory operand** (`[rip + disp32]`) with an **immediate**
///    cannot macro-fuse with a following `Jcc` because an instruction cannot carry two 32-bit
///    immediate/displacement fields in a single macro-fused DSB entry.
#[inline]
pub const fn causes_unlamination<Reg: Copy>(mem: &MemoryOperand<Reg>, has_immediate: bool) -> bool {
    if !has_immediate {
        return false;
    }
    match *mem {
        MemoryOperand::BaseIndex { index: Some(_), .. } => true,
        MemoryOperand::RipRelative { .. } => true,
        MemoryOperand::BaseIndex { index: None, .. } => false,
    }
}

/// Returns `true` if `can_macro_fuse(alu_op, cc)` holds AND the memory/immediate
/// combination does not trigger hardware un-lamination.
#[inline]
pub const fn can_macro_fuse_mem<Reg: Copy>(
    alu_op: Opcode,
    mem: &MemoryOperand<Reg>,
    has_immediate: bool,
    cc: CondCode,
) -> bool {
    if causes_unlamination(mem, has_immediate) {
        return false;
    }
    can_macro_fuse(alu_op, cc)
}

/// Returns `true` if a register-to-register move executes in **0 clock cycles** using
/// **0 ALU execution ports** via the CPU's Register Alias Table (RAT).
///
/// Hardware rules:
/// - 64-bit `mov r64, r64` (`Opcode::Mov64rr`) is eliminated in 0 cycles when `dst != src`.
/// - 32-bit `mov r32, r32` (`Opcode::Mov32rr`) fails move elimination when `dst == src`
///   (`mov eax, eax` used for zero-extension).
#[inline]
pub const fn is_rat_zero_cycle_move(op: Opcode, dst: Gpr, src: Gpr) -> bool {
    if dst as u8 == src as u8 {
        return false; // Self-moves (`mov eax, eax` / `mov rax, rax`) fail RAT elimination on Intel/AMD!
    }
    match op {
        Opcode::Mov64rr | Opcode::Mov32rr => true,
        _ => false,
    }
}

/// Returns `true` if the instruction is a zeroing idiom recognized by the CPU register
/// renamer (`xor eax, eax` / `sub eax, eax` / `xorps xmm0, xmm0` / `vpxor xmm0, xmm0, xmm0`)
/// that breaks all prior dependencies and executes in **0 ALU cycles**.
#[inline]
pub const fn is_zero_cycle_zeroing_idiom(op: Opcode, dst_id: u8, src_id: u8) -> bool {
    if dst_id != src_id {
        return false;
    }
    matches!(
        op,
        Opcode::Xor32rr
            | Opcode::Xor64rr
            | Opcode::Sub32rr
            | Opcode::Sub64rr
            | Opcode::XorpsRr
            | Opcode::XorpdRr
            | Opcode::PxorRr
            | Opcode::VxorpsRrr
            | Opcode::VpxorRrr
    )
}
