#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod liveness;
pub mod scan;

pub use crate::liveness::{LiveRange, LivenessInfo};
pub use crate::scan::{allocate_registers, RegAllocResult};

#[cfg(test)]
mod tests {
    use crate::allocate_registers;
    use obsidian_ir as ir;
    use obsidian_isa_aarch64::regs as aarch64_regs;
    use obsidian_isa_m68k::regs as m68k_regs;
    use obsidian_isa_x86::regs as x86_regs;

    #[test]
    fn test_x86_two_address_and_return_coalescing() {
        let mut f = ir::Function::export(
            "sum_two",
            &[ir::Type::I64, ir::Type::I64],
            &[ir::Type::I64],
        );
        let [v_a, v_b] = f.params();
        let v_sum = f.i64_add(v_a, v_b);
        f.ret(&[v_sum]);

        let ra = allocate_registers(ir::Target::x86_64_linux(), &f);
        assert_eq!(ra.loc(v_a).as_x86_gpr(), Some(x86_regs::Gpr::Rdi));
        assert_eq!(ra.loc(v_b).as_x86_gpr(), Some(x86_regs::Gpr::Rsi));
        // `v_sum` is returned directly, so it coalesces into `RAX`:
        assert_eq!(ra.loc(v_sum).as_x86_gpr(), Some(x86_regs::Gpr::Rax));
        assert!(ra.is_leaf);
        assert_eq!(ra.frame.sp_sub_bytes, 0);
    }

    #[test]
    fn test_false_dep_ring_rotates_even_when_dead() {
        let mut f = ir::Function::export("ring_test", &[ir::Type::I64], &[ir::Type::I64]);
        let [v_x] = f.params();

        // Sequential chain of `ctz` instructions where each previous result dies:
        // Without ring rotation, a naive linear-scan allocator would assign RAX -> RAX -> RAX,
        // triggering hardware false-dependency stalls on x86-64!
        let c1 = f.i64_ctz(v_x);
        let c2 = f.i64_ctz(c1);
        let c3 = f.i64_ctz(c2);
        f.ret(&[c3]);

        let ra = allocate_registers(ir::Target::x86_64_linux(), &f);
        assert_ne!(ra.loc(c1), ra.loc(c2));
    }

    #[test]
    fn test_m68k_split_pointer_and_data_allocation() {
        let mut f = ir::Function::export(
            "m68k_ptr_offset",
            &[ir::Type::Ptr, ir::Type::I64],
            &[ir::Type::I32],
        );
        let [v_base, v_off] = f.params();
        let v_ptr = f.ptr_add(v_base, v_off);
        let v_val = f.i32_load(v_ptr, 4);
        f.ret(&[v_val]);

        let ra = allocate_registers(ir::Target::m68k_bare(), &f);
        // `v_base` is in A0, `v_ptr` allocates from the Address Register bank (`A0` or `A1`),
        // and `v_val` (`i32`) allocates from the Data Register bank (`D0`)!
        assert_eq!(ra.loc(v_base).as_m68k_addr(), Some(m68k_regs::AddrReg::A0));
        assert!(ra.loc(v_ptr).as_m68k_addr().is_some());
        assert_eq!(ra.loc(v_val).as_m68k_data(), Some(m68k_regs::DataReg::D0));
    }

    #[test]
    fn test_loop_block_params_and_aarch64_allocation() {
        let mut f = ir::Function::export("loop_sum", &[ir::Type::I64], &[ir::Type::I64]);
        let [v_n] = f.params();
        let loop_bb = f.new_block("loop", &[ir::Type::I64, ir::Type::I64]);
        let exit_bb = f.new_block("exit", &[ir::Type::I64]);

        let zero = f.i64_const(0);
        f.br(loop_bb, &[zero, zero]);

        let [v_i, v_sum] = f.switch_to(loop_bb);
        let v_next_sum = f.i64_add(v_sum, v_i);
        let v_next_i   = f.i64_add_imm(v_i, 1);
        let v_cont     = f.i64_cmp(ir::IntCC::Ult, v_next_i, v_n);
        f.br_if(v_cont, loop_bb, &[v_next_i, v_next_sum], exit_bb, &[v_next_sum]);

        let [v_final] = f.switch_to(exit_bb);
        f.ret(&[v_final]);

        let ra = allocate_registers(ir::Target::aarch64_macos(), &f);
        assert_eq!(ra.loc(v_n).as_aarch64_gpr(), Some(aarch64_regs::Gpr::X0));
        assert!(ra.loc(v_i).as_aarch64_gpr().is_some());
        assert!(ra.loc(v_sum).as_aarch64_gpr().is_some());
        assert_ne!(ra.loc(v_i), ra.loc(v_sum));
    }
}
