# `obsidian-isa-x86`

Pure 1:1 **x86-64 Silicon Specification** for the **Obsidian** compiler infrastructure (`#![no_std]`, zero dependencies, 100% `const`-evaluatable).

---

## Core Design Rules

1. **1:1 Hardware Truth—Nothing More, Nothing Less:**
   Unlike LLVM (which pollutes `X86InstrInfo.td` with hundreds of fake compiler pseudo-instructions), `obsidian-isa-x86` **never invents opcodes, virtual registers, or synthetic encodings**. Everything in this crate is strictly what the physical x86-64 CPU silicon decodes and executes according to the Intel SDM and AMD64 Architecture Programmer's Manual.
2. **Static Hardware Oracle, Smart Downstream Packing:**
   This crate does **not** perform optimization passes or make scheduling decisions. Instead, it organizes all immutable hardware facts (`Gpr`, `SimdReg`, `CondCode`, `MemoryOperand`, `Opcode`, `InstDesc`, `uarch`) and stateless 1-instruction bit-packers (`encode`) so higher layers (`obsidian-ir`, `obsidian-abi`, `obsidian-regalloc`, `obsidian-codegen-x86`, `obsidian-obj`) reuse the exact same Rust types without translation.

---

## Module Layout

| Module | What Lives Here |
| :--- | :--- |
| [`regs`] | Physical registers (`Gpr::Rax..R15`, `SimdReg::V0..V31`, `KReg::K0..K7`, `HighByteReg`), operand widths (`GprSize`), bitmasks (`GprMask`), and hardware encoding quirks (`RSP`/`R12` SIB requirement, `RBP`/`R13` `disp8=0` requirement, `R8..R15` `REX` bits). |
| [`cond`] | Architectural status flags (`FlagSet`: `CF`, `PF`, `AF`, `ZF`, `SF`, `OF`, `DF`) and the 16 hardware condition codes (`CondCode`: `O`, `NO`, `B`, `AE`, `E`, `NE`, `BE`, `A`, `S`, `NS`, `P`, `NP`, `L`, `GE`, `LE`, `G`) with `invert()` and `commute()`. |
| [`addr`] | Hardware memory addressing (`Scale`: `S1`, `S2`, `S4`, `S8`; `Disp`: `Auto`, `ForceDisp8`, `ForceDisp32`; `MemoryOperand`: `[base + index*scale + disp]` and `[rip + disp32]`) + exact byte-length calculation (`addressing_byte_len`). |
| [`opcode`] | Real x86-64 silicon opcodes (`Opcode`) and static hardware descriptors (`InstDesc`: `tied_dst_src0`, `flags_read`, `flags_written`, `fixed_uses`, `fixed_defs`, `uops`, `has_false_dep`, `rat_move_elim`, `macro_fusion`). |
| [`uarch`] | Micro-architectural hardware rules: 32-byte $\mu\text{op}$ cache (`DSB`) limits, macro-fusion pairing (`can_macro_fuse`), un-lamination detection (`causes_unlamination`), 0-cycle RAT move-elimination checks (`is_rat_zero_cycle_move`), and the 4-register false-dependency ring (`FALSE_DEP_GPR_RING`). |
| [`encode`] | Stateless single-instruction byte encoders (`EncodedInst`: `[u8; 15]` + `len: u8`) covering Legacy, `REX`, `0F`, `0F 38`, 2-byte `VEX` (`C5`), and 3-byte `VEX` (`C4`). |

---

## End-to-End Usage Examples

### 1. Inspecting Physical Registers & Silicon Encoding Quirks

In x86-64 silicon, register IDs are not uniform—specific registers force extra `SIB`, `disp8`, or `REX` bytes when used in memory addressing:

```rust
use obsidian_isa_x86::{Gpr, GprSize, MemoryOperand};

// 1. Hardware 4-bit register numbers (0..=15) and assembly names:
assert_eq!(Gpr::Rax.hw_id(), 0);
assert_eq!(Gpr::R8.hw_id(), 8);
assert_eq!(Gpr::Rax.name(GprSize::B64), "rax");
assert_eq!(Gpr::Rax.name(GprSize::B32), "eax");
assert_eq!(Gpr::R8.name(GprSize::B8),   "r8b");

// 2. Silicon Quirk #1: RSP (4) and R12 (12) share low 3 bits == 0b100,
//    which forces an extra SIB byte (0x24) when used as a base pointer `[rsp]` or `[r12]`:
assert!(Gpr::Rsp.base_requires_sib());
assert!(Gpr::R12.base_requires_sib());
assert_eq!(MemoryOperand::base(Gpr::Rcx).addressing_byte_len(), 1); // ModRM only
assert_eq!(MemoryOperand::base(Gpr::Rsp).addressing_byte_len(), 2); // ModRM + SIB

// 3. Silicon Quirk #2: RBP (5) and R13 (13) share low 3 bits == 0b101,
//    which forces a 1-byte `disp8 = 0x00` when used as `[rbp]` or `[r13]`:
assert!(Gpr::Rbp.base_requires_disp8_zero());
assert!(Gpr::R13.base_requires_disp8_zero());
assert_eq!(MemoryOperand::base(Gpr::Rbp).disp_byte_len(), 1);

// 4. Silicon Quirk #3: RSP cannot be a scaled index register, but R12 CAN!
assert!(!Gpr::Rsp.can_be_index());
assert!(Gpr::R12.can_be_index());
```

---

### 2. Querying Static Opcode Hardware Facts (`Opcode::desc()`)

Every real x86-64 instruction exposes its exact operand constraints, flag clobbers, fixed register uses/defs, and $\mu\text{op}$ profile via `const fn desc()`:

```rust
use obsidian_isa_x86::{FlagSet, Gpr, Opcode};

// `ADD r64, r64` is a destructive 2-operand instruction (`dst == src0`) that clobbers all 6 flags:
let add = Opcode::Add64rr.desc();
assert!(add.tied_dst_src0);
assert!(add.is_commutative);
assert_eq!(add.flags_written, FlagSet::ALL_ARITH);
assert_eq!(add.uops, 1);

// `LEA r64, [base + index*scale + disp]` is non-destructive 3-way math and touches ZERO flags:
let lea = Opcode::Lea64.desc();
assert!(!lea.tied_dst_src0);
assert!(lea.flags_written.is_empty());
assert_eq!(lea.uops, 1);

// Legacy variable shift `SHL r64, CL` implicitly requires `RCX` and takes 2 uops,
// whereas BMI2 `SHLX r64, src, cnt` is non-destructive, flagless, 1 uop, with NO `RCX` tie:
let shl_cl = Opcode::Shl64rcl.desc();
assert!(shl_cl.tied_dst_src0);
assert!(shl_cl.fixed_uses.contains(Gpr::Rcx));
assert_eq!(shl_cl.uops, 2);

let shlx = Opcode::Shlx64rrr.desc();
assert!(!shlx.tied_dst_src0);
assert!(shlx.fixed_uses.is_empty());
assert!(shlx.flags_written.is_empty());
assert_eq!(shlx.uops, 1);

// `MOVABS r64, imm64` consumes 2 slots in the CPU's 32-byte DSB uop cache:
assert_eq!(Opcode::Movabs64ri64.desc().uops, 2);
```

---

### 3. Querying Micro-Architectural Rules (`uarch`)

Downstream passes in `obsidian-codegen-x86` and `obsidian-regalloc` query `obsidian_isa_x86::uarch` to enforce macro-fusion, prevent hardware un-lamination, eliminate moves in 0 cycles via the RAT, and break false dependencies:

```rust
use obsidian_isa_x86::{uarch, CondCode, Gpr, MemoryOperand, Opcode, Scale};

// 1. Macro-Fusion & Un-Lamination Check:
//    `cmp [rsi + rdi], 0` + `jne` combines an indexed memory operand with an immediate,
//    which triggers hardware un-lamination and fails macro-fusion!
let indexed_mem = MemoryOperand::base_index_disp(Gpr::Rsi, Gpr::Rdi, Scale::S1, 0);
assert!(uarch::causes_unlamination(&indexed_mem, /* has_immediate = */ true));
assert!(!uarch::can_macro_fuse_mem(Opcode::Cmp8mi, &indexed_mem, true, CondCode::NE));

//    If `obsidian-ssa-x86` hoists the 0 into a register (`xor eax, eax`) and uses `cmp [rsi + rdi], al`,
//    un-lamination disappears and the CPU macro-fuses load + compare + branch into 1 uop:
assert!(!uarch::causes_unlamination(&indexed_mem, /* has_immediate = */ false));
assert!(uarch::can_macro_fuse_mem(Opcode::Cmp64mr, &indexed_mem, false, CondCode::NE));

// 2. False Destination Dependency Detection (`TZCNT`, `LZCNT`, `POPCNT`, `SQRTSD`, `CVTSI2SD`):
assert!(Opcode::Tzcnt64rr.desc().has_false_dep);
assert_eq!(uarch::FALSE_DEP_GPR_RING, [Gpr::Rax, Gpr::Rdx, Gpr::R8, Gpr::R9]);

// 3. Zero-Cycle Register Alias Table (RAT) Move Elimination:
//    64-bit `mov rax, rbx` costs 0 cycles and 0 ALU ports in the RAT,
//    whereas a 32-bit self-move `mov eax, eax` fails RAT elimination!
assert!(uarch::is_rat_zero_cycle_move(Opcode::Mov64rr, Gpr::Rax, Gpr::Rbx));
assert!(!uarch::is_rat_zero_cycle_move(Opcode::Mov32rr, Gpr::Rax, Gpr::Rax));
```

---

### 4. Stateless Single-Instruction Byte Encoding (`encode`)

`obsidian_isa_x86::encode` packs any single real x86-64 instruction into a stack-allocated `EncodedInst` (`[u8; 15]` + `len: u8`) at compile time or runtime, including explicit displacement-width control (`Disp::ForceDisp32`) for 32-byte DSB window alignment without `NOP`s:

```rust
use obsidian_isa_x86::{encode, Disp, Gpr, MemoryOperand, Scale, SimdReg};

// 1. Standard REX + ALU / LEA encodings:
//    `add r8, r9` -> `4D 01 C8` (3 bytes)
let add = encode::add64_rr(Gpr::R8, Gpr::R9);
assert_eq!(add.as_slice(), &[0x4D, 0x01, 0xC8]);

//    `lea rax, [rdi + rsi*8 + 16]` -> `48 8D 44 F7 10` (5 bytes)
let lea = encode::lea64(
    Gpr::Rax,
    MemoryOperand::base_index_disp(Gpr::Rdi, Gpr::Rsi, Scale::S8, 16),
);
assert_eq!(lea.as_slice(), &[0x48, 0x8D, 0x44, 0xF7, 0x10]);

// 2. Exact byte-length differences caused by hardware register choices:
//    `mov eax, [rcx]` -> 2 bytes (`8B 01`)
//    `mov eax, [rsp]` -> 3 bytes (`8B 04 24`)
//    `mov eax, [rbp]` -> 3 bytes (`8B 45 00`)
//    `mov r8d, [r12]` -> 4 bytes (`45 8B 04 24`)
assert_eq!(encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rcx)).len, 2);
assert_eq!(encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rsp)).len, 3);
assert_eq!(encode::mov32_rm(Gpr::Rax, MemoryOperand::base(Gpr::Rbp)).len, 3);
assert_eq!(encode::mov32_rm(Gpr::R8,  MemoryOperand::base(Gpr::R12)).len, 4);

// 3. Explicit displacement widening (for 32-byte DSB uop-cache window alignment without NOPs):
//    Widening `mov eax, [rcx]` from 0-byte disp (2B) to standard `disp32=0` (6B):
let widened = encode::mov32_rm(
    Gpr::Rax,
    MemoryOperand::BaseIndex {
        base: Some(Gpr::Rcx),
        index: None,
        disp: Disp::ForceDisp32(0),
    },
);
assert_eq!(widened.as_slice(), &[0x8B, 0x81, 0x00, 0x00, 0x00, 0x00]);

// 4. VEX2 (`C5`) & VEX3 (`C4`) BMI2 / AVX / FMA3 encodings:
//    `vxorps xmm0, xmm0, xmm0` -> 2-byte VEX `C5 F8 57 C0` (4 bytes)
let vxor = encode::vxorps_xmm(SimdReg::XMM0, SimdReg::XMM0, SimdReg::XMM0);
assert_eq!(vxor.as_slice(), &[0xC5, 0xF8, 0x57, 0xC0]);

//    `shlx rax, rdi, rsi` -> 3-byte VEX `C4 E2 C9 F7 C7` (5 bytes)
let shlx = encode::shlx64_rrr(Gpr::Rax, Gpr::Rdi, Gpr::Rsi);
assert_eq!(shlx.as_slice(), &[0xC4, 0xE2, 0xC9, 0xF7, 0xC7]);

//    `vfmadd213sd xmm0, xmm1, xmm2` -> `C4 E2 F1 A9 C2` (5 bytes)
let fma = encode::vfmadd213sd_xmm(SimdReg::XMM0, SimdReg::XMM1, SimdReg::XMM2);
assert_eq!(fma.as_slice(), &[0xC4, 0xE2, 0xF1, 0xA9, 0xC2]);
```

---

## Running Tests

```bash
cargo test -p obsidian-isa-x86
```
