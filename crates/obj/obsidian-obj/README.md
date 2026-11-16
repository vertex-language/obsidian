# `obsidian-obj`

**Format-Agnostic Binary `Artifact`, Section (`SectionId`), Symbol (`SymbolRecord`), and Relocation (`Reloc`, `RelocKind`) Model for Obsidian.**

`obsidian-obj` is a `#![no_std]` (`alloc`), zero-dependency crate that serves as the contract between per-CPU code generation (`obsidian-codegen-<arch>`) and binary container writers (`obsidian-elf`, `obsidian-macho`, `obsidian-coff`, `obsidian-wasm-bin`, `obsidian-ar`, `obsidian-ld`, and `obsidian-jit`).

---

## What Lives in `obsidian-obj`

1. **`Artifact`:** Holds compiled `.text` machine code bytes, `.rodata` constants, `.data`/`.bss` globals, `.tdata`/`.tbss` thread-local variables, `SymbolRecord`s, and `Reloc` patch records.
2. **`SectionId`:** Canonical section kinds (`Text`, `Rodata`, `Data`, `Bss`, `Tdata`, `Tbss`) with automatic section-name mapping for ELF (`.text`, `.rodata`), Mach-O (`("__TEXT", "__text")`, `("__TEXT", "__const")`), and Windows PE/COFF (`.text`, `.rdata`).
3. **`Reloc` & `RelocKind::apply_in_place`:** Supports x86-64 (`PcRel32`, `GotPcRel32`, `Abs64`), AArch64 (`AArch64Call26`, `AArch64AdrpPage21`, `AArch64PageOff12Add`, `AArch64PageOff12Ldr64`), SystemZ (`S390PcRel32Dbl`), and M68k (`PcRel32`, `Abs32`), including direct in-place bitfield patching for `obsidian-ld` and `obsidian-jit`.

---

## End-to-End Usage Examples

### 1. Building an `Artifact` with Functions, Read-Only Data, Globals & Relocations

```rust
use obsidian_ir::{Linkage, Target};
use obsidian_obj::{Artifact, Reloc, RelocKind, SectionId};

let mut art = Artifact::new("demo_mod", Target::x86_64_linux());

// 1. Define a read-only constant in `.rodata` and a zero-initialized global in `.bss`:
let msg_sym = art.define_rodata("GREETING", Linkage::Local, 1, b"hello\0");
let cnt_sym = art.define_data("COUNTER", Linkage::Export, 8, &[0u8; 8]);

// All-zero `.data` definitions are automatically placed in `.bss` (0 file bytes!):
assert_eq!(art.symbols[cnt_sym.index()].section, Some(SectionId::Bss));
assert_eq!(art.bss_size, 8);

// 2. Declare an imported external symbol (`@write`):
let write_sym = art.declare_import("write");

// 3. Emit a function into `.text` with a PC-relative relocation (`call @write`):
//    `E8 00 00 00 00` (call rel32) + `C3` (ret)
let fn_sym = art.define_function(
    "call_write",
    Linkage::Export,
    16,
    0x90, // x86 NOP alignment byte
    &[0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3],
);
art.add_reloc(Reloc {
    section: SectionId::Text,
    offset: 1, // 4-byte displacement starts at byte 1 of `E8 00 00 00 00`
    sym: write_sym,
    kind: RelocKind::PcRel32,
    addend: -4,
});

assert_eq!(art.symbols[fn_sym.index()].size, 6);
assert_eq!(art.relocs.len(), 1);
assert_eq!(msg_sym.0, 0);
```

### 2. In-Place Relocation Resolution for `obsidian-ld` and `obsidian-jit`

`Artifact::resolve_internal_relocs` and `RelocKind::apply_in_place` patch target virtual addresses directly into x86, AArch64, SystemZ, and M68k machine code:

```rust
use obsidian_ir::{Linkage, Target};
use obsidian_obj::{Artifact, Reloc, RelocKind, SectionId};

let mut art = Artifact::new("aarch64_patch", Target::aarch64_macos());

// Define `@callee` at offset 0 (`ret` = 0xD65F03C0) and `@caller` at offset 4 (`bl #0` = 0x94000000):
let callee_sym = art.define_function("callee", Linkage::Local, 4, 0, &0xD65F_03C0u32.to_le_bytes());
let _caller_sym = art.define_function("caller", Linkage::Export, 4, 0, &0x9400_0000u32.to_le_bytes());

// Relocate the `BL` at `.text + 4` to target `@callee` at `.text + 0` (-1 instruction = -4 bytes):
art.add_reloc(Reloc {
    section: SectionId::Text,
    offset: 4,
    sym: callee_sym,
    kind: RelocKind::AArch64Call26,
    addend: 0,
});

// Resolve internal relocations with `.text` mapped at virtual address `0x1000_0000`:
let vaddrs = [0x1000_0000, 0x1000_1000, 0x1000_2000, 0x1000_3000, 0, 0];
assert!(art.resolve_internal_relocs(&vaddrs));

// The `BL` word at `.text[4..8]` is now `bl #-4` (`0x97FFFF_FF`)!
let patched_bl = u32::from_le_bytes([art.text[4], art.text[5], art.text[6], art.text[7]]);
assert_eq!(patched_bl, 0x97FF_FFFF);
```
