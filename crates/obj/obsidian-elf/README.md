# `obsidian-elf`

**From-Scratch System V ELF64 Relocatable Object (`.o` `ET_REL`) and Standalone Executable (`ET_EXEC`) Writer for Obsidian.**

`obsidian-elf` is a `#![no_std]` (`alloc`), zero-dependency ELF serializer that converts an [`obsidian_obj::Artifact`] into a spec-compliant Little-Endian or Big-Endian ELF64 `.o` relocatable object file or a standalone runnable ELF64 executable binary.

---

## Key Capabilities

1. **Relocatable Object Emission (`emit_relocatable_elf` $\rightarrow$ `.o` `ET_REL`):**
   - Emits `.text`, `.rodata`, `.data`, `.bss`, `.rela.text`, `.symtab`, `.strtab`, and `.shstrtab` sections.
   - Automatically partitions `.symtab` so all `STB_LOCAL` symbols precede all `STB_GLOBAL` symbols and sets `sh_info` to the first global symbol index as required by the System V ELF specification.
   - Maps architecture-neutral `RelocKind`s to official `R_X86_64_*`, `R_AARCH64_*`, `R_390_*`, and `R_68K_*` relocation codes.
2. **Little-Endian & Big-Endian Support (`ELFDATA2LSB` & `ELFDATA2MSB`):**
   - Automatically serializes all ELF headers, section headers, symbol entries, and relocations in Little-Endian (`x86_64`, `aarch64`, `riscv64`) or Big-Endian (`systemz` `s390x`, `m68k`, `sparc64`) based on `Artifact::target`.
3. **Standalone Executable Emission (`emit_executable_elf` $\rightarrow$ `ET_EXEC`):**
   - Resolves internal relocations across `.text`, `.rodata`, `.data`, and `.bss` and writes a runnable ELF64 executable with `PT_LOAD` program headers (`PF_R | PF_X` and `PF_R | PF_W`).

---

## End-to-End Usage Examples

### 1. From `.obs` IR $\rightarrow$ `obsidian-codegen-x86` $\rightarrow$ `.o` Relocatable ELF File

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_elf::{defs::*, emit_relocatable_elf};
use obsidian_ir as ir;

let mut module = ir::Module::new("math_mod", ir::Target::x86_64_linux());

let mut f = ir::Function::export("add_two", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_a, v_b] = f.params();
let v_sum = f.i64_add(v_a, v_b);
f.ret(&[v_sum]);
module.add_function(f);

let artifact = compile_module(&module);
let elf_o_bytes = emit_relocatable_elf(&artifact);

// Verify the 64-byte ELF64 header:
assert_eq!(&elf_o_bytes[0..4], &ELF_MAGIC);
assert_eq!(elf_o_bytes[4], ELFCLASS64);
assert_eq!(elf_o_bytes[5], ELFDATA2LSB);
let e_type = u16::from_le_bytes([elf_o_bytes[16], elf_o_bytes[17]]);
let e_machine = u16::from_le_bytes([elf_o_bytes[18], elf_o_bytes[19]]);
assert_eq!(e_type, ET_REL);
assert_eq!(e_machine, 62); // EM_X86_64
```

### 2. Emitting a Standalone Linked ELF64 Executable (`emit_executable_elf`)

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_elf::{defs::*, emit_executable_elf};
use obsidian_ir as ir;

let mut module = ir::Module::new("app_mod", ir::Target::x86_64_linux());
let msg_sym = module.add_const("BANNER", b"Obsidian\0", 8);

let mut main_fn = ir::Function::export("_start", &[], &[ir::Type::I64]);
let v_ptr = main_fn.ptr_sym(msg_sym, 0);
let v_first = main_fn.i64_load(v_ptr, 0);
main_fn.ret(&[v_first]);
module.add_function(main_fn);

let artifact = compile_module(&module);
let exe_bytes = emit_executable_elf(&artifact, "_start").unwrap();

// Verify `ET_EXEC` header and non-zero entry point virtual address (`0x0040_00B0`):
assert_eq!(&exe_bytes[0..4], &ELF_MAGIC);
let e_type = u16::from_le_bytes([exe_bytes[16], exe_bytes[17]]);
let e_entry = u64::from_le_bytes(exe_bytes[24..32].try_into().unwrap());
assert_eq!(e_type, ET_EXEC);
assert_eq!(e_entry, 0x0040_00B0);
```

### 3. Big-Endian ELF64 Generation for IBM SystemZ (`s390x`)

```rust
use obsidian_elf::{defs::*, emit_relocatable_elf};
use obsidian_ir::{Linkage, Target};
use obsidian_obj::Artifact;

let mut art = Artifact::new("s390x_mod", Target::systemz_linux());
art.define_function("s390_ret", Linkage::Export, 4, 0x07, &[0x07, 0xFE]); // `br %r14`

let elf_be = emit_relocatable_elf(&art);
assert_eq!(elf_be[5], ELFDATA2MSB); // Big-Endian!
let e_machine = u16::from_be_bytes([elf_be[18], elf_be[19]]);
assert_eq!(e_machine, 22); // EM_S390
```
