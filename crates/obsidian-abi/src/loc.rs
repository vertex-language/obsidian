//! Physical argument/return locations (`ArgLoc`), computed ABI signatures (`AbiSig`),
//! and stack frame layout calculation (`FrameLayout`).

use alloc::vec::Vec;
use obsidian_ir::{CallConv, Function, Target, Type};
use obsidian_isa_aarch64::regs as aarch64_regs;
use obsidian_isa_m68k::regs as m68k_regs;
use obsidian_isa_systemz::regs as systemz_regs;
use obsidian_isa_x86::regs as x86_regs;

/// Physical hardware register bank used for passing an argument or return value.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RegBank {
    /// General-Purpose / Data Register bank (`RAX..R15` on x86, `X0..X30` on AArch64, `R0..R15` on SystemZ, `D0..D7` on M68k).
    Gpr      = 0,
    /// Dedicated Address Register bank (`A0..A6` on Motorola 680x0).
    Addr     = 1,
    /// Floating-Point / SIMD Vector Register bank (`XMM0..XMM15` on x86, `V0..V31` on AArch64, `F0..F15` on SystemZ).
    FloatVec = 2,
}

/// Physical location of a single function parameter or return value.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArgLoc {
    /// Passed in a single physical register `(bank, hw_reg)`.
    Reg {
        bank: RegBank,
        hw_reg: u8,
    },
    /// Passed across a pair of physical registers (e.g., `i128` on 64-bit CPUs or `i64` on 32-bit CPUs).
    RegPair {
        bank: RegBank,
        lo_reg: u8,
        hi_reg: u8,
    },
    /// Passed in the caller's outgoing stack argument area at `[SP + offset]`.
    Stack {
        /// Byte offset from the outgoing stack pointer at the call site.
        offset: i32,
        /// Size in bytes occupied by this argument (`1`, `2`, `4`, `8`, or `16`).
        size: u8,
        /// Required byte alignment on the stack.
        align: u8,
    },
}

impl ArgLoc {
    #[inline]
    pub const fn gpr(hw_reg: u8) -> Self {
        Self::Reg {
            bank: RegBank::Gpr,
            hw_reg,
        }
    }

    #[inline]
    pub const fn addr_reg(hw_reg: u8) -> Self {
        Self::Reg {
            bank: RegBank::Addr,
            hw_reg,
        }
    }

    #[inline]
    pub const fn float_vec(hw_reg: u8) -> Self {
        Self::Reg {
            bank: RegBank::FloatVec,
            hw_reg,
        }
    }

    #[inline]
    pub const fn gpr_pair(lo_reg: u8, hi_reg: u8) -> Self {
        Self::RegPair {
            bank: RegBank::Gpr,
            lo_reg,
            hi_reg,
        }
    }

    #[inline]
    pub const fn is_reg(self) -> bool {
        matches!(self, Self::Reg { .. } | Self::RegPair { .. })
    }

    #[inline]
    pub const fn is_stack(self) -> bool {
        matches!(self, Self::Stack { .. })
    }

    /// Converts a `RegBank::Gpr` location to a typed x86-64 [`x86_regs::Gpr`].
    #[inline]
    pub const fn as_x86_gpr(self) -> Option<x86_regs::Gpr> {
        match self {
            Self::Reg {
                bank: RegBank::Gpr,
                hw_reg,
            } => x86_regs::Gpr::from_hw_id(hw_reg),
            _ => None,
        }
    }

    /// Converts a `RegBank::FloatVec` location to a typed x86-64 [`x86_regs::SimdReg`].
    #[inline]
    pub const fn as_x86_simd(self) -> Option<x86_regs::SimdReg> {
        match self {
            Self::Reg {
                bank: RegBank::FloatVec,
                hw_reg: id @ 0..=15,
            } => {
                const ALL: [x86_regs::SimdReg; 16] = [
                    x86_regs::SimdReg::V0,  x86_regs::SimdReg::V1,  x86_regs::SimdReg::V2,  x86_regs::SimdReg::V3,
                    x86_regs::SimdReg::V4,  x86_regs::SimdReg::V5,  x86_regs::SimdReg::V6,  x86_regs::SimdReg::V7,
                    x86_regs::SimdReg::V8,  x86_regs::SimdReg::V9,  x86_regs::SimdReg::V10, x86_regs::SimdReg::V11,
                    x86_regs::SimdReg::V12, x86_regs::SimdReg::V13, x86_regs::SimdReg::V14, x86_regs::SimdReg::V15,
                ];
                Some(ALL[id as usize])
            }
            _ => None,
        }
    }

    /// Converts a `RegBank::Gpr` location to a typed AArch64 [`aarch64_regs::Gpr`].
    #[inline]
    pub const fn as_aarch64_gpr(self) -> Option<aarch64_regs::Gpr> {
        match self {
            Self::Reg {
                bank: RegBank::Gpr,
                hw_reg,
            } => aarch64_regs::Gpr::from_hw_id(hw_reg),
            _ => None,
        }
    }

    /// Converts a `RegBank::FloatVec` location to a typed AArch64 [`aarch64_regs::VecReg`].
    #[inline]
    pub const fn as_aarch64_vec(self) -> Option<aarch64_regs::VecReg> {
        match self {
            Self::Reg {
                bank: RegBank::FloatVec,
                hw_reg: id @ 0..=7,
            } => {
                const ALL: [aarch64_regs::VecReg; 8] = [
                    aarch64_regs::VecReg::V0, aarch64_regs::VecReg::V1,
                    aarch64_regs::VecReg::V2, aarch64_regs::VecReg::V3,
                    aarch64_regs::VecReg::V4, aarch64_regs::VecReg::V5,
                    aarch64_regs::VecReg::V6, aarch64_regs::VecReg::V7,
                ];
                Some(ALL[id as usize])
            }
            _ => None,
        }
    }

    /// Converts a `RegBank::Gpr` location to a typed SystemZ [`systemz_regs::Gpr`].
    #[inline]
    pub const fn as_systemz_gpr(self) -> Option<systemz_regs::Gpr> {
        match self {
            Self::Reg {
                bank: RegBank::Gpr,
                hw_reg,
            } => systemz_regs::Gpr::from_hw_id(hw_reg),
            _ => None,
        }
    }

    /// Converts a `RegBank::Gpr` location to a typed M68k [`m68k_regs::DataReg`].
    #[inline]
    pub const fn as_m68k_data(self) -> Option<m68k_regs::DataReg> {
        match self {
            Self::Reg {
                bank: RegBank::Gpr,
                hw_reg,
            } => m68k_regs::DataReg::from_hw_id(hw_reg),
            _ => None,
        }
    }

    /// Converts a `RegBank::Addr` location to a typed M68k [`m68k_regs::AddrReg`].
    #[inline]
    pub const fn as_m68k_addr(self) -> Option<m68k_regs::AddrReg> {
        match self {
            Self::Reg {
                bank: RegBank::Addr,
                hw_reg,
            } => m68k_regs::AddrReg::from_hw_id(hw_reg),
            _ => None,
        }
    }
}

/// Complete computed calling-convention layout for a function or call site.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbiSig {
    /// Physical location of each parameter (`params[i]`).
    pub params: Vec<ArgLoc>,
    /// Physical location of each return value (`returns[i]`).
    pub returns: Vec<ArgLoc>,
    /// Total bytes required on the stack for outgoing arguments (including any mandatory shadow space),
    /// rounded up to `stack_align`.
    pub stack_arg_bytes: u32,
    /// Mandatory caller-allocated register home/shadow space in bytes
    /// (`32` on Windows x64, `160` on SystemZ `s390x`, `0` on SysV64 / AAPCS64).
    pub shadow_space_bytes: u8,
    /// Required stack pointer alignment in bytes at call boundaries (`16` on x86-64/AArch64, `8` on SystemZ, `4` on M68k).
    pub stack_align: u8,
    /// Size of the leaf-function Red Zone below `SP` in bytes (`128` on SysV64 and Darwin ARM64, `0` on Win64/Linux ARM64).
    pub red_zone_bytes: u8,
    /// Bitmask of caller-saved (volatile) GPRs clobbered across function calls.
    pub caller_saved_gpr_mask: u32,
    /// Bitmask of callee-saved (non-volatile) GPRs that must be preserved by a function if modified.
    pub callee_saved_gpr_mask: u32,
    /// Bitmask of hardware/OS-reserved GPRs (`RSP` on x86-64, `X18|FP|LR` on Darwin ARM64, etc.).
    pub reserved_gpr_mask: u32,
}

impl AbiSig {
    /// Computes the physical ABI locations and register masks for `(target, call_conv, params, returns)`.
    pub fn compute(
        target: Target,
        call_conv: CallConv,
        params: &[Type],
        returns: &[Type],
    ) -> Self {
        crate::compute_abi_sig(target, call_conv, params, returns)
    }

    /// Computes the physical ABI signature directly for an [`ir::Function`] on `target`.
    #[inline]
    pub fn for_function(target: Target, func: &Function) -> Self {
        Self::compute(target, func.call_conv, &func.param_types, &func.return_types)
    }
}

/// Computed stack frame layout for a function prologue and epilogue.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FrameLayout {
    /// Total bytes to subtract from `SP` in the function prologue (`0` if leaf Red-Zone optimization applies!).
    pub sp_sub_bytes: u32,
    /// Byte offset from `SP` where local `stack.slot` allocations begin.
    pub locals_offset: u32,
    /// Byte offset from `SP` where spilled callee-saved registers are stored.
    pub callee_save_offset: u32,
    /// Number of callee-saved GPRs saved in the prologue.
    pub callee_saved_count: u8,
    /// `true` if this is a leaf function whose entire frame fits inside the target's Red Zone (`sp_sub_bytes == 0`).
    pub uses_red_zone: bool,
}

impl FrameLayout {
    /// Computes the aligned stack frame layout for a function.
    ///
    /// - `abi`: The function's [`AbiSig`].
    /// - `local_slot_bytes`: Total bytes requested by `stack.slot` / `regalloc` spills.
    /// - `max_outgoing_call_stack_bytes`: Maximum `stack_arg_bytes` across any call inside the function.
    /// - `is_leaf`: `true` if the function makes zero calls (`Opcode::Call` / `CallIndirect`).
    /// - `dirty_callee_saved_mask`: Bitmask of GPRs modified by the function.
    /// - `has_pushed_return_addr`: `true` on architectures where `CALL` pushes an 8-byte return address (`x86_64`),
    ///   shifting initial `RSP % 16` by 8 bytes on entry.
    pub const fn compute(
        abi: &AbiSig,
        local_slot_bytes: u32,
        max_outgoing_call_stack_bytes: u32,
        is_leaf: bool,
        dirty_callee_saved_mask: u32,
        reg_size_bytes: u8,
        has_pushed_return_addr: bool,
    ) -> Self {
        let saved_mask = dirty_callee_saved_mask & abi.callee_saved_gpr_mask;
        let saved_count = saved_mask.count_ones() as u8;
        let saved_bytes = (saved_count as u32) * (reg_size_bytes as u32);

        let outgoing = if is_leaf { 0 } else { max_outgoing_call_stack_bytes };
        let locals_offset = outgoing;
        let callee_save_offset = align_up(locals_offset + local_slot_bytes, reg_size_bytes as u32);
        let raw_frame = callee_save_offset + saved_bytes;

        // Leaf Red-Zone check: if the function makes no calls, saves no registers via push,
        // and fits within `abi.red_zone_bytes` (e.g., 128 bytes on x86-64 SysV or Darwin ARM64),
        // it can address locals at `[SP - offset]` with `sp_sub_bytes = 0`!
        if is_leaf && saved_count == 0 && raw_frame <= (abi.red_zone_bytes as u32) {
            return Self {
                sp_sub_bytes: 0,
                locals_offset: 0,
                callee_save_offset: 0,
                callee_saved_count: 0,
                uses_red_zone: raw_frame > 0,
            };
        }

        let align = abi.stack_align as u32;
        let sp_sub_bytes = if has_pushed_return_addr {
            // On x86-64, `CALL` pushed an 8-byte return address before entry, so `RSP % 16 == 8`.
            // If `saved_count` GPRs are pushed in prologue, `RSP` shifts by `saved_bytes`.
            // Therefore `(sp_sub_bytes + saved_bytes + 8)` must be a multiple of 16!
            let total_with_ret = align_up(raw_frame + (reg_size_bytes as u32), align);
            total_with_ret - (reg_size_bytes as u32) - saved_bytes
        } else {
            align_up(raw_frame, align)
        };

        Self {
            sp_sub_bytes,
            locals_offset,
            callee_save_offset,
            callee_saved_count: saved_count,
            uses_red_zone: false,
        }
    }
}

#[inline]
pub(crate) const fn align_up(val: u32, align: u32) -> u32 {
    if align <= 1 {
        val
    } else {
        (val + align - 1) & !(align - 1)
    }
}
