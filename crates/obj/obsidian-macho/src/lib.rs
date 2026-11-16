#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod defs;
pub mod writer;

pub use crate::writer::{emit_relocatable_macho, emit_universal_macho};

#[cfg(test)]
mod tests {
    use crate::{defs::*, emit_relocatable_macho, emit_universal_macho};
    use obsidian_codegen_aarch64::compile_module as compile_aarch64;
    use obsidian_codegen_x86::compile_module as compile_x86;
    use obsidian_ir as ir;

    #[test]
    fn test_arm64_and_x86_64_macho_object_generation() {
        let mut m = ir::Module::new("macho_test", ir::Target::aarch64_macos());
        let ext = m.add_extern_fn("puts", ir::CallConv::C, &[ir::Type::Ptr], &[ir::Type::I32], false);
        let msg = m.add_const("MSG", b"hello\0", 1);

        let mut f = ir::Function::export("say_hi", &[], &[ir::Type::I32]);
        let ptr = f.ptr_sym(msg, 0);
        let res = f.call(ext, &[ptr], ir::Type::I32);
        f.ret(&[res]);
        m.add_function(f);

        let art = compile_aarch64(&m);
        let macho = emit_relocatable_macho(&art);
        assert_eq!(u32::from_le_bytes(macho[0..4].try_into().unwrap()), MH_MAGIC_64);
        assert_eq!(u32::from_le_bytes(macho[4..8].try_into().unwrap()), CPU_TYPE_ARM64);
        assert_eq!(u32::from_le_bytes(macho[12..16].try_into().unwrap()), MH_OBJECT);

        let mut m_x86 = ir::Module::new("macho_x86", ir::Target::x86_64_macos());
        let mut fx = ir::Function::export("id", &[ir::Type::I64], &[ir::Type::I64]);
        let [vx] = fx.params();
        fx.ret(&[vx]);
        m_x86.add_function(fx);

        let macho_x86 = emit_relocatable_macho(&compile_x86(&m_x86));
        let fat = emit_universal_macho(&macho_x86, &macho);
        assert_eq!(u32::from_be_bytes(fat[0..4].try_into().unwrap()), FAT_MAGIC);
    }
}
