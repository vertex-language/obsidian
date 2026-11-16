#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod defs;
pub mod writer;

pub use crate::writer::{emit_executable_pe, emit_relocatable_coff};

#[cfg(test)]
mod tests {
    use crate::{defs::*, emit_executable_pe, emit_relocatable_coff};
    use obsidian_codegen_x86::compile_module as compile_x86;
    use obsidian_ir as ir;

    #[test]
    fn test_windows_x64_coff_obj_and_pe_exe() {
        let mut m = ir::Module::new("coff_test", ir::Target::x86_64_windows());
        let tbl = m.add_const("WIN_CONST_DATA_LONG_NAME", &[42, 0, 0, 0, 0, 0, 0, 0], 8);

        let mut f = ir::Function::export("mainCRTStartup", &[], &[ir::Type::I64]);
        let ptr = f.ptr_sym(tbl, 0);
        let val = f.i64_load(ptr, 0);
        f.ret(&[val]);
        m.add_function(f);

        let art = compile_x86(&m);
        let obj = emit_relocatable_coff(&art);
        assert_eq!(u16::from_le_bytes([obj[0], obj[1]]), IMAGE_FILE_MACHINE_AMD64);

        let exe = emit_executable_pe(&art, "mainCRTStartup").unwrap();
        assert_eq!(u16::from_le_bytes([exe[0], exe[1]]), MZ_MAGIC);
        assert_eq!(&exe[64..68], &PE_SIGNATURE);
    }
}
