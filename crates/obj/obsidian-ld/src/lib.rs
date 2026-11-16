#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod linker;

pub use crate::linker::{emit_relocatable_object, link_executable, merge_artifacts, LinkError};

#[cfg(test)]
mod tests {
    use crate::{emit_relocatable_object, link_executable, merge_artifacts, LinkError};
    use obsidian_codegen_x86::compile_module as compile_x86;
    use obsidian_ir as ir;

    #[test]
    fn test_cross_module_linking_and_duplicate_symbol_detection() {
        let target = ir::Target::x86_64_windows();

        let mut mod1 = ir::Module::new("m1", target);
        let mut f1 = ir::Function::export("helper", &[ir::Type::I64], &[ir::Type::I64]);
        let [v] = f1.params();
        let v2 = f1.i64_add_imm(v, 10);
        f1.ret(&[v2]);
        mod1.add_function(f1);

        let mut mod2 = ir::Module::new("m2", target);
        let h_sym = mod2.add_extern_fn("helper", ir::CallConv::C, &[ir::Type::I64], &[ir::Type::I64], false);
        let mut f2 = ir::Function::export("mainCRTStartup", &[], &[ir::Type::I64]);
        let c5 = f2.i64_const(5);
        let res = f2.call(h_sym, &[c5], ir::Type::I64);
        f2.ret(&[res]);
        mod2.add_function(f2);

        let art1 = compile_x86(&mod1);
        let art2 = compile_x86(&mod2);

        // Link both modules into a standalone Windows PE32+ `.exe`:
        let pe = link_executable(&[art1.clone(), art2], "mainCRTStartup").unwrap();
        assert_eq!(&pe[0..2], b"MZ");

        // Linking `art1` with itself triggers `DuplicateSymbol("helper")`:
        assert_eq!(
            merge_artifacts(&[art1.clone(), art1.clone()], false),
            Err(LinkError::DuplicateSymbol("helper".into()))
        );

        // `emit_relocatable_object` dispatches to COFF `.obj` (`Machine = 0x8664`):
        let coff = emit_relocatable_object(&art1);
        assert_eq!(u16::from_le_bytes([coff[0], coff[1]]), 0x8664);
    }
}
