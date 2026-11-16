# Proposed Obsidian Architecture & End-to-End Compiler Layout

> **Companion Specification:**
> See [proposed_obsidian_ir.md](proposed_obsidian_ir.md) for the complete **`.obs` SSA Bedrock IR Specification** and the **`obsidian-ir` (`ir::Module`, `ir::Target`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`)** crate design.

---

## 1. Core Architectural Principles

1. **One Flat SSA Bedrock IR (`obsidian-ir` / `.obs`):**
   - High-level frontends (`rustc`, C, Zig, DSLs) lower once into `obsidian-ir` (`ir::Module` and `ir::Function`), where all language sugar (`pub`, `const fn`, `&self`, `&[u8]`, loops, `match`) is flattened into the **10 Bedrock types** (`i1`, `i8..i128`, `f32`, `f64`, `ptr`, `v128`) and namespaced general-purpose opcodes (`i64.add`, `ptr.add`, `i64.load`).
   - Every instruction node (`ir::InstNode`) is a fixed **16-byte** `#[repr(C, align(16))]` struct in a contiguous `Vec<InstNode>`, where `ir::Value(i)` is the direct array index `insts[i]`.
2. **`obsidian-isa-<arch>` Modules Are 1:1 Silicon Truth—Nothing More, Nothing Less:**
   - Every `obsidian-isa-<arch>` crate (`obsidian-isa-x86`, `obsidian-isa-systemz`, `obsidian-isa-m68k`, `obsidian-isa-aarch64`, etc.) is a `#![no_std]`, zero-dependency, `const`-evaluatable hardware specification.
   - It contains **zero** compiler pseudo-instructions, **zero** virtual registers, and **zero** synthetic multi-instruction macros. Everything in `obsidian-isa-<arch>` is strictly what the physical CPU silicon decodes and executes according to the official vendor manual.
3. **Static Hardware Oracle in `isa`, Smart Packing in `codegen` & `regalloc`:**
   - Because `obsidian-isa-<arch>` only holds immutable physical registers, condition codes, addressing modes, static `InstDesc` tables, micro-architectural legality queries (`uarch`), and stateless 1-instruction encoders (`encode`), it is written once and never changes.
   - All hardware speedups happen in **`obsidian-codegen-<arch>`**, **`obsidian-abi`**, and **`obsidian-regalloc`**, which consume `&ir::Function`, fuse multi-node Bedrock patterns (`shl + ptr.add + load`) into target addressing modes (`LEA`, barrel-shifted `ADD`, `LDP`/`STP` pairs, macro-fused `CMP+Jcc`), allocate physical registers, and emit raw machine bytes via `obsidian-isa-<arch>::encode`.
4. **From-Scratch Binary, Object, Archive, Linker & JIT Emission (`obj/*`):**
   - Machine bytes, read-only data (`.rodata`), globals (`.data`/`.bss`), TLS (`.tdata`/`.tbss`), unwind tables, DWARF/CodeView debug info, and relocations flow into **`obsidian-obj`** (`obsidian-elf`, `obsidian-macho`, `obsidian-coff`, `obsidian-wasm-bin`, `obsidian-ar`, `obsidian-ld`, and `obsidian-jit`) to produce relocatable `.o`/`.obj` files, static `.a`/`.lib` archives, linked executables (`ELF`, `Mach-O`, `.exe`), or in-memory executable JIT buffers with zero external toolchain dependencies.

---

## 2. End-to-End Pipeline: From `.obs` IR to Machine Binary

```text
  ┌─────────────────────────────────────────────────────────────────────────┐
  │ LAYER 1: `obsidian-ir` (.obs Bedrock SSA IR)                            │
  │   • `ir::Module`, `ir::Target` (Arch, Os, Abi, Endian, Features)        │
  │   • `ir::Function`, `ir::Block` (Block Params), `ir::Value` (u32 index) │
  │   • 16-byte `InstNode` arena with inline `use_count` & 0-heap Metadata  │
  └───────────────────────────────────┬─────────────────────────────────────┘
                                      │
                                      ▼
  ┌─────────────────────────────────────────────────────────────────────────┐
  │ LAYER 2: `obsidian-abi` + `obsidian-codegen-<arch>` + `obsidian-regalloc`│
  │   • ABI Mapping: maps params/returns to physical regs (`RDI/RAX`, `X0`) │
  │   • O(1) Multi-Node Idiom Fusion (`shl + ptr.add + load` -> SIB / LSL)  │
  │   • Adjacent Load/Store Pairing (`LDP` / `STP` 128-bit fusion)          │
  │   • 3-Tier Legalization (Native 1:1 -> Inline Synthesis -> Intrinsics)  │
  │   • Hardware-Aware RegAlloc (2-op ties, 0-cycle RAT moves, 4-reg rings) │
  │   • 32-Byte uop-Cache (DSB) Window Alignment & Branch Layout            │
  └─────────────────┬───────────────────────────────────┬───────────────────┘
                    │ Queries & Calls                   │ Produces
                    ▼                                   ▼
  ┌───────────────────────────────────┐ ┌───────────────────────────────────┐
  │ LAYER 0: `obsidian-isa-<arch>`    │ │ `obsidian-obj::Artifact`          │
  │   (1:1 Silicon Truth — 25 CPUs)   │ │   • `.text` machine code bytes    │
  │   • `regs`, `cond`, `addr`        │ │   • `.rodata`, `.data`, `.bss`    │
  │   • `opcode::Opcode::desc()`      │ │   • `.tdata`, `.tbss` (TLS)       │
  │   • `uarch` legality rules        │ │   • `SymbolDef`s & `Reloc`s       │
  │   • `encode` stateless byte/word  │ │   • `.eh_frame` & Debug Line Info │
  └───────────────────────────────────┘ └───────────────┬───────────────────┘
                                                        │
        ┌───────────────────────────────────────────────┼───────────────────┐
        ▼                                               ▼                   ▼
  ┌─────────────────────────────┐ ┌───────────────────────────┐ ┌───────────┐
  │ Relocatable `.o` & `.a`     │ │ Standalone Executables    │ │ In-Memory │
  │   • `obsidian-elf`   (.o)   │ │   • `obsidian-ld` links   │ │ JIT Engine│
  │   • `obsidian-macho` (.o)   │ │     sections & relocs:    │ │ `obsidian`│
  │   • `obsidian-coff`  (.obj) │ │   • Linux/BSD ELF binary  │ │ `-jit`    │
  │   • `obsidian-ar`    (.a)   │ │   • macOS Mach-O binary   │ │ (`W^X`    │
  │   • `obsidian-wasm-bin`     │ │   • Windows PE32+ `.exe`  │ │  pages)   │
  └─────────────────────────────┘ └───────────────────────────┘ └───────────┘
```

---

## 3. How Every Layer Connects in Code

### 3.1 Step 1: Constructing the `.obs` Bedrock SSA in `obsidian-ir`
```rust
use obsidian_ir as ir;

pub fn build_max_pair_ir() -> ir::Module {
    let mut module = ir::Module::new("math_mod", ir::Target::x86_64_linux());

    // export fn @max_pair_at(%base: ptr, %idx: i64) -> i64
    let mut f = ir::Function::export(
        "max_pair_at",
        &[ir::Type::Ptr, ir::Type::I64],
        &[ir::Type::I64],
    );
    let [v_base, v_idx] = f.params();

    let v_scaled = f.i64_shl_imm(v_idx, 3);                     // %v2 = i64.shl %idx, 3
    let v_elem   = f.ptr_add(v_base, v_scaled);                 // %v3 = ptr.add %base, %v2
    let v_a      = f.i64_load(v_elem, 16);                      // %v4 = i64.load %v3, offset=16
    let v_b      = f.i64_load(v_elem, 24);                      // %v5 = i64.load %v3, offset=24
    let v_gt     = f.i64_cmp(ir::IntCC::Sgt, v_a, v_b);         // %v6 = i64.cmp.sgt %v4, %v5
    let v_best   = f.i64_select(v_gt, v_a, v_b);                // %v7 = i64.select %v6, %v4, %v5
    f.ret(&[v_best]);                                           // ret %v7

    module.add_function(f);
    module
}
```

### 3.2 Step 2: How `obsidian-abi`, `obsidian-regalloc`, and `obsidian-codegen-<arch>` Lower to `obsidian-isa-<arch>`
1. **`obsidian-abi`** inspects `ir::Target` and the function's signature `(&[Ptr, I64]) -> &[I64]`:
   - On `x86_64` SysV: binds `%base -> Rdi`, `%idx -> Rsi`, return `%best -> Rax`, caller-saved mask `GprMask::SYSV64_CALLER_SAVED`.
   - On `AArch64` AAPCS64: binds `%base -> X0`, `%idx -> X1`, return `%best -> X0`, caller-saved mask `GprMask::AAPCS64_CALLER_SAVED`.
   - On `SystemZ` s390x: binds `%base -> R2`, `%idx -> R3`, return `%best -> R2`.
   - On `M68k`: binds `%base -> A0` (address register!), `%idx -> D0` (data register!), return `%best -> D0`.
2. **`obsidian-codegen-<arch>` + `obsidian-regalloc`** fuse multi-node idioms via `use_count == 1` and call `obsidian-isa-<arch>::encode`:

- **On `x86_64` (`obsidian-codegen-x86` + `obsidian-isa-x86`):**
  ```rust
  use obsidian_isa_x86::{
      addr::{Disp, MemAddr, Scale},
      cond::CondCode,
      encode,
      regs::Gpr,
  };

  // `%scaled` + `%elem` + `offset=16` fold directly into x86 SIB addressing:
  let addr_a = MemAddr::new(Some(Gpr::Rdi), Some(Gpr::Rsi), Scale::By8, Disp::Disp8(16));
  let addr_b = MemAddr::new(Some(Gpr::Rdi), Some(Gpr::Rsi), Scale::By8, Disp::Disp8(24));

  let inst0 = encode::mov64_rm(Gpr::Rax, addr_a);                   // mov rax, [rdi + rsi*8 + 16]
  let inst1 = encode::mov64_rm(Gpr::Rcx, addr_b);                   // mov rcx, [rdi + rsi*8 + 24]
  let inst2 = encode::cmp64_rr(Gpr::Rax, Gpr::Rcx);                 // cmp rax, rcx
  let inst3 = encode::cmovcc64_rr(CondCode::L, Gpr::Rax, Gpr::Rcx); // cmovl rax, rcx
  let inst4 = encode::ret();                                        // ret
  ```
- **On `AArch64` (`obsidian-codegen-aarch64` + `obsidian-isa-aarch64`):**
  ```rust
  use obsidian_isa_aarch64::{
      addr::Shift,
      cond::CondCode,
      encode,
      regs::{Gpr, GprOrSp, GprOrXzr},
      uarch,
  };

  // 1. `%scaled` + `%elem` fuse into 1 barrel-shifted ADD: `add x8, x0, x1, lsl #3`
  let inst0 = encode::add64_shifted(
      GprOrXzr::Gpr(Gpr::X8),
      GprOrXzr::Gpr(Gpr::X0),
      GprOrXzr::Gpr(Gpr::X1),
      Shift::lsl(3),
  );

  // 2. Both 8-byte loads (`offset=16` and `offset=24`) fuse into 1 128-bit `LDP`:
  let pair_off = uarch::can_fuse_into_ldp64(
      Gpr::X9, Gpr::X10, GprOrSp::Gpr(Gpr::X8), 16, 24, false
  ).unwrap();
  let inst1 = encode::ldp64_offset(
      GprOrXzr::Gpr(Gpr::X9),
      GprOrXzr::Gpr(Gpr::X10),
      GprOrSp::Gpr(Gpr::X8),
      pair_off,
  );

  // 3. `cmp x9, x10` + `csel x0, x9, x10, gt` + `ret`
  let inst2 = encode::cmp64_shifted(GprOrXzr::Gpr(Gpr::X9), GprOrXzr::Gpr(Gpr::X10), Shift::NONE);
  let inst3 = encode::csel64(
      GprOrXzr::Gpr(Gpr::X0),
      GprOrXzr::Gpr(Gpr::X9),
      GprOrXzr::Gpr(Gpr::X10),
      CondCode::GT,
  );
  let inst4 = encode::ret_lr();
  ```

### 3.3 Step 3: How `obsidian-obj::Artifact` Bridges Codegen to `.o`, Executables, and JIT
The output of `obsidian-codegen` is an `Artifact`—a flat, format-agnostic representation of compiled sections, symbols, and relocations:

```rust
/// Produced by `obsidian_codegen::compile_module(&ir::Module)`:
pub struct Artifact {
    pub target: ir::Target,
    pub text: Vec<u8>,               // Executable machine bytes (.text / __TEXT,__text)
    pub rodata: Vec<u8>,             // Read-only constants (.rodata / __TEXT,__const)
    pub data: Vec<u8>,               // Initialized mutable globals (.data / __DATA,__data)
    pub bss_size: u64,               // Zero-initialized globals (.bss)
    pub tdata: Vec<u8>,              // Thread-local initialized data (.tdata)
    pub tbss_size: u64,              // Thread-local zeroed data (.tbss)
    pub symbols: Vec<SymbolRecord>,  // Defined & imported symbols (@max_pair_at, @write)
    pub relocs: Vec<Reloc>,          // Patch sites in `text` / `data`
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Reloc {
    /// Section containing the bytes to patch (`Text` or `Data`).
    pub section: SectionId,
    /// Byte offset inside `section` where the patch is applied.
    pub offset: u32,
    /// Target symbol being referenced.
    pub sym: ir::Sym,
    /// Hardware relocation kind (`X86PcRel32`, `X86GotPcRel32`, `AArch64Call26`, `AArch64Page21`, etc.).
    pub kind: RelocKind,
    /// Signed byte addend (e.g., `-4` for x86 PC-relative 32-bit displacements).
    pub addend: i64,
}
```

From that single `Artifact`, the caller can emit any output format in 1 call:
```rust
// 1. Relocatable `.o` / `.obj` file (dispatches to `obsidian-elf`, `obsidian-macho`, or `obsidian-coff`):
let obj_bytes: Vec<u8> = artifact.to_object_bytes();
std::fs::write("max_pair.o", &obj_bytes)?;

// 2. Standalone linked executable via `obsidian-ld` (zero external `ld` process required):
let exe_bytes: Vec<u8> = obsidian_ld::link_executable(&[artifact], "main")?;
std::fs::write("app", &exe_bytes)?;

// 3. Direct in-memory execution via `obsidian-jit`:
let jit = obsidian_jit::JitModule::from_artifact(&artifact)?;
let max_pair_fn: unsafe extern "C" fn(*const u64, u64) -> u64 = unsafe { jit.get_fn("max_pair_at")? };
```

---

## 4. What Lives Inside `crates/obj/` (`ELF`, `Mach-O`, `PE/COFF`, `Wasm`, `.a`, `DWARF`, `ld`, `jit`)

| Crate | Output Formats | Core Responsibilities | Status |
| :--- | :--- | :--- | :---: |
| **[obsidian-obj](../crates/obj/obsidian-obj)** | `Artifact` & `.o` dispatch | Defines `Artifact`, `SectionId`, `SymbolRecord`, `Reloc`, and `RelocKind` (with `apply_in_place`). | ✅ Complete |
| **[obsidian-elf](../crates/obj/obsidian-elf)** | `.o` (`ET_REL`), `.so` (`ET_DYN`), Executable (`ET_EXEC` / PIE `ET_DYN`) | From-scratch 32/64-bit Little & Big-Endian ELF writer: ELF header, `PT_LOAD` program headers, `.symtab`/`.strtab`/`.rela.text` sections, and `R_<ARCH>_*` relocation encodings. | ✅ Complete |
| **[obsidian-macho](../crates/obj/obsidian-macho)** | `.o` (`MH_OBJECT`), Universal Fat Binary (`FAT_MAGIC`) | From-scratch Mach-O 64 writer for macOS/iOS (`x86_64` & `arm64`): `LC_SEGMENT_64`, `LC_SYMTAB`, `LC_DYSYMTAB`, `LC_BUILD_VERSION`, and Universal Fat binaries. | ✅ Complete |
| **[obsidian-coff](../crates/obj/obsidian-coff)** | `.obj` (COFF Relocatable), `.exe` / `.dll` (PE32 / PE32+) | From-scratch Windows/UEFI COFF & PE32+ writer: DOS header, PE signature, `IMAGE_FILE_HEADER`, `IMAGE_OPTIONAL_HEADER64`, `.text`/`.rdata`/`.data`/`.bss`. | ✅ Complete |
| **`obsidian-wasm-bin`** | `.wasm` (WebAssembly Binary Module) | Encodes WebAssembly binary sections (`Type`, `Import`, `Function`, `Memory`, `Global`, `Export`, `Code`, `Data`, and `linking` custom section for relocatable Wasm `.o`). | Planned |
| **`obsidian-ar`** | `.a` (Unix `ar` Archive) & `.lib` (Windows COFF Archive) | Packs multiple `.o` / `.obj` files and a symbol index table (`/` / `__.SYMDEF`) into static libraries (`.a` / `.lib`, used for `rlib` / `staticlib`). | Planned |
| **`obsidian-dwarf`** | `.debug_info`, `.debug_line`, `.eh_frame`, `.pdata` | Translates `ir::SourceLoc` and frame-unwind records into DWARF v5 line/unwind tables (ELF/Mach-O) and Windows `.pdata`/`.xdata` unwind info. | Planned |
| **[obsidian-ld](../crates/obj/obsidian-ld)** | Linked `ELF` or `PE32+` Executable / Relocatable Object | Fast built-in linker: merges sections across `Artifact`s, resolves cross-module `Reloc`s, and writes the runnable executable or merged `.o`/`.obj`. | ✅ Complete |
| **`obsidian-jit`** | In-Memory Executable Code (`W^X`) | Allocates page-aligned memory, patches `Artifact` relocations in RAM, transitions pages from `RW` to `RX` (`mprotect` / `VirtualProtect` + `sys_icache_invalidate`), and exposes callable function pointers. | Planned |

---

## 5. All 25 Hardware Targets Grouped by Hardware Archetype

Verified against upstream LLVM (`llvm/CMakeLists.txt` and `Triple.h`), all **25 targets** (`21` standard + `4` experimental) follow the exact same `obsidian-isa-<arch>` $\rightarrow$ `obsidian-codegen-<arch>` architecture:

| # | Target Name | `ir::Arch` Variants | Hardware Archetype Family | Pure Silicon Crate (`crates/isa/*`) | Status |
| :- | :--- | :--- | :--- | :--- | :---: |
| 1 | **`X86`** | `X86_64`, `X86_32` | **Archetype I:** Variable-Length CISC + Status Flags | [obsidian-isa-x86](../crates/isa/obsidian-isa-x86) | ✅ Complete |
| 2 | **`SystemZ`** | `SystemZ` (`s390x`) | **Archetype I:** Variable-Length Big-Endian CISC + `CC` | [obsidian-isa-systemz](../crates/isa/obsidian-isa-systemz) | ✅ Complete |
| 3 | **`M68k`** | `M68k` (`680x0`) | **Archetype I:** Variable-Length Split `D`/`A` CISC + `CCR` | [obsidian-isa-m68k](../crates/isa/obsidian-isa-m68k) | ✅ Complete |
| 4 | **`AArch64`** | `AArch64` (`ARM64`) | **Archetype II:** 32-Bit Flagged RISC (`NZCV` + Shifter) | [obsidian-isa-aarch64](../crates/isa/obsidian-isa-aarch64) | ✅ Complete |
| 5 | **`ARM`** | `Arm`, `Thumb2` | **Archetype II:** 32/16-Bit Flagged RISC (`APSR` + `IT`) | `obsidian-isa-arm` | Next |
| 6 | **`PowerPC`** | `Ppc64Le`, `Ppc64`, `Ppc32` | **Archetype II:** 32-Bit Multi-Flag RISC (`CR0..CR7`) | `obsidian-isa-powerpc` | Planned |
| 7 | **`Sparc`** | `SparcV9`, `SparcV8` | **Archetype II:** 32-Bit Flagged RISC + Register Windows | `obsidian-isa-sparc` | Planned |
| 8 | **`RISCV`** | `Riscv64`, `Riscv32` | **Archetype III:** 32/16-Bit **Flagless** Compare-Branch RISC | `obsidian-isa-riscv` | Planned |
| 9 | **`LoongArch`** | `LoongArch64`, `LoongArch32` | **Archetype III:** 32-Bit **Flagless** RISC | `obsidian-isa-loongarch` | Planned |
| 10 | **`Mips`** | `Mips64`, `Mips32` | **Archetype III:** 32-Bit **Flagless** RISC | `obsidian-isa-mips` | Planned |
| 11 | **`Lanai`** | `Lanai` | **Archetype III:** 32-Bit Network RISC | `obsidian-isa-lanai` | Planned |
| 12 | **`ARC`** | `Arc` | **Archetype III:** 16/32-Bit Compact RISC | `obsidian-isa-arc` | Planned |
| 13 | **`CSKY`** | `CSky` | **Archetype III:** 16/32-Bit Embedded RISC | `obsidian-isa-csky` | Planned |
| 14 | **`AMDGPU`** | `AmdGpu` (`GCN`/`RDNA`) | **Archetype IV:** SIMT Vector/GPU (`SGPR` + `VGPR` + `EXEC`) | `obsidian-isa-amdgpu` | Planned |
| 15 | **`NVPTX`** | `NvPtx64`, `NvPtx32` | **Archetype IV:** SIMT GPU Virtual ISA (`@p` Predicates) | `obsidian-isa-nvptx` | Planned |
| 16 | **`SPIRV`** | `SpirV` | **Archetype IV:** Typed SSA GPU/Compute Binary Format | `obsidian-isa-spirv` | Planned |
| 17 | **`DirectX`** | `DirectX` (`DXIL`) | **Archetype IV:** Shader Bytecode Target | `obsidian-isa-directx` | Planned |
| 18 | **`VE`** | `Ve` (SX-Aurora) | **Archetype IV:** 256-Element Long-Vector Supercomputer ISA | `obsidian-isa-ve` | Planned |
| 19 | **`WebAssembly`** | `Wasm32`, `Wasm64` | **Archetype V:** Verifier-First Structured VM | `obsidian-isa-wasm` | Planned |
| 20 | **`BPF`** | `BpfLe`, `BpfBe` | **Archetype V:** Verifier-First Kernel VM (`R0..R10`) | `obsidian-isa-bpf` | Planned |
| 21 | **`Hexagon`** | `Hexagon` | **Archetype VI:** 4-Slot Packet VLIW DSP | `obsidian-isa-hexagon` | Planned |
| 22 | **`AVR`** | `Avr` | **Archetype VI:** 8-Bit Register-Pair MCU (`R0..R31`, `X/Y/Z`) | `obsidian-isa-avr` | Planned |
| 23 | **`MSP430`** | `Msp430` | **Archetype VI:** 16-Bit Orthogonal CISC MCU | `obsidian-isa-msp430` | Planned |
| 24 | **`XCore`** | `XCore` | **Archetype VI:** 32-Bit Event-Driven Multicore MCU | `obsidian-isa-xcore` | Planned |
| 25 | **`Xtensa`** | `Xtensa` (ESP32) | **Archetype VI:** 16/24-Bit Windowed MCU/DSP | `obsidian-isa-xtensa` | Planned |

---

## 6. Complete Rust Workspace & Repository Layout

```text
obsidian/
├── Cargo.toml                        # Workspace & top-level `obsidian` umbrella crate
├── README.md                         # Top-level overview & end-to-end usage examples
├── src/lib.rs                        # Re-exports `ir`, `abi`, `regalloc`, `codegen_*`, `obj`, `elf`, `macho`, `coff`, `ld`, `isa::*`
├── docs/
│   ├── proposed_obsidian.md          # Full compiler & target architecture specification
│   └── proposed_obsidian_ir.md       # .obs SSA Bedrock IR & `obsidian-ir` crate specification
├── tests/
│   └── e2e_macos.rs                  # Live end-to-end macOS arm64 executable & JIT test suite
│
├── crates/
│   │
│   │── /* =====================================================================
│   │    LAYER 0: PURE SILICON ISA MODULES (1:1 Hardware Truth — All 25 Targets)
│   │    • #![no_std], zero dependencies, const-evaluatable.
│   │    • Physical registers, condition codes, addressing modes, opcodes,
│   │      static InstDesc tables, uarch legality rules, and 1-inst encoders.
│   │   ===================================================================== */
│   │
│   ├── isa/
│   │   ├── obsidian-isa-x86/         # [1]  x86 & x86_64 (Complete & Tested)
│   │   ├── obsidian-isa-systemz/     # [2]  IBM z/Architecture s390x (Complete & Tested)
│   │   ├── obsidian-isa-m68k/        # [3]  Motorola 680x0 (Complete & Tested)
│   │   ├── obsidian-isa-aarch64/     # [4]  ARM64 / A64 (Complete & Tested)
│   │   ├── obsidian-isa-arm/         # [5]  ARM32 & Thumb-2
│   │   ├── obsidian-isa-powerpc/     # [6]  PowerPC 32/64
│   │   ├── obsidian-isa-sparc/       # [7]  SPARC v8/v9
│   │   ├── obsidian-isa-riscv/       # [8]  RISC-V 32/64 + RVC + Zba/Zbb
│   │   ├── obsidian-isa-loongarch/   # [9]  LoongArch 32/64
│   │   ├── obsidian-isa-mips/        # [10] MIPS 32/64
│   │   ├── obsidian-isa-lanai/       # [11] Lanai
│   │   ├── obsidian-isa-arc/         # [12] Synopsys ARC
│   │   ├── obsidian-isa-csky/        # [13] C-SKY
│   │   ├── obsidian-isa-amdgpu/      # [14] AMDGPU (GCN / RDNA / CDNA)
│   │   ├── obsidian-isa-nvptx/       # [15] NVIDIA PTX
│   │   ├── obsidian-isa-spirv/       # [16] Khronos SPIR-V
│   │   ├── obsidian-isa-directx/     # [17] DirectX DXIL
│   │   ├── obsidian-isa-ve/          # [18] NEC SX-Aurora VE
│   │   ├── obsidian-isa-wasm/        # [19] WebAssembly
│   │   ├── obsidian-isa-bpf/         # [20] eBPF
│   │   ├── obsidian-isa-hexagon/     # [21] Qualcomm Hexagon VLIW
│   │   ├── obsidian-isa-avr/         # [22] Atmel AVR 8-bit
│   │   ├── obsidian-isa-msp430/      # [23] TI MSP430 16-bit
│   │   ├── obsidian-isa-xcore/       # [24] XMOS XCore
│   │   └── obsidian-isa-xtensa/      # [25] Tensilica Xtensa
│   │
│   │── /* =====================================================================
│   │    LAYER 1: UNIVERSAL .obs BEDROCK SSA IR (`obsidian-ir`)
│   │    • `ir::Module`, `ir::Target`, `ir::Function`, `ir::Block`, `ir::Value`,
│   │      `ir::Sym`, `ir::Type` (10 Bedrock types), `ir::Opcode`.
│   │    • 16-byte `InstNode` arena, Block Parameters, `ResultProj`, `.obs` printer.
│   │   ===================================================================== */
│   │
│   ├── obsidian-ir/                  # (Complete & Tested)
│   │
│   │── /* =====================================================================
│   │    LAYER 2: ABI, REGISTER ALLOCATION & PER-CPU CODEGEN (`codegen/*`)
│   │    • Consumes `&ir::Module` / `&ir::Function`.
│   │    • Performs O(1) multi-node idiom fusion, 3-Tier opcode legalization,
│   │      register allocation, 32B uop window alignment, and byte encoding.
│   │   ===================================================================== */
│   │
│   ├── obsidian-abi/                 # Calling convention & frame layout rules (Complete & Tested)
│   ├── obsidian-regalloc/            # Hardware-aware register allocator (Complete & Tested)
│   ├── codegen/
│   │   ├── obsidian-codegen-x86/     # x86-64 codegen: SIB fusion, LEA, CMOVcc (Complete & Tested)
│   │   ├── obsidian-codegen-aarch64/ # AArch64 codegen: Shifter fusion, LDP/STP, CBZ (Complete & Tested)
│   │   ├── obsidian-codegen-systemz/ # IBM SystemZ s390x codegen
│   │   ├── obsidian-codegen-m68k/    # Motorola 680x0 codegen
│   │   └── ... (all 25 targets)
│   │
│   │── /* =====================================================================
│   │    LAYER 3: OBJECT CONTAINERS, ARCHIVES, DEBUG, LINKER & JIT (`obj/*`)
│   │    • Emits relocatable `.o`/`.obj`, static `.a`/`.lib`, linked executables
│   │      (`ELF`, `Mach-O`, `PE/COFF`, `.wasm`), and in-memory JIT buffers.
│   │   ===================================================================== */
│   │
│   ├── obj/
│   │   ├── obsidian-obj/             # Unified `Artifact`, `SectionId`, `SymbolRecord`, `Reloc` (Complete & Tested)
│   │   ├── obsidian-elf/             # From-scratch ELF64 `.o` (LE/BE) & `ET_EXEC` writer (Complete & Tested)
│   │   ├── obsidian-macho/           # From-scratch Mach-O 64 `.o` & Universal Fat Binary writer (Complete & Tested)
│   │   ├── obsidian-coff/            # From-scratch COFF `.obj` & Windows PE32+ `.exe` writer (Complete & Tested)
│   │   ├── obsidian-ld/              # Built-in fast in-memory linker (Complete & Tested)
│   │   ├── obsidian-wasm-bin/        # From-scratch `.wasm` binary module writer
│   │   ├── obsidian-ar/              # From-scratch Unix `.a` & Windows `.lib` static archive builder
│   │   ├── obsidian-dwarf/           # DWARF v5 (`.debug_line`, `.eh_frame`) & Windows `.pdata`/`.xdata` generator
│   │   └── obsidian-jit/             # In-memory `W^X` JIT loader & relocation patcher
│   │
│   │── /* =====================================================================
│   │    LAYER 4: COMPILER DRIVERS & FRONTEND INTEGRATIONS
│   │   ===================================================================== */
│   │
│   ├── rustc_codegen_obsidian/       # Drop-in `rustc` codegen backend (`MIR -> obsidian-ir -> obsidian-obj`)
│   └── obsidian-cli/                 # CLI tool (`obsidian compile foo.obs -o foo.o` / `obsidian build foo.obs -o app`)
```
