//! The 16-byte cache-aligned `InstNode` arena and `Value`, `Block`, and `Sym` handles.

use alloc::string::String;
use alloc::vec::Vec;
use crate::opcode::Opcode;
use crate::types::Type;

/// A 32-bit SSA value handle.
///
/// **Invariant:** `Value(i)` is always defined by `Function::insts[i as usize]`.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Value(pub u32);

impl Value {
    pub const NONE: Self = Self(u32::MAX);

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A 32-bit basic block handle (`Function::blocks[id as usize]`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Block(pub u32);

impl Block {
    pub const ENTRY: Self = Self(0);

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A 32-bit module symbol handle (`Module::symbols[id as usize]`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Sym(pub u32);

impl Sym {
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Bitflags packed into `InstNode::flags`.
pub struct InstFlags;

impl InstFlags {
    /// `args[1]` holds an inline signed 32-bit immediate (`i32 as u32`) rather than a `Value`.
    pub const INLINE_IMM1: u8 = 1 << 0;
    /// Memory access is `volatile` (MMIO / hardware register; must not be elided or reordered).
    pub const VOLATILE: u8 = 1 << 1;
    /// `args[0]` holds an index into `Function::const_pool` (`u64`) rather than an inline `i32`.
    pub const CONST_POOL64: u8 = 1 << 2;
    /// Instruction produces a secondary `Opcode::ResultProj` output at `Value(self_idx + 1)`.
    pub const HAS_SECOND_RESULT: u8 = 1 << 3;
}

/// Exactly 16 bytes (`#[repr(C, align(16))]` — 4 instructions per 64-byte L1 cache line).
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstNode {
    /// 1-byte Bedrock opcode (`Add`, `Load`, `ICmp`, `BrIf`, etc.).
    pub op: Opcode,
    /// 1-byte output `Type` (`I64`, `Ptr`, `I1`, `Void`, etc.).
    pub ty: Type,
    /// 1-byte modifier (`IntCC`, `FloatCC`, `BranchHint`, `MemOrder`, `LoadExtKind`, or `SimdLane`).
    pub subop: u8,
    /// 1-byte flags (`InstFlags::INLINE_IMM1`, `VOLATILE`, `CONST_POOL64`, `HAS_SECOND_RESULT`, plus `align_log2 << 4`).
    pub flags: u8,
    /// 2-byte saturating reference count of downstream instructions reading this `Value`.
    pub use_count: u16,
    /// 2-byte auxiliary field (signed 16-bit memory byte offset `offset=-32768..=32767` or 3rd `Value` / pool index).
    pub aux16: i16,
    /// Two 32-bit operand slots (`Value`, `Block`, `Sym`, inline `i32`, or side-table index).
    pub args: [u32; 2],
}

const _: () = assert!(core::mem::size_of::<InstNode>() == 16);

impl InstNode {
    pub const TOMBSTONE: Self = Self {
        op: Opcode::Tombstone,
        ty: Type::Void,
        subop: 0,
        flags: 0,
        use_count: 0,
        aux16: 0,
        args: [0, 0],
    };

    #[inline]
    pub const fn has_inline_imm1(self) -> bool {
        (self.flags & InstFlags::INLINE_IMM1) != 0
    }

    #[inline]
    pub const fn inline_imm1(self) -> i32 {
        self.args[1] as i32
    }

    #[inline]
    pub const fn is_volatile(self) -> bool {
        (self.flags & InstFlags::VOLATILE) != 0
    }

    #[inline]
    pub const fn has_second_result(self) -> bool {
        (self.flags & InstFlags::HAS_SECOND_RESULT) != 0
    }

    /// Returns the alignment in bytes if an explicit alignment was encoded in `flags[7:4]`.
    #[inline]
    pub const fn explicit_align(self) -> Option<u32> {
        let log2_plus1 = self.flags >> 4;
        if log2_plus1 == 0 {
            None
        } else {
            Some(1u32 << (log2_plus1 - 1))
        }
    }
}

/// Side-table record for a conditional branch (`br_if %cond, then_bb(args...), else_bb(args...)`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrIfTarget {
    pub then_bb: Block,
    pub then_args: Vec<Value>,
    pub else_bb: Block,
    pub else_args: Vec<Value>,
}

/// Side-table record for a jump table (`br_table %idx, default=def_bb(args...), [...]`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrTableData {
    pub default_bb: Block,
    pub targets: Vec<Block>,
}

/// Basic block descriptor inside [`ir::Function`](crate::Function).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockData {
    /// Human-readable label (`"entry"`, `"loop"`, `"in_bounds"`).
    pub name: String,
    /// Block parameter `Value`s (`%i: i64, %sum: i64`).
    pub params: Vec<Value>,
    /// Ordered list of instruction indices (`Value(i)`) inside this basic block.
    pub insts: Vec<Value>,
    /// Optional loop unroll hint (`[unroll(N)]`).
    pub unroll_hint: u8,
}
