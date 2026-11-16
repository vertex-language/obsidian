# Proposed Obsidian IR Specification & Rust Architecture (`obsidian-ir`)

> **Unified Blueprint (`.obs` Human-Readable SSA IR + `obsidian-ir` Rust Crate):**
> This document combines the complete specification for Obsidian's **`.obs` SSA Bedrock IR** (Part I) and its **Rust implementation in [obsidian-ir](../crates/obsidian-ir)** (`ir::Module`, `ir::Target`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`) (Part II). See [proposed_obsidian.md](proposed_obsidian.md) for the complete multi-layer compiler architecture.

---

# Part I: The `.obs` Human-Readable SSA Bedrock Specification

## 1. What Is "SSA Bedrock"?

Every high-level programming language (Rust, C, C++, Zig, Go, Swift) constantly evolves new surface syntax, type-system rules, and abstractions (`async`/`await`, closures, `match`, traits, generics, iterators, borrow checking, slices `&[u8]`, `Option<T>`). Yet underneath all surface syntax, every program eventually hits **computational bedrock**:

> **Definition — SSA Bedrock (`.obs` / `obsidian-ir`):**
> **Bedrock** is the exact boundary where high-level frontend abstractions cannot be decomposed any further without naming a specific CPU's physical registers and byte encodings.
>
> At Bedrock, all language syntax disappears because **every possible computation reduces to a closed, finite set of primitive operations on fixed-width bitvectors (`i1`, `i8..i128`, `f32`, `f64`, `ptr`, `v128`)**:
> 1. **Move bits** between registers and memory (`load`, `store`, `atomic.*`, `mem.copy`, `mem.fill`, `stack.slot`).
> 2. **Transform bits** inside registers (integer math, overflow/carry math, bitwise logic, shifts/rotates, bit counting, float math, width casts, `v128` SIMD).
> 3. **Compare bits and route control flow** (`cmp.*`, `select`, `br`, `br_if`, `br_table`, `call`, `tailcall`, `ret`, `trap`).

### The 3-Rule Bedrock Boundary Test

```text
+-----------------------------------------------------------------------------+
| 1. ABOVE BEDROCK: Frontend Language (Rust / C / Zig)                        |
|    - Infinite shapes: generics, traits, lifetimes, closures, async/await,   |
|      slices &[u8], enums, loops, operator symbols (+, -, ?).                |
|    - Rule: If it can be desugared into existing Bedrock opcodes without     |
|      losing hardware semantics, it is lowered before reaching .obs.         |
+-----------------------------------------------------------------------------+
                                      |
                                      v  (Flattens once into)
+-----------------------------------------------------------------------------+
| 2. AT BEDROCK: Obsidian IR (.obs / obsidian-ir)                             |
|    - Finite, closed set of orthogonal opcodes on i1, i8..i128, f32/f64,     |
|      ptr, and v128, plus explicit Target and zero-bloat Metadata.           |
|    - Rule: An operation is a Bedrock opcode if and only if decomposing it   |
|      further in the frontend would destroy a 1-instruction hardware         |
|      capability (e.g., popcnt, clz, rotl, byte_swap, fshl, clmul,           |
|      add.ovf_u, mul.wide_u, divrem, fma, atomic.cas).                       |
+-----------------------------------------------------------------------------+
                                      |
                                      v  (Per-CPU lowering packs into)
+-----------------------------------------------------------------------------+
| 3. BELOW BEDROCK: Hardware ISA (obsidian-isa-x86, aarch64, systemz, m68k)   |
|    - Immutable 1:1 silicon truth: physical registers (RAX, X0, D0),         |
|      status flags (EFLAGS, NZCV), addressing modes, and byte encodings.     |
+-----------------------------------------------------------------------------+
```

---

## 2. Module Header, `Target` Specification & Metadata

### 2.1 Explicit `target` Header (Without Cryptic Layout Strings)
To lower `obsidian-ir` into machine code, the backend needs to know:
1. Which of the **25 `obsidian-isa-*` architectures** is being targeted (`x86_64`, `aarch64`, `systemz`, `m68k`, `riscv64`, `wasm32`, `avr`, etc.), which automatically determines **pointer width** (`16`, `32`, or `64` bits) and **endianness** (`little` vs `big`).
2. The target **OS & ABI** (`linux`, `macos`/`darwin`, `windows`/`msvc`, `none`/`eabi`), which determines default calling conventions, object file format (`elf`, `macho`, `coff`, `wasm`), and platform-reserved registers (such as `X18` on Apple Darwin / Windows ARM64).
3. Enabled **CPU subtarget features** (`+avx2`, `+bmi2`, `+lse`, `+sve2`, `+zbb`), which tell the backend whether to emit Tier-1 native instructions or Tier-2 inline instruction sequences.

Instead of unreadable magic layout strings (like LLVM's `target datalayout = "e-m:e-p270:32:32..."`), `.obs` uses a clean, human-readable header at the top of a module:

```text
module @buffer_mod
target arch=aarch64, os=macos, abi=darwin, cpu=apple-m4, features=[+lse, +neon, +sha3]
```

### 2.2 Zero-Bloat Metadata in `.obs`
In `.obs`, metadata is restricted to **four concrete, hardware-meaningful forms** that require zero heap-allocated graph nodes:

| Metadata Kind | `.obs` Syntax | Purpose & How Backends Use It |
| :--- | :--- | :--- |
| **1. Function Attributes (`FnAttrs`)** | `inline(always)`, `inline(never)`, `cold`, `noreturn`, `naked`, `align=N`, `section="name"` | Guides inlining, cold-block placement, stack-frame omission (`naked`), cache-line alignment, and custom linker sections. |
| **2. Symbol Attributes (`SymAttrs`)** | `align=N`, `section="name"`, `used` | Places globals/constants in specific ELF/Mach-O/COFF sections (e.g. `section=".boot"`) and enforces alignment. |
| **3. Branch & Memory Hints** | `br_if.likely`, `br_if.unlikely`, `volatile`, `align=N`, `[unroll(N)]` | Tells the per-CPU backend which branch is the hot fallthrough path (crucial for x86 32-byte $\mu\text{op}$ cache windows!) and enforces MMIO/alignment rules. |
| **4. Debug Source Locations (`@loc`)** | `@loc(file_id, line, col)` (optional trailing annotation) | Maps instructions to `(file, line, col)` for DWARF/CodeView debug info and panic backtraces; stored in an optional side-array outside `InstNode`. |

---

## 3. How Frontend Constructs Flatten into `.obs` Bedrock

| Frontend Concept (Rust) | `.obs` Bedrock Representation | Why |
| :--- | :--- | :--- |
| `pub fn` / `#[no_mangle]` | `export fn @name(...)` | Visible to external object files and the system linker. |
| Private / internal `fn` | `local fn @name(...)` | Internal to the module; eligible for inlining and dead-stripping. |
| `const fn` | Normal `fn` (or folded constant) | `const fn` is a frontend compile-time rule; at codegen time it is normal machine code. |
| `&T`, `&mut T`, `*const T`, `Box<T>` | `ptr` | All references and pointers are untyped machine addresses at Bedrock. |
| `&[u8]`, `&str` (fat pointers) | `(ptr, i64)` | Lowered to scalar value pairs (`data_ptr`, `len`) passed/returned in two registers. |
| `struct Point { x: f64, y: f64 }` | `(f64, f64)` in regs or `stack.slot` | Small aggregates are scalarized into SSA values; large/address-taken ones use `stack.slot`. |
| Pure local `let mut x = 1; x += 2;` | `%x0 = ...`, `%x1 = i64.add %x0, 2` | Pure locals never touch memory—they are direct SSA values (zero `alloca`/`mem2reg` bloat). |
| Address-taken local `let x = 1; f(&x);` | `%slot = stack.slot size=8, align=8` | Only variables whose memory address is actually taken allocate a stack frame slot. |
| `x + y` (wrapping/checked/saturating) | `i64.add` / `i64.add.ovf_s` / `i64.add.sat_s` | Overflow behavior is encoded explicitly in the opcode, never hidden by a `+` symbol. |
| `if` / `while` / `loop` / `match` | Basic blocks + `br`, `br_if`, `br_table` | All structured control flow flattens into basic blocks with explicit block parameters. |

---

## 4. Core Syntax & Structural Rules of `.obs`

1. **Distinct Token Prefixes (Zero-Lookup Lexing):**
   - **`@name` — Module-Level Symbols:** Module name, functions (`@main`), globals (`@COUNTER`), constants (`@HEX_CHARS`), and imports (`@write`).
   - **`%name` — Function-Local SSA Values:** Function parameters (`%self`), block parameters (`%i`), and instruction results (`%v1`). Assigned **at most once** per function.
   - **`label:` — Basic Block Labels:** Unprefixed identifier followed by `:` (`entry:`, `loop(%i: i64, %acc: i64):`).
2. **Parameter Order (`%name: type`):**
   - Always value name first, colon, type second (`%self: ptr, %offset: i64`).
3. **Namespaced Opcodes (`<type>.<op>`):**
   - Every value-producing instruction begins with its primary operand/result type (`i64.add`, `i32.shr.u`, `f64.fma`, `ptr.add`).
4. **Multi-Value Assignments (`%a, %b = ...`):**
   - Two-output hardware instructions (`add.ovf_u`, `divrem.u`, `mul.wide_u`, `atomic.cas`, or multi-return calls) bind multiple SSA values directly on the left-hand side.
5. **Basic Block Parameters (No `phi` Instructions):**
   - Control-flow merges pass arguments directly on branch edges (`br loop(0, 0)`) into parameterized basic blocks (`loop(%i: i64, %sum: i64):`).

---

## 5. Complete `.obs` Bedrock Type System (10 Types)

| Type | Rust `ir::Type` | Width | Description |
| :--- | :--- | :--- | :--- |
| `i1` | `Type::I1` | 1-bit | Boolean condition value produced by `*.cmp.*`, `.ovf_*`, `.cas`, and consumed by `br_if`, `select`, `add.carry`. |
| `i8` | `Type::I8` | 8-bit | Signless byte integer (`u8` / `i8`). |
| `i16` | `Type::I16` | 16-bit | Signless 16-bit integer (`u16` / `i16`). |
| `i32` | `Type::I32` | 32-bit | Signless 32-bit integer (`u32` / `i32`). |
| `i64` | `Type::I64` | 64-bit | Signless 64-bit integer (`u64` / `i64`). |
| `i128` | `Type::I128` | 128-bit | Signless 128-bit integer (`u128` / `i128`; lowered to register pairs on 64-bit CPUs). |
| `f32` | `Type::F32` | 32-bit | IEEE-754 binary32 single-precision float. |
| `f64` | `Type::F64` | 64-bit | IEEE-754 binary64 double-precision float. |
| `ptr` | `Type::Ptr` | Target pointer | Memory address (`64-bit`, `32-bit`, or `16-bit` per `target arch`). |
| `v128` | `Type::V128` | 128-bit | SIMD vector register (`XMM` on x86, `V0..V31` on AArch64/SystemZ). |

---

## 6. Complete `.obs` Bedrock Opcode Catalog

### 6.1 Constants, Pointers, Stack Slots & Variadics (`const`, `ptr`, `stack`, `frame`, `va`)
| Opcode Syntax | Output | Semantics |
| :--- | :--- | :--- |
| `<type>.const <literal>` | `T` | Materialize an immediate constant (`%c = i64.const 42`, `%f = f64.const 3.14`). *(Constants may also be written inline as operands: `i64.add %x, 42`).* |
| `ptr.sym @symbol [, offset=N]` | `ptr` | Address of a global, constant, TLS variable, or function (`@TABLE`, `@main`). |
| `ptr.add %base, %byte_offset` | `ptr` | Offset pointer by `%byte_offset` (`i64`/`isize`) bytes. |
| `ptr.diff %p1, %p2` | `i64` | Signed byte distance `%p1 - %p2` between two pointers. |
| `ptr.to_int %p` / `ptr.from_int %i` | `i64` / `ptr` | Explicit integer $\leftrightarrow$ pointer cast (`expose_provenance` / `with_exposed_provenance`). |
| `stack.slot size=N, align=M` | `ptr` | Reserves a static `N`-byte, `M`-aligned slot in the current function's stack frame. |
| `stack.alloc_dyn %bytes, align=M` | `ptr` | Dynamically extends the stack frame by `%bytes` (for C `alloca` or dynamic stack buffers). |
| `frame.addr` / `frame.return_addr` | `ptr` | Current frame pointer (`FP`/`RBP`) or return address (`LR` / `[RBP+8]`) for backtraces/profilers. |
| `va.start %va_list_ptr` / `va.end %va_list_ptr` | `void` | Initialize / clean up C variadic argument cursor (`...` FFI). |
| `<type>.va.arg %va_list_ptr` | `T` | Fetch next variadic argument of type `T` from `%va_list_ptr`. |

### 6.2 Integer Arithmetic — Standard, Overflowing, Saturating, Carry & Widening (`i8..i128`)
| Opcode Syntax | Output | Semantics & Hardware Mapping |
| :--- | :--- | :--- |
| `<int>.add %a, %b` | `T` | Wrapping two's-complement addition (`a + b`). |
| `<int>.sub %a, %b` | `T` | Wrapping two's-complement subtraction (`a - b`). |
| `<int>.mul %a, %b` | `T` | Wrapping low-half multiplication (`a * b`). |
| `<int>.neg %a` / `<int>.abs %a` | `T` | Two's-complement negation (`0 - a`) / absolute value. |
| `<int>.div.s %a, %b` / `<int>.div.u %a, %b` | `T` | Signed / unsigned integer quotient (`a / b`). |
| `<int>.rem.s %a, %b` / `<int>.rem.u %a, %b` | `T` | Signed / unsigned integer remainder (`a % b`). |
| `<int>.divrem.s %a, %b` / `<int>.divrem.u %a, %b` | `(T, T)` | Simultaneous `(quotient, remainder)` (1 `DIV`/`IDIV` on x86, `DLGR` on SystemZ, `DIVU.L` on M68k). |
| `<int>.add.ovf_s %a, %b` / `<int>.add.ovf_u %a, %b` | `(T, i1)` | Wrapping sum + `i1` overflow/carry flag (Rust `checked_add` / `overflowing_add`). |
| `<int>.sub.ovf_s %a, %b` / `<int>.sub.ovf_u %a, %b` | `(T, i1)` | Wrapping difference + `i1` overflow/borrow flag. |
| `<int>.mul.ovf_s %a, %b` / `<int>.mul.ovf_u %a, %b` | `(T, i1)` | Wrapping product + `i1` overflow flag. |
| `<int>.add.sat_s %a, %b` / `<int>.add.sat_u %a, %b` | `T` | Saturating addition clamped to `MIN..=MAX` (Rust `saturating_add`). |
| `<int>.sub.sat_s %a, %b` / `<int>.sub.sat_u %a, %b` | `T` | Saturating subtraction clamped to `MIN..=MAX`. |
| `<int>.add.carry %a, %b, %cin_i1` | `(T, i1)` | Add with carry-in and carry-out (`ADC` on x86/AArch64, `ALCG` on SystemZ, `ADDX` on M68k). |
| `<int>.sub.borrow %a, %b, %bin_i1` | `(T, i1)` | Subtract with borrow-in and borrow-out (`SBB`/`SBC`/`SLBG`/`SUBX`). |
| `<int>.mul.wide_s %a, %b` / `<int>.mul.wide_u %a, %b` | `(T, T)` | Full double-width product `(low, high)` (1 `MUL`/`IMUL` on x86, `MUL+UMULH` on AArch64). |

### 6.3 Bitwise, Shifts & Bit Manipulation (`i8..i128`)
| Opcode Syntax | Output | Semantics & Hardware Mapping |
| :--- | :--- | :--- |
| `<int>.and %a, %b` | `T` | Bitwise AND (`a & b`). |
| `<int>.or %a, %b` | `T` | Bitwise OR (`a \| b`). |
| `<int>.xor %a, %b` | `T` | Bitwise XOR (`a ^ b`). |
| `<int>.not %a` | `T` | Bitwise inversion (`~a`). |
| `<int>.and_not %a, %b` | `T` | Bitwise AND-NOT (`a & ~b` — maps to `ANDN` on x86 BMI1, `BIC` on AArch64). |
| `<int>.or_not %a, %b` / `<int>.xor_not %a, %b` | `T` | Bitwise OR-NOT (`ORN` on AArch64) / XOR-NOT (`EON` on AArch64). |
| `<int>.shl %a, %amt` | `T` | Logical shift left (`a << (amt & mask)`). |
| `<int>.shr.u %a, %amt` | `T` | Logical zero-filling shift right (`a >> (amt & mask)`). |
| `<int>.shr.s %a, %amt` | `T` | Arithmetic sign-extending shift right (`a >> (amt & mask)`). |
| `<int>.rotl %a, %amt` / `<int>.rotr %a, %amt` | `T` | Rotate left / rotate right (`ROL`/`ROR` on x86/M68k, `ROR`/`EXTR` on AArch64, `RLLG` on SystemZ). |
| `<int>.fshl %hi, %lo, %amt` / `<int>.fshr %hi, %lo, %amt` | `T` | Funnel shift across two words (`SHLD`/`SHRD` on x86, `EXTR` on AArch64). |
| `<int>.clz %a` | `T` | Count leading zeros (`LZCNT` on x86, `CLZ` on AArch64, `FLOGR` on SystemZ). |
| `<int>.ctz %a` | `T` | Count trailing zeros (`TZCNT` on x86, `RBIT+CLZ` on AArch64). |
| `<int>.popcnt %a` | `T` | Count set `1` bits (`POPCNT` on x86/SystemZ, `CNT` on AArch64, or Tier-2 SWAR synthesis). |
| `<int>.byte_swap %a` | `T` | Reverse byte order (`BSWAP` on x86, `REV` on AArch64, `LRVG` on SystemZ). |
| `<int>.bit_reverse %a` | `T` | Reverse all bits (`RBIT` on AArch64, or Tier-2 synthesis). |
| `<int>.clmul %a, %b` | `T` | Carry-less polynomial multiply (`PCLMULQDQ` on x86, `PMULL` on AArch64, `Zbc` on RISC-V). |

### 6.4 Integer Comparisons & Branchless Selection
| Opcode Syntax | Output | Semantics |
| :--- | :--- | :--- |
| `<int>.cmp.eq %a, %b` / `<int>.cmp.ne %a, %b` | `i1` | Equal (`==`) / Not Equal (`!=`). *(Also works on `ptr`: `ptr.cmp.eq`, `ptr.cmp.ult`).* |
| `<int>.cmp.slt %a, %b` / `<int>.cmp.sle %a, %b` | `i1` | Signed `<` / Signed `<=`. |
| `<int>.cmp.sgt %a, %b` / `<int>.cmp.sge %a, %b` | `i1` | Signed `>` / Signed `>=`. |
| `<int>.cmp.ult %a, %b` / `<int>.cmp.ule %a, %b` | `i1` | Unsigned `<` / Unsigned `<=`. |
| `<int>.cmp.ugt %a, %b` / `<int>.cmp.uge %a, %b` | `i1` | Unsigned `>` / Unsigned `>=`. |
| `<type>.select %cond_i1, %if_true, %if_false` | `T` | Branchless conditional value selection (`CMOVcc` on x86, `CSEL` on AArch64, `LOCGR` on SystemZ). |
| `<int>.min.s`, `<int>.min.u`, `<int>.max.s`, `<int>.max.u` | `T` | Branchless integer minimum / maximum. |

### 6.5 Width Conversions, Casts & Bitcasts
| Opcode Syntax | Output | Semantics |
| :--- | :--- | :--- |
| `<dst_int>.ext.u %src_narrow` | `dst_int` | Zero-extend narrower integer (`i8/i16/i32`) to wider integer (`i16/i32/i64/i128`). |
| `<dst_int>.ext.s %src_narrow` | `dst_int` | Sign-extend narrower integer (`i8/i16/i32`) to wider integer (`i16/i32/i64/i128`). |
| `<dst_int>.trunc %src_wide` | `dst_int` | Truncate wider integer to low bits of narrower integer. |
| `<dst>.bitcast %src` | `dst` | Zero-cost bit reinterpretation between equal-width types (`i64 <-> f64`, `i32 <-> f32`). |
| `<int>.from_float.sat_s %f` / `.sat_u %f` | `int` | Saturating float-to-int conversion (Rust `as i64`: `NaN -> 0`, clamps out-of-range). |
| `<int>.from_float.trunc_s %f` / `.trunc_u %f` | `int` | Unchecked hardware float-to-int truncation (`CVTTSD2SI` / `FCVTZS`). |
| `<float>.from_int.s %i` / `.u %i` | `float` | Signed / unsigned integer to `f32`/`f64` conversion (`CVTSI2SD` / `SCVTF` / `UCVTF`). |
| `f64.promote %x_f32` / `f32.demote %x_f64` | `f64` / `f32` | Float precision widening (`f32 -> f64`) and narrowing (`f64 -> f32`). |

### 6.6 Floating-Point Arithmetic & Comparisons (`f32`, `f64`)
| Opcode Syntax | Output | Semantics |
| :--- | :--- | :--- |
| `<float>.add`, `sub`, `mul`, `div`, `sqrt` | `F` | IEEE-754 arithmetic and square root. |
| `<float>.fma %a, %b, %c` | `F` | Fused multiply-add `(a * b) + c` with single rounding (`VFMADD` on x86, `FMADD` on AArch64/SystemZ). |
| `<float>.abs`, `neg`, `copysign` | `F` | Sign-bit manipulation (`and`/`xor` bitmasks on x86, `FABS`/`FNEG` on AArch64). |
| `<float>.floor`, `ceil`, `trunc`, `nearest` | `F` | IEEE-754 rounding to integral float value (`ROUNDSS`/`ROUNDSD` on x86, `FRINT*` on AArch64). |
| `<float>.min`, `<float>.max` | `F` | IEEE-754 minimum and maximum. |
| `<float>.cmp.eq`, `ne`, `lt`, `le`, `gt`, `ge`, `ord`, `uno` | `i1` | IEEE-754 ordered/unordered comparisons (`uno` tests `is_nan()`). |

### 6.7 Memory Loads, Stores, Bulk Memory, Prefetch & Atomics
| Opcode Syntax | Output | Semantics & Hardware Mapping |
| :--- | :--- | :--- |
| `<type>.load %ptr [, offset=N, align=M, volatile]` | `T` | Load `T` from `[%ptr + N]`. |
| `<int>.load.ext_u8 / ext_s8 / ext_u16 / ext_s16 / ext_u32 / ext_s32 %ptr [, offset=N]` | `T` | Extending load (fuses directly into `MOVZX`/`MOVSX` on x86, `LDRB`/`LDRSB`/`LDRSW` on AArch64, `LGB`/`LGF` on SystemZ). |
| `<type>.store %ptr, %val [, offset=N, align=M, volatile]` | `void` | Store `%val` to `[%ptr + N]`. |
| `mem.copy %dst, %src, %len [, align=M]` | `void` | Copy `%len` bytes (inlines to `LDP/STP`, `XMM` moves, or `MVC` for small constant lengths). |
| `mem.fill %dst, %byte_i8, %len [, align=M]` | `void` | Fill `%len` bytes with `%byte_i8` (inlines to `STP xzr, xzr` or `XMM` stores when zeroing). |
| `mem.prefetch.read %ptr, locality=0..3` / `.write` | `void` | Non-faulting cache prefetch hint (`PREFETCHT0..NTA` on x86, `PRFM` on AArch64, `PFD` on SystemZ). |
| `<int>.atomic.load.<ord> %ptr` | `T` | Atomic load (`<ord>` $\in$ `{relaxed, acquire, seqcst}`). |
| `<int>.atomic.store.<ord> %ptr, %val` | `void` | Atomic store (`<ord>` $\in$ `{relaxed, release, seqcst}`). |
| `<int>.atomic.rmw.<op>.<ord> %ptr, %val` | `T` | Atomic read-modify-write returning old value (`<op>` $\in$ `{add, sub, and, or, xor, nand, xchg, max_s, min_s, max_u, min_u}`). |
| `<int>.atomic.cas.<succ_ord>.<fail_ord> %ptr, %exp, %new` | `(T, i1)` | Atomic compare-and-swap returning `(loaded_old_val, did_succeed_i1)`. |
| `mem.fence.<ord>` / `cpu.spin_hint` | `void` | Hardware memory barrier (`MFENCE`/`DMB`) or spin-loop hint (`PAUSE`/`YIELD`). |

### 6.8 Control Flow, Branch Hints, Calls & Unwinding (Block Terminators)
Every basic block ends with **exactly one** terminator instruction:
| Terminator Syntax | Semantics |
| :--- | :--- |
| `br <block>(args...)` | Unconditional jump to `<block>`, passing SSA arguments to its block parameters. |
| `br_if[.likely\|.unlikely] %cond_i1, <then_bb>(args...), <else_bb>(args...)` | Conditional branch with optional branch-prediction/layout hint (`.likely` or `.unlikely`). |
| `br_table %idx_i32, default=<def_bb>(args...), [0: <bb0>, 1: <bb1>, ...]` | Jump table (`match` / `switch` dispatch). |
| `call.unwind @fn(args...) -> <ok_bb>(%ret), <unwind_bb>(%exc_ptr)` | Call that may unwind (branches to `<ok_bb>` on return or `<unwind_bb>(%exc_ptr: ptr)` on unwind). |
| `resume.unwind %exc_ptr` | Continue unwinding stack propagation after running drop/cleanup code in `<unwind_bb>`. |
| `ret %v0, %v1, ...` / `ret` | Return zero, one, or multiple scalar values from the current function. |
| `tailcall @fn(args...)` / `tailcall.indirect %fn_ptr(args...)` | Guaranteed tail call (reuses caller stack frame and jumps via `JMP`/`B`). |
| `trap <reason>` | Intentional hardware trap (`UD2` on x86, `BRK #1` on AArch64 — used for `unreachable` / panic abort). |

*(Standard non-unwinding calls `%res = call @fn(args...) -> T` and `%res = call.indirect %fn_ptr(args...) -> T` appear inside basic blocks as normal value-producing instructions).*

### 6.9 Portable 128-Bit SIMD (`v128`)
`v128` represents a 128-bit vector register. Operations specify the lane interpretation (`i8x16`, `i16x8`, `i32x4`, `i64x2`, `f32x4`, `f64x2`):
- **Load / Store / Splat / Lanes:** `v128.load`, `v128.store`, `<lanes>.splat %scalar`, `<lanes>.extract_lane %vec, idx`, `<lanes>.replace_lane %vec, idx, %val`, `i8x16.shuffle %a, %b, [m0..m15]`, `i8x16.swizzle %a, %indices`.
- **Lane Arithmetic & Conversions:** `<lanes>.add`, `sub`, `mul`, `min_s`, `min_u`, `max_s`, `max_u`, `add.sat_s`, `add.sat_u`, `f32x4.fma`, `f64x2.fma`, `i16x8.narrow_i32x4.sat_s`, `i32x4.ext_low_i16x8.s`.
- **Bitwise, Comparisons & Horizontal Reductions:** `v128.and`, `v128.or`, `v128.xor`, `v128.and_not`, `v128.bitselect`, `<lanes>.cmp.eq/lt/gt`, `i8x16.bitmask`, `<lanes>.reduce_add`, `<lanes>.reduce_min`, `<lanes>.reduce_max`.

---

## 7. How `.obs` Bedrock Lowers to `obsidian-isa-<arch>` (3-Tier Rule)

Because `.obs` is closed at Bedrock, **every core `.obs` opcode is guaranteed to compile on all 25 targets**:

1. **Tier 1 — Native 1:1 or Multi-Node Hardware Fusion (Fast Path):**
   - When the target's `obsidian-isa-<arch>` supports the operation (or a multi-node pattern like `shl + ptr.add + load`), the per-CPU lowering module packs it into native instructions:
     - `i64.shl %i, 3` + `ptr.add %base, %scaled` + `i64.load %p, offset=16` $\rightarrow$ `mov rax, [rdi + rsi*8 + 16]` (`x86::Opcode::Mov64rm`).
     - Two adjacent `i64.load`s $\rightarrow$ `ldp x8, x9, [x0, #16]` (`aarch64::Opcode::Ldp64`).
     - `i64.add.ovf_u` + `br_if` $\rightarrow$ `adds` + `b.hs` on AArch64 or `add` + `jc` on x86-64.
2. **Tier 2 — Inline Multi-Instruction Synthesis (Hardware Fallback Path):**
   - When a CPU lacks a single instruction for a Bedrock opcode (checked via `Target::features`), the CPU's lowering module expands it inline into legal `obsidian-isa-<arch>` instructions:
     - `i64.popcnt` on M68k or baseline RISC-V (`rv64i` without `Zbb`) $\rightarrow$ expands inline to the 12-instruction SWAR bit-population sequence.
     - `i64.ctz` on AArch64 $\rightarrow$ expands inline to `RBIT` + `CLZ` (2 instructions).
     - `i64.add` on 32-bit targets (`ARM32`, `M68k`, `RISC-V32`) $\rightarrow$ split into 32-bit register pairs (`lo`, `hi`) using `ADD` + `ADC`/`ADDX`.
3. **Tier 3 — Explicit Target Intrinsics (`arch.<cpu>.*`):**
   - Only when a program explicitly calls a vendor-specific hardware intrinsic (`arch.x86.aesenc`, `arch.aarch64.sve_ld1`) on an incompatible target does the compiler emit a compile-time error.

---
---

# Part II: The `obsidian-ir` Rust Crate (`obsidian_ir`)

## 8. Naming Convention: `obsidian_ir` (`ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`)

Following modern compiler conventions (`ir::Module`, `ir::Function`, `ir::Block`, `ir::Value`, `ir::Type`, `ir::Opcode`), `obsidian-ir` exports its core types both at the crate root and under `obsidian_ir::ir` (and can be re-exported by `obsidian-codegen` as `obsidian_codegen::ir`):

| `.obs` Blueprint Concept | `obsidian_ir` (`ir::*`) Type | How the Caller Uses It |
| :--- | :--- | :--- |
| `module @name` | `ir::Module` | `let mut m = ir::Module::new("buffer_mod", ir::Target::aarch64_macos());` |
| `target arch=..., os=..., abi=...` | `ir::Target` (`Arch`, `Os`, `Abi`) | Configures pointer width (`16/32/64`), endianness, ABI, and CPU feature flags. |
| `inline(always)`, `cold`, `section=...` | `ir::FnAttrs` / `ir::SymAttrs` | `f.set_inline(ir::InlineAttr::Always);` / `f.set_cold(true);` |
| `@loc(file, line, col)` | `ir::SourceLoc` | `f.set_source_loc(ir::SourceLoc::new(1, 42, 13));` |
| `export fn @name(...)` | `ir::Function` | `let mut f = ir::Function::export("name", &[ir::Type::Ptr], &[ir::Type::I64]);` |
| Function parameters `(%a: ptr, %b: i64)` | `f.params::<N>()` | `let [v_self, v_off] = f.params();` — const-generic array destructuring in 1 line. |
| Basic block `loop(%i: i64, %sum: i64):` | `ir::Block` | `let loop_bb = f.new_block("loop", &[ir::Type::I64, ir::Type::I64]);` |
| Switching to a block & binding its params | `f.switch_to::<N>(bb)` | `let [v_i, v_sum] = f.switch_to(loop_bb);` — switches block and returns its parameters. |
| SSA value `%v1` | `ir::Value` (`Copy` `u32`) | Returned by every instruction method (`let v1 = f.i64_add(a, b);`). |
| Module symbol `@COUNTER` / `@write` | `ir::Sym` (`Copy` `u32`) | Returned by `m.add_global(...)`, `m.add_const(...)`, `m.add_extern_fn(...)`. |
| Bedrock types (`i1`, `i8..i128`, `ptr`, ...) | `ir::Type` | `ir::Type::I64`, `ir::Type::Ptr`, `ir::Type::V128`. |
| Bedrock opcodes (`Add`, `Load`, `ICmp`, ...) | `ir::Opcode` | `ir::Opcode::Add`, `ir::Opcode::Load`, `ir::Opcode::ICmp`. |

---

## 9. End-to-End Rust Usage Examples (`obsidian_ir`)

### Example 1: Single-Block `ir::Function` (4 Lines of Rust)
**`.obs` Blueprint:**
```text
export fn @Slice_len(%self: ptr) -> i64 {
entry:
    %n = i64.load %self, offset=8
    ret %n
}
```
**Rust (`obsidian_ir`):**
```rust
use obsidian_ir as ir;

// A new `ir::Function` starts with its `entry` block already created and selected:
let mut f = ir::Function::export("Slice_len", &[ir::Type::Ptr], &[ir::Type::I64]);
let [v_self] = f.params();

let v_n = f.i64_load(v_self, 8);
f.ret(&[v_n]);
```

---

### Example 2: Loop with Block Parameters (`loop(%i: i64, %sum: i64):`)
**`.obs` Blueprint:**
```text
export fn @sum_to_n(%n: i64) -> i64 {
entry:
    br loop(0, 0)

loop(%i: i64, %sum: i64):
    %next_sum = i64.add %sum, %i
    %next_i   = i64.add %i, 1
    %cont     = i64.cmp.ult %next_i, %n
    br_if %cont, loop(%next_i, %next_sum), exit(%next_sum)

exit(%final_sum: i64):
    ret %final_sum
}
```
**Rust (`obsidian_ir`):**
```rust
use obsidian_ir as ir;

let mut f = ir::Function::export("sum_to_n", &[ir::Type::I64], &[ir::Type::I64]);
let [v_n] = f.params();

let loop_bb = f.new_block("loop", &[ir::Type::I64, ir::Type::I64]);
let exit_bb = f.new_block("exit", &[ir::Type::I64]);

// entry:
let zero = f.i64_const(0);
f.br(loop_bb, &[zero, zero]);

// loop(%i: i64, %sum: i64):
let [v_i, v_sum] = f.switch_to(loop_bb);
let v_next_sum = f.i64_add(v_sum, v_i);
let v_next_i   = f.i64_add_imm(v_i, 1);
let v_cont     = f.i64_cmp(ir::IntCC::Ult, v_next_i, v_n);
f.br_if(v_cont, loop_bb, &[v_next_i, v_next_sum], exit_bb, &[v_next_sum]);

// exit(%final_sum: i64):
let [v_final] = f.switch_to(exit_bb);
f.ret(&[v_final]);
```

---

### Example 3: Full `ir::Module` with `ir::Target`, `Metadata`, Globals & Multi-Value Returns
**`.obs` Output:**
```text
module @buffer_mod
target arch=aarch64, os=macos, abi=darwin, cpu=apple-m4, features=[+lse, +neon]

const @HEX_CHARS: [16 x i8] = "0123456789abcdef", align=1
global @REQUEST_COUNT: i64 = 0, align=8
tls @LAST_ERROR: i32 = 0, align=4
extern cc(c) fn @write(%fd: i32, %buf: ptr, %count: i64) -> i64

export inline(always) fn @Buffer_slice_from(%self: ptr, %offset: i64) -> (ptr, i64) {
entry:
    %len = i64.load %self, offset=8
    %oob = i64.cmp.ugt %offset, %len
    br_if.unlikely %oob, ret_none, in_bounds

in_bounds:
    %base    = ptr.load %self, offset=0
    %sub_ptr = ptr.add %base, %offset
    %rem_len = i64.sub %len, %offset
    ret %sub_ptr, %rem_len

ret_none:
    %null = ptr.const 0
    ret %null, 0
}
```
**Rust (`obsidian_ir`):**
```rust
use obsidian_ir as ir;

let mut module = ir::Module::new("buffer_mod", ir::Target::aarch64_macos());

// 1. Module-level constants, globals, TLS, and external C imports:
let _hex_sym = module.add_const("HEX_CHARS", b"0123456789abcdef", 1);
let _cnt_sym = module.add_global_i64("REQUEST_COUNT", 0, 8);
let _tls_sym = module.add_tls_i32("LAST_ERROR", 0, 4);
let _write   = module.add_extern_fn(
    "write",
    ir::CallConv::C,
    &[ir::Type::I32, ir::Type::Ptr, ir::Type::I64],
    &[ir::Type::I64],
    false,
);

// 2. Function with `inline(always)` metadata and `.unlikely` branch hint:
let mut f = ir::Function::export(
    "Buffer_slice_from",
    &[ir::Type::Ptr, ir::Type::I64], // (%self: ptr, %offset: i64)
    &[ir::Type::Ptr, ir::Type::I64], // -> (ptr, i64)
);
f.set_inline(ir::InlineAttr::Always);

let [v_self, v_offset] = f.params();
let in_bounds = f.new_block("in_bounds", &[]);
let ret_none  = f.new_block("ret_none", &[]);

// entry:
let v_len = f.i64_load(v_self, 8);
let v_oob = f.i64_cmp(ir::IntCC::Ugt, v_offset, v_len);
f.br_if_unlikely(v_oob, ret_none, &[], in_bounds, &[]);

// in_bounds:
f.switch_to_block(in_bounds);
let v_base    = f.ptr_load(v_self, 0);
let v_sub_ptr = f.ptr_add(v_base, v_offset);
let v_rem_len = f.i64_sub(v_len, v_offset);
f.ret(&[v_sub_ptr, v_rem_len]);

// ret_none:
f.switch_to_block(ret_none);
let v_null = f.ptr_const(0);
let v_zero = f.i64_const(0);
f.ret(&[v_null, v_zero]);

module.add_function(f);

// Formats the module into the exact human-readable `.obs` blueprint!
println!("{module}");
```

---

## 10. Workspace Layout & Internal Rust Data Structures (`obsidian-ir`)

```text
obsidian/crates/
├── isa/                           # Layer 0: Immutable Hardware Specifications (1:1 Silicon Truth)
│   ├── obsidian-isa-x86/
│   ├── obsidian-isa-systemz/
│   ├── obsidian-isa-m68k/
│   ├── obsidian-isa-aarch64/
│   └── ... (all 25 targets)
│
├── obsidian-ir/                   # Layer 1: Universal .obs Bedrock SSA IR
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs                 # Public exports (`Module`, `Target`, `Function`, `Block`, `Value`, `Sym`, `Type`, `Opcode`)
│       ├── target.rs              # `Target`, `Arch` (all 25 targets), `Os`, `Abi`, `Endian`, `ObjFormat`
│       ├── meta.rs                # `FnAttrs`, `SymAttrs`, `InlineAttr`, `BranchHint`, `SourceLoc`
│       ├── types.rs               # `Type` (`I1`, `I8..I128`, `F32`, `F64`, `Ptr`, `V128`), `Linkage`, `CallConv`
│       ├── opcode.rs              # Bedrock `Opcode` enum, `IntCC`, `FloatCC`, `MemOrder`, `AtomicRmwOp`
│       ├── node.rs                # 16-byte `InstNode`, `Value`, `Block`, `Sym`, `BlockData`
│       ├── func.rs                # `Function` arena, `params()`, `switch_to()`, constant/identity folding
│       ├── module.rs              # `Module` (`ConstDef`, `GlobalDef`, `TlsDef`, `ExternFn`, `Function` list)
│       └── fmt.rs                 # Human-readable `.obs` `Display` formatter
│
└── codegen/                       # Layer 2: Per-CPU Lowering, Fusion, RegAlloc & Byte Emission
    ├── obsidian-codegen/          # Top-level target dispatcher (`compile_module(&ir::Module) -> Artifact`)
    ├── obsidian-codegen-x86/
    ├── obsidian-codegen-aarch64/
    ├── obsidian-codegen-systemz/
    ├── obsidian-codegen-m68k/
    └── ... (all 25 targets, feeding into `crates/obj/*`: `obsidian-obj`, `obsidian-elf`, `obsidian-macho`, `obsidian-coff`, `obsidian-ar`, `obsidian-ld`, `obsidian-jit`)
```

### 10.1 The 16-Byte `InstNode` & `ir::Value` Invariant (`node.rs`)
1. **`ir::Value(u32)` Is Always a Direct Index into `Function::insts` (`Vec<InstNode>`):**
   - Every SSA value `ir::Value(i)` is defined by `func.insts[i as usize]`. Looking up the opcode, type, or operands of any value is an $O(1)$ array index with zero pointer chasing.
2. **Multi-Value Outputs via `Opcode::ResultProj`:**
   - When a two-output Bedrock instruction (`%sum, %ovf = i64.add.ovf_u %a, %b`, `%lo, %hi = i64.mul.wide_u`, `%q, %r = i64.divrem.u`, `%old, %ok = i64.atomic.cas`) is emitted:
     - Slot `i` (`Value(i)`) holds the primary instruction (`ty = Type::I64`).
     - Slot `i + 1` (`Value(i + 1)`) holds `Opcode::ResultProj` (`ty = Type::I1`, `args = [i, 1]`).
     - It returns `(Value(i), Value(i + 1))`.

```rust
/// A 32-bit SSA value handle.
/// Invariant: `Value(i)` is always defined by `Function::insts[i as usize]`.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Value(pub u32);

impl Value {
    pub const NONE: Self = Self(u32::MAX);

    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A 32-bit basic block handle (`Function::blocks[id as usize]`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Block(pub u32);

/// A 32-bit module symbol handle (`Module::symbols[id as usize]`).
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Sym(pub u32);

/// Exactly 16 bytes (`#[repr(C, align(16))]` — 4 instructions per 64-byte L1 cache line).
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct InstNode {
    /// 1-byte Bedrock opcode (`Add`, `Load`, `ICmp`, `BrIf`, etc.).
    pub op: Opcode,
    /// 1-byte output `Type` (`I64`, `Ptr`, `I1`, `Void`, etc.).
    pub ty: Type,
    /// 1-byte modifier (`IntCC`, `FloatCC`, `BranchHint`, `MemOrder`, `ExtKind`, or SIMD lane tag).
    pub subop: u8,
    /// 1-byte flags (bit 0 = `args[1]` is an inline `i32` immediate; bit 1 = `volatile`; bits 4..7 = `align_log2`).
    pub flags: u8,
    /// 2-byte saturating reference count of downstream instructions reading this `Value`.
    pub use_count: u16,
    /// 2-byte auxiliary field (signed 16-bit memory byte offset `offset=-32768..=32767`).
    pub aux16: i16,
    /// Two 32-bit operand slots (`Value`, `Block`, `Sym`, inline `i32`, or side-table index).
    pub args: [u32; 2],
}

const _: () = assert!(core::mem::size_of::<InstNode>() == 16);
```

### 10.2 How Per-CPU Codegen Consumes `&ir::Function` (`obsidian-codegen-<arch>`)
Because `ir::Function` stores a flat `Vec<InstNode>` with pre-computed `use_count`s, a target backend (`obsidian-codegen-x86`, `obsidian-codegen-aarch64`, etc.) lowers `&ir::Function` into `obsidian-isa-<arch>` instructions in $O(1)$ per node:

```rust
// Inside `obsidian-codegen-aarch64`:
pub fn lower_load_i64(func: &ir::Function, load_node: &ir::InstNode) -> AArch64Inst {
    let ptr_val = ir::Value(load_node.args[0]);
    let offset  = load_node.aux16;

    // O(1) array lookup into `func.insts`!
    let ptr_def = &func.insts[ptr_val.index()];

    // Check if `ptr_val` is a single-use `ptr.add %base, %scaled` where `offset == 0`:
    if ptr_def.op == ir::Opcode::PtrAdd && ptr_def.use_count == 1 && offset == 0 {
        let base_val = ir::Value(ptr_def.args[0]);
        let idx_val  = ir::Value(ptr_def.args[1]);
        let idx_def  = &func.insts[idx_val.index()];

        // Check if `idx_val` is `i64.shl %i, 3` -> fuse into `ldr x_dst, [x_base, x_i, lsl #3]`!
        if idx_def.op == ir::Opcode::Shl && idx_def.use_count == 1 && func.const_val(idx_def.args[1]) == Some(3) {
            return AArch64Inst::Ldr64RegScaled {
                base: base_val,
                index: ir::Value(idx_def.args[0]),
            };
        }
    }

    // Standard scaled immediate load `ldr x_dst, [x_ptr, #offset]` via `obsidian-isa-aarch64`:
    AArch64Inst::Ldr64Imm { base: ptr_val, byte_offset: offset }
}
```
