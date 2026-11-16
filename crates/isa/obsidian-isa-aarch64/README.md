# `obsidian-isa-aarch64`

**1:1 Silicon Truth for 64-bit ARM (`AArch64` / `ARMv8-A` & `ARMv9-A` `A64` Instruction Set) — Nothing More, Nothing Less.**

`obsidian-isa-aarch64` is a `#![no_std]`, zero-dependency, `const`-evaluatable hardware specification and stateless 32-bit instruction encoder for the AArch64 (`A64`) architecture.

---

## Architectural Philosophy

1. **1:1 Silicon Truth:** This crate contains **zero** compiler pseudo-instructions, **zero** virtual registers, and **zero** synthetic multi-instruction macro expansions. Every type and constant directly models physical ARMv8-A / ARMv9-A silicon as specified in the *Arm Architecture Reference Manual for A-profile architecture*.
2. **Static Hardware Oracle, Smart Downstream Packing:**
   - **What lives here:** Physical register files (`X0..X30`, the context-sensitive `31` encoding split between `XZR` and `SP`, `V0..V31` Neon/FP, `P0..P15` SVE predicates), `NZCV` condition flags, the 13-bit rotating bitmask immediate encoder (`LogicalImm`), `LDP`/`STP` 7-bit scaled paired offsets (`PairOffset7`), static instruction descriptors (`Opcode::desc()`), micro-architectural fusion queries (`uarch`), and stateless 4-byte Little-Endian instruction encoders (`encode`).
   - **Where speedups happen:** All performance wins occur downstream in `obsidian-codegen-aarch64` and `obsidian-regalloc`, which query this crate to pack consecutive loads/stores into single-cycle `LDP`/`STP` pairs, fold shifts/extends into single-cycle arithmetic (`ADD Xd, Xn, Xm, LSL #3`), replace short branches with `CSEL`/`CSINC`, and schedule `CMP` + `B.cond` macro-fusion pairs.

---

## End-to-End Usage Examples

### 1. Enforcing the Register `31` (`XZR` vs `SP`) Silicon Split at Compile Time

In `A64` machine code, the 5-bit register field `0b11111` (`31`) does **not** have a single meaning:
- In data, shifted-register ALU, and branch target fields, `31` is the **Zero Register (`XZR` / `WZR`)**.
- In memory base-register fields and immediate `ADD`/`SUB` fields, `31` is the **Stack Pointer (`SP` / `WSP`)**.

`obsidian-isa-aarch64` models this directly in the Rust type system via `Gpr` (`0..=30`), `GprOrXzr`, and `GprOrSp`:

```rust
use obsidian_isa_aarch64::regs::{Gpr, GprMask, GprOrSp, GprOrXzr};

// Both XZR and SP encode to the exact same 5-bit hardware value (31 / 0b11111),
// but their distinct types prevent mixing them up in instruction encoders:
assert_eq!(GprOrXzr::Xzr.hw_id5(), 31);
assert_eq!(GprOrSp::Sp.hw_id5(), 31);

// Named AAPCS64 architectural aliases:
assert_eq!(Gpr::FP, Gpr::X29);
assert_eq!(Gpr::LR, Gpr::X30);

// Constant-time 32-bit bitset for register allocation:
let live = GprMask::EMPTY.insert(Gpr::X0).insert(Gpr::X19).insert(Gpr::LR);
assert_eq!(live.len(), 3);
assert_eq!(live.first(), Some(Gpr::X0));
```

### 2. Querying Static Hardware Descriptors & `LDP`/`STP` Fusion Legality

Unlike x86, standard AArch64 ALU instructions (`ADD`, `SUB`, `AND`, `ORR`, `MADD`) **never** clobber the `NZCV` flags—only `S`-suffixed instructions (`ADDS`, `SUBS`/`CMP`, `ANDS`/`TST`) write `NZCV`. Furthermore, fusing two 64-bit loads into `LDP Xt1, Xt2, [Xn, #imm]` is illegal in silicon (`CONSTRAINED UNPREDICTABLE` / `SIGILL`) if `Xt1 == Xt2`:

```rust
use obsidian_isa_aarch64::cond::NzcvFlags;
use obsidian_isa_aarch64::opcode::Opcode;
use obsidian_isa_aarch64::regs::{Gpr, GprOrSp};
use obsidian_isa_aarch64::uarch;

// Standard AddShifted never modifies NZCV flags, allowing freely reordered scheduling:
let add_desc = Opcode::AddShifted.desc();
assert!(add_desc.flags_written.is_empty());

// SubsShifted (which backs CMP) writes all four N, Z, C, V flags:
let subs_desc = Opcode::SubsShifted.desc();
assert_eq!(subs_desc.flags_written, NzcvFlags::ALL);

// Query whether two 8-byte loads `[sp, #16]` and `[sp, #24]` can fuse into `LDP`:
let pair_off = uarch::can_fuse_into_ldp64(
    Gpr::X19,
    Gpr::X20,
    GprOrSp::Sp,
    16,
    24,
    false,
);
assert!(pair_off.is_some());
assert_eq!(pair_off.unwrap().byte_offset_64(), 16);

// Silicon hazard check: `ldp x0, x0, [sp, #16]` (identical destination registers) is rejected!
assert!(uarch::can_fuse_into_ldp64(
    Gpr::X0,
    Gpr::X0,
    GprOrSp::Sp,
    16,
    24,
    false,
).is_none());

// However, storing two adjacent 64-bit values at `[sp, #16]` and `[sp, #24]` via `STP` IS legal:
assert!(uarch::can_fuse_into_stp64(16, 24).is_some());
```

### 3. Encoding AArch64's 13-bit Rotating Bitmask Immediates (`LogicalImm`)

AArch64 `AND`, `ORR`, `EOR`, and `ANDS` immediate instructions encode 64-bit bitmasks (repeating patterns of $2, 4, 8, 16, 32,$ or $64$ bits of contiguous ones rotated by any amount) into a 13-bit `N:immr:imms` field:

```rust
use obsidian_isa_aarch64::addr::LogicalImm;
use obsidian_isa_aarch64::encode;
use obsidian_isa_aarch64::regs::{Gpr, GprOrSp, GprOrXzr};

// 0x0000_0000_0000_FFFF is a 64-bit element with 16 contiguous ones (N=1, immr=0, imms=15):
let mask = LogicalImm::try_encode64(0x0000_0000_0000_FFFF).unwrap();
assert_eq!(mask.n, 1);
assert_eq!(mask.immr, 0);
assert_eq!(mask.imms, 15);

// Encode `and x0, x1, #0xffff` -> 0x92403C20
let inst = encode::and64_imm(GprOrSp::Gpr(Gpr::X0), GprOrXzr::Gpr(Gpr::X1), mask);
assert_eq!(inst.word(), 0x9240_3C20);

// Repeating byte mask `0x5555555555555555` (alternating bits, element size = 2) is also valid!
assert!(LogicalImm::try_encode64(0x5555_5555_5555_5555).is_some());

// All-zeros and all-ones are forbidden by ARMv8 hardware in logical immediates:
assert!(LogicalImm::try_encode64(0).is_none());
assert!(LogicalImm::try_encode64(u64::MAX).is_none());
```

### 4. Stateless 32-bit Little-Endian Instruction Encoding (`encode`)

```rust
use obsidian_isa_aarch64::addr::{PairOffset7, Shift};
use obsidian_isa_aarch64::cond::CondCode;
use obsidian_isa_aarch64::encode;
use obsidian_isa_aarch64::regs::{Gpr, GprOrSp, GprOrXzr};

// 1. `stp x29, x30, [sp, #-16]!` (Prologue save FP & LR with pre-indexed writeback) -> 0xA9BF7BFD
let off_neg16 = PairOffset7::try_new_64(-16).unwrap();
let stp = encode::stp64_pre(
    GprOrXzr::Gpr(Gpr::FP),
    GprOrXzr::Gpr(Gpr::LR),
    GprOrSp::Sp,
    off_neg16,
);
assert_eq!(stp.word(), 0xA9BF_7BFD);
assert_eq!(stp.to_le_bytes(), [0xFD, 0x7B, 0xBF, 0xA9]);

// 2. `add x0, x0, x1, lsl #3` (Scaled pointer/index addition: x0 = x0 + (x1 << 3)) -> 0x8B010C00
let add_lsl = encode::add64_shifted(
    GprOrXzr::Gpr(Gpr::X0),
    GprOrXzr::Gpr(Gpr::X0),
    GprOrXzr::Gpr(Gpr::X1),
    Shift::lsl(3),
);
assert_eq!(add_lsl.word(), 0x8B01_0C00);

// 3. `csel x0, x1, x2, eq` (Branchless select: x0 = (Z == 1) ? x1 : x2) -> 0x9A820020
let csel = encode::csel64(
    GprOrXzr::Gpr(Gpr::X0),
    GprOrXzr::Gpr(Gpr::X1),
    GprOrXzr::Gpr(Gpr::X2),
    CondCode::EQ,
);
assert_eq!(csel.word(), 0x9A82_0020);

// 4. `ldp x29, x30, [sp], #16` + `ret` (Epilogue restore FP & LR and return)
let off_pos16 = PairOffset7::try_new_64(16).unwrap();
let ldp = encode::ldp64_post(
    GprOrXzr::Gpr(Gpr::FP),
    GprOrXzr::Gpr(Gpr::LR),
    GprOrSp::Sp,
    off_pos16,
);
assert_eq!(ldp.word(), 0xA8C1_7BFD);
assert_eq!(encode::ret_lr().word(), 0xD65F_03C0);
```
