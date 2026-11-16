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
    fn test_slice_len_aarch64_codegen() {
        let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
        let [v_self] = f.params();
        let v_n = f.i64_load(v_self, 8);
        f.ret(&[v_n]);

        let compiled = compile_function(ir::Target::aarch64_macos(), &f);
        assert_eq!(compiled.code.len(), 8);
        let w0 = u32::from_le_bytes(compiled.code[0..4].try_into().unwrap());
        let w1 = u32::from_le_bytes(compiled.code[4..8].try_into().unwrap());
        assert_eq!(w0, 0xF940_0400); // ldr x0, [x0, #8]
        assert_eq!(w1, 0xD65F_03C0); // ret
    }

    #[test]
    fn test_stp_store_pair_fusion() {
        let mut f = ir::Function::export(
            "store_pair",
            &[ir::Type::Ptr, ir::Type::I64, ir::Type::I64],
            &[],
        );
        let [v_ptr, v_a, v_b] = f.params();
        f.i64_store(v_ptr, v_a, 0);
        f.i64_store(v_ptr, v_b, 8);
        f.ret(&[]);

        let compiled = compile_function(ir::Target::aarch64_macos(), &f);
        // Two 8-byte stores fused into 1 `stp x1, x2, [x0, #0]` + `ret` = 8 bytes total!
        assert_eq!(compiled.code.len(), 8);
        let w0 = u32::from_le_bytes(compiled.code[0..4].try_into().unwrap());
        assert_eq!(w0, 0xA900_0801); // stp x1, x2, [x0]
    }

    #[test]
    fn test_adrp_and_bl_relocations() {
        let mut m = ir::Module::new("aarch64_reloc", ir::Target::aarch64_macos());
        let cnt_sym = m.add_global_i64("COUNTER", 0, 8);
        let ext_sym = m.add_extern_fn("helper", ir::CallConv::C, &[ir::Type::Ptr], &[ir::Type::I64], false);

        let mut f = ir::Function::export("call_with_global", &[], &[ir::Type::I64]);
        let v_ptr = f.ptr_sym(cnt_sym, 0);
        let v_res = f.call(ext_sym, &[v_ptr], ir::Type::I64);
        f.ret(&[v_res]);
        m.add_function(f);

        let art = compile_module(&m);
        // 3 relocations: ADRP Page21, ADD PageOff12, and BL Call26:
        assert_eq!(art.relocs.len(), 3);
    }
}
