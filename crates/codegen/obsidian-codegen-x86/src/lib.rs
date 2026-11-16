#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod lower;

pub use crate::lower::{compile_function, compile_module, CompiledFunction};

#[cfg(test)]
mod tests {
    use crate::{compile_function, compile_module};
    use obsidian_ir as ir;

    #[test]
    fn test_slice_len_codegen() {
        let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
        let [v_self] = f.params();
        let v_n = f.i64_load(v_self, 8);
        f.ret(&[v_n]);

        let compiled = compile_function(ir::Target::x86_64_linux(), &f);
        // mov rax, [rdi + 8] (48 8B 47 08) + ret (C3)
        assert_eq!(compiled.code, &[0x48, 0x8B, 0x47, 0x08, 0xC3]);
    }

    #[test]
    fn test_lea_fusion_and_branch_patching() {
        let mut f = ir::Function::export("clamp_zero", &[ir::Type::I64], &[ir::Type::I64]);
        let [v_x] = f.params();
        let neg_bb = f.new_block("neg", &[]);
        let pos_bb = f.new_block("pos", &[]);

        let v_is_neg = f.i64_cmp_imm(ir::IntCC::Slt, v_x, 0);
        f.br_if(v_is_neg, neg_bb, &[], pos_bb, &[]);

        f.switch_to_block(neg_bb);
        let zero = f.i64_const(0);
        f.ret(&[zero]);

        f.switch_to_block(pos_bb);
        f.ret(&[v_x]);

        let compiled = compile_function(ir::Target::x86_64_linux(), &f);
        assert!(!compiled.code.is_empty());
        // Last byte of the function is `ret` (`0xC3`):
        assert_eq!(*compiled.code.last().unwrap(), 0xC3);
    }

    #[test]
    fn test_module_compilation_with_relocations() {
        let mut m = ir::Module::new("reloc_mod", ir::Target::x86_64_linux());
        let ext_sym = m.add_extern_fn("ext_fn", ir::CallConv::C, &[ir::Type::I64], &[ir::Type::I64], false);

        let mut f = ir::Function::export("caller", &[ir::Type::I64], &[ir::Type::I64]);
        let [v_x] = f.params();
        let v_res = f.call(ext_sym, &[v_x], ir::Type::I64);
        f.ret(&[v_res]);
        m.add_function(f);

        let art = compile_module(&m);
        assert_eq!(art.relocs.len(), 1);
        assert_eq!(art.relocs[0].sym, ext_sym);
        assert_eq!(art.relocs[0].addend, -4);
    }
}
