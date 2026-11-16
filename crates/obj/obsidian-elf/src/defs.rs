//! System V ELF64 and ELF32 specification constants, machine IDs (`EM_*`), and relocation codes (`R_*`).

use obsidian_ir::Arch;
use obsidian_obj::RelocKind;

/// Standard 4-byte ELF magic header (`0x7F 'E' 'L' 'F'`).
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];

pub const ELFCLASS32: u8 = 1;
pub const ELFCLASS64: u8 = 2;

pub const ELFDATA2LSB: u8 = 1;
pub const ELFDATA2MSB: u8 = 2;

pub const EV_CURRENT: u8 = 1;
pub const ELFOSABI_NONE: u8 = 0;

// --- e_type (Object File Type) ---
pub const ET_REL: u16  = 1;
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16  = 3;

// --- Section Header Types (sh_type) ---
pub const SHT_NULL: u32     = 0;
pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32   = 2;
pub const SHT_STRTAB: u32   = 3;
pub const SHT_RELA: u32     = 4;
pub const SHT_NOBITS: u32   = 8;

// --- Section Header Flags (sh_flags) ---
pub const SHF_WRITE: u64     = 0x1;
pub const SHF_ALLOC: u64     = 0x2;
pub const SHF_EXECINSTR: u64 = 0x4;
pub const SHF_INFO_LINK: u64 = 0x40;
pub const SHF_TLS: u64       = 0x400;

// --- Symbol Binding & Type (st_info) ---
pub const STB_LOCAL: u8  = 0;
pub const STB_GLOBAL: u8 = 1;

pub const STT_NOTYPE: u8  = 0;
pub const STT_OBJECT: u8  = 1;
pub const STT_FUNC: u8    = 2;
pub const STT_SECTION: u8 = 3;
pub const STT_TLS: u8     = 6;

pub const SHN_UNDEF: u16 = 0;

// --- Program Header Types & Flags (p_type, p_flags) ---
pub const PT_LOAD: u32 = 1;
pub const PT_TLS: u32  = 7;

pub const PF_X: u32 = 0x1;
pub const PF_W: u32 = 0x2;
pub const PF_R: u32 = 0x4;

/// Maps an Obsidian [`Arch`] to its official ELF `e_machine` (`EM_*`) constant.
#[inline]
pub const fn elf_machine(arch: Arch) -> u16 {
    match arch {
        Arch::X86_64      => 62,  // EM_X86_64
        Arch::X86_32      => 3,   // EM_386
        Arch::AArch64     => 183, // EM_AARCH64
        Arch::SystemZ     => 22,  // EM_S390
        Arch::M68k        => 4,   // EM_68K
        Arch::Arm | Arch::Thumb2 => 40, // EM_ARM
        Arch::PowerPC64   => 21,  // EM_PPC64
        Arch::Sparc64     => 43,  // EM_SPARCV9
        Arch::Riscv64 | Arch::Riscv32 => 243, // EM_RISCV
        Arch::LoongArch64 => 258, // EM_LOONGARCH
        Arch::Mips64      => 8,   // EM_MIPS
        Arch::Lanai       => 244, // EM_LANAI
        Arch::Arc         => 195, // EM_ARC_COMPACT2
        Arch::CSky        => 252, // EM_CSKY
        Arch::AmdGpu      => 224, // EM_AMDGPU
        Arch::Ve          => 251, // EM_VE
        Arch::Bpf         => 247, // EM_BPF
        Arch::Hexagon     => 164, // EM_HEXAGON
        Arch::Avr         => 83,  // EM_AVR
        Arch::Msp430      => 105, // EM_MSP430
        Arch::XCore       => 203, // EM_XCORE
        Arch::Xtensa      => 94,  // EM_XTENSA
        _                 => 0,
    }
}

/// Maps `(arch, reloc_kind)` to the target's official ELF relocation type code (`R_<ARCH>_*`).
#[inline]
pub const fn elf_reloc_type(arch: Arch, kind: RelocKind) -> u32 {
    match (arch, kind) {
        // --- x86-64 (EM_X86_64) ---
        (Arch::X86_64, RelocKind::Abs64)      => 1,  // R_X86_64_64
        (Arch::X86_64, RelocKind::PcRel32)    => 2,  // R_X86_64_PC32
        (Arch::X86_64, RelocKind::GotPcRel32) => 9,  // R_X86_64_GOTPCREL
        (Arch::X86_64, RelocKind::Abs32)      => 10, // R_X86_64_32

        // --- AArch64 (EM_AARCH64) ---
        (Arch::AArch64, RelocKind::Abs64)                 => 257, // R_AARCH64_ABS64
        (Arch::AArch64, RelocKind::Abs32)                 => 258, // R_AARCH64_ABS32
        (Arch::AArch64, RelocKind::PcRel32)               => 261, // R_AARCH64_PREL32
        (Arch::AArch64, RelocKind::AArch64AdrpPage21)     => 275, // R_AARCH64_ADR_PREL_PG_HI21
        (Arch::AArch64, RelocKind::AArch64PageOff12Add)   => 277, // R_AARCH64_ADD_ABS_LO12_NC
        (Arch::AArch64, RelocKind::AArch64Call26)         => 283, // R_AARCH64_CALL26
        (Arch::AArch64, RelocKind::AArch64PageOff12Ldr64) => 286, // R_AARCH64_LDST64_ABS_LO12_NC

        // --- SystemZ s390x (EM_S390) ---
        (Arch::SystemZ, RelocKind::Abs32)          => 4,  // R_390_32
        (Arch::SystemZ, RelocKind::PcRel32)        => 5,  // R_390_PC32
        (Arch::SystemZ, RelocKind::S390PcRel32Dbl) => 19, // R_390_PC32DBL
        (Arch::SystemZ, RelocKind::Abs64)          => 22, // R_390_64

        // --- Motorola 680x0 (EM_68K) ---
        (Arch::M68k, RelocKind::Abs32)   => 1, // R_68K_32
        (Arch::M68k, RelocKind::PcRel32) => 4, // R_68K_PC32

        _ => 0,
    }
}
