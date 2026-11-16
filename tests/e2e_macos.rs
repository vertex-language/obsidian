//! End-to-End macOS (`arm64`) Execution Tests for Obsidian:
//! 1. `.obs` IR -> `obsidian-codegen-aarch64` -> `obsidian-macho` (`.o`) -> Native macOS Executable (`stdout` + exit code).
//! 2. `.obs` IR -> `obsidian-codegen-aarch64` -> In-Memory `MAP_JIT` execution of `@max_pair_at` (`LDP` + barrel shift + `CSEL`).
//! 3. `.obs` IR -> `obsidian-codegen-aarch64` -> In-Memory `MAP_JIT` execution of `@sum_to_n` (Basic Block Parameter loop).

use std::process::Command;
use obsidian::{codegen_aarch64, ir, macho};

#[test]
fn test_e2e_macos_native_macho_executable_stdout_and_exit_code() {
    let target = ir::Target::aarch64_macos();
    let mut module = ir::Module::new("hello_mac", target);

    let msg = b"Hello from Obsidian IR on Mac!\n";
    let greeting_sym = module.add_const("GREETING", msg, 8);
    let write_sym = module.add_extern_fn(
        "write",
        ir::CallConv::C,
        &[ir::Type::I32, ir::Type::Ptr, ir::Type::I64],
        &[ir::Type::I64],
        false,
    );

    // 1. Helper function: `export fn @compute_magic(%a: i64, %b: i64) -> i64`
    //    Computes `(a + b) * 2` -> for (13, 8) returns 42!
    let mut fn_magic = ir::Function::export(
        "compute_magic",
        &[ir::Type::I64, ir::Type::I64],
        &[ir::Type::I64],
    );
    let [v_a, v_b] = fn_magic.params();
    let v_sum = fn_magic.i64_add(v_a, v_b);
    let v_ans = fn_magic.i64_shl_imm(v_sum, 1);
    fn_magic.ret(&[v_ans]);
    let magic_sym = module.add_function(fn_magic);

    // 2. Entry function: `export fn @main() -> i64`
    //    Calls `@write(1, @GREETING, 31)` and returns `@compute_magic(13, 8)` (42).
    let mut fn_main = ir::Function::export("main", &[], &[ir::Type::I64]);
    let v_fd  = fn_main.i32_const(1);
    let v_buf = fn_main.ptr_sym(greeting_sym, 0);
    let v_len = fn_main.i64_const(msg.len() as i64);
    let _written = fn_main.call(write_sym, &[v_fd, v_buf, v_len], ir::Type::I64);

    let v_13 = fn_main.i64_const(13);
    let v_8  = fn_main.i64_const(8);
    let v_exit_code = fn_main.call(magic_sym, &[v_13, v_8], ir::Type::I64);
    fn_main.ret(&[v_exit_code]);
    module.add_function(fn_main);

    // Print the human-readable `.obs` IR blueprint:
    println!("--- Generated .obs IR Module ---\n{module}");

    // 3. Compile `.obs` IR -> AArch64 machine code `Artifact` -> Mach-O 64 `.o` bytes:
    let artifact = codegen_aarch64::compile_module(&module);
    let macho_o_bytes = macho::emit_relocatable_macho(&artifact);

    let tmp_dir = std::env::temp_dir();
    let obj_path = tmp_dir.join("obsidian_e2e_hello.o");
    let exe_path = tmp_dir.join("obsidian_e2e_hello_bin");
    std::fs::write(&obj_path, &macho_o_bytes).expect("failed to write Mach-O .o file");

    // 4. Link the Mach-O `.o` file into a standalone macOS arm64 executable and run it:
    let link_status = Command::new("cc")
        .arg(&obj_path)
        .arg("-o")
        .arg(&exe_path)
        .status()
        .expect("failed to invoke system linker on Mach-O .o");
    assert!(link_status.success(), "Mach-O .o linking failed");

    let output = Command::new(&exe_path)
        .output()
        .expect("failed to execute compiled Obsidian binary");

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    println!("--- Program Output ---\n{stdout_str}");
    assert_eq!(stdout_str, "Hello from Obsidian IR on Mac!\n");
    assert_eq!(output.status.code(), Some(42));

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&exe_path);
}

#[test]
fn test_e2e_macos_in_memory_jit_max_pair_at_and_loop() {
    let target = ir::Target::aarch64_macos();

    // 1. Compile `@max_pair_at(%base: ptr, %idx: i64) -> i64`:
    let mut fn_max = ir::Function::export(
        "max_pair_at",
        &[ir::Type::Ptr, ir::Type::I64],
        &[ir::Type::I64],
    );
    let [v_base, v_idx] = fn_max.params();
    let v_scaled = fn_max.i64_shl_imm(v_idx, 3);
    let v_elem   = fn_max.ptr_add(v_base, v_scaled);
    let v_a      = fn_max.i64_load(v_elem, 16);
    let v_b      = fn_max.i64_load(v_elem, 24);
    let v_gt     = fn_max.i64_cmp(ir::IntCC::Sgt, v_a, v_b);
    let v_best   = fn_max.i64_select(v_gt, v_a, v_b);
    fn_max.ret(&[v_best]);

    let compiled_max = codegen_aarch64::compile_function(target, &fn_max);

    // Execute `@max_pair_at` directly in memory on Apple Silicon via `MAP_JIT`:
    unsafe {
        let jit_buf = JitBuffer::new(&compiled_max.code);
        let max_pair_fn: unsafe extern "C" fn(*const i64, u64) -> i64 =
            core::mem::transmute(jit_buf.ptr);

        // Array elements: index 0..8
        // For idx = 1: elem = base + 8, loads *(elem + 16) = data[3] = 999 and *(elem + 24) = data[4] = 444 -> returns 999!
        // For idx = 2: elem = base + 16, loads *(elem + 16) = data[4] = 444 and *(elem + 24) = data[5] = 777 -> returns 777!
        let data: [i64; 8] = [10, 20, 30, 999, 444, 777, 888, 123];
        assert_eq!(max_pair_fn(data.as_ptr(), 1), 999);
        assert_eq!(max_pair_fn(data.as_ptr(), 2), 777);
        assert_eq!(max_pair_fn(data.as_ptr(), 3), 888);
    }

    // 2. Compile `@sum_to_n(%n: i64) -> i64` with Basic Block Parameters (`loop(%i, %sum):`):
    let mut fn_sum = ir::Function::export("sum_to_n", &[ir::Type::I64], &[ir::Type::I64]);
    let [v_n] = fn_sum.params();
    let loop_bb = fn_sum.new_block("loop", &[ir::Type::I64, ir::Type::I64]);
    let exit_bb = fn_sum.new_block("exit", &[ir::Type::I64]);

    let zero = fn_sum.i64_const(0);
    fn_sum.br(loop_bb, &[zero, zero]);

    let [v_i, v_sum] = fn_sum.switch_to(loop_bb);
    let v_next_sum = fn_sum.i64_add(v_sum, v_i);
    let v_next_i   = fn_sum.i64_add_imm(v_i, 1);
    let v_cont     = fn_sum.i64_cmp(ir::IntCC::Ult, v_next_i, v_n);
    fn_sum.br_if(v_cont, loop_bb, &[v_next_i, v_next_sum], exit_bb, &[v_next_sum]);

    let [v_final] = fn_sum.switch_to(exit_bb);
    fn_sum.ret(&[v_final]);

    let compiled_sum = codegen_aarch64::compile_function(target, &fn_sum);

    unsafe {
        let jit_buf = JitBuffer::new(&compiled_sum.code);
        let sum_fn: unsafe extern "C" fn(u64) -> u64 = core::mem::transmute(jit_buf.ptr);
        // sum(0..100) = 99 * 100 / 2 = 4950
        assert_eq!(sum_fn(100), 4950);
        // sum(0..1000) = 999 * 1000 / 2 = 499_500
        assert_eq!(sum_fn(1000), 499_500);
    }
}

/// Minimal macOS `MAP_JIT` executable page wrapper using `mmap` + `pthread_jit_write_protect_np` + `sys_icache_invalidate`.
struct JitBuffer {
    ptr: *mut u8,
    size: usize,
}

unsafe extern "C" {
    fn mmap(
        addr: *mut core::ffi::c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut core::ffi::c_void;
    fn munmap(addr: *mut core::ffi::c_void, len: usize) -> i32;
    fn pthread_jit_write_protect_np(enabled: i32);
    fn sys_icache_invalidate(start: *const core::ffi::c_void, len: usize);
}

impl JitBuffer {
    unsafe fn new(code: &[u8]) -> Self {
        const PROT_READ: i32 = 0x01;
        const PROT_WRITE: i32 = 0x02;
        const PROT_EXEC: i32 = 0x04;
        const MAP_PRIVATE: i32 = 0x0002;
        const MAP_ANON: i32 = 0x1000;
        const MAP_JIT: i32 = 0x0800;

        let size = 16384; // 16 KiB macOS ARM64 page
        let ptr = unsafe {
            mmap(
                core::ptr::null_mut(),
                size,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                MAP_PRIVATE | MAP_ANON | MAP_JIT,
                -1,
                0,
            ) as *mut u8
        };
        assert!(!ptr.is_null() && (ptr as usize) != usize::MAX, "mmap MAP_JIT failed");

        unsafe {
            pthread_jit_write_protect_np(0); // Enable writing to MAP_JIT page
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            pthread_jit_write_protect_np(1); // Switch MAP_JIT page back to R-X
            sys_icache_invalidate(ptr as *const core::ffi::c_void, code.len());
        }

        Self { ptr, size }
    }
}

impl Drop for JitBuffer {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr as *mut core::ffi::c_void, self.size);
        }
    }
}
