# `obsidian-ir`

**The `.obs` SSA Bedrock Intermediate Representation (`ir::Module`, `ir::Target`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`).**

`obsidian-ir` is a `#![no_std]` (with `alloc`), zero-dependency SSA Intermediate Representation designed around a **16-byte cache-line-aligned `InstNode` arena**, **Basic Block Parameters**, **Zero-Heap Metadata**, and **1:1 `.obs` textual formatting**.

---

## Core Design Principles

1. **SSA Bedrock (`.obs`):** High-level frontend constructs (`pub`, `const fn`, `&self`, `&[u8]`, loops, `match`) flatten into **10 primitive types** (`i1`, `i8..i128`, `f32`, `f64`, `ptr`, `v128`) and namespaced general-purpose opcodes (`i64.add`, `ptr.add`, `i64.load`, `i64.cmp.ugt`).
2. **`ir::Value(u32)` Is a Direct Array Index (`Function::insts[i]`):** Every `InstNode` is strictly 16 bytes (`#[repr(C, align(16))]`). Looking up the opcode, type, reader count (`use_count`), or operands of any `Value` is an $O(1)$ array lookup with zero pointer chasing.
3. **Ergonomic Rust API (`ir::Function`, `f.params()`, `f.switch_to()`):** Function parameters and basic-block parameters destructure directly into Rust arrays (`let [v_self, v_offset] = f.params();`, `let [v_i, v_sum] = f.switch_to(loop_bb);`).

---

## End-to-End Usage Examples

### 1. Single-Block `ir::Function` in 4 Lines of Rust

```rust
use obsidian_ir as ir;

let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
let [v_self] = f.params();

let v_n = f.i64_load(v_self, 8);
f.ret(&[v_n]);

assert_eq!(
    f.to_string(),
    "\
export fn @Slice_len(%v0: ptr) -> i64 {
entry:
    %v1 = i64.load %v0, offset=8
    ret %v1
}
"
);
```

### 2. Loops with Basic Block Parameters (`loop(%i: i64, %sum: i64):`)

Instead of fragile `phi` pseudo-instructions, `obsidian-ir` uses **Basic Block Parameters**. Calling `f.switch_to(loop_bb)` switches the active block and returns its parameter `Value`s in one step:

```rust
use obsidian_ir as ir;

let mut f = ir::Function::export("sum_to_n", &[ir::Type::I64], &[ir::Type::I64]);
let [v_n] = f.params();

let loop_bb = f.new_block("loop", &[ir::Type::I64, ir::Type::I64]);
let exit_bb = f.new_block("exit", &[ir::Type::I64]);

// entry:
let zero = f.i64_const(0);
f.br(loop_bb, &[zero, zero]);

// loop(%v1: i64, %v2: i64):
let [v_i, v_sum] = f.switch_to(loop_bb);
let v_next_sum = f.i64_add(v_sum, v_i);
let v_next_i   = f.i64_add_imm(v_i, 1);
let v_cont     = f.i64_cmp(ir::IntCC::Ult, v_next_i, v_n);
f.br_if(v_cont, loop_bb, &[v_next_i, v_next_sum], exit_bb, &[v_next_sum]);

// exit(%v3: i64):
let [v_final] = f.switch_to(exit_bb);
f.ret(&[v_final]);

assert_eq!(f.value_opcode(v_next_sum), ir::Opcode::Add);
assert_eq!(f.use_count(v_next_sum), 2); // used by both loop_bb and exit_bb branch args!
```

### 3. Multi-Value Hardware Instructions (`add.ovf_u`, `mul.wide_u`, `divrem.u`)

Hardware operations that produce two outputs in registers bind directly to `(ir::Value, ir::Value)` tuples via a secondary `Opcode::ResultProj` slot:

```rust
use obsidian_ir as ir;

let mut f = ir::Function::export("checked_math", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_a, v_b] = f.params();

// %v2, %v3 = i64.add.ovf_u %v0, %v1
let (v_sum, v_ovf) = f.i64_add_ovf_u(v_a, v_b);
assert_eq!(f.value_type(v_sum), ir::Type::I64);
assert_eq!(f.value_type(v_ovf), ir::Type::I1);

// %v4, %v5 = i64.mul.wide_u %v2, %v1
let (v_lo, v_hi) = f.i64_mul_wide_u(v_sum, v_b);
let v_res = f.i64_select(v_ovf, v_hi, v_lo);
f.ret(&[v_res]);
```

### 4. Complete `ir::Module` with `ir::Target`, Metadata, Globals & `.obs` Formatting

```rust
use obsidian_ir as ir;

let mut module = ir::Module::new("buffer_mod", ir::Target::aarch64_macos());

let _hex = module.add_const("HEX_CHARS", b"0123456789abcdef", 1);
let _cnt = module.add_global_i64("REQUEST_COUNT", 0, 8);
let _tls = module.add_tls_i32("LAST_ERROR", 0, 4);
let _wrt = module.add_extern_fn(
    "write",
    ir::CallConv::C,
    &[ir::Type::I32, ir::Type::Ptr, ir::Type::I64],
    &[ir::Type::I64],
    false,
);

let mut f = ir::Function::export(
    "Buffer_slice_from",
    &[ir::Type::Ptr, ir::Type::I64],
    &[ir::Type::Ptr, ir::Type::I64],
);
f.set_inline(ir::InlineAttr::Always);

let [v_self, v_offset] = f.params();
let in_bounds = f.new_block("in_bounds", &[]);
let ret_none  = f.new_block("ret_none", &[]);

// entry:
let v_len = f.i64_load(v_self, 8);
let v_oob = f.i64_cmp(ir::IntCC::Ugt, v_offset, v_len);
f.br_if_unlikely(v_oob, ret_none, &[], in_bounds, &[]);

// in_bounds:
f.switch_to_block(in_bounds);
let v_base    = f.ptr_load(v_self, 0);
let v_sub_ptr = f.ptr_add(v_base, v_offset);
let v_rem_len = f.i64_sub(v_len, v_offset);
f.ret(&[v_sub_ptr, v_rem_len]);

// ret_none:
f.switch_to_block(ret_none);
let v_null = f.ptr_const(0);
let v_zero = f.i64_const(0);
f.ret(&[v_null, v_zero]);

module.add_function(f);

let obs_text = module.to_string();
assert!(obs_text.contains("module @buffer_mod"));
assert!(obs_text.contains("target arch=aarch64, os=macos, abi=darwin"));
assert!(obs_text.contains("export inline(always) fn @Buffer_slice_from(%v0: ptr, %v1: i64) -> (ptr, i64)"));
assert!(obs_text.contains("br_if.unlikely %v3, ret_none, in_bounds"));
```
