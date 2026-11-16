# Obsidian Compiler Infrastructure (`obsidian`)

**A `#![no_std]`-first, zero-dependency, hardware-faithful compiler infrastructure in Rust—from `.obs` Bedrock SSA IR to native machine code (`x86_64`, `AArch64`, `SystemZ`, `M68k`), relocatable object files (`ELF`, `Mach-O`, `COFF`), and linked executables.**

- **Architecture & Target Specification:** [docs/proposed_obsidian.md](docs/proposed_obsidian.md)
- **`.obs` SSA Bedrock IR Specification:** [docs/proposed_obsidian_ir.md](docs/proposed_obsidian_ir.md)
- **Live macOS ARM64 End-to-End Integration Suite:** [tests/e2e_macos.rs](tests/e2e_macos.rs)

---

## Core Design Principles

1. **Layer 0 — 1:1 Silicon Truth (`crates/isa/*`):**
   - Every `obsidian-isa-<arch>` crate (`obsidian-isa-x86`, `obsidian-isa-aarch64`, `obsidian-isa-systemz`, `obsidian-isa-m68k`) is `#![no_std]`, zero-dependency, and `const`-evaluatable.
   - Contains **zero** compiler pseudo-instructions, **zero** virtual registers, and **zero** synthetic macros—only physical registers, hardware condition codes, addressing modes, static `InstDesc` tables, micro-architectural legality queries (`uarch`), and stateless 1-instruction byte/word encoders (`encode`).
2. **Layer 1 — Universal `.obs` SSA Bedrock IR (`crates/obsidian-ir`):**
   - Frontends lower once into `obsidian-ir` (`ir::Module`, `ir::Target`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Sym`, `ir::Type`, `ir::Opcode`) using the **10 Bedrock types** (`i1`, `i8..i128`, `f32`, `f64`, `ptr`, `v128`), namespaced opcodes (`i64.add`, `ptr.add`, `i64.load`), and **Basic Block Parameters** (no `phi` nodes).
   - Every instruction node (`ir::InstNode`) is a fixed **16-byte** `#[repr(C, align(16))]` struct in a contiguous arena (`4` instructions per 64-byte L1 cache line), where `ir::Value(i)` is the direct array index `insts[i]`.
3. **Layer 2 — Static Hardware Oracle, Smart Downstream Packing (`abi`, `regalloc`, `codegen`):**
   - Calling conventions (`obsidian-abi`), linear-scan register allocation (`obsidian-regalloc`), and per-CPU code generators (`obsidian-codegen-x86`, `obsidian-codegen-aarch64`) query `use_count` in $O(1)$ to pack multi-node IR patterns into native addressing modes (SIB `[base + index*scale + disp]`, `LEA`, inline barrel-shifted `ADD`, 128-bit `LDP`/`STP` pairs, flagless `CBZ`/`CBNZ`, and branchless `CMOVcc`/`CSEL`).
4. **Layer 3 — From-Scratch Binary Containers & Built-In Linker (`crates/obj/*`):**
   - `obsidian-obj`, `obsidian-elf`, `obsidian-macho`, `obsidian-coff`, and `obsidian-ld` serialize relocatable `.o`/`.obj` files, Universal Fat Binaries (`FAT_MAGIC`), and standalone linked executables (`ELF64` and `PE32+` `.exe`) directly from memory without external assemblers.

---

## Completed Workspace Crates

| Layer | Crate | Path | Description |
| :--- | :--- | :--- | :--- |
| **Umbrella** | **`obsidian`** | `.` | Top-level crate re-exporting `ir`, `abi`, `regalloc`, `codegen_*`, `obj`, `elf`, `macho`, `coff`, `ld`, and `isa::*`. |
| **Layer 0 (`isa`)** | **`obsidian-isa-x86`** | [`crates/isa/obsidian-isa-x86`](crates/isa/obsidian-isa-x86) | 1:1 `x86` / `x86_64` silicon specification (`REX`, `ModR/M`, `SIB`, `EFLAGS`, `uarch` 32B $\mu\text{op}$-window & macro-fusion rules, `encode`). |
| **Layer 0 (`isa`)** | **`obsidian-isa-aarch64`** | [`crates/isa/obsidian-isa-aarch64`](crates/isa/obsidian-isa-aarch64) | 1:1 64-bit ARM (`A64`) silicon specification (`X0..X30`, `SP`/`XZR`, 13-bit `LogicalImm` `N:immr:imms`, `LDP`/`STP` hazard oracle, `encode`). |
| **Layer 0 (`isa`)** | **`obsidian-isa-systemz`** | [`crates/isa/obsidian-isa-systemz`](crates/isa/obsidian-isa-systemz) | 1:1 IBM z/Architecture (`s390x`) Big-Endian CISC specification (`R0` base-register hazard, 20-bit `Disp20`, `EvenOddPair`, `encode`). |
| **Layer 0 (`isa`)** | **`obsidian-isa-m68k`** | [`crates/isa/obsidian-isa-m68k`](crates/isa/obsidian-isa-m68k) | 1:1 Motorola `680x0` split `D0..D7` / `A0..A7` Big-Endian CISC specification (`MOVEA`/`ADDA` `CCR` preservation, `MOVEM` bitmask reversal, `encode`). |
| **Layer 1 (`ir`)** | **`obsidian-ir`** | [`crates/obsidian-ir`](crates/obsidian-ir) | `.obs` SSA Bedrock IR (`ir::Module`, `ir::Target` for all 25 targets, `ir::Function`, `ir::Block`, `ir::Value`, 16-byte `InstNode`, `.obs` formatter). |
| **Layer 2 (`abi`)** | **`obsidian-abi`** | [`crates/obsidian-abi`](crates/obsidian-abi) | Calling conventions & stack frame layout (`SysV64`, `Win64`, `AAPCS64`, `Darwin ARM64`, `SystemZ s390x`, `M68k` split `A0..A1`/`D0..D1`). |
| **Layer 2 (`regalloc`)** | **`obsidian-regalloc`** | [`crates/obsidian-regalloc`](crates/obsidian-regalloc) | Hardware-aware linear-scan register allocator (return coalescing, 2-op destructive ties, 4-reg false-dep rotation ring, call-spanning promotion). |
| **Layer 2 (`codegen`)** | **`obsidian-codegen-x86`** | [`crates/codegen/obsidian-codegen-x86`](crates/codegen/obsidian-codegen-x86) | `x86_64` code generator ($O(1)$ SIB fusion, 3-op `LEA`, 2-byte `xor` zero idiom, `CMP+Jcc`, `CMOVcc`). |
| **Layer 2 (`codegen`)** | **`obsidian-codegen-aarch64`** | [`crates/codegen/obsidian-codegen-aarch64`](crates/codegen/obsidian-codegen-aarch64) | `AArch64` code generator (inline barrel-shift fusion, 128-bit `LDP`/`STP` pairing, `LogicalImm`, flagless `CBZ`/`CBNZ`, `CSEL`). |
| **Layer 3 (`obj`)** | **`obsidian-obj`** | [`crates/obj/obsidian-obj`](crates/obj/obsidian-obj) | Format-agnostic `Artifact`, `SectionId`, `SymbolRecord`, `Reloc`, and `RelocKind` (with `apply_in_place`). |
| **Layer 3 (`obj`)** | **`obsidian-elf`** | [`crates/obj/obsidian-elf`](crates/obj/obsidian-elf) | From-scratch Little & Big-Endian ELF64 relocatable `.o` (`ET_REL`) and standalone executable (`ET_EXEC`) writer. |
| **Layer 3 (`obj`)** | **`obsidian-macho`** | [`crates/obj/obsidian-macho`](crates/obj/obsidian-macho) | From-scratch Apple Mach-O 64 relocatable `.o` (`MH_OBJECT` with `LC_DYSYMTAB` partitioning) and Universal Fat Binary (`FAT_MAGIC`) writer. |
| **Layer 3 (`obj`)** | **`obsidian-coff`** | [`crates/obj/obsidian-coff`](crates/obj/obsidian-coff) | From-scratch Windows/UEFI COFF `.obj` and standalone 64-bit PE32+ `.exe` writer. |
| **Layer 3 (`obj`)** | **`obsidian-ld`** | [`crates/obj/obsidian-ld`](crates/obj/obsidian-ld) | Built-in in-memory linker (`merge_artifacts`, `link_executable`, `emit_relocatable_object`). |

---

## Quick Start: From `.obs` IR to Native Machine Code & Object Files

```rust
use obsidian::{codegen_aarch64, codegen_x86, elf, ir, macho};

// 1. Build `.obs` SSA Bedrock IR for `@max_pair_at(%base: ptr, %idx: i64) -> i64`:
let mut f = ir::Function::export(
    "max_pair_at",
    &[ir::Type::Ptr, ir::Type::I64],
    &[ir::Type::I64],
);
let [v_base, v_idx] = f.params();

let v_scaled = f.i64_shl_imm(v_idx, 3);
let v_elem   = f.ptr_add(v_base, v_scaled);
let v_a      = f.i64_load(v_elem, 16);
let v_b      = f.i64_load(v_elem, 24);
let v_gt     = f.i64_cmp(ir::IntCC::Sgt, v_a, v_b);
let v_best   = f.i64_select(v_gt, v_a, v_b);
f.ret(&[v_best]);

// 2. Compile to x86-64 machine code (fuses `shl` + `ptr.add` into SIB `[rdi + rsi*8 + 16]`):
let mut mod_x86 = ir::Module::new("math", ir::Target::x86_64_linux());
mod_x86.add_function(f.clone());
let art_x86 = codegen_x86::compile_module(&mod_x86);
let elf_o = elf::emit_relocatable_elf(&art_x86);
assert_eq!(&elf_o[0..4], &[0x7F, b'E', b'L', b'F']);

// 3. Compile the exact same IR to Apple Silicon AArch64 (fuses `add lsl #3` + 128-bit `ldp` + `csel`):
let mut mod_arm = ir::Module::new("math", ir::Target::aarch64_macos());
mod_arm.add_function(f);
let art_arm = codegen_aarch64::compile_module(&mod_arm);
assert_eq!(art_arm.text.len(), 20); // 5 A64 instructions = 20 bytes!
let macho_o = macho::emit_relocatable_macho(&art_arm);
assert_eq!(u32::from_le_bytes(macho_o[0..4].try_into().unwrap()), macho::defs::MH_MAGIC_64);
```

---

## Running the Test Suite

```bash
cargo test
```

Running `cargo test` executes all unit tests across all 15 workspace crates, all README documentation tests, and the live macOS `arm64` end-to-end execution test suite in [`tests/e2e_macos.rs`](tests/e2e_macos.rs).
