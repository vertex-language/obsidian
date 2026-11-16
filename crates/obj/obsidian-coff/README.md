# `obsidian-coff`

**From-Scratch Windows / UEFI COFF Relocatable Object (`.obj`) and PE32+ Executable (`.exe`) Writer for Obsidian.**

`obsidian-coff` is a `#![no_std]` (`alloc`), zero-dependency COFF and PE32+ serializer that converts an [`obsidian_obj::Artifact`] (`x86_64` or `aarch64`) into a spec-compliant Windows `.obj` relocatable object file or a standalone 64-bit PE32+ `.exe` executable.

---

## Key Capabilities

1. **Relocatable COFF `.obj` Emission (`emit_relocatable_coff`):**
   - Emits `IMAGE_FILE_HEADER` (`IMAGE_FILE_MACHINE_AMD64 = 0x8664` or `IMAGE_FILE_MACHINE_ARM64 = 0xAA64`), 4 `IMAGE_SECTION_HEADER` entries (`.text`, `.rdata`, `.data`, `.bss`), 10-byte `IMAGE_RELOCATION` records, 18-byte `IMAGE_SYMBOL` records (with automatic inline short names $\le 8$ bytes and string-table offsets for long names $> 8$ bytes), and the COFF string table.
2. **Standalone PE32+ `.exe` Emission (`emit_executable_pe`):**
   - Resolves internal relocations and writes a complete 64-bit PE32+ image (`MZ` DOS header, `"PE\0\0"` signature, `IMAGE_FILE_HEADER`, 240-byte `IMAGE_OPTIONAL_HEADER64` with `0x020B` magic, and 512-byte aligned section bodies).

---

## End-to-End Usage Examples

### 1. From `.obs` IR $\rightarrow$ `obsidian-codegen-x86` $\rightarrow$ Windows x64 COFF `.obj` File

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_coff::{defs::*, emit_relocatable_coff};
use obsidian_ir as ir;

let mut module = ir::Module::new("win_mod", ir::Target::x86_64_windows());

let mut f = ir::Function::export("win_add_two", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_a, v_b] = f.params(); // Windows x64 ABI: RCX, RDX
let v_sum = f.i64_add(v_a, v_b);
f.ret(&[v_sum]);
module.add_function(f);

let artifact = compile_module(&module);
let obj_bytes = emit_relocatable_coff(&artifact);

// Verify `IMAGE_FILE_HEADER` (`Machine = 0x8664`, `NumberOfSections = 4`):
let machine = u16::from_le_bytes([obj_bytes[0], obj_bytes[1]]);
let num_sects = u16::from_le_bytes([obj_bytes[2], obj_bytes[3]]);
assert_eq!(machine, IMAGE_FILE_MACHINE_AMD64);
assert_eq!(num_sects, 4);
```

### 2. Emitting a Standalone Linked 64-Bit PE32+ `.exe` Executable (`emit_executable_pe`)

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_coff::{defs::*, emit_executable_pe};
use obsidian_ir as ir;

let mut module = ir::Module::new("win_exe", ir::Target::x86_64_windows());
let msg_sym = module.add_const("BANNER", b"WinObsidian\0", 8);

let mut main_fn = ir::Function::export("mainCRTStartup", &[], &[ir::Type::I64]);
let v_ptr = main_fn.ptr_sym(msg_sym, 0);
let v_val = main_fn.i64_load(v_ptr, 0);
main_fn.ret(&[v_val]);
module.add_function(main_fn);

let artifact = compile_module(&module);
let pe_bytes = emit_executable_pe(&artifact, "mainCRTStartup").unwrap();

// Verify `"MZ"` header at offset 0 and `"PE\0\0"` signature at offset 64:
assert_eq!(u16::from_le_bytes([pe_bytes[0], pe_bytes[1]]), MZ_MAGIC);
assert_eq!(&pe_bytes[64..68], &PE_SIGNATURE);
let opt_magic = u16::from_le_bytes([pe_bytes[88], pe_bytes[89]]);
assert_eq!(opt_magic, PE32_PLUS_MAGIC);
```
