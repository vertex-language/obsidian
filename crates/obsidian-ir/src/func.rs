//! The `ir::Function` SSA arena container and instruction emission API.

use alloc::string::String;
use alloc::vec::Vec;
use crate::meta::{BranchHint, FnAttrs, InlineAttr, SourceLoc};
use crate::node::{Block, BlockData, BrIfTarget, InstFlags, InstNode, Sym, Value};
use crate::opcode::{AtomicRmwOp, FloatCC, IntCC, LoadExtKind, MemOrder, Opcode, TrapReason};
use crate::types::{CallConv, Linkage, Type};

/// An SSA function in `obsidian-ir` (`export fn @name(...)` or `local fn @name(...)`).
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// Function symbol name (without `@`).
    pub name: String,
    /// Symbol linkage (`Export` or `Local`).
    pub linkage: Linkage,
    /// Calling convention (`Fast`, `C`, `Cold`, `Preserve`).
    pub call_conv: CallConv,
    /// Function-level attributes (`inline(...)`, `cold`, `noreturn`, `align`, `section`).
    pub attrs: FnAttrs,
    /// Parameter types (`%p0: ty0, %p1: ty1, ...`).
    pub param_types: Vec<Type>,
    /// Return types (`-> ty0` or `-> (ty0, ty1)`).
    pub return_types: Vec<Type>,
    /// Flat array of 16-byte instruction nodes (`Value(i)` indexes `insts[i]`).
    pub insts: Vec<InstNode>,
    /// Basic blocks (`Block(0)` is always `entry`).
    pub blocks: Vec<BlockData>,
    /// Currently selected basic block for appending instructions.
    pub current_block: Block,
    /// 64-bit constant pool (for constants that do not fit in a signed 32-bit inline immediate).
    pub const_pool: Vec<u64>,
    /// Variable-length `Value` argument lists (used by `br`, `ret`, and `call`).
    pub value_lists: Vec<Vec<Value>>,
    /// Conditional branch target records (indexed by `BrIf` `args[1]`).
    pub br_if_targets: Vec<BrIfTarget>,
    /// Optional parallel array of debug source locations (`locs[i]` corresponds to `insts[i]`).
    pub locs: Vec<SourceLoc>,
    /// Active source location automatically attached to newly emitted instructions when set.
    pub active_loc: SourceLoc,
}

impl Function {
    /// Creates a new function with the given `linkage`, parameter types, and return types.
    ///
    /// Automatically creates and selects the `entry` block (`Block(0)`) and populates
    /// its incoming parameter `Value`s (`0..param_types.len()`).
    pub fn new(
        name: &str,
        linkage: Linkage,
        params: &[Type],
        returns: &[Type],
    ) -> Self {
        let mut func = Self {
            name: String::from(name),
            linkage,
            call_conv: CallConv::Fast,
            attrs: FnAttrs::default(),
            param_types: params.to_vec(),
            return_types: returns.to_vec(),
            insts: Vec::with_capacity(32),
            blocks: Vec::with_capacity(4),
            current_block: Block::ENTRY,
            const_pool: Vec::new(),
            value_lists: Vec::new(),
            br_if_targets: Vec::new(),
            locs: Vec::new(),
            active_loc: SourceLoc::NONE,
        };

        let entry = func.new_block("entry", params);
        debug_assert_eq!(entry, Block::ENTRY);
        func.current_block = Block::ENTRY;
        func
    }

    /// Creates an `export fn @name(params...) -> returns...` function.
    #[inline]
    pub fn export(name: &str, params: &[Type], returns: &[Type]) -> Self {
        Self::new(name, Linkage::Export, params, returns)
    }

    /// Creates a `local fn @name(params...) -> returns...` function.
    #[inline]
    pub fn local(name: &str, params: &[Type], returns: &[Type]) -> Self {
        Self::new(name, Linkage::Local, params, returns)
    }

    /// Returns the `entry` basic block (`Block(0)`).
    #[inline]
    pub const fn entry_block(&self) -> Block {
        Block::ENTRY
    }

    /// Returns the function's entry parameter `Value`s as a fixed-size array for ergonomic destructuring:
    /// `let [v_self, v_offset] = f.params();`
    #[inline]
    pub fn params<const N: usize>(&self) -> [Value; N] {
        self.block_params(Block::ENTRY)
    }

    /// Returns the slice of parameter `Value`s for `block`.
    #[inline]
    pub fn block_param_slice(&self, block: Block) -> &[Value] {
        &self.blocks[block.index()].params
    }

    /// Returns the parameter `Value`s of `block` as a fixed-size array `[Value; N]`.
    #[inline]
    pub fn block_params<const N: usize>(&self, block: Block) -> [Value; N] {
        let slice = &self.blocks[block.index()].params;
        assert_eq!(
            slice.len(),
            N,
            "block `{}` has {} parameters, but {} were requested",
            self.blocks[block.index()].name,
            slice.len(),
            N
        );
        let mut out = [Value::NONE; N];
        let mut i = 0;
        while i < N {
            out[i] = slice[i];
            i += 1;
        }
        out
    }

    /// Creates a new basic block with the given label and parameter types.
    pub fn new_block(&mut self, name: &str, param_types: &[Type]) -> Block {
        let block_id = Block(self.blocks.len() as u32);
        let mut params = Vec::with_capacity(param_types.len());
        for (idx, &ty) in param_types.iter().enumerate() {
            let val = Value(self.insts.len() as u32);
            self.insts.push(InstNode {
                op: Opcode::BlockParam,
                ty,
                subop: 0,
                flags: 0,
                use_count: 0,
                aux16: idx as i16,
                args: [block_id.0, idx as u32],
            });
            if !self.locs.is_empty() || self.active_loc.is_present() {
                self.locs.resize(self.insts.len(), SourceLoc::NONE);
            }
            params.push(val);
        }
        self.blocks.push(BlockData {
            name: String::from(name),
            params,
            insts: Vec::new(),
            unroll_hint: 0,
        });
        block_id
    }

    /// Switches the active insertion point to `block` and returns its `[Value; N]` block parameters:
    /// `let [v_i, v_sum] = f.switch_to(loop_bb);`
    #[inline]
    pub fn switch_to<const N: usize>(&mut self, block: Block) -> [Value; N] {
        self.current_block = block;
        self.block_params(block)
    }

    /// Switches the active insertion point to a parameterless `block`.
    #[inline]
    pub fn switch_to_block(&mut self, block: Block) {
        self.current_block = block;
    }

    // =========================================================================
    // Metadata & Attribute Setters
    // =========================================================================

    #[inline]
    pub fn set_call_conv(&mut self, cc: CallConv) {
        self.call_conv = cc;
    }

    #[inline]
    pub fn set_inline(&mut self, inline: InlineAttr) {
        self.attrs.inline = inline;
    }

    #[inline]
    pub fn set_cold(&mut self, cold: bool) {
        self.attrs.cold = cold;
    }

    #[inline]
    pub fn set_noreturn(&mut self, noreturn: bool) {
        self.attrs.noreturn = noreturn;
    }

    #[inline]
    pub fn set_align(&mut self, align_bytes: u16) {
        self.attrs.align = align_bytes;
    }

    #[inline]
    pub fn set_section(&mut self, section: &str) {
        self.attrs.section = Some(String::from(section));
    }

    #[inline]
    pub fn set_source_loc(&mut self, loc: SourceLoc) {
        self.active_loc = loc;
    }

    // =========================================================================
    // Value Inspection & Internal Arena Helpers
    // =========================================================================

    #[inline]
    pub fn value_type(&self, val: Value) -> Type {
        self.insts[val.index()].ty
    }

    #[inline]
    pub fn value_opcode(&self, val: Value) -> Opcode {
        self.insts[val.index()].op
    }

    #[inline]
    pub fn use_count(&self, val: Value) -> u16 {
        self.insts[val.index()].use_count
    }

    /// If `val` is a `Const` instruction, returns its 64-bit constant bit-pattern.
    #[inline]
    pub fn const_bits(&self, val: Value) -> Option<u64> {
        let node = self.insts.get(val.index())?;
        if node.op != Opcode::Const {
            return None;
        }
        if (node.flags & InstFlags::CONST_POOL64) != 0 {
            self.const_pool.get(node.args[0] as usize).copied()
        } else {
            Some((node.args[0] as i32 as i64) as u64)
        }
    }

    #[inline]
    fn bump_use(&mut self, val: Value) {
        if let Some(node) = self.insts.get_mut(val.index()) {
            node.use_count = node.use_count.saturating_add(1);
        }
    }

    fn push_node(&mut self, node: InstNode) -> Value {
        let val = Value(self.insts.len() as u32);
        self.insts.push(node);
        if !self.locs.is_empty() || self.active_loc.is_present() {
            self.locs.resize(self.insts.len() - 1, SourceLoc::NONE);
            self.locs.push(self.active_loc);
        }
        self.blocks[self.current_block.index()].insts.push(val);
        val
    }

    fn push_two_result_node(&mut self, mut primary: InstNode, second_ty: Type) -> (Value, Value) {
        primary.flags |= InstFlags::HAS_SECOND_RESULT;
        let v0 = self.push_node(primary);
        // Secondary `ResultProj` lives at `Value(v0.0 + 1)` in `insts` (not pushed separately to block.insts
        // because printing/lowering `v0` handles both results together).
        let v1 = Value(self.insts.len() as u32);
        self.insts.push(InstNode {
            op: Opcode::ResultProj,
            ty: second_ty,
            subop: 1,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [v0.0, 1],
        });
        if !self.locs.is_empty() {
            self.locs.push(self.active_loc);
        }
        (v0, v1)
    }

    fn alloc_value_list(&mut self, vals: &[Value]) -> u32 {
        for &v in vals {
            self.bump_use(v);
        }
        let idx = self.value_lists.len() as u32;
        self.value_lists.push(vals.to_vec());
        idx
    }

    // =========================================================================
    // 1. Constants, Symbols, Pointers & Stack Slots
    // =========================================================================

    /// Emits `<ty>.const <bits>` (using an inline 32-bit immediate when possible).
    pub fn iconst(&mut self, ty: Type, val: i64) -> Value {
        if val >= (i32::MIN as i64) && val <= (i32::MAX as i64) {
            self.push_node(InstNode {
                op: Opcode::Const,
                ty,
                subop: 0,
                flags: 0,
                use_count: 0,
                aux16: 0,
                args: [(val as i32) as u32, 0],
            })
        } else {
            let pool_idx = self.const_pool.len() as u32;
            self.const_pool.push(val as u64);
            self.push_node(InstNode {
                op: Opcode::Const,
                ty,
                subop: 0,
                flags: InstFlags::CONST_POOL64,
                use_count: 0,
                aux16: 0,
                args: [pool_idx, 0],
            })
        }
    }

    #[inline]
    pub fn i32_const(&mut self, val: i32) -> Value {
        self.iconst(Type::I32, val as i64)
    }

    #[inline]
    pub fn i64_const(&mut self, val: i64) -> Value {
        self.iconst(Type::I64, val)
    }

    #[inline]
    pub fn ptr_const(&mut self, addr: u64) -> Value {
        self.iconst(Type::Ptr, addr as i64)
    }

    #[inline]
    pub fn f64_const(&mut self, val: f64) -> Value {
        let pool_idx = self.const_pool.len() as u32;
        self.const_pool.push(val.to_bits());
        self.push_node(InstNode {
            op: Opcode::Const,
            ty: Type::F64,
            subop: 0,
            flags: InstFlags::CONST_POOL64,
            use_count: 0,
            aux16: 0,
            args: [pool_idx, 0],
        })
    }

    /// Emits `ptr.sym @sym [, offset]`.
    pub fn ptr_sym(&mut self, sym: Sym, offset: i32) -> Value {
        self.push_node(InstNode {
            op: Opcode::PtrSym,
            ty: Type::Ptr,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [sym.0, offset as u32],
        })
    }

    /// Emits `ptr.add %base, %offset` (with $O(1)$ identity folding if `%offset == 0`).
    pub fn ptr_add(&mut self, base: Value, byte_offset: Value) -> Value {
        if self.const_bits(byte_offset) == Some(0) {
            return base;
        }
        self.bump_use(base);
        self.bump_use(byte_offset);
        self.push_node(InstNode {
            op: Opcode::PtrAdd,
            ty: Type::Ptr,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [base.0, byte_offset.0],
        })
    }

    /// Emits `ptr.add %base, imm32` with an inline byte offset.
    pub fn ptr_add_imm(&mut self, base: Value, imm: i32) -> Value {
        if imm == 0 {
            return base;
        }
        self.bump_use(base);
        self.push_node(InstNode {
            op: Opcode::PtrAdd,
            ty: Type::Ptr,
            subop: 0,
            flags: InstFlags::INLINE_IMM1,
            use_count: 0,
            aux16: 0,
            args: [base.0, imm as u32],
        })
    }

    /// Emits `stack.slot size=N, align=M`.
    pub fn stack_slot(&mut self, size: u32, align: u32) -> Value {
        self.push_node(InstNode {
            op: Opcode::StackSlot,
            ty: Type::Ptr,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [size, align],
        })
    }

    // =========================================================================
    // 2. Integer Arithmetic & Bitwise Operations (with O(1) Constant/Identity Folding)
    // =========================================================================

    pub fn binop(&mut self, op: Opcode, ty: Type, a: Value, b: Value) -> Value {
        // O(1) Constant Folding when both inputs are `Const`:
        if let (Some(ca), Some(cb)) = (self.const_bits(a), self.const_bits(b)) {
            match op {
                Opcode::Add => return self.iconst(ty, (ca as i64).wrapping_add(cb as i64)),
                Opcode::Sub => return self.iconst(ty, (ca as i64).wrapping_sub(cb as i64)),
                Opcode::Mul => return self.iconst(ty, (ca as i64).wrapping_mul(cb as i64)),
                Opcode::And => return self.iconst(ty, (ca & cb) as i64),
                Opcode::Or  => return self.iconst(ty, (ca | cb) as i64),
                Opcode::Xor => return self.iconst(ty, (ca ^ cb) as i64),
                Opcode::Shl => return self.iconst(ty, ca.wrapping_shl(cb as u32) as i64),
                Opcode::ShrU => return self.iconst(ty, ca.wrapping_shr(cb as u32) as i64),
                _ => {}
            }
        }
        // O(1) Algebraic Identities (`x + 0 -> x`, `x - 0 -> x`, `x * 1 -> x`, `x ^ x -> 0`):
        if self.const_bits(b) == Some(0)
            && matches!(op, Opcode::Add | Opcode::Sub | Opcode::Or | Opcode::Xor | Opcode::Shl | Opcode::ShrU | Opcode::ShrS)
        {
            return a;
        }
        if self.const_bits(b) == Some(1) && matches!(op, Opcode::Mul | Opcode::DivU | Opcode::DivS) {
            return a;
        }
        if a == b && matches!(op, Opcode::Sub | Opcode::Xor) {
            return self.iconst(ty, 0);
        }

        self.bump_use(a);
        self.bump_use(b);
        self.push_node(InstNode {
            op,
            ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [a.0, b.0],
        })
    }

    pub fn binop_imm(&mut self, op: Opcode, ty: Type, a: Value, imm: i32) -> Value {
        if imm == 0
            && matches!(op, Opcode::Add | Opcode::Sub | Opcode::Or | Opcode::Xor | Opcode::Shl | Opcode::ShrU | Opcode::ShrS)
        {
            return a;
        }
        if let Some(ca) = self.const_bits(a) {
            let cb = imm as i64;
            match op {
                Opcode::Add => return self.iconst(ty, (ca as i64).wrapping_add(cb)),
                Opcode::Sub => return self.iconst(ty, (ca as i64).wrapping_sub(cb)),
                Opcode::Shl => return self.iconst(ty, ca.wrapping_shl(imm as u32) as i64),
                _ => {}
            }
        }
        self.bump_use(a);
        self.push_node(InstNode {
            op,
            ty,
            subop: 0,
            flags: InstFlags::INLINE_IMM1,
            use_count: 0,
            aux16: 0,
            args: [a.0, imm as u32],
        })
    }

    #[inline]
    pub fn add(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::Add, ty, a, b)
    }

    #[inline]
    pub fn add_imm(&mut self, ty: Type, a: Value, imm: i32) -> Value {
        self.binop_imm(Opcode::Add, ty, a, imm)
    }

    #[inline]
    pub fn sub(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::Sub, ty, a, b)
    }

    #[inline]
    pub fn sub_imm(&mut self, ty: Type, a: Value, imm: i32) -> Value {
        self.binop_imm(Opcode::Sub, ty, a, imm)
    }

    #[inline]
    pub fn mul(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::Mul, ty, a, b)
    }

    #[inline]
    pub fn and(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::And, ty, a, b)
    }

    #[inline]
    pub fn and_imm(&mut self, ty: Type, a: Value, imm: i32) -> Value {
        self.binop_imm(Opcode::And, ty, a, imm)
    }

    #[inline]
    pub fn or(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::Or, ty, a, b)
    }

    #[inline]
    pub fn xor(&mut self, ty: Type, a: Value, b: Value) -> Value {
        self.binop(Opcode::Xor, ty, a, b)
    }

    #[inline]
    pub fn shl(&mut self, ty: Type, a: Value, amt: Value) -> Value {
        self.binop(Opcode::Shl, ty, a, amt)
    }

    #[inline]
    pub fn shl_imm(&mut self, ty: Type, a: Value, amt: i32) -> Value {
        self.binop_imm(Opcode::Shl, ty, a, amt)
    }

    #[inline]
    pub fn shr_u(&mut self, ty: Type, a: Value, amt: Value) -> Value {
        self.binop(Opcode::ShrU, ty, a, amt)
    }

    #[inline]
    pub fn shr_s(&mut self, ty: Type, a: Value, amt: Value) -> Value {
        self.binop(Opcode::ShrS, ty, a, amt)
    }

    pub fn unop(&mut self, op: Opcode, ty: Type, a: Value) -> Value {
        self.bump_use(a);
        self.push_node(InstNode {
            op,
            ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [a.0, 0],
        })
    }

    // --- Two-Output Arithmetic (`add.ovf_u`, `mul.wide_u`, `divrem.u`) ---

    pub fn add_ovf_u(&mut self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.bump_use(a);
        self.bump_use(b);
        self.push_two_result_node(
            InstNode {
                op: Opcode::AddOvfU,
                ty,
                subop: 0,
                flags: 0,
                use_count: 0,
                aux16: 0,
                args: [a.0, b.0],
            },
            Type::I1,
        )
    }

    pub fn mul_wide_u(&mut self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.bump_use(a);
        self.bump_use(b);
        self.push_two_result_node(
            InstNode {
                op: Opcode::MulWideU,
                ty,
                subop: 0,
                flags: 0,
                use_count: 0,
                aux16: 0,
                args: [a.0, b.0],
            },
            ty,
        )
    }

    pub fn divrem_u(&mut self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.bump_use(a);
        self.bump_use(b);
        self.push_two_result_node(
            InstNode {
                op: Opcode::DivRemU,
                ty,
                subop: 0,
                flags: 0,
                use_count: 0,
                aux16: 0,
                args: [a.0, b.0],
            },
            ty,
        )
    }

    // --- Explicit `i32_*` and `i64_*` Convenience Wrappers ---

    #[inline]
    pub fn i32_add(&mut self, a: Value, b: Value) -> Value {
        self.add(Type::I32, a, b)
    }

    #[inline]
    pub fn i32_sub(&mut self, a: Value, b: Value) -> Value {
        self.sub(Type::I32, a, b)
    }

    #[inline]
    pub fn i64_add(&mut self, a: Value, b: Value) -> Value {
        self.add(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_add_imm(&mut self, a: Value, imm: i32) -> Value {
        self.add_imm(Type::I64, a, imm)
    }

    #[inline]
    pub fn i64_sub(&mut self, a: Value, b: Value) -> Value {
        self.sub(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_sub_imm(&mut self, a: Value, imm: i32) -> Value {
        self.sub_imm(Type::I64, a, imm)
    }

    #[inline]
    pub fn i64_mul(&mut self, a: Value, b: Value) -> Value {
        self.mul(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_and(&mut self, a: Value, b: Value) -> Value {
        self.and(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_and_imm(&mut self, a: Value, imm: i32) -> Value {
        self.and_imm(Type::I64, a, imm)
    }

    #[inline]
    pub fn i64_or(&mut self, a: Value, b: Value) -> Value {
        self.or(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_xor(&mut self, a: Value, b: Value) -> Value {
        self.xor(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_shl(&mut self, a: Value, amt: Value) -> Value {
        self.shl(Type::I64, a, amt)
    }

    #[inline]
    pub fn i64_shl_imm(&mut self, a: Value, amt: i32) -> Value {
        self.shl_imm(Type::I64, a, amt)
    }

    #[inline]
    pub fn i64_shr_u(&mut self, a: Value, amt: Value) -> Value {
        self.shr_u(Type::I64, a, amt)
    }

    #[inline]
    pub fn i64_shr_s(&mut self, a: Value, amt: Value) -> Value {
        self.shr_s(Type::I64, a, amt)
    }

    #[inline]
    pub fn i64_clz(&mut self, a: Value) -> Value {
        self.unop(Opcode::Clz, Type::I64, a)
    }

    #[inline]
    pub fn i64_ctz(&mut self, a: Value) -> Value {
        self.unop(Opcode::Ctz, Type::I64, a)
    }

    #[inline]
    pub fn i64_popcnt(&mut self, a: Value) -> Value {
        self.unop(Opcode::Popcnt, Type::I64, a)
    }

    #[inline]
    pub fn i64_byte_swap(&mut self, a: Value) -> Value {
        self.unop(Opcode::ByteSwap, Type::I64, a)
    }

    #[inline]
    pub fn i64_add_ovf_u(&mut self, a: Value, b: Value) -> (Value, Value) {
        self.add_ovf_u(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_mul_wide_u(&mut self, a: Value, b: Value) -> (Value, Value) {
        self.mul_wide_u(Type::I64, a, b)
    }

    #[inline]
    pub fn i64_divrem_u(&mut self, a: Value, b: Value) -> (Value, Value) {
        self.divrem_u(Type::I64, a, b)
    }

    // =========================================================================
    // 3. Comparisons & Branchless Select
    // =========================================================================

    pub fn icmp(&mut self, _operand_ty: Type, cc: IntCC, a: Value, b: Value) -> Value {
        self.bump_use(a);
        self.bump_use(b);
        self.push_node(InstNode {
            op: Opcode::ICmp,
            ty: Type::I1,
            subop: cc as u8,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [a.0, b.0],
        })
    }

    pub fn icmp_imm(&mut self, _operand_ty: Type, cc: IntCC, a: Value, imm: i32) -> Value {
        self.bump_use(a);
        self.push_node(InstNode {
            op: Opcode::ICmp,
            ty: Type::I1,
            subop: cc as u8,
            flags: InstFlags::INLINE_IMM1,
            use_count: 0,
            aux16: 0,
            args: [a.0, imm as u32],
        })
    }

    #[inline]
    pub fn i64_cmp(&mut self, cc: IntCC, a: Value, b: Value) -> Value {
        self.icmp(Type::I64, cc, a, b)
    }

    #[inline]
    pub fn i64_cmp_imm(&mut self, cc: IntCC, a: Value, imm: i32) -> Value {
        self.icmp_imm(Type::I64, cc, a, imm)
    }

    pub fn fcmp(&mut self, cc: FloatCC, a: Value, b: Value) -> Value {
        self.bump_use(a);
        self.bump_use(b);
        self.push_node(InstNode {
            op: Opcode::FCmp,
            ty: Type::I1,
            subop: cc as u8,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [a.0, b.0],
        })
    }

    /// Emits `<ty>.select %cond, %if_true, %if_false`.
    pub fn select(&mut self, ty: Type, cond: Value, if_true: Value, if_false: Value) -> Value {
        self.bump_use(cond);
        self.bump_use(if_true);
        self.bump_use(if_false);
        self.push_node(InstNode {
            op: Opcode::Select,
            ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: cond.0 as i16,
            args: [if_true.0, if_false.0],
        })
    }

    #[inline]
    pub fn i64_select(&mut self, cond: Value, if_true: Value, if_false: Value) -> Value {
        self.select(Type::I64, cond, if_true, if_false)
    }

    // =========================================================================
    // 4. Memory Loads, Stores & Atomics
    // =========================================================================

    /// Emits `<ty>.load %ptr [, offset=N]`.
    pub fn load(&mut self, ty: Type, ptr: Value, offset: i16) -> Value {
        self.bump_use(ptr);
        self.push_node(InstNode {
            op: Opcode::Load,
            ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: offset,
            args: [ptr.0, 0],
        })
    }

    /// Emits `<ty>.load.ext_<kind> %ptr [, offset=N]`.
    pub fn load_ext(&mut self, ty: Type, ext: LoadExtKind, ptr: Value, offset: i16) -> Value {
        self.bump_use(ptr);
        self.push_node(InstNode {
            op: Opcode::LoadExt,
            ty,
            subop: ext as u8,
            flags: 0,
            use_count: 0,
            aux16: offset,
            args: [ptr.0, 0],
        })
    }

    #[inline]
    pub fn i8_load(&mut self, ptr: Value, offset: i16) -> Value {
        self.load(Type::I8, ptr, offset)
    }

    #[inline]
    pub fn i32_load(&mut self, ptr: Value, offset: i16) -> Value {
        self.load(Type::I32, ptr, offset)
    }

    #[inline]
    pub fn i64_load(&mut self, ptr: Value, offset: i16) -> Value {
        self.load(Type::I64, ptr, offset)
    }

    #[inline]
    pub fn ptr_load(&mut self, ptr: Value, offset: i16) -> Value {
        self.load(Type::Ptr, ptr, offset)
    }

    /// Emits `<ty>.store %ptr, %val [, offset=N]`.
    pub fn store(&mut self, ty: Type, ptr: Value, val: Value, offset: i16) {
        self.bump_use(ptr);
        self.bump_use(val);
        self.push_node(InstNode {
            op: Opcode::Store,
            ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: offset,
            args: [ptr.0, val.0],
        });
    }

    #[inline]
    pub fn i64_store(&mut self, ptr: Value, val: Value, offset: i16) {
        self.store(Type::I64, ptr, val, offset);
    }

    #[inline]
    pub fn ptr_store(&mut self, ptr: Value, val: Value, offset: i16) {
        self.store(Type::Ptr, ptr, val, offset);
    }

    /// Emits `<ty>.atomic.rmw.<op>.<order> %ptr, %val`.
    pub fn atomic_rmw(
        &mut self,
        ty: Type,
        rmw_op: AtomicRmwOp,
        order: MemOrder,
        ptr: Value,
        val: Value,
    ) -> Value {
        self.bump_use(ptr);
        self.bump_use(val);
        let subop = ((order as u8) << 4) | (rmw_op as u8 & 0x0F);
        self.push_node(InstNode {
            op: Opcode::AtomicRmw,
            ty,
            subop,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [ptr.0, val.0],
        })
    }

    // =========================================================================
    // 5. Calls & Control-Flow Terminators
    // =========================================================================

    /// Emits `%res = call @callee(args...) -> ty`.
    pub fn call(&mut self, callee: Sym, args: &[Value], ret_ty: Type) -> Value {
        let list_idx = self.alloc_value_list(args);
        self.push_node(InstNode {
            op: Opcode::Call,
            ty: ret_ty,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [callee.0, list_idx],
        })
    }

    /// Emits `br target(args...)`.
    pub fn br(&mut self, target: Block, args: &[Value]) {
        let list_idx = self.alloc_value_list(args);
        self.push_node(InstNode {
            op: Opcode::Br,
            ty: Type::Void,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [target.0, list_idx],
        });
    }

    /// Emits `br_if[.likely|.unlikely] %cond, then_bb(then_args...), else_bb(else_args...)`.
    pub fn br_if_with_hint(
        &mut self,
        hint: BranchHint,
        cond: Value,
        then_bb: Block,
        then_args: &[Value],
        else_bb: Block,
        else_args: &[Value],
    ) {
        self.bump_use(cond);
        for &v in then_args {
            self.bump_use(v);
        }
        for &v in else_args {
            self.bump_use(v);
        }
        let target_idx = self.br_if_targets.len() as u32;
        self.br_if_targets.push(BrIfTarget {
            then_bb,
            then_args: then_args.to_vec(),
            else_bb,
            else_args: else_args.to_vec(),
        });
        self.push_node(InstNode {
            op: Opcode::BrIf,
            ty: Type::Void,
            subop: hint as u8,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [cond.0, target_idx],
        });
    }

    #[inline]
    pub fn br_if(
        &mut self,
        cond: Value,
        then_bb: Block,
        then_args: &[Value],
        else_bb: Block,
        else_args: &[Value],
    ) {
        self.br_if_with_hint(BranchHint::Neutral, cond, then_bb, then_args, else_bb, else_args);
    }

    #[inline]
    pub fn br_if_likely(
        &mut self,
        cond: Value,
        then_bb: Block,
        then_args: &[Value],
        else_bb: Block,
        else_args: &[Value],
    ) {
        self.br_if_with_hint(BranchHint::LikelyThen, cond, then_bb, then_args, else_bb, else_args);
    }

    #[inline]
    pub fn br_if_unlikely(
        &mut self,
        cond: Value,
        then_bb: Block,
        then_args: &[Value],
        else_bb: Block,
        else_args: &[Value],
    ) {
        self.br_if_with_hint(
            BranchHint::UnlikelyThen,
            cond,
            then_bb,
            then_args,
            else_bb,
            else_args,
        );
    }

    /// Emits `ret %v0, %v1, ...` (or `ret` when `vals` is empty).
    pub fn ret(&mut self, vals: &[Value]) {
        let list_idx = self.alloc_value_list(vals);
        self.push_node(InstNode {
            op: Opcode::Ret,
            ty: Type::Void,
            subop: 0,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [list_idx, vals.len() as u32],
        });
    }

    /// Emits `trap <reason>`.
    pub fn trap(&mut self, reason: TrapReason) {
        self.push_node(InstNode {
            op: Opcode::Trap,
            ty: Type::Void,
            subop: reason as u8,
            flags: 0,
            use_count: 0,
            aux16: 0,
            args: [0, 0],
        });
    }
}
