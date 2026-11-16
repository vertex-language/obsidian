#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod fmt;
pub mod func;
pub mod meta;
pub mod module;
pub mod node;
pub mod opcode;
pub mod target;
pub mod types;

pub use crate::func::Function;
pub use crate::meta::{BranchHint, FnAttrs, InlineAttr, SourceLoc, SymAttrs};
pub use crate::module::{ConstDef, ExternFn, GlobalDef, Module, SymKind, TlsDef};
pub use crate::node::{Block, BlockData, BrIfTarget, BrTableData, InstFlags, InstNode, Sym, Value};
pub use crate::opcode::{
    AtomicRmwOp, FloatCC, IntCC, LoadExtKind, MemOrder, Opcode, TrapReason,
};
pub use crate::target::{Abi, Arch, Endian, ObjFormat, Os, Target};
pub use crate::types::{CallConv, Linkage, SimdLane, Type};

/// Namespace alias `obsidian_ir::ir` so callers can write `use obsidian_ir::ir;`
/// (`ir::Module`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`).
pub mod ir {
    pub use crate::{
        Abi, Arch, AtomicRmwOp, Block, BlockData, BranchHint, BrIfTarget, BrTableData, CallConv,
        ConstDef, Endian, ExternFn, FloatCC, FnAttrs, Function, GlobalDef, InlineAttr, InstFlags,
        InstNode, IntCC, Linkage, LoadExtKind, MemOrder, Module, ObjFormat, Opcode, Os, SimdLane,
        SourceLoc, Sym, SymAttrs, SymKind, Target, TlsDef, TrapReason, Type, Value,
    };
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use crate as ir;

    #[test]
    fn test_inst_node_is_16_bytes() {
        assert_eq!(core::mem::size_of::<ir::InstNode>(), 16);
        assert_eq!(core::mem::align_of::<ir::InstNode>(), 16);
    }

    #[test]
    fn test_constant_and_identity_folding() {
        let mut f = ir::Function::export("fold_test", &[ir::Type::I64], &[ir::Type::I64]);
        let [v_x] = f.params();

        // 1. `x + 0` folds immediately to `x` without emitting an Add node:
        let v_add0 = f.i64_add_imm(v_x, 0);
        assert_eq!(v_add0, v_x);

        // 2. `const(10) + const(32)` folds immediately to `const(42)`:
        let c10 = f.i64_const(10);
        let c32 = f.i64_const(32);
        let c42 = f.i64_add(c10, c32);
        assert_eq!(f.value_opcode(c42), ir::Opcode::Const);
        assert_eq!(f.const_bits(c42), Some(42));

        // 3. `x ^ x` folds immediately to `const(0)`:
        let v_zero = f.i64_xor(v_x, v_x);
        assert_eq!(f.const_bits(v_zero), Some(0));

        f.ret(&[c42]);
    }

    #[test]
    fn test_multi_value_result_projections() {
        let mut f = ir::Function::export(
            "ovf_and_divrem",
            &[ir::Type::I64, ir::Type::I64],
            &[ir::Type::I64, ir::Type::I64],
        );
        let [v_a, v_b] = f.params();

        let (v_sum, v_ovf) = f.i64_add_ovf_u(v_a, v_b);
        assert_eq!(v_ovf.0, v_sum.0 + 1);
        assert_eq!(f.value_type(v_sum), ir::Type::I64);
        assert_eq!(f.value_type(v_ovf), ir::Type::I1);

        let (v_q, v_r) = f.i64_divrem_u(v_sum, v_b);
        let v_picked = f.i64_select(v_ovf, v_r, v_q);
        f.ret(&[v_picked, v_r]);

        assert_eq!(f.use_count(v_ovf), 1);
        assert_eq!(f.use_count(v_r), 2);
    }

    #[test]
    fn test_targets_and_module_formatting() {
        let s390x = ir::Target::systemz_linux();
        assert!(s390x.is_big_endian());
        assert_eq!(s390x.ptr_bytes(), 8);

        let m68k = ir::Target::m68k_bare();
        assert!(m68k.is_big_endian());
        assert_eq!(m68k.ptr_bytes(), 4);

        let mut m = ir::Module::new("demo", ir::Target::x86_64_linux());
        let cnt = m.add_global_i64("COUNTER", 0, 8);

        let mut f = ir::Function::export("inc_counter", &[], &[ir::Type::I64]);
        let ptr = f.ptr_sym(cnt, 0);
        let one = f.i64_const(1);
        let old = f.atomic_rmw(
            ir::Type::I64,
            ir::AtomicRmwOp::Add,
            ir::MemOrder::Relaxed,
            ptr,
            one,
        );
        f.ret(&[old]);
        m.add_function(f);

        let text = m.to_string();
        assert!(text.contains("module @demo"));
        assert!(text.contains("target arch=x86_64, os=linux, abi=sysv64"));
        assert!(text.contains("global @COUNTER: i64 = 0, align=8"));
        assert!(text.contains("%v0 = ptr.sym @COUNTER"));
    }
}
