# `obsidian-codegen-x86`

**x86-64 Code Generator for Obsidian (`compile_module`, `compile_function`, `CompiledFunction`).**

`obsidian-codegen-x86` lowers [`obsidian_ir::Module`] and [`obsidian_ir::Function`] into physical x86-64 machine code using [`obsidian_regalloc`], [`obsidian_abi`], and [`obsidian_isa_x86`], outputting an [`obsidian_obj::Artifact`].

---

## Hardware Optimizations & Idiom Packing

1. **$O(1)$ Multi-Node SIB Addressing Fusion (`[base + index * scale + disp]`):**
   - When lowering `i64.load %ptr, offset=16` where `%ptr` is `ptr.add %base, (i64.shl %idx, 3)`, `obsidian-codegen-x86` inspects `use_count` in $O(1)$, folds the shift, base pointer, and displacement directly into the load's SIB byte (`mov rax, [rdi + rsi*8 + 16]`), and elides the standalone `shl` and `ptr.add` instructions completely.
2. **Non-Destructive 3-Operand `LEA` Lowering:**
   - When `i64.add` or `ptr.add` writes to a destination register `dst != lhs` (or adds a scaled operand `lhs + (idx << 1|2|3)`), `obsidian-codegen-x86` emits a single `LEA` instruction (`lea dst, [lhs + idx*8 + disp]`) instead of `mov` + `add`, preserving `EFLAGS` and saving an instruction slot.
3. **2-Byte Zero-Idiom (`xor r32, r32`) & 5-Byte Unsigned 32-Bit Move (`mov r32, imm32`):**
   - Materializes `0` via 2-byte `xor eax, eax` (`31 C0`) and positive 32-bit constants via 5-byte `mov eax, imm32` (exploiting implicit 32-to-64-bit zero extension instead of 7-byte or 10-byte 64-bit moves).
4. **Macro-Fused Compare & Branch (`CMP`/`TEST` + `Jcc`) and Branchless `CMOVcc`:**
   - Folds single-use `i64.cmp.*` nodes directly into `br_if` (`cmp` + `jcc` adjacent for hardware macro-fusion) and `select` (`cmovcc`).

---

## End-to-End Usage Examples

### 1. Compiling `Slice_len` to Exact x86-64 Machine Bytes (`5 Bytes` Total!)

```rust
use obsidian_codegen_x86::compile_function;
use obsidian_ir as ir;

// export fn @Slice_len(%self: ptr) -> i64 {
// entry:
//     %n = i64.load %self, offset=8
//     ret %n
// }
let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
let [v_self] = f.params(); // RDI
let v_n = f.i64_load(v_self, 8);
f.ret(&[v_n]);

let compiled = compile_function(ir::Target::x86_64_linux(), &f);

// Emitted machine code (5 bytes total):
//   48 8B 47 08   mov rax, [rdi + 8]
//   C3            ret
assert_eq!(compiled.code, &[0x48, 0x8B, 0x47, 0x08, 0xC3]);
```

### 2. Multi-Node SIB Fusion + Branchless `CMOVcc` (`@max_pair_at`)

```rust
use obsidian_codegen_x86::compile_function;
use obsidian_ir as ir;

// export fn @max_pair_at(%base: ptr, %idx: i64) -> i64 {
// entry:
//     %scaled = i64.shl %idx, 3
//     %elem   = ptr.add %base, %scaled
//     %a      = i64.load %elem, offset=16
//     %b      = i64.load %elem, offset=24
//     %gt     = i64.cmp.sgt %a, %b
//     %best   = i64.select %gt, %a, %b
//     ret %best
// }
let mut f = ir::Function::export(
    "max_pair_at",
    &[ir::Type::Ptr, ir::Type::I64],
    &[ir::Type::I64],
);
let [v_base, v_idx] = f.params(); // RDI, RSI

let v_scaled = f.i64_shl_imm(v_idx, 3);
let v_elem   = f.ptr_add(v_base, v_scaled);
let v_a      = f.i64_load(v_elem, 16);
let v_b      = f.i64_load(v_elem, 24);
let v_gt     = f.i64_cmp(ir::IntCC::Sgt, v_a, v_b);
let v_best   = f.i64_select(v_gt, v_a, v_b);
f.ret(&[v_best]);

let compiled = compile_function(ir::Target::x86_64_linux(), &f);

// Both `shl` and `ptr.add` were folded directly into the SIB bytes of the two `mov` loads,
// and `cmp.sgt` + `select` lowered to `cmp` + `cmovg` + `ret` (18 bytes total):
//   48 8B 4C F7 10   mov   rcx, [rdi + rsi*8 + 16]
//   48 8B 44 F7 18   mov   rax, [rdi + rsi*8 + 24]
//   48 39 C1         cmp   rcx, rax
//   48 0F 4F C1      cmovg rax, rcx
//   C3               ret
assert_eq!(
    compiled.code,
    &[
        0x48, 0x8B, 0x4C, 0xF7, 0x10,
        0x48, 0x8B, 0x44, 0xF7, 0x18,
        0x48, 0x39, 0xC1,
        0x48, 0x0F, 0x4F, 0xC1,
        0xC3,
    ]
);
```

### 3. Compiling a Full `ir::Module` to an `obsidian_obj::Artifact`

```rust
use obsidian_codegen_x86::compile_module;
use obsidian_ir as ir;

let mut module = ir::Module::new("demo", ir::Target::x86_64_linux());
let _msg = module.add_const("MSG", b"hi\0", 1);

let mut f = ir::Function::export("add_scaled", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_a, v_b] = f.params();
let v_shl = f.i64_shl_imm(v_b, 2);
let v_sum = f.i64_add(v_a, v_shl);
f.ret(&[v_sum]);
module.add_function(f);

let artifact = compile_module(&module);
// `add_scaled` lowered to `lea rax, [rdi + rsi*4]` (48 8D 04 B7) + `ret` (C3):
assert_eq!(artifact.text, &[0x48, 0x8D, 0x04, 0xB7, 0xC3]);
assert_eq!(artifact.rodata, b"hi\0");
```
