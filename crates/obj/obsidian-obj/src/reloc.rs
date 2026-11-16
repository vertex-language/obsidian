//! Section identifiers (`SectionId`), relocation records (`Reloc`, `RelocKind`),
//! and in-place hardware instruction/data patching.

use obsidian_ir::Sym;

/// Standard binary section kinds inside an [`Artifact`](crate::Artifact).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SectionId {
    /// Executable machine instructions (`.text` / `__TEXT,__text`).
    Text   = 0,
    /// Read-only constant data (`.rodata` / `__TEXT,__const` / `.rdata`).
    Rodata = 1,
    /// Initialized mutable global data (`.data` / `__DATA,__data`).
    Data   = 2,
    /// Zero-initialized mutable global data (`.bss` / `__DATA,__bss`).
    Bss    = 3,
    /// Thread-local initialized data (`.tdata` / `__DATA,__thread_data`).
    Tdata  = 4,
    /// Thread-local zero-initialized data (`.tbss` / `__DATA,__thread_bss`).
    Tbss   = 5,
}

impl SectionId {
    /// Standard ELF section name (`.text`, `.rodata`, `.data`, `.bss`, `.tdata`, `.tbss`).
    pub const fn elf_name(self) -> &'static str {
        match self {
            Self::Text   => ".text",
            Self::Rodata => ".rodata",
            Self::Data   => ".data",
            Self::Bss    => ".bss",
            Self::Tdata  => ".tdata",
            Self::Tbss   => ".tbss",
        }
    }

    /// Standard Mach-O `(segment, section)` names (`("__TEXT", "__text")`, etc.).
    pub const fn macho_segment_section(self) -> (&'static str, &'static str) {
        match self {
            Self::Text   => ("__TEXT", "__text"),
            Self::Rodata => ("__TEXT", "__const"),
            Self::Data   => ("__DATA", "__data"),
            Self::Bss    => ("__DATA", "__bss"),
            Self::Tdata  => ("__DATA", "__thread_data"),
            Self::Tbss   => ("__DATA", "__thread_bss"),
        }
    }

    /// Standard Windows PE/COFF 8-byte section name (`.text`, `.rdata`, `.data`, `.bss`, `.tls$`).
    pub const fn coff_name(self) -> &'static str {
        match self {
            Self::Text   => ".text",
            Self::Rodata => ".rdata",
            Self::Data   => ".data",
            Self::Bss    => ".bss",
            Self::Tdata | Self::Tbss => ".tls$",
        }
    }

    #[inline]
    pub const fn is_executable(self) -> bool {
        matches!(self, Self::Text)
    }

    #[inline]
    pub const fn is_writable(self) -> bool {
        matches!(self, Self::Data | Self::Bss | Self::Tdata | Self::Tbss)
    }

    #[inline]
    pub const fn is_tls(self) -> bool {
        matches!(self, Self::Tdata | Self::Tbss)
    }
}

/// Architecture-aware relocation kind describing how a symbol reference is patched into bytes.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RelocKind {
    /// 64-bit absolute address (`S + A`):
    /// - ELF x86-64: `R_X86_64_64` (1)
    /// - ELF AArch64: `R_AARCH64_ABS64` (257)
    /// - ELF SystemZ: `R_390_64` (22)
    /// - COFF x64/ARM64: `IMAGE_REL_AMD64_ADDR64` (1)
    Abs64 = 0,

    /// 32-bit absolute address (`S + A`):
    /// - ELF x86-64: `R_X86_64_32` (10)
    /// - ELF M68k: `R_68K_32` (1)
    Abs32 = 1,

    /// 32-bit signed PC-relative byte offset (`S + A - P`):
    /// - ELF x86-64: `R_X86_64_PC32` (2) / `R_X86_64_PLT32` (4)
    /// - COFF x64: `IMAGE_REL_AMD64_REL32` (4)
    /// - ELF M68k: `R_68K_PC32` (4)
    PcRel32 = 2,

    /// 32-bit signed PC-relative offset to Global Offset Table slot (`G + GOT + A - P`):
    /// - ELF x86-64: `R_X86_64_GOTPCREL` (9)
    GotPcRel32 = 3,

    /// SystemZ 32-bit signed PC-relative halfword offset (`(S + A - P) >> 1`):
    /// - ELF s390x: `R_390_PC32DBL` (19) / `R_390_PLT32DBL` (20)
    ///   (Used by 6-byte `RIL` instructions `LARL` and `BRASL` at `offset + 2`).
    S390PcRel32Dbl = 4,

    /// AArch64 26-bit signed instruction-word offset (`(S + A - P) >> 2` in bits `0..26`):
    /// - ELF AArch64: `R_AARCH64_CALL26` (283) / `R_AARCH64_JUMP26` (282)
    /// - Mach-O ARM64: `ARM64_RELOC_BRANCH26` (2)
    AArch64Call26 = 5,

    /// AArch64 21-bit 4KiB page-relative offset for `ADRP` (`(Page(S + A) - Page(P)) >> 12`):
    /// - ELF AArch64: `R_AARCH64_ADR_PREL_PG_HI21` (275)
    /// - Mach-O ARM64: `ARM64_RELOC_PAGE21` (3)
    AArch64AdrpPage21 = 6,

    /// AArch64 12-bit unscaled page offset for `ADD Xd, Xn, #lo12` (`(S + A) & 0xFFF` in bits `10..22`):
    /// - ELF AArch64: `R_AARCH64_ADD_ABS_LO12_NC` (277)
    /// - Mach-O ARM64: `ARM64_RELOC_PAGEOFF12` (4)
    AArch64PageOff12Add = 7,

    /// AArch64 12-bit 8-byte-scaled page offset for 64-bit `LDR`/`STR` (`((S + A) & 0xFFF) >> 3` in bits `10..22`):
    /// - ELF AArch64: `R_AARCH64_LDST64_ABS_LO12_NC` (286)
    /// - Mach-O ARM64: `ARM64_RELOC_PAGEOFF12` (4)
    AArch64PageOff12Ldr64 = 8,
}

impl RelocKind {
    /// Applies this relocation directly into `section_bytes[offset..]` given:
    /// - `patch_vaddr` (`P`): virtual address of `section_bytes[offset]`
    /// - `target_vaddr` (`S`): resolved virtual address of the target symbol
    /// - `addend` (`A`): signed byte addend
    /// - `is_big_endian`: `true` for Big-Endian targets (`SystemZ`, `M68k`, `Sparc64`), `false` for Little-Endian
    ///
    /// Returns `true` if the target displacement fits within the hardware bitfield, or `false` on overflow.
    pub fn apply_in_place(
        self,
        section_bytes: &mut [u8],
        offset: usize,
        patch_vaddr: u64,
        target_vaddr: u64,
        addend: i64,
        is_big_endian: bool,
    ) -> bool {
        let s_plus_a = (target_vaddr as i128) + (addend as i128);
        let p = patch_vaddr as i128;

        match self {
            Self::Abs64 => {
                if offset + 8 > section_bytes.len() {
                    return false;
                }
                let val = s_plus_a as u64;
                let bytes = if is_big_endian {
                    val.to_be_bytes()
                } else {
                    val.to_le_bytes()
                };
                section_bytes[offset..offset + 8].copy_from_slice(&bytes);
                true
            }
            Self::Abs32 => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let val = s_plus_a as u32;
                let bytes = if is_big_endian {
                    val.to_be_bytes()
                } else {
                    val.to_le_bytes()
                };
                section_bytes[offset..offset + 4].copy_from_slice(&bytes);
                true
            }
            Self::PcRel32 | Self::GotPcRel32 => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let delta = s_plus_a - p;
                if delta < (i32::MIN as i128) || delta > (i32::MAX as i128) {
                    return false;
                }
                let val = delta as i32;
                let bytes = if is_big_endian {
                    val.to_be_bytes()
                } else {
                    val.to_le_bytes()
                };
                section_bytes[offset..offset + 4].copy_from_slice(&bytes);
                true
            }
            Self::S390PcRel32Dbl => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let delta_halfwords = (s_plus_a - p) >> 1;
                if delta_halfwords < (i32::MIN as i128) || delta_halfwords > (i32::MAX as i128) {
                    return false;
                }
                let bytes = (delta_halfwords as i32).to_be_bytes();
                section_bytes[offset..offset + 4].copy_from_slice(&bytes);
                true
            }
            Self::AArch64Call26 => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let delta_words = (s_plus_a - p) >> 2;
                if delta_words < -(1 << 25) || delta_words >= (1 << 25) {
                    return false;
                }
                let mut word = u32::from_le_bytes([
                    section_bytes[offset],
                    section_bytes[offset + 1],
                    section_bytes[offset + 2],
                    section_bytes[offset + 3],
                ]);
                word = (word & 0xFC00_0000) | ((delta_words as u32) & 0x03FF_FFFF);
                section_bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                true
            }
            Self::AArch64AdrpPage21 => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let target_page = (s_plus_a & !0xFFF) >> 12;
                let patch_page = (p & !0xFFF) >> 12;
                let page_delta = target_page - patch_page;
                if page_delta < -(1 << 20) || page_delta >= (1 << 20) {
                    return false;
                }
                let u = (page_delta as u32) & 0x1F_FFFF;
                let immlo = u & 0x3;
                let immhi = (u >> 2) & 0x7_FFFF;
                let mut word = u32::from_le_bytes([
                    section_bytes[offset],
                    section_bytes[offset + 1],
                    section_bytes[offset + 2],
                    section_bytes[offset + 3],
                ]);
                word = (word & 0x9F00_001F) | (immlo << 29) | (immhi << 5);
                section_bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                true
            }
            Self::AArch64PageOff12Add => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let lo12 = (s_plus_a as u32) & 0xFFF;
                let mut word = u32::from_le_bytes([
                    section_bytes[offset],
                    section_bytes[offset + 1],
                    section_bytes[offset + 2],
                    section_bytes[offset + 3],
                ]);
                word = (word & !(0xFFF << 10)) | (lo12 << 10);
                section_bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                true
            }
            Self::AArch64PageOff12Ldr64 => {
                if offset + 4 > section_bytes.len() {
                    return false;
                }
                let lo12_scaled = ((s_plus_a as u32) & 0xFFF) >> 3;
                let mut word = u32::from_le_bytes([
                    section_bytes[offset],
                    section_bytes[offset + 1],
                    section_bytes[offset + 2],
                    section_bytes[offset + 3],
                ]);
                word = (word & !(0xFFF << 10)) | (lo12_scaled << 10);
                section_bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                true
            }
        }
    }
}

/// A relocation record marking a patch site inside a compiled section.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Reloc {
    /// Section containing the bytes to patch (`SectionId::Text` or `SectionId::Data`).
    pub section: SectionId,
    /// Byte offset inside `section` where the patch is applied.
    pub offset: u32,
    /// Target symbol in `Artifact::symbols`.
    pub sym: Sym,
    /// Hardware relocation kind (`PcRel32`, `AArch64Call26`, `Abs64`, etc.).
    pub kind: RelocKind,
    /// Signed byte addend (`-4` for x86 `CALL`/`JMP` PC-relative 32-bit displacements, `0` for AArch64 `BL`).
    pub addend: i64,
}
