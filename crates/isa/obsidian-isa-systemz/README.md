# `obsidian-isa-systemz`

Pure 1:1 **IBM SystemZ (`s390x` / z/Architecture) Silicon Specification** for the **Obsidian** compiler infrastructure (`#![no_std]`, zero dependencies, 100% `const`-evaluatable).

---

## Core Design Rules

1. **1:1 Hardware Truth—Nothing More, Nothing Less:**
   `obsidian-isa-systemz` never invents compiler pseudo-instructions, virtual registers, or synthetic encodings. Everything in this crate is strictly what physical IBM SystemZ (`z15` / `z16` / `Telum`) silicon decodes and executes according to the *IBM z/Architecture Principles of Operation (SA22-7832)*.
2. **Static Hardware Oracle, Smart Downstream Packing:**
   This crate does not perform optimization passes or make scheduling decisions. Instead, it organizes all immutable hardware facts (`Gpr`, `GprPair`, `VecReg`, `CondMask`, `Disp12`, `Disp20`, `MemBx`, `Opcode`, `InstDesc`, `uarch`) and stateless 1-instruction big-endian bit-packers (`encode`) so higher layers (`obsidian-ir`, `obsidian-abi`, `obsidian-regalloc`, `obsidian-codegen-systemz`, `obsidian-obj`) reuse the exact same Rust types without translation.

---

## Module Layout

| Module | What Lives Here |
| :--- | :--- |
| [`regs`] | Physical registers (`Gpr::R0..R15`, `GprPair::R0R1..R14R15` for 128-bit even/odd ops, `VecReg::V0..V31` with `F0..F15` high-64 aliasing and `RXB` bits, `AccessReg::A0..A15`) and the **`R0` base/index zero substitution rule** (`can_be_base_or_index`). |
| [`cond`] | The 2-bit `PSW` Condition Code (`CcValue`: `0..=3`) and 4-bit branch mask (`CondMask`: `E = 8`, `L = 4`, `H = 2`, `NE_CMP = 6`, `LE = 12`, `HE = 10`, `ALWAYS = 15`) with `invert_cmp()`, `invert_all()`, and `commute_cmp()`. |
| [`addr`] | Unsigned 12-bit displacements (`Disp12`: `0..=4095`), signed 20-bit split displacements (`Disp20`: `-524_288..=524_287` with `dl_12()` and `dh_8()`), and `MemBx<Reg>` (`D(X, B)`). |
| [`opcode`] | Real SystemZ silicon opcodes (`Opcode`), hardware formats (`InstFormat`: `RR`, `RRE`, `RRF`, `RX`, `RI`, `RXY`, `RSY`, `RIL`, `RIE`, `SS`, `VRR`, `VRX`), and static `InstDesc` tables (`byte_len`, `tied_dst_src0`, `reads_cc`, `writes_cc`, `preserves_high32`, `requires_even_pair`). |
| [`uarch`] | Micro-architectural queries: first-byte instruction length decoding (`inst_byte_len_from_first_byte`), Address Generation Interlock (`AGI`) hazard detection (`causes_agi_hazard` and `LA`/`LAY` bypass), `CC`-preserving fused compare-and-branch checks (`CGRJ`/`CGIJ`), and 256-byte `XC` hardware block zeroing (`is_fast_xc_self_zero`). |
| [`encode`] | Stateless `const fn` 1-instruction big-endian encoders (`EncodedInst`: `2`, `4`, or `6` bytes) covering `RR`, `RRE`, `RRF` (3-op `AGRK`), `RI`, `RX`, `RXY`, `RSY`, `RIL`, `RIE` (`RISBGN`, `CGRJ`, `CGIJ`), `SS` (`MVC`, `XC`), and `VRR` (`VX`). |

---

## End-to-End Usage Examples

### 1. Inspecting SystemZ Register Rules (`%r0` Base/Index Trap, `Even:Odd` Pairs & `RXB`)

SystemZ silicon has unique architectural rules that differ sharply from x86-64 and ARM64:

```rust
use obsidian_isa_systemz::{Disp20, Gpr, GprPair, MemBx, VecReg};

// 1. Silicon Rule #1: `%r0` in a Base (`B`) or Index (`X`) field is decoded by hardware
//    as the constant `0` (no register), NEVER the value inside `%r0`!
assert!(!Gpr::R0.can_be_base_or_index());
assert!(Gpr::R1.can_be_base_or_index());
assert!(Gpr::R15.can_be_base_or_index());

let illegal_mem = MemBx::base(Gpr::R0);
assert!(!illegal_mem.is_legal_in_silicon());

let legal_mem = MemBx::base(Gpr::R15);
assert!(legal_mem.is_legal_in_silicon());

// 2. Silicon Rule #2: 128-bit `Even:Odd` Register Pairs (`MLGR`, `DLGR`, `DSGR`, `FLOGR`):
assert_eq!(GprPair::from_even(Gpr::R2), Some(GprPair::R2R3));
assert_eq!(GprPair::from_even(Gpr::R3), None); // Odd registers cannot anchor a pair!
assert_eq!(GprPair::R2R3.even(), Gpr::R2);
assert_eq!(GprPair::R2R3.odd(),  Gpr::R3);

// 3. Silicon Rule #3: Vector registers `V0..V15` overlay `F0..F15`, while `V16..V31` set `RXB`:
assert!(VecReg::V0.aliases_fpr());
assert!(!VecReg::V16.aliases_fpr());
assert_eq!(VecReg::V18.low4(), 2);
assert_eq!(VecReg::V18.rxb_bit(), 1);
```

---

### 2. Working With SystemZ's 2-Bit `PSW.CC` & 4-Bit `CondMask`

SystemZ encodes branch conditions as a 4-bit mask over the 4 states of `CC` (`CC0 = Equal`, `CC1 = Low`, `CC2 = High`, `CC3 = Overflow`):

```rust
use obsidian_isa_systemz::{CcValue, CondMask};

// Bit 3 (8) = Equal (CC0), Bit 2 (4) = Low (CC1), Bit 1 (2) = High (CC2):
assert!(CondMask::E.matches(CcValue::Cc0));
assert!(CondMask::L.matches(CcValue::Cc1));
assert!(CondMask::H.matches(CcValue::Cc2));
assert_eq!(CondMask::L.union(CondMask::E), CondMask::LE); // 4 | 8 = 12 (0b1100)

// Inverting and commuting integer comparisons:
assert_eq!(CondMask::E.invert_cmp(),  CondMask::NE_CMP);  // 8 (0b1000) <-> 6 (0b0110)
assert_eq!(CondMask::L.invert_cmp(),  CondMask::HE);      // 4 (<)      <-> 10 (>=)
assert_eq!(CondMask::L.commute_cmp(), CondMask::H);       // a < b      <-> b > a
```

---

### 3. Querying Opcode Descriptors & Micro-Architectural Rules (`uarch`)

```rust
use obsidian_isa_systemz::{uarch, Disp20, Gpr, MemBx, Opcode};

// 1. 32-bit instructions (`LR`, `AR`, `L`) preserve the upper 32 bits of the destination GPR!
assert!(Opcode::Lr.desc().preserves_high32);
assert!(Opcode::Ar.desc().preserves_high32);
assert!(!Opcode::Lgr.desc().preserves_high32);

// 2. 2-operand `AGR` is destructive, whereas Distinct-Operands `AGRK` is non-destructive 3-op:
assert!(Opcode::Agr.desc().tied_dst_src0);
assert!(!Opcode::Agrk.desc().tied_dst_src0);

// 3. 64-bit shifts (`SLLG`, `SRLG`) are non-destructive 3-operand AND do not touch `PSW.CC`:
assert!(!Opcode::Sllg.desc().tied_dst_src0);
assert!(!Opcode::Sllg.desc().writes_cc);

// 4. Address Generation Interlock (AGI) Hazard Detection:
//    Computing an address with `AGR` and immediately dereferencing it stalls AGEN,
//    whereas computing it with `LA` / `LAY` executes in AGEN and avoids the stall!
let mem = MemBx::base_disp(Gpr::R3, Disp20::try_new(16).unwrap());
assert!(uarch::causes_agi_hazard(Opcode::Agr, Gpr::R3, &mem));
assert!(!uarch::causes_agi_hazard(Opcode::La,  Gpr::R3, &mem));

// 5. Fused Compare-and-Branch (`CGRJ` / `CGIJ`) branches without clobbering `PSW.CC`:
assert!(uarch::is_cc_preserving_compare_branch(Opcode::Cgrj));
assert!(!Opcode::Cgrj.desc().writes_cc);
```

---

### 4. Stateless Big-Endian Instruction Encoding (`encode`)

Every SystemZ instruction encodes into `2`, `4`, or `6` Big-Endian bytes, where the top 2 bits of the first byte always identify the instruction length to the hardware decoder:

```rust
use obsidian_isa_systemz::{encode, uarch, CondMask, Disp12, Disp20, Gpr, MemBx, VecReg};

// 1. 2-Byte `RR` Format:
//    `br %r14` (`bcr 15, %r14`) -> `07 FE` (2 bytes)
let ret = encode::ret_br_r14();
assert_eq!(ret.as_slice(), &[0x07, 0xFE]);
assert_eq!(uarch::inst_byte_len_from_first_byte(ret.bytes[0]), 2);

// 2. 4-Byte `RRE` & `RRF` (Distinct-Operands 3-Op) Formats:
//    `lgr %r2, %r3` -> `B9 04 00 23` (4 bytes)
assert_eq!(encode::lgr(Gpr::R2, Gpr::R3).as_slice(), &[0xB9, 0x04, 0x00, 0x23]);

//    `agrk %r2, %r3, %r4` (`%r2 = %r3 + %r4`) -> `B9 E8 40 23` (4 bytes)
let agrk = encode::agrk(Gpr::R2, Gpr::R3, Gpr::R4);
assert_eq!(agrk.as_slice(), &[0xB9, 0xE8, 0x40, 0x23]);
assert_eq!(uarch::inst_byte_len_from_first_byte(agrk.bytes[0]), 4);

// 3. 6-Byte `RXY` Format with Signed 20-Bit Split Displacement (`DL` + `DH`):
//    `lg %r2, -8(%r15)` -> `E3 20 FFF8 FF 04` (6 bytes)
let disp_neg8 = Disp20::try_new(-8).unwrap();
let lg = encode::lg(Gpr::R2, MemBx::base_disp(Gpr::R15, disp_neg8));
assert_eq!(lg.as_slice(), &[0xE3, 0x20, 0xFF, 0xF8, 0xFF, 0x04]);
assert_eq!(uarch::inst_byte_len_from_first_byte(lg.bytes[0]), 6);

// 4. 6-Byte `RIE` Format (`RISBGN` & Fused Compare-and-Branch `CGRJ`):
//    `cgrj %r2, %r3, 8 (==), +16 halfwords` -> `EC 23 00 10 80 64` (6 bytes)
let cgrj = encode::cgrj(Gpr::R2, Gpr::R3, CondMask::E, 16);
assert_eq!(cgrj.as_slice(), &[0xEC, 0x23, 0x00, 0x10, 0x80, 0x64]);

// 5. 6-Byte `SS` Format (Hardware 256-Byte `MVC` Copy & `XC` Zeroing):
//    `xc 0(256, %r2), 0(%r2)` -> `D7 FF 20 00 20 00` (zeroes 256 bytes in 1 instruction!)
let xc_zero = encode::xc(Gpr::R2, Disp12::ZERO, Gpr::R2, Disp12::ZERO, 256);
assert_eq!(xc_zero.as_slice(), &[0xD7, 0xFF, 0x20, 0x00, 0x20, 0x00]);

// 6. 6-Byte `VRR` Vector Format with `RXB` Extension Bits (`%v16..%v31`):
//    `vx %v16, %v17, %v18` -> `E7 01 20 00 0E 6D` (`RXB = 0b1110 = 0x0E`)
let vx = encode::vx(VecReg::V16, VecReg::V17, VecReg::V18);
assert_eq!(vx.as_slice(), &[0xE7, 0x01, 0x20, 0x00, 0x0E, 0x6D]);
```

---

## Running Tests

```bash
cargo test -p obsidian-isa-systemz
```
