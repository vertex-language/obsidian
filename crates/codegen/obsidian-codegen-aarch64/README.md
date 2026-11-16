# `obsidian-codegen-aarch64`

**64-Bit ARM (`AArch64` / `A64`) Code Generator for Obsidian (`compile_module`, `compile_function`, `CompiledFunction`).**

`obsidian-codegen-aarch64` lowers [`obsidian_ir::Module`] and [`obsidian_ir::Function`] into physical 32-bit Little-Endian `A64` machine code using [`obsidian_regalloc`], [`obsidian_abi`], and [`obsidian_isa_aarch64`], outputting an [`obsidian_obj::Artifact`].

---

## Hardware Optimizations & Idiom Packing

1. **$O(1)$ Inline Barrel-Shifter Fusion (`ADD Xd, Xn, Xm, LSL #amt`):**
   - When `i64.add`, `i64.sub`, or `ptr.add` consumes a single-use `i64.shl %idx, amt` (or `i64.shr.u`), `obsidian-codegen-aarch64` folds the shift directly into the arithmetic instruction's inline barrel shifter and elides the standalone shift instruction.
2. **Adjacent 64-Bit Load/Store Pairing (`LDP` / `STP` 128-Bit Fusion):**
   - Scans basic blocks for adjacent 64-bit loads (`[base, #off]` and `[base, #(off + 8)]`) or stores, validates silicon legality via `obsidian_isa_aarch64::uarch::can_fuse_into_ldp64` (rejecting `rt1 == rt2` hazards), and fuses them into a single 128-bit `LDP` or `STP` instruction.
3. **13-Bit `LogicalImm` Bitmask Encoding (`N:immr:imms`):**
   - Encodes repeating bitmask immediates directly into `AND`, `ORR`, and `EOR` immediate instructions via `LogicalImm::try_encode64`.
4. **Flagless `CBZ`/`CBNZ` & Branchless `CSEL`:**
   - Lowers zero-comparisons (`cmp.eq %x, 0` / `cmp.ne %x, 0`) + `br_if` into a single flagless `CBZ` or `CBNZ` instruction, and lowers `select` into `CSEL`.

---

## End-to-End Usage Examples

### 1. Compiling `Slice_len` to Exact `A64` Machine Words (`8 Bytes` / 2 Instructions!)

```rust
use obsidian_codegen_aarch64::compile_function;
use obsidian_ir as ir;

// export fn @Slice_len(%self: ptr) -> i64 {
// entry:
//     %n = i64.load %self, offset=8
//     ret %n
// }
let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
let [v_self] = f.params(); // X0
let v_n = f.i64_load(v_self, 8);
f.ret(&[v_n]);

let compiled = compile_function(ir::Target::aarch64_macos(), &f);

// Emitted 32-bit A64 machine words (8 bytes total):
//   ldr x0, [x0, #8]   (0xF9400400)
//   ret                (0xD65F03C0)
let w0 = u32::from_le_bytes(compiled.code[0..4].try_into().unwrap());
let w1 = u32::from_le_bytes(compiled.code[4..8].try_into().unwrap());
assert_eq!(w0, 0xF940_0400);
assert_eq!(w1, 0xD65F_03C0);
```

### 2. Barrel-Shifted `ADD` + 128-Bit `LDP` Pair Fusion + `CSEL` (`@max_pair_at`)

```rust
use obsidian_codegen_aarch64::compile_function;
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
let [v_base, v_idx] = f.params(); // X0, X1

let v_scaled = f.i64_shl_imm(v_idx, 3);
let v_elem   = f.ptr_add(v_base, v_scaled);
let v_a      = f.i64_load(v_elem, 16);
let v_b      = f.i64_load(v_elem, 24);
let v_gt     = f.i64_cmp(ir::IntCC::Sgt, v_a, v_b);
let v_best   = f.i64_select(v_gt, v_a, v_b);
f.ret(&[v_best]);

let compiled = compile_function(ir::Target::aarch64_macos(), &f);

// 7 IR instructions packed into 5 A64 instructions (20 bytes total):
//   1. `add x0, x0, x1, lsl #3`  (fuses %scaled + %elem!)
//   2. `ldp x1, x0, [x0, #16]`   (fuses both 64-bit loads into 1 128-bit LDP!)
//   3. `cmp x1, x0`
//   4. `csel x0, x1, x0, gt`
//   5. `ret`
assert_eq!(compiled.code.len(), 20);
let w_add = u32::from_le_bytes(compiled.code[0..4].try_into().unwrap());
let w_ldp = u32::from_le_bytes(compiled.code[4..8].try_into().unwrap());
assert_eq!(w_add, 0x8B01_0C00); // add x0, x0, x1, lsl #3
assert_eq!(w_ldp, 0xA941_0001); // ldp x1, x0, [x0, #16]
```

### 3. Flagless `CBZ` Branch Lowering & Full `ir::Module` Compilation

```rust
use obsidian_codegen_aarch64::compile_module;
use obsidian_ir as ir;

let mut module = ir::Module::new("cbz_demo", ir::Target::aarch64_macos());

let mut f = ir::Function::export("or_default", &[ir::Type::I64, ir::Type::I64], &[ir::Type::I64]);
let [v_x, v_def] = f.params();
let non_zero_bb = f.new_block("non_zero", &[]);
let is_zero_bb  = f.new_block("is_zero", &[]);

let v_eq0 = f.i64_cmp_imm(ir::IntCC::Eq, v_x, 0);
f.br_if(v_eq0, is_zero_bb, &[], non_zero_bb, &[]);

f.switch_to_block(non_zero_bb);
f.ret(&[v_x]);

f.switch_to_block(is_zero_bb);
f.ret(&[v_def]);

module.add_function(f);
let art = compile_module(&module);

// First instruction is a flagless `cbz x0, #8` (0xB4000040):
let first_word = u32::from_le_bytes(art.text[0..4].try_into().unwrap());
assert_eq!(first_word, 0xB400_0040);
```
