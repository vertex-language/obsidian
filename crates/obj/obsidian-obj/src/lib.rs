#![no_std]
#![doc = include_str!("../README.md")]

extern crate alloc;

pub mod artifact;
pub mod reloc;

pub use crate::artifact::{Artifact, SymbolKind, SymbolRecord};
pub use crate::reloc::{Reloc, RelocKind, SectionId};

#[cfg(test)]
mod tests {
    use crate::{Artifact, Reloc, RelocKind, SectionId};
    use obsidian_ir::{Linkage, Target};

    #[test]
    fn test_section_names_across_formats() {
        assert_eq!(SectionId::Text.elf_name(), ".text");
        assert_eq!(SectionId::Rodata.macho_segment_section(), ("__TEXT", "__const"));
        assert_eq!(SectionId::Rodata.coff_name(), ".rdata");
        assert!(SectionId::Text.is_executable());
        assert!(!SectionId::Rodata.is_writable());
        assert!(SectionId::Data.is_writable());
        assert!(SectionId::Tdata.is_tls());
    }

    #[test]
    fn test_x86_pcrel32_and_s390x_reloc_patching() {
        // 1. x86-64 `call rel32`: `E8 00 00 00 00` at vaddr 0x401000 targeting 0x401025
        //    patch_vaddr = 0x401001, addend = -4 -> S + A - P = 0x401025 - 4 - 0x401001 = 0x20
        let mut x86_code = [0xE8u8, 0, 0, 0, 0];
        assert!(RelocKind::PcRel32.apply_in_place(&mut x86_code, 1, 0x401001, 0x401025, -4, false));
        assert_eq!(&x86_code[1..5], &0x20i32.to_le_bytes());

        // 2. SystemZ Big-Endian `LARL %r2, target`: `C0 20 00 00 00 00`
        //    patch_vaddr = 0x1002, addend = 2 -> S + A - P = 0x1100 + 2 - 0x1002 = 0x100 bytes = 0x80 halfwords
        let mut s390_code = [0xC0u8, 0x20, 0, 0, 0, 0];
        assert!(RelocKind::S390PcRel32Dbl.apply_in_place(&mut s390_code, 2, 0x1002, 0x1100, 2, true));
        assert_eq!(&s390_code[2..6], &0x80i32.to_be_bytes());
    }

    #[test]
    fn test_aarch64_adrp_and_add_lo12_reloc_patching() {
        let mut art = Artifact::new("adrp_test", Target::aarch64_linux());
        let tbl_sym = art.define_rodata("TABLE", Linkage::Local, 8, &[1, 2, 3, 4, 5, 6, 7, 8]);

        // `adrp x0, #0` (0x90000000) + `add x0, x0, #0` (0x91000000)
        let mut code = [0u8; 8];
        code[0..4].copy_from_slice(&0x9000_0000u32.to_le_bytes());
        code[4..8].copy_from_slice(&0x9100_0000u32.to_le_bytes());
        let _fn_sym = art.define_function("get_table", Linkage::Export, 4, 0, &code);

        art.add_reloc(Reloc {
            section: SectionId::Text,
            offset: 0,
            sym: tbl_sym,
            kind: RelocKind::AArch64AdrpPage21,
            addend: 0,
        });
        art.add_reloc(Reloc {
            section: SectionId::Text,
            offset: 4,
            sym: tbl_sym,
            kind: RelocKind::AArch64PageOff12Add,
            addend: 0,
        });

        // `.text` at 0x1000_0000, `.rodata` at 0x1000_2040 (+2 pages, page offset 0x040):
        let vaddrs = [0x1000_0000, 0x1000_2040, 0, 0, 0, 0];
        assert!(art.resolve_internal_relocs(&vaddrs));

        let adrp_word = u32::from_le_bytes([art.text[0], art.text[1], art.text[2], art.text[3]]);
        let add_word = u32::from_le_bytes([art.text[4], art.text[5], art.text[6], art.text[7]]);

        // +2 pages -> immlo = 2 (bits 30:29 = 0b10 -> 0x40000000), so `adrp x0, #0x2000` = 0xD0000000
        assert_eq!(adrp_word, 0xD000_0000);
        // page offset 0x40 -> `add x0, x0, #0x40` = 0x91010000
        assert_eq!(add_word, 0x9101_0000);
    }
}
