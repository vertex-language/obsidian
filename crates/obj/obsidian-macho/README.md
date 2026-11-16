# `obsidian-macho`

**From-Scratch Apple Mach-O 64 Relocatable Object (`.o` `MH_OBJECT`) and Universal Fat Binary (`FAT_MAGIC`) Writer for Obsidian.**

`obsidian-macho` is a `#![no_std]` (`alloc`), zero-dependency Mach-O serializer that converts an [`obsidian_obj::Artifact`] (`arm64` or `x86_64`) into a spec-compliant macOS/iOS Mach-O 64 `.o` relocatable object file or combines `x86_64` and `arm64` Mach-O slices into an Apple Universal Fat Binary.

---

## Key Capabilities

1. **Relocatable Mach-O 64 Object Emission (`emit_relocatable_macho` $\rightarrow$ `.o` `MH_OBJECT`):**
   - Emits `mach_header_64` (`0xFEEDFACF`), `LC_SEGMENT_64` (`("__TEXT", "__text")`, `("__TEXT", "__const")`, `("__DATA", "__data")`, `("__DATA", "__bss")`), `LC_BUILD_VERSION`, `LC_SYMTAB`, and `LC_DYSYMTAB`.
   - Automatically prepends the Darwin `'_'` symbol prefix (`_main`, `_write`) and partitions `nlist_64` into local defined, external defined (`N_SECT | N_EXT`), and undefined external (`N_UNDF | N_EXT`) groups as required by Apple's `ld-prime` / `ld64` linker.
   - Encodes `ARM64_RELOC_BRANCH26`, `ARM64_RELOC_PAGE21`, `ARM64_RELOC_PAGEOFF12`, `X86_64_RELOC_BRANCH`, and `X86_64_RELOC_GOT_LOAD` relocation records.
2. **Apple Universal Fat Binary Emission (`emit_universal_macho` $\rightarrow$ `FAT_MAGIC`):**
   - Combines an `x86_64` Mach-O slice and an `arm64` Mach-O slice into a single 16 KiB (`0x4000`) page-aligned Universal binary (`0xCAFEBABE`).

---

## End-to-End Usage Examples

### 1. From `.obs` IR $\rightarrow$ `obsidian-codegen-aarch64` $\rightarrow$ macOS `arm64` Mach-O `.o` File

```rust
use obsidian_codegen_aarch64::compile_module;
use obsidian_ir as ir;
use obsidian_macho::{defs::*, emit_relocatable_macho};

let mut module = ir::Module::new("darwin_mod", ir::Target::aarch64_macos());

let mut f = ir::Function::export("add_two", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_a, v_b] = f.params();
let v_sum = f.i64_add(v_a, v_b);
f.ret(&[v_sum]);
module.add_function(f);

let artifact = compile_module(&module);
let macho_bytes = emit_relocatable_macho(&artifact);

// Verify the 32-byte `mach_header_64`:
let magic = u32::from_le_bytes(macho_bytes[0..4].try_into().unwrap());
let cputype = u32::from_le_bytes(macho_bytes[4..8].try_into().unwrap());
let filetype = u32::from_le_bytes(macho_bytes[12..16].try_into().unwrap());
assert_eq!(magic, MH_MAGIC_64);
assert_eq!(cputype, CPU_TYPE_ARM64);
assert_eq!(filetype, MH_OBJECT);
```

### 2. Building an Apple Universal Fat Binary (`x86_64` + `arm64`)

```rust
use obsidian_codegen_aarch64::compile_module as compile_arm64;
use obsidian_codegen_x86::compile_module as compile_x86;
use obsidian_ir as ir;
use obsidian_macho::{defs::*, emit_relocatable_macho, emit_universal_macho};

// Compile for macOS x86_64:
let mut mod_x86 = ir::Module::new("univ", ir::Target::x86_64_macos());
let mut f1 = ir::Function::export("ret_42", &[], &[ir::Type::I64]);
let c42 = f1.i64_const(42);
f1.ret(&[c42]);
mod_x86.add_function(f1);
let macho_x86 = emit_relocatable_macho(&compile_x86(&mod_x86));

// Compile for macOS arm64:
let mut mod_arm = ir::Module::new("univ", ir::Target::aarch64_macos());
let mut f2 = ir::Function::export("ret_42", &[], &[ir::Type::I64]);
let c42_arm = f2.i64_const(42);
f2.ret(&[c42_arm]);
mod_arm.add_function(f2);
let macho_arm = emit_relocatable_macho(&compile_arm64(&mod_arm));

// Pack both slices into a 16 KiB-aligned Universal Fat Binary:
let universal = emit_universal_macho(&macho_x86, &macho_arm);
let fat_magic = u32::from_be_bytes(universal[0..4].try_into().unwrap());
let nfat_arch = u32::from_be_bytes(universal[4..8].try_into().unwrap());
assert_eq!(fat_magic, FAT_MAGIC);
assert_eq!(nfat_arch, 2);
```
