#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod defs;
pub mod writer;

pub use crate::writer::{emit_executable_elf, emit_relocatable_elf};

#[cfg(test)]
mod tests {
    use crate::{defs::*, emit_executable_elf, emit_relocatable_elf};
    use obsidian_codegen_aarch64::compile_module as compile_aarch64;
    use obsidian_codegen_x86::compile_module as compile_x86;
    use obsidian_ir as ir;

    #[test]
    fn test_x86_64_relocatable_and_executable_elf() {
        let mut m = ir::Module::new("elf_x86", ir::Target::x86_64_linux());
        let tbl = m.add_const("DATA_TBL", &[1, 2, 3, 4, 5, 6, 7, 8], 8);

        let mut f = ir::Function::export("_start", &[], &[ir::Type::I64]);
        let ptr = f.ptr_sym(tbl, 0);
        let val = f.i64_load(ptr, 0);
        f.ret(&[val]);
        m.add_function(f);

        let art = compile_x86(&m);
        let rel_o = emit_relocatable_elf(&art);
        assert_eq!(&rel_o[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([rel_o[16], rel_o[17]]), ET_REL);

        let exe = emit_executable_elf(&art, "_start").unwrap();
        assert_eq!(&exe[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([exe[16], exe[17]]), ET_EXEC);
    }

    #[test]
    fn test_aarch64_linux_relocatable_elf() {
        let mut m = ir::Module::new("elf_arm64", ir::Target::aarch64_linux());
        let ext = m.add_extern_fn("puts", ir::CallConv::C, &[ir::Type::Ptr], &[ir::Type::I32], false);

        let mut f = ir::Function::export("greet", &[ir::Type::Ptr], &[ir::Type::I32]);
        let [v_s] = f.params();
        let v_r = f.call(ext, &[v_s], ir::Type::I32);
        f.ret(&[v_r]);
        m.add_function(f);

        let art = compile_aarch64(&m);
        let rel_o = emit_relocatable_elf(&art);
        assert_eq!(&rel_o[0..4], &ELF_MAGIC);
        assert_eq!(u16::from_le_bytes([rel_o[18], rel_o[19]]), 183); // EM_AARCH64
    }
}
