//! Microsoft PE32+ and COFF (`.obj` / `.exe`) specification constants and relocation codes.

use obsidian_ir::Arch;
use obsidian_obj::RelocKind;

/// MS-DOS `"MZ"` header magic (`0x5A4D`).
pub const MZ_MAGIC: u16 = 0x5A4D;

/// 4-byte PE header signature (`"PE\0\0"`).
pub const PE_SIGNATURE: [u8; 4] = [b'P', b'E', 0, 0];

/// 64-bit PE32+ optional header magic (`0x020B`).
pub const PE32_PLUS_MAGIC: u16 = 0x020B;

// --- IMAGE_FILE_HEADER Machine Types ---
pub const IMAGE_FILE_MACHINE_I386: u16  = 0x014C;
pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
pub const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;

// --- IMAGE_FILE_HEADER Characteristics ---
pub const IMAGE_FILE_EXECUTABLE_IMAGE: u16    = 0x0002;
pub const IMAGE_FILE_LARGE_ADDRESS_AWARE: u16 = 0x0020;
pub const IMAGE_FILE_DLL: u16                 = 0x2000;

// --- Subsystem Constants ---
pub const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;

// --- IMAGE_SECTION_HEADER Characteristics ---
pub const IMAGE_SCN_CNT_CODE: u32               = 0x0000_0020;
pub const IMAGE_SCN_CNT_INITIALIZED_DATA: u32   = 0x0000_0040;
pub const IMAGE_SCN_CNT_UNINITIALIZED_DATA: u32 = 0x0000_0080;
pub const IMAGE_SCN_ALIGN_8BYTES: u32           = 0x0040_0000;
pub const IMAGE_SCN_ALIGN_16BYTES: u32          = 0x0050_0000;
pub const IMAGE_SCN_MEM_EXECUTE: u32            = 0x2000_0000;
pub const IMAGE_SCN_MEM_READ: u32               = 0x4000_0000;
pub const IMAGE_SCN_MEM_WRITE: u32              = 0x8000_0000;

// --- COFF Symbol Storage Class & Type ---
pub const IMAGE_SYM_CLASS_EXTERNAL: u8 = 2;
pub const IMAGE_SYM_CLASS_STATIC: u8   = 3;
pub const IMAGE_SYM_DTYPE_FUNCTION: u16 = 0x0020;

// --- COFF Relocation Types (x64 & ARM64) ---
pub const IMAGE_REL_AMD64_ADDR64: u16 = 0x0001;
pub const IMAGE_REL_AMD64_ADDR32: u16 = 0x0002;
pub const IMAGE_REL_AMD64_REL32: u16  = 0x0004;

pub const IMAGE_REL_ARM64_ADDR64: u16         = 0x0002;
pub const IMAGE_REL_ARM64_BRANCH26: u16       = 0x0003;
pub const IMAGE_REL_ARM64_PAGEBASE_REL21: u16 = 0x0004;
pub const IMAGE_REL_ARM64_PAGEOFFSET_12A: u16 = 0x0006;
pub const IMAGE_REL_ARM64_PAGEOFFSET_12L: u16 = 0x0007;

/// Maps an Obsidian [`Arch`] to its official COFF `Machine` constant.
#[inline]
pub const fn coff_machine(arch: Arch) -> u16 {
    match arch {
        Arch::X86_64  => IMAGE_FILE_MACHINE_AMD64,
        Arch::AArch64 => IMAGE_FILE_MACHINE_ARM64,
        Arch::X86_32  => IMAGE_FILE_MACHINE_I386,
        _             => IMAGE_FILE_MACHINE_AMD64,
    }
}

/// Maps `(arch, reloc_kind)` to the target's COFF 16-bit relocation type (`IMAGE_REL_*`).
#[inline]
pub const fn coff_reloc_type(arch: Arch, kind: RelocKind) -> u16 {
    match (arch, kind) {
        (Arch::X86_64, RelocKind::Abs64)   => IMAGE_REL_AMD64_ADDR64,
        (Arch::X86_64, RelocKind::Abs32)   => IMAGE_REL_AMD64_ADDR32,
        (Arch::X86_64, RelocKind::PcRel32) | (Arch::X86_64, RelocKind::GotPcRel32) => IMAGE_REL_AMD64_REL32,

        (Arch::AArch64, RelocKind::Abs64)                 => IMAGE_REL_ARM64_ADDR64,
        (Arch::AArch64, RelocKind::AArch64Call26)         => IMAGE_REL_ARM64_BRANCH26,
        (Arch::AArch64, RelocKind::AArch64AdrpPage21)     => IMAGE_REL_ARM64_PAGEBASE_REL21,
        (Arch::AArch64, RelocKind::AArch64PageOff12Add)   => IMAGE_REL_ARM64_PAGEOFFSET_12A,
        (Arch::AArch64, RelocKind::AArch64PageOff12Ldr64) => IMAGE_REL_ARM64_PAGEOFFSET_12L,
        _ => 0,
    }
}
