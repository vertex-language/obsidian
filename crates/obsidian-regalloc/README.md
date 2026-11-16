# `obsidian-regalloc`

**Hardware-Aware Linear-Scan Register Allocator for Obsidian (`allocate_registers`, `RegAllocResult`, `LivenessInfo`).**

`obsidian-regalloc` is a `#![no_std]` (`alloc`) register allocator that maps every `ir::Value` in an `ir::Function` to a physical hardware register (`ArgLoc::Reg` / `ArgLoc::RegPair`) or stack spill slot (`ArgLoc::Stack`), and computes the final `FrameLayout` via `obsidian-abi`.

---

## Hardware-Specific Allocation Features

1. **Zero-Move Return Hint Coalescing (`RAX`, `X0`, `R2`, `D0`/`A0`):**
   - Values passed to `ret` receive a preference hint for their target return register so the defining instruction writes directly into `RAX` / `X0` without a trailing register move.
2. **Two-Address Destructive Operand Tie Reuse (`tied_dst_src0` on x86 & M68k):**
   - When an arithmetic or bitwise instruction (`Add`, `Sub`, `And`, `Or`, `Xor`, `Shl`) on a 2-operand CISC target consumes a `src0` whose live interval ends at that exact instruction, `dst` reuses `src0`'s physical register in place.
3. **4-Register False-Dependency Rotation Ring (`Clz`, `Ctz`, `Popcnt` on x86-64):**
   - On x86-64, unary bit-counting instructions (`LZCNT`, `TZCNT`, `POPCNT`) suffer from a hardware false dependency on the destination register on many cores. `obsidian-regalloc` rotates through up to 4 distinct volatile destination registers across consecutive bit-counting operations, breaking the dependency chain in 0 extra bytes and 0 extra cycles.
4. **Automatic Call-Spanning Promotion (`spans_call`):**
   - Any `ir::Value` whose live range crosses a `Call` is automatically placed in a **callee-saved (non-volatile) register** (`RBX, R12..R15` on x86-64, `X19..X28` on AArch64, `R6..R13` on SystemZ) and recorded in `dirty_callee_saved_mask` so it survives the call in a register without stack spilling around the call site.
5. **Split Address/Data Register Bank Allocation on Motorola 680x0 (`M68k`):**
   - Pointer values (`Type::Ptr`) allocate from `A0..A5` (`RegBank::Addr`), while integer values allocate from `D0..D7` (`RegBank::Gpr`).

---

## End-to-End Usage Examples

### 1. Return Coalescing & Two-Address Register Reuse on `x86_64`

```rust
use obsidian_ir as ir;
use obsidian_isa_x86::regs::Gpr;
use obsidian_regalloc::allocate_registers;

let mut f = ir::Function::export(
    "add_three",
    &[ir::Type::I64, ir::Type::I64, ir::Type::I64],
    &[ir::Type::I64],
);
let [v_a, v_b, v_c] = f.params(); // SysV64: RDI, RSI, RDX

let v_sum1 = f.i64_add(v_a, v_b);
let v_sum2 = f.i64_add(v_sum1, v_c);
f.ret(&[v_sum2]);

let ra = allocate_registers(ir::Target::x86_64_linux(), &f);

// Incoming parameters are pre-colored to RDI, RSI, RDX:
assert_eq!(ra.loc(v_a).as_x86_gpr(), Some(Gpr::Rdi));
assert_eq!(ra.loc(v_b).as_x86_gpr(), Some(Gpr::Rsi));
assert_eq!(ra.loc(v_c).as_x86_gpr(), Some(Gpr::Rdx));

// `v_a` (RDI) dies at `v_sum1`, so `v_sum1` reuses RDI in place (`add rdi, rsi`),
// and `v_sum2` is coalesced directly into the return register RAX!
assert_eq!(ra.loc(v_sum1).as_x86_gpr(), Some(Gpr::Rdi));
assert_eq!(ra.loc(v_sum2).as_x86_gpr(), Some(Gpr::Rax));
assert_eq!(ra.dirty_callee_saved_mask, 0);
assert_eq!(ra.frame.sp_sub_bytes, 0);
```

### 2. 4-Register Rotation Ring on x86-64 `popcnt` / `ctz` Chains

```rust
use obsidian_ir as ir;
use obsidian_regalloc::allocate_registers;

let mut f = ir::Function::export(
    "parallel_popcnt",
    &[ir::Type::I64, ir::Type::I64, ir::Type::I64],
    &[ir::Type::I64],
);
let [v0, v1, v2] = f.params();

// Three independent `popcnt` operations whose outputs are all live at the same time
// (or even back-to-back) receive rotating destination registers from the ring:
let p0 = f.i64_popcnt(v0);
let p1 = f.i64_popcnt(v1);
let p2 = f.i64_popcnt(v2);
let sum01 = f.i64_add(p0, p1);
let total = f.i64_add(sum01, p2);
f.ret(&[total]);

let ra = allocate_registers(ir::Target::x86_64_linux(), &f);
assert_ne!(ra.loc(p0), ra.loc(p1));
assert_ne!(ra.loc(p1), ra.loc(p2));
assert_ne!(ra.loc(p0), ra.loc(p2));
```

### 3. Automatic Call-Spanning Promotion to Callee-Saved Registers (`RBX` / `X19`)

```rust
use obsidian_ir as ir;
use obsidian_isa_aarch64::regs::Gpr;
use obsidian_regalloc::allocate_registers;

let mut module = ir::Module::new("call_test", ir::Target::aarch64_macos());
let helper_sym = module.add_extern_fn(
    "helper",
    ir::CallConv::C,
    &[ir::Type::I64],
    &[ir::Type::I64],
    false,
);

let mut f = ir::Function::export("keep_across_call", &[ir::Type::I64], &[ir::Type::I64]);
let [v_x] = f.params();

// `v_saved` is defined BEFORE `call @helper` and used AFTER `call @helper`:
let v_saved = f.i64_add_imm(v_x, 10);
let v_call  = f.call(helper_sym, &[v_x], ir::Type::I64);
let v_res   = f.i64_add(v_saved, v_call);
f.ret(&[v_res]);

let ra = allocate_registers(ir::Target::aarch64_macos(), &f);

// Because `v_saved` spans a call, it is automatically assigned to callee-saved `X19`!
assert!(ra.liveness.ranges[v_saved.index()].spans_call);
assert_eq!(ra.loc(v_saved).as_aarch64_gpr(), Some(Gpr::X19));
assert_ne!(ra.dirty_callee_saved_mask & (1 << (Gpr::X19 as u8)), 0);
```
