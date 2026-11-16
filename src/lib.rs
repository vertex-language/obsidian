#![no_std]
#![doc = include_str!("../README.md")]

//! Top-level `obsidian` compiler crate re-exporting `.obs` Bedrock SSA IR (`ir`),
//! calling conventions (`abi`), register allocation (`regalloc`), per-CPU code generators
//! (`codegen_x86`, `codegen_aarch64`), binary object writers (`obj`, `elf`, `macho`, `coff`, `ld`),
//! and 1:1 hardware ISA specifications (`isa`).

pub use obsidian_abi as abi;
pub use obsidian_codegen_aarch64 as codegen_aarch64;
pub use obsidian_codegen_x86 as codegen_x86;
pub use obsidian_coff as coff;
pub use obsidian_elf as elf;
pub use obsidian_ir as ir;
pub use obsidian_ld as ld;
pub use obsidian_macho as macho;
pub use obsidian_obj as obj;
pub use obsidian_regalloc as regalloc;

/// Hardware 1:1 Silicon ISA specifications.
pub mod isa {
    pub use obsidian_isa_aarch64 as aarch64;
    pub use obsidian_isa_m68k as m68k;
    pub use obsidian_isa_systemz as systemz;
    pub use obsidian_isa_x86 as x86;
}
