# `obsidian-abi`

**Calling Convention, Physical Argument/Return Mapping, and Stack Frame ABI Oracle for Obsidian (`SysV64`, `Win64`, `AAPCS64`, `Darwin ARM64`, `SystemZ s390x`, `M68k`).**

`obsidian-abi` is a `#![no_std]` (`alloc`) crate that bridges `obsidian-ir` (`ir::Target`, `ir::CallConv`, `ir::Function`, `ir::Type`) with the physical register files in `obsidian-isa-<arch>`.

---

## What `obsidian-abi` Computes

1. **Physical Parameter & Multi-Value Return Locations (`ArgLoc`):**
   - Maps each `ir::Type` parameter and return value to its exact physical register (`RegBank::Gpr`, `RegBank::Addr`, `RegBank::FloatVec`), register pair (`RegPair` for `i128` on 64-bit CPUs or `i64` on 32-bit CPUs), or outgoing stack offset (`ArgLoc::Stack`).
2. **Platform & OS Calling Convention Quirks:**
   - **x86-64 System V (`SysV64` / `Darwin`):** `RDI, RSI, RDX, RCX, R8, R9` + independent `XMM0..XMM7`, 128-byte leaf Red Zone, 0 shadow space.
   - **x86-64 Windows (`Win64` / `Msvc`):** Positional slot sharing (`slot 0..4` shared between `RCX, RDX, R8, R9` and `XMM0..XMM3`), mandatory 32-byte shadow space (`[RSP + 0..32]`), non-volatile `RDI`/`RSI`, and 0 Red Zone.
   - **AArch64 Linux (`AAPCS64`) vs. Apple (`Darwin`):** `X0..X7` + `V0..V7`; on Apple Darwin ARM64, narrow stack arguments (`i8`, `i16`, `i32`) pack tightly to their natural alignment instead of 8-byte slots, `X18` is reserved, and leaf functions have a 128-byte Red Zone.
   - **IBM SystemZ (`s390x`):** `R2..R6` (`5` GPRs) + `F0, F2, F4, F6`, with a 160-byte caller register save area (`[R15 + 0..160]`).
   - **Motorola 680x0 (`M68k`):** Exploits the hardware `D0..D7` / `A0..A7` split by passing pointer arguments (`Type::Ptr`) in **Address Registers (`A0, A1`)** and integer arguments in **Data Registers (`D0, D1`)**.
3. **Prologue / Epilogue Stack Frame Layout (`FrameLayout`):**
   - Computes aligned `SP` adjustments (`sp_sub_bytes`), callee-save spill areas, and automatic leaf Red-Zone elision (`sp_sub_bytes = 0`).

---

## End-to-End Usage Examples

### 1. Mapping an `ir::Function` Signature Across `SysV64` vs. `Win64`

```rust
use obsidian_abi::{AbiSig, ArgLoc};
use obsidian_ir as ir;
use obsidian_isa_x86::regs::{Gpr, SimdReg};

// export fn @mix(%ptr: ptr, %scale: f64, %idx: i64) -> (ptr, i64)
let f = ir::Function::export(
    "mix",
    &[ir::Type::Ptr, ir::Type::F64, ir::Type::I64],
    &[ir::Type::Ptr, ir::Type::I64],
);

// 1. On Linux / macOS (System V AMD64 ABI):
//    GPR and XMM counters are independent -> %ptr = RDI, %scale = XMM0, %idx = RSI!
let sysv = AbiSig::for_function(ir::Target::x86_64_linux(), &f);
assert_eq!(sysv.params[0].as_x86_gpr(), Some(Gpr::Rdi));
assert_eq!(sysv.params[1].as_x86_simd(), Some(SimdReg::XMM0));
assert_eq!(sysv.params[2].as_x86_gpr(), Some(Gpr::Rsi));
assert_eq!(sysv.returns[0].as_x86_gpr(), Some(Gpr::Rax));
assert_eq!(sysv.returns[1].as_x86_gpr(), Some(Gpr::Rdx));
assert_eq!(sysv.red_zone_bytes, 128);

// 2. On Windows x64 (MSVC ABI):
//    Positional slots 0..4 are shared -> slot 0 = RCX, slot 1 = XMM1, slot 2 = R8!
let win = AbiSig::for_function(ir::Target::x86_64_windows(), &f);
assert_eq!(win.params[0].as_x86_gpr(), Some(Gpr::Rcx));
assert_eq!(win.params[1].as_x86_simd(), Some(SimdReg::XMM1));
assert_eq!(win.params[2].as_x86_gpr(), Some(Gpr::R8));
assert_eq!(win.shadow_space_bytes, 32);
assert_eq!(win.red_zone_bytes, 0);
```

### 2. Apple Darwin ARM64 vs. Linux `AAPCS64` Stack Packing & `X18` Reservation

```rust
use obsidian_abi::{AbiSig, ArgLoc};
use obsidian_ir as ir;
use obsidian_isa_aarch64::regs::Gpr;

// 8 register args (X0..X7) followed by two narrow `i8` stack arguments:
let params = [
    ir::Type::I64, ir::Type::I64, ir::Type::I64, ir::Type::I64,
    ir::Type::I64, ir::Type::I64, ir::Type::I64, ir::Type::I64,
    ir::Type::I8,  ir::Type::I8,
];

let linux_arm = AbiSig::compute(ir::Target::aarch64_linux(), ir::CallConv::C, &params, &[]);
let macos_arm = AbiSig::compute(ir::Target::aarch64_macos(), ir::CallConv::C, &params, &[]);

// On Linux AAPCS64, each stack argument occupies an 8-byte slot (offsets 0 and 8):
assert_eq!(linux_arm.params[8], ArgLoc::Stack { offset: 0, size: 8, align: 8 });
assert_eq!(linux_arm.params[9], ArgLoc::Stack { offset: 8, size: 8, align: 8 });

// On Apple Darwin ARM64, narrow `i8` stack arguments pack tightly at offsets 0 and 1!
assert_eq!(macos_arm.params[8], ArgLoc::Stack { offset: 0, size: 1, align: 1 });
assert_eq!(macos_arm.params[9], ArgLoc::Stack { offset: 1, size: 1, align: 1 });

// And Darwin reserves platform register X18:
assert_ne!(macos_arm.reserved_gpr_mask & (1 << (Gpr::X18 as u8)), 0);
```

### 3. Motorola 680x0 Split Address (`A0..A1`) vs. Data (`D0..D1`) Parameter Assignment

```rust
use obsidian_abi::AbiSig;
use obsidian_ir as ir;
use obsidian_isa_m68k::regs::{AddrReg, DataReg};

// fn @read_elem(%base: ptr, %idx: i32) -> ptr
let m68k = AbiSig::compute(
    ir::Target::m68k_bare(),
    ir::CallConv::Fast,
    &[ir::Type::Ptr, ir::Type::I32],
    &[ir::Type::Ptr],
);

// %base (ptr) goes in Address Register A0, while %idx (i32) goes in Data Register D0!
assert_eq!(m68k.params[0].as_m68k_addr(), Some(AddrReg::A0));
assert_eq!(m68k.params[1].as_m68k_data(), Some(DataReg::D0));
assert_eq!(m68k.returns[0].as_m68k_addr(), Some(AddrReg::A0));
```

### 4. Stack Frame Layout & Leaf Red-Zone Optimization (`FrameLayout`)

```rust
use obsidian_abi::{AbiSig, FrameLayout};
use obsidian_ir as ir;

let sysv = AbiSig::compute(ir::Target::x86_64_linux(), ir::CallConv::Fast, &[ir::Type::I64], &[ir::Type::I64]);

// A leaf function with 32 bytes of local stack slots and 0 saved registers uses the 128B Red Zone
// (`sp_sub_bytes == 0`, avoiding `sub rsp` / `add rsp` completely!):
let leaf_frame = FrameLayout::compute(&sysv, 32, 0, true, 0, 8, true);
assert!(leaf_frame.uses_red_zone);
assert_eq!(leaf_frame.sp_sub_bytes, 0);

// A non-leaf function on x86-64 (where `CALL` pushed an 8-byte return address) with 0 saved GPRs
// and 16 bytes of locals adjusts `RSP` by 24 bytes so `(24 + 8) == 32` is 16-byte aligned!
let non_leaf_frame = FrameLayout::compute(&sysv, 16, 0, false, 0, 8, true);
assert!(!non_leaf_frame.uses_red_zone);
assert_eq!(non_leaf_frame.sp_sub_bytes, 24);
```
