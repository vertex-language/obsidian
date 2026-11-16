//! `.obs` Bedrock SSA Opcodes (`ir::Opcode`), comparison predicates (`ir::IntCC`, `ir::FloatCC`),
//! and memory / atomic modifiers.

/// Integer and pointer comparison predicate (`cmp.eq`, `cmp.slt`, `cmp.ult`, etc.).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum IntCC {
    Eq  = 0,
    Ne  = 1,
    Slt = 2,
    Sle = 3,
    Sgt = 4,
    Sge = 5,
    Ult = 6,
    Ule = 7,
    Ugt = 8,
    Uge = 9,
}

impl IntCC {
    pub const ALL: [Self; 10] = [
        Self::Eq,
        Self::Ne,
        Self::Slt,
        Self::Sle,
        Self::Sgt,
        Self::Sge,
        Self::Ult,
        Self::Ule,
        Self::Ugt,
        Self::Uge,
    ];

    #[inline]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        if (raw as usize) < Self::ALL.len() {
            Some(Self::ALL[raw as usize])
        } else {
            None
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq  => "eq",
            Self::Ne  => "ne",
            Self::Slt => "slt",
            Self::Sle => "sle",
            Self::Sgt => "sgt",
            Self::Sge => "sge",
            Self::Ult => "ult",
            Self::Ule => "ule",
            Self::Ugt => "ugt",
            Self::Uge => "uge",
        }
    }

    /// Logical negation of the comparison (`!(a cc b)`).
    #[inline]
    pub const fn invert(self) -> Self {
        match self {
            Self::Eq  => Self::Ne,
            Self::Ne  => Self::Eq,
            Self::Slt => Self::Sge,
            Self::Sge => Self::Slt,
            Self::Sle => Self::Sgt,
            Self::Sgt => Self::Sle,
            Self::Ult => Self::Uge,
            Self::Uge => Self::Ult,
            Self::Ule => Self::Ugt,
            Self::Ugt => Self::Ule,
        }
    }

    /// Swaps left and right operands (`a cc b` <-> `b cc' a`).
    #[inline]
    pub const fn commute(self) -> Self {
        match self {
            Self::Eq  => Self::Eq,
            Self::Ne  => Self::Ne,
            Self::Slt => Self::Sgt,
            Self::Sgt => Self::Slt,
            Self::Sle => Self::Sge,
            Self::Sge => Self::Sle,
            Self::Ult => Self::Ugt,
            Self::Ugt => Self::Ult,
            Self::Ule => Self::Uge,
            Self::Uge => Self::Ule,
        }
    }
}

/// IEEE-754 floating-point comparison predicate.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FloatCC {
    Eq  = 0,
    Ne  = 1,
    Lt  = 2,
    Le  = 3,
    Gt  = 4,
    Ge  = 5,
    /// Ordered (neither operand is `NaN`).
    Ord = 6,
    /// Unordered (at least one operand is `NaN` — `is_nan()`).
    Uno = 7,
}

impl FloatCC {
    pub const ALL: [Self; 8] = [
        Self::Eq,
        Self::Ne,
        Self::Lt,
        Self::Le,
        Self::Gt,
        Self::Ge,
        Self::Ord,
        Self::Uno,
    ];

    #[inline]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        if (raw as usize) < Self::ALL.len() {
            Some(Self::ALL[raw as usize])
        } else {
            None
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq  => "eq",
            Self::Ne  => "ne",
            Self::Lt  => "lt",
            Self::Le  => "le",
            Self::Gt  => "gt",
            Self::Ge  => "ge",
            Self::Ord => "ord",
            Self::Uno => "uno",
        }
    }
}

/// Atomic memory ordering (`relaxed`, `acquire`, `release`, `acqrel`, `seqcst`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MemOrder {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcqRel  = 3,
    SeqCst  = 4,
}

impl MemOrder {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Acquire => "acquire",
            Self::Release => "release",
            Self::AcqRel  => "acqrel",
            Self::SeqCst  => "seqcst",
        }
    }
}

/// Atomic read-modify-write operation kind.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum AtomicRmwOp {
    Add  = 0,
    Sub  = 1,
    And  = 2,
    Or   = 3,
    Xor  = 4,
    Nand = 5,
    Xchg = 6,
    MaxS = 7,
    MinS = 8,
    MaxU = 9,
    MinU = 10,
}

impl AtomicRmwOp {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add  => "add",
            Self::Sub  => "sub",
            Self::And  => "and",
            Self::Or   => "or",
            Self::Xor  => "xor",
            Self::Nand => "nand",
            Self::Xchg => "xchg",
            Self::MaxS => "max_s",
            Self::MinS => "min_s",
            Self::MaxU => "max_u",
            Self::MinU => "min_u",
        }
    }
}

/// Extending load kind (`load.ext_u8`, `load.ext_s8`, etc.).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum LoadExtKind {
    U8  = 0,
    S8  = 1,
    U16 = 2,
    S16 = 3,
    U32 = 4,
    S32 = 5,
}

impl LoadExtKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::U8  => "ext_u8",
            Self::S8  => "ext_s8",
            Self::U16 => "ext_u16",
            Self::S16 => "ext_s16",
            Self::U32 => "ext_u32",
            Self::S32 => "ext_s32",
        }
    }
}

/// Intentional hardware trap reason (`trap <reason>`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum TrapReason {
    Unreachable = 0,
    BoundsCheck = 1,
    IntOverflow = 2,
    DivByZero   = 3,
    Abort       = 4,
}

impl TrapReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::BoundsCheck => "bounds_check",
            Self::IntOverflow => "int_overflow",
            Self::DivByZero   => "div_by_zero",
            Self::Abort       => "abort",
        }
    }
}

/// `.obs` Bedrock SSA Opcodes (`1 byte` discriminant).
///
/// Paired with `InstNode::ty` (`ir::Type`), `(ty, op)` uniquely identifies `<type>.<op>`
/// (for example, `(Type::I64, Opcode::Add)` is `i64.add`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Opcode {
    // --- 0. Structural SSA Slots ---
    /// Dead or fused instruction slot; ignored during codegen.
    Tombstone = 0,
    /// Incoming function or basic-block parameter: `args = [block_id, param_index]`.
    BlockParam,
    /// Secondary output projection of a multi-value instruction: `args = [primary_inst_id, result_idx]`.
    ResultProj,

    // --- 1. Constants, Symbols, Pointers, Stack & Variadics ---
    /// Inline 32-bit sign-extended constant (`args[0] as i32`) or 64-bit constant pool index.
    Const,
    /// `ptr.sym @sym, offset`: `args = [sym_id, offset_i32]`.
    PtrSym,
    /// `ptr.add %base, %offset`: `args = [base, offset]`.
    PtrAdd,
    /// `ptr.diff %p1, %p2`: `args = [p1, p2]`.
    PtrDiff,
    /// `ptr.to_int` / `ptr.from_int`.
    PtrToInt,
    PtrFromInt,
    /// `stack.slot size=N, align=M`: `args = [size_bytes, align_bytes]`.
    StackSlot,
    /// `stack.alloc_dyn %bytes, align=M`: `args = [bytes_val, align_bytes]`.
    StackAllocDyn,
    FrameAddr,
    ReturnAddr,
    VaStart,
    VaEnd,
    VaArg,

    // --- 2. Integer Arithmetic (Standard, Overflow, Saturating, Carry, Widening) ---
    Add,
    Sub,
    Mul,
    Neg,
    Abs,
    DivS,
    DivU,
    RemS,
    RemU,
    /// Produces `(quotient, remainder)` via `ResultProj`.
    DivRemS,
    DivRemU,
    /// Produces `(result, overflow_i1)` via `ResultProj`.
    AddOvfS,
    AddOvfU,
    SubOvfS,
    SubOvfU,
    MulOvfS,
    MulOvfU,
    /// Saturating arithmetic (`MIN..=MAX`).
    AddSatS,
    AddSatU,
    SubSatS,
    SubSatU,
    /// Carry-chained arithmetic (`args = [a, b]`, `aux16 = carry_in_i1`), produces `(res, carry_out_i1)`.
    AddCarry,
    SubBorrow,
    /// Widening multiply (`args = [a, b]`), produces `(low, high)` via `ResultProj`.
    MulWideS,
    MulWideU,

    // --- 3. Bitwise, Shifts & Bit Manipulation ---
    And,
    Or,
    Xor,
    Not,
    AndNot,
    OrNot,
    XorNot,
    Shl,
    ShrU,
    ShrS,
    Rotl,
    Rotr,
    /// Funnel shifts: `args = [hi, lo]`, `aux16 = amt`.
    Fshl,
    Fshr,
    Clz,
    Ctz,
    Popcnt,
    ByteSwap,
    BitReverse,
    Clmul,

    // --- 4. Comparisons & Branchless Selection ---
    /// Integer / pointer comparison: `subop = IntCC as u8`, `args = [lhs, rhs]`, `ty = Type::I1`.
    ICmp,
    /// Floating-point comparison: `subop = FloatCC as u8`, `args = [lhs, rhs]`, `ty = Type::I1`.
    FCmp,
    /// `<type>.select %cond_i1, %if_true, %if_false`.
    Select,
    MinS,
    MinU,
    MaxS,
    MaxU,

    // --- 5. Width Conversions, Casts & Bitcasts ---
    ExtU,
    ExtS,
    Trunc,
    Bitcast,
    FromFloatSatS,
    FromFloatSatU,
    FromFloatTruncS,
    FromFloatTruncU,
    FloatFromIntS,
    FloatFromIntU,
    FloatPromote,
    FloatDemote,

    // --- 6. Floating-Point Math (`f32`, `f64`) ---
    FAdd,
    FSub,
    FMul,
    FDiv,
    FSqrt,
    FFma,
    FCopysign,
    FFloor,
    FCeil,
    FTrunc,
    FNearest,
    FMin,
    FMax,

    // --- 7. Memory, Bulk Memory, Prefetch & Atomics ---
    /// `<type>.load %ptr, offset=N`: `args[0] = ptr`, `aux16 = offset`.
    Load,
    /// Extending load (`subop = LoadExtKind as u8`).
    LoadExt,
    /// `<type>.store %ptr, %val, offset=N`: `args = [ptr, val]`, `aux16 = offset`.
    Store,
    /// `mem.copy %dst, %src, %len`.
    MemCopy,
    /// `mem.fill %dst, %byte, %len`.
    MemFill,
    MemPrefetch,
    AtomicLoad,
    AtomicStore,
    AtomicRmw,
    /// Produces `(old_val, success_i1)` via `ResultProj`.
    AtomicCas,
    MemFence,
    SpinHint,

    // --- 8. Calls, Control Flow & Terminators ---
    Call,
    CallIndirect,
    CallUnwind,
    ResumeUnwind,
    TailCall,
    TailCallIndirect,
    Br,
    BrIf,
    BrTable,
    Ret,
    Trap,

    // --- 9. Portable 128-Bit SIMD (`v128`, lane format in `subop`) ---
    V128Splat,
    V128ExtractLane,
    V128ReplaceLane,
    V128Shuffle,
    V128Swizzle,
    V128Add,
    V128Sub,
    V128Mul,
    V128Fma,
    V128Bitselect,
    V128Bitmask,
    V128ReduceAdd,
    V128ReduceMin,
    V128ReduceMax,
}

impl Opcode {
    /// Returns `true` if this opcode is a basic-block terminator (`br`, `br_if`, `ret`, `trap`, etc.).
    #[inline]
    pub const fn is_terminator(self) -> bool {
        matches!(
            self,
            Self::Br
                | Self::BrIf
                | Self::BrTable
                | Self::CallUnwind
                | Self::ResumeUnwind
                | Self::TailCall
                | Self::TailCallIndirect
                | Self::Ret
                | Self::Trap
        )
    }

    /// Returns the `.obs` operation suffix (the `<op>` in `<type>.<op>`).
    pub const fn op_name(self) -> &'static str {
        match self {
            Self::Tombstone        => "tombstone",
            Self::BlockParam       => "param",
            Self::ResultProj       => "proj",
            Self::Const            => "const",
            Self::PtrSym           => "sym",
            Self::PtrAdd           => "add",
            Self::PtrDiff          => "diff",
            Self::PtrToInt         => "to_int",
            Self::PtrFromInt       => "from_int",
            Self::StackSlot        => "stack.slot",
            Self::StackAllocDyn    => "stack.alloc_dyn",
            Self::FrameAddr        => "frame.addr",
            Self::ReturnAddr       => "frame.return_addr",
            Self::VaStart          => "va.start",
            Self::VaEnd            => "va.end",
            Self::VaArg            => "va.arg",
            Self::Add | Self::FAdd | Self::V128Add => "add",
            Self::Sub | Self::FSub | Self::V128Sub => "sub",
            Self::Mul | Self::FMul | Self::V128Mul => "mul",
            Self::Neg              => "neg",
            Self::Abs              => "abs",
            Self::DivS             => "div.s",
            Self::DivU             => "div.u",
            Self::RemS             => "rem.s",
            Self::RemU             => "rem.u",
            Self::DivRemS          => "divrem.s",
            Self::DivRemU          => "divrem.u",
            Self::AddOvfS          => "add.ovf_s",
            Self::AddOvfU          => "add.ovf_u",
            Self::SubOvfS          => "sub.ovf_s",
            Self::SubOvfU          => "sub.ovf_u",
            Self::MulOvfS          => "mul.ovf_s",
            Self::MulOvfU          => "mul.ovf_u",
            Self::AddSatS          => "add.sat_s",
            Self::AddSatU          => "add.sat_u",
            Self::SubSatS          => "sub.sat_s",
            Self::SubSatU          => "sub.sat_u",
            Self::AddCarry         => "add.carry",
            Self::SubBorrow        => "sub.borrow",
            Self::MulWideS         => "mul.wide_s",
            Self::MulWideU         => "mul.wide_u",
            Self::And              => "and",
            Self::Or               => "or",
            Self::Xor              => "xor",
            Self::Not              => "not",
            Self::AndNot           => "and_not",
            Self::OrNot            => "or_not",
            Self::XorNot           => "xor_not",
            Self::Shl              => "shl",
            Self::ShrU             => "shr.u",
            Self::ShrS             => "shr.s",
            Self::Rotl             => "rotl",
            Self::Rotr             => "rotr",
            Self::Fshl             => "fshl",
            Self::Fshr             => "fshr",
            Self::Clz              => "clz",
            Self::Ctz              => "ctz",
            Self::Popcnt           => "popcnt",
            Self::ByteSwap         => "byte_swap",
            Self::BitReverse       => "bit_reverse",
            Self::Clmul            => "clmul",
            Self::ICmp | Self::FCmp => "cmp",
            Self::Select           => "select",
            Self::MinS             => "min.s",
            Self::MinU             => "min.u",
            Self::MaxS             => "max.s",
            Self::MaxU             => "max.u",
            Self::ExtU             => "ext.u",
            Self::ExtS             => "ext.s",
            Self::Trunc            => "trunc",
            Self::Bitcast          => "bitcast",
            Self::FromFloatSatS    => "from_float.sat_s",
            Self::FromFloatSatU    => "from_float.sat_u",
            Self::FromFloatTruncS  => "from_float.trunc_s",
            Self::FromFloatTruncU  => "from_float.trunc_u",
            Self::FloatFromIntS    => "from_int.s",
            Self::FloatFromIntU    => "from_int.u",
            Self::FloatPromote     => "promote",
            Self::FloatDemote      => "demote",
            Self::FDiv             => "div",
            Self::FSqrt            => "sqrt",
            Self::FFma | Self::V128Fma => "fma",
            Self::FCopysign        => "copysign",
            Self::FFloor           => "floor",
            Self::FCeil            => "ceil",
            Self::FTrunc           => "trunc",
            Self::FNearest         => "nearest",
            Self::FMin             => "min",
            Self::FMax             => "max",
            Self::Load             => "load",
            Self::LoadExt          => "load",
            Self::Store            => "store",
            Self::MemCopy          => "mem.copy",
            Self::MemFill          => "mem.fill",
            Self::MemPrefetch      => "mem.prefetch",
            Self::AtomicLoad       => "atomic.load",
            Self::AtomicStore      => "atomic.store",
            Self::AtomicRmw        => "atomic.rmw",
            Self::AtomicCas        => "atomic.cas",
            Self::MemFence         => "mem.fence",
            Self::SpinHint         => "cpu.spin_hint",
            Self::Call             => "call",
            Self::CallIndirect     => "call.indirect",
            Self::CallUnwind       => "call.unwind",
            Self::ResumeUnwind     => "resume.unwind",
            Self::TailCall         => "tailcall",
            Self::TailCallIndirect => "tailcall.indirect",
            Self::Br               => "br",
            Self::BrIf             => "br_if",
            Self::BrTable          => "br_table",
            Self::Ret              => "ret",
            Self::Trap             => "trap",
            Self::V128Splat        => "splat",
            Self::V128ExtractLane  => "extract_lane",
            Self::V128ReplaceLane  => "replace_lane",
            Self::V128Shuffle      => "shuffle",
            Self::V128Swizzle      => "swizzle",
            Self::V128Bitselect    => "bitselect",
            Self::V128Bitmask      => "bitmask",
            Self::V128ReduceAdd    => "reduce_add",
            Self::V128ReduceMin    => "reduce_min",
            Self::V128ReduceMax    => "reduce_max",
        }
    }
}
