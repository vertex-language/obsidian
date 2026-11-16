//! Apple Mach-O 64 and Universal Binary (`FAT_MAGIC`) constants, load commands, and relocation codes.

use obsidian_ir::Arch;
use obsidian_obj::RelocKind;

/// 64-bit Little-Endian Mach-O magic number (`0xFEEDFACF`).
pub const MH_MAGIC_64: u32 = 0xFEED_FACF;

/// Universal Fat Binary magic number (`0xCAFEBABE`, always stored Big-Endian!).
pub const FAT_MAGIC: u32 = 0xCAFE_BABE;

// --- CPU Types & Subtypes ---
pub const CPU_ARCH_ABI64: u32        = 0x0100_0000;
pub const CPU_TYPE_X86_64: u32       = 7 | CPU_ARCH_ABI64;  // 0x0100_0007
pub const CPU_TYPE_ARM64: u32        = 12 | CPU_ARCH_ABI64; // 0x0100_000C
pub const CPU_SUBTYPE_X86_64_ALL: u32 = 3;
pub const CPU_SUBTYPE_ARM64_ALL: u32  = 0;

// --- Mach-O File Types (filetype) ---
pub const MH_OBJECT: u32  = 0x1;
pub const MH_EXECUTE: u32 = 0x2;
pub const MH_DYLIB: u32   = 0x6;

// --- Mach-O Header Flags ---
pub const MH_NOUNDEFS: u32                = 0x0000_0001;
pub const MH_SUBSECTIONS_VIA_SYMBOLS: u32 = 0x0000_2000;
pub const MH_PIE: u32                     = 0x0020_0000;

// --- Load Commands (cmd) ---
pub const LC_SYMTAB: u32        = 0x0000_0002;
pub const LC_DYSYMTAB: u32      = 0x0000_000B;
pub const LC_SEGMENT_64: u32    = 0x0000_0019;
pub const LC_BUILD_VERSION: u32 = 0x0000_0032;
pub const LC_MAIN: u32          = 0x8000_0028;

// --- Build Version Platforms ---
pub const PLATFORM_MACOS: u32 = 1;
pub const PLATFORM_IOS: u32   = 2;

// --- VM Protection Flags ---
pub const VM_PROT_READ: u32    = 0x01;
pub const VM_PROT_WRITE: u32   = 0x02;
pub const VM_PROT_EXECUTE: u32 = 0x04;

// --- Section Types & Attributes ---
pub const S_REGULAR: u32                  = 0x0;
pub const S_ZEROFILL: u32                 = 0x1;
pub const S_ATTR_SOME_INSTRUCTIONS: u32   = 0x0000_0400;
pub const S_ATTR_PURE_INSTRUCTIONS: u32   = 0x8000_0000;

// --- nlist_64 Symbol Type Flags (n_type) ---
pub const N_UNDF: u8 = 0x00;
pub const N_SECT: u8 = 0x0E;
pub const N_EXT: u8  = 0x01;

// --- Relocation Types (x86_64 & arm64) ---
pub const X86_64_RELOC_UNSIGNED: u8 = 0;
pub const X86_64_RELOC_SIGNED: u8   = 1;
pub const X86_64_RELOC_BRANCH: u8   = 2;
pub const X86_64_RELOC_GOT_LOAD: u8 = 3;

pub const ARM64_RELOC_UNSIGNED: u8  = 0;
pub const ARM64_RELOC_BRANCH26: u8  = 2;
pub const ARM64_RELOC_PAGE21: u8    = 3;
pub const ARM64_RELOC_PAGEOFF12: u8 = 4;

/// Maps an Obsidian [`Arch`] to Mach-O `(cputype, cpusubtype)`.
#[inline]
pub const fn macho_cpu_type(arch: Arch) -> (u32, u32) {
    match arch {
        Arch::X86_64  => (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL),
        Arch::AArch64 => (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
        _             => (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
    }
}

/// Maps `(arch, reloc_kind)` to Mach-O `(r_type, r_pcrel, r_length_log2)`.
#[inline]
pub const fn macho_reloc_info(arch: Arch, kind: RelocKind) -> (u8, bool, u8) {
    match (arch, kind) {
        (Arch::X86_64, RelocKind::Abs64)      => (X86_64_RELOC_UNSIGNED, false, 3), // 8 bytes (2^3)
        (Arch::X86_64, RelocKind::PcRel32)    => (X86_64_RELOC_BRANCH, true, 2),    // 4 bytes (2^2)
        (Arch::X86_64, RelocKind::GotPcRel32) => (X86_64_RELOC_GOT_LOAD, true, 2),

        (Arch::AArch64, RelocKind::Abs64)                 => (ARM64_RELOC_UNSIGNED, false, 3),
        (Arch::AArch64, RelocKind::AArch64Call26)         => (ARM64_RELOC_BRANCH26, true, 2),
        (Arch::AArch64, RelocKind::AArch64AdrpPage21)     => (ARM64_RELOC_PAGE21, true, 2),
        (Arch::AArch64, RelocKind::AArch64PageOff12Add)   => (ARM64_RELOC_PAGEOFF12, false, 2),
        (Arch::AArch64, RelocKind::AArch64PageOff12Ldr64) => (ARM64_RELOC_PAGEOFF12, false, 2),
        _ => (0, false, 2),
    }
}
