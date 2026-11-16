# `obsidian-ld`

**Built-In In-Memory Linker for Obsidian (`merge_artifacts`, `link_executable`, `emit_relocatable_object`, `LinkError`).**

`obsidian-ld` is a `#![no_std]` (`alloc`) linker that combines multiple [`obsidian_obj::Artifact`] modules in memory, resolves cross-module symbol imports (`Extern` in Module A matched to `Export` in Module B), rebases section offsets and relocations, and emits either a merged `.o`/`.obj` object file or a standalone runnable executable (`ELF` or `PE32+` `.exe`) with zero external linker dependencies.

---

## End-to-End Usage Examples

### 1. Linking Two Separate `.obs` Modules (`@main` Calling `@double_val`) Into an Executable

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_ir as ir;
use obsidian_ld::link_executable;

let target = ir::Target::x86_64_linux();

// Module A: defines `export fn @double_val(%x: i64) -> i64`
let mut mod_a = ir::Module::new("math_lib", target);
let mut fn_double = ir::Function::export("double_val", &[ir::Type::I64], &[ir::Type::I64]);
let [v_x] = fn_double.params();
let v_res = fn_double.i64_add(v_x, v_x);
fn_double.ret(&[v_res]);
mod_a.add_function(fn_double);

// Module B: declares `extern fn @double_val` and defines `export fn @_start() -> i64`
let mut mod_b = ir::Module::new("app_main", target);
let double_sym = mod_b.add_extern_fn(
    "double_val",
    ir::CallConv::C,
    &[ir::Type::I64],
    &[ir::Type::I64],
    false,
);
let mut fn_start = ir::Function::export("_start", &[], &[ir::Type::I64]);
let v21 = fn_start.i64_const(21);
let v42 = fn_start.call(double_sym, &[v21], ir::Type::I64);
fn_start.ret(&[v42]);
mod_b.add_function(fn_start);

// Compile both modules independently and link them in memory:
let art_a = compile_module(&mod_a);
let art_b = compile_module(&mod_b);
let exe_bytes = link_executable(&[art_a, art_b], "_start").unwrap();

// Resulting binary is a linked ELF64 `ET_EXEC` executable:
assert_eq!(&exe_bytes[0..4], &[0x7F, b'E', b'L', b'F']);
```

### 2. Detecting Undefined or Duplicate Symbols at Link Time (`LinkError`)

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_ir as ir;
use obsidian_ld::{link_executable, merge_artifacts, LinkError};

let target = ir::Target::x86_64_linux();

let mut mod_missing = ir::Module::new("broken", target);
let missing_sym = mod_missing.add_extern_fn("missing_fn", ir::CallConv::C, &[], &[ir::Type::I64], false);
let mut f = ir::Function::export("_start", &[], &[ir::Type::I64]);
let r = f.call(missing_sym, &[], ir::Type::I64);
f.ret(&[r]);
mod_missing.add_function(f);

let art = compile_module(&mod_missing);

// Full executable linking rejects unresolved imports:
assert_eq!(
    link_executable(&[art.clone()], "_start"),
    Err(LinkError::UndefinedSymbol("missing_fn".into()))
);

// Partial linking (`require_all_resolved = false`, equivalent to `ld -r`) succeeds and keeps the import:
let partial = merge_artifacts(&[art], false).unwrap();
assert_eq!(partial.relocs.len(), 1);
```
