# `obsidian-isa-m68k`

Pure 1:1 **Motorola 680x0 (`M68k` — 68000 / 68020 / 68040 / 68060) Silicon Specification** for the **Obsidian** compiler infrastructure (`#![no_std]`, zero dependencies, 100% `const`-evaluatable).

---

## Core Design Rules

1. **1:1 Hardware Truth—Nothing More, Nothing Less:**
   `obsidian-isa-m68k` never invents compiler pseudo-instructions, virtual registers, or synthetic encodings. Everything in this crate is strictly what physical Motorola 680x0 silicon decodes and executes according to the *Motorola M68000 Family Programmer's Reference Manual (M68000PM/AD)*.
2. **Static Hardware Oracle, Smart Downstream Packing:**
   This crate does not perform optimization passes or make scheduling decisions. Instead, it organizes all immutable hardware facts (`DataReg`, `AddrReg`, `Reg`, `OpSize`, `RegListMask`, `CcrFlags`, `CondCode`, `EffAddr`, `Opcode`, `InstDesc`, `uarch`) and stateless 1-instruction Big-Endian word-stream encoders (`encode`) so higher layers (`obsidian-ir`, `obsidian-abi`, `obsidian-regalloc`, `obsidian-codegen-m68k`, `obsidian-obj`) reuse the exact same Rust types without translation.

---

## Module Layout

| Module | What Lives Here |
| :--- | :--- |
| [`regs`] | Split register banks (`DataReg::D0..D7` and `AddrReg::A0..A7`), unified 4-bit `Reg`, operand widths (`OpSize`: `Byte`, `Word`, `Long`), `A7` (`SP`) byte auto-increment alignment rules (`auto_inc_dec_step`), and `MOVEM` 16-bit register masks (`RegListMask` with automatic bit-reversal for `-(An)` pre-decrement mode). |
| [`cond`] | The 5-bit Condition Code Register (`CcrFlags`: `X`, `N`, `Z`, `V`, `C` — separating multi-precision Extend `X` from Carry `C`) and all 16 4-bit hardware condition codes (`CondCode`: `T`, `F`, `HI`, `LS`, `CC`, `CS`, `NE`, `EQ`, `VC`, `VS`, `PL`, `MI`, `GE`, `LT`, `GT`, `LE`) with `invert()` and `commute()`. |
| [`addr`] | The 6-bit Effective Address (`EffAddr`: `Dn`, `An`, `(An)`, `(An)+`, `-(An)`, `(d16, An)`, `(d8, An, Xn*scale)`, `(xxx).W`, `(xxx).L`, `(d16, PC)`, `(d8, PC, Xn*scale)`, `#imm`), Brief Extension Word packing (`BriefIndex`), reversed `MOVE` destination EA bits (`move_dst_ea6_reg_mode`), and control/alterable mode validation. |
| [`opcode`] | Real M68k silicon opcodes (`Opcode`) and static `InstDesc` tables (`base_words`, `tied_dst_src0`, `flags_read`, `flags_written`, `targets_addr_reg`). |
| [`uarch`] | Silicon legality queries: `MOVEQ` 8-bit signed range checks (`fits_moveq_i8`), `ADDQ`/`SUBQ`/shift `1..=8` 3-bit encoding where `8` encodes as `0b000` (`encode_quick_1_to_8`), `CCR` preservation when targeting `An` (`preserves_ccr_flags`), and exact instruction byte length (`single_ea_inst_byte_len`). |
| [`encode`] | Stateless `const fn` 1-instruction Big-Endian 16-bit word-stream encoders (`EncodedInst`: `2..=10` bytes) covering `MOVEQ`, `MOVE`, `MOVEA`, `MOVEM`, `LEA`, `PEA`, `ADD`/`ADDA`/`ADDQ`, `SUB`/`SUBA`/`SUBQ`, `CMP`, `TST`, `CLR`, `LSL`/`LSR`, `Bcc`, `DBcc`, `JSR`, `RTS`, `LINK`, and `UNLK`. |

---

## End-to-End Usage Examples

### 1. Split `DataReg` (`D0..D7`) vs. `AddrReg` (`A0..A7`) Silicon Rules

M68k silicon physically partitions its 16 general registers into 8 Data Registers (`D0..D7`) and 8 Address Registers (`A0..A7`) with distinct hardware behavior:

```rust
use obsidian_isa_m68k::{AddrReg, DataReg, OpSize, Reg, RegListMask};

// 1. Silicon Rule #1: Address Registers (`A0..A7`) forbid 8-bit `Byte` operations!
assert!(Reg::Data(DataReg::D0).supports_size(OpSize::Byte));
assert!(!Reg::Addr(AddrReg::A0).supports_size(OpSize::Byte));
assert!(Reg::Addr(AddrReg::A0).supports_size(OpSize::Word));

// 2. Silicon Rule #2: Stepping `A7` (`SP`) with `(sp)+` or `-(sp)` in `Byte` mode
//    steps `A7` by 2 bytes instead of 1 to preserve hardware 16-bit stack alignment:
assert_eq!(AddrReg::A0.auto_inc_dec_step(OpSize::Byte), 1);
assert_eq!(AddrReg::SP.auto_inc_dec_step(OpSize::Byte), 2);
assert_eq!(AddrReg::SP.auto_inc_dec_step(OpSize::Long), 4);

// 3. Silicon Rule #3: `MOVEM` 16-bit register masks reverse their bit order
//    when using Pre-Decrement `-(An)` addressing (`A7 = bit 0 .. D0 = bit 15`):
let mask = RegListMask::from_regs(&[Reg::Data(DataReg::D0)]);
assert_eq!(mask.movem_extension_word(/* is_predecrement = */ false), 0x0001);
assert_eq!(mask.movem_extension_word(/* is_predecrement = */ true),  0x8000);
```

---

### 2. Querying `CCR` Flags (`X` vs. `C`) & Address-Register Flag Suppression

Unlike x86, M68k separates the `Extend (X)` flag from `Carry (C)`, and suppresses **all** flag updates whenever an operation targets an Address Register (`A0..A7`):

```rust
use obsidian_isa_m68k::{uarch, AddrReg, CcrFlags, CondCode, DataReg, EffAddr, Opcode};

// 1. `MOVE`, `CMP`, `TST`, `AND`, `OR`, `EOR` update `N, Z, V, C` but preserve `X`:
assert_eq!(Opcode::Move.desc().flags_written, CcrFlags::NZVC);
assert!(!Opcode::Move.desc().flags_written.contains(CcrFlags::X));

//    `ADD` and `SUB` update all 5 flags (`X, N, Z, V, C`):
assert_eq!(Opcode::Add.desc().flags_written, CcrFlags::XNZVC);

// 2. `ADDA`, `SUBA`, `MOVEA`, `LEA`—and even `ADDQ`/`SUBQ` when targeting `An`—
//    leave all 5 `CCR` flags completely untouched!
assert!(uarch::preserves_ccr_flags(Opcode::Adda, None));
assert!(uarch::preserves_ccr_flags(Opcode::Addq, Some(&EffAddr::AddrDirect(AddrReg::A0))));
assert!(!uarch::preserves_ccr_flags(Opcode::Addq, Some(&EffAddr::DataDirect(DataReg::D0))));

// 3. Flipping bit 0 of any `CondCode` inverts it (`HI <-> LS`, `GE <-> LT`, `GT <-> LE`):
assert_eq!(CondCode::HI.invert(), CondCode::LS);
assert_eq!(CondCode::GE.invert(), CondCode::LT);
assert_eq!(CondCode::GT.commute(), Some(CondCode::LT));
```

---

### 3. Effective Addressing (`EffAddr`) & Brief Extension Words

```rust
use obsidian_isa_m68k::{AddrReg, BriefIndex, DataReg, EffAddr, IndexScale, OpSize, Reg};

// Control addressing mode check (required by `LEA`, `PEA`, `JSR`, `JMP`):
let disp_ea = EffAddr::AddrDisp16(AddrReg::A6, -16);
assert!(disp_ea.is_control_addressing_mode());
assert_eq!(disp_ea.extension_byte_len(OpSize::Long), 2);

// Post-increment `(a0)+` is alterable, but NOT a legal control addressing mode for `LEA`:
let post_inc = EffAddr::AddrPostInc(AddrReg::A0);
assert!(!post_inc.is_control_addressing_mode());
assert!(post_inc.is_alterable());

// Indexed addressing `(12, a0, d1.l * 4)` packs into a 16-bit Brief Extension Word:
let idx = BriefIndex {
    index_reg: Reg::Data(DataReg::D1),
    index_is_long: true,
    scale: IndexScale::S4,
    disp8: 12,
};
assert_eq!(idx.to_extension_word(), 0x1C0C);
```

---

### 4. Stateless Big-Endian Word-Stream Encoding (`encode`)

Every M68k instruction encodes into `2..=10` Big-Endian bytes (1 to 5 16-bit words):

```rust
use obsidian_isa_m68k::{encode, uarch, AddrReg, CondCode, DataReg, EffAddr, OpSize};

// 1. `moveq #-1, d0` -> `70 FF` (2 bytes, sign-extends -1 to 32-bit `0xFFFF_FFFF` in `d0`):
assert!(uarch::fits_moveq_i8(-1));
assert_eq!(encode::moveq(DataReg::D0, -1).as_slice(), &[0x70, 0xFF]);

// 2. `addq.l #8, a7` -> `50 8F` (2 bytes; constant `8` encodes as `0b000` in bits 9..=11!):
assert_eq!(uarch::encode_quick_1_to_8(8), Some(0));
assert_eq!(
    encode::addq(OpSize::Long, 8, EffAddr::AddrDirect(AddrReg::SP)).as_slice(),
    &[0x50, 0x8F]
);

// 3. `lea (-16, a6), a0` -> `41 EE FF F0` (4 bytes):
let lea = encode::lea(EffAddr::AddrDisp16(AddrReg::FP, -16), AddrReg::A0);
assert_eq!(lea.as_slice(), &[0x41, 0xEE, 0xFF, 0xF0]);

// 4. `move.l (8, a6), -(sp)` -> `2F 2E 00 08` (4 bytes, destination EA reverses reg & mode!):
let push_arg = encode::move_ea_ea(
    OpSize::Long,
    EffAddr::AddrDisp16(AddrReg::FP, 8),
    EffAddr::AddrPreDec(AddrReg::SP),
);
assert_eq!(push_arg.as_slice(), &[0x2F, 0x2E, 0x00, 0x08]);

// 5. `dbra d0, -4` (`dbcc CondCode::F, d0, -4`) -> `51 C8 FF FC` (4 bytes):
let dbra = encode::dbcc(CondCode::F, DataReg::D0, -4);
assert_eq!(dbra.as_slice(), &[0x51, 0xC8, 0xFF, 0xFC]);

// 6. `rts` -> `4E 75` (2 bytes):
assert_eq!(encode::rts().as_slice(), &[0x4E, 0x75]);
```

---

## Running Tests

```bash
cargo test -p obsidian-isa-m68k
```
