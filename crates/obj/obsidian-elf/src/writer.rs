//! From-scratch ELF64 relocatable object (`.o` `ET_REL`) and standalone executable (`ET_EXEC`) serializer.

use alloc::vec;
use alloc::vec::Vec;
use obsidian_ir::Linkage;
use obsidian_obj::{Artifact, SectionId, SymbolKind};
use crate::defs::*;

/// Helper byte-buffer writer that respects target endianness (`Little` vs `Big`).
struct EndianBuf {
    buf: Vec<u8>,
    big_endian: bool,
}

impl EndianBuf {
    fn new(capacity: usize, big_endian: bool) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            big_endian,
        }
    }

    #[inline]
    fn push_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    #[inline]
    fn push_u16(&mut self, v: u16) {
        let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
        self.buf.extend_from_slice(&b);
    }

    #[inline]
    fn push_u32(&mut self, v: u32) {
        let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
        self.buf.extend_from_slice(&b);
    }

    #[inline]
    fn push_u64(&mut self, v: u64) {
        let b = if self.big_endian { v.to_be_bytes() } else { v.to_le_bytes() };
        self.buf.extend_from_slice(&b);
    }

    #[inline]
    fn push_bytes(&mut self, slice: &[u8]) {
        self.buf.extend_from_slice(slice);
    }

    #[inline]
    fn align_to(&mut self, align: usize) {
        if align > 1 {
            let rem = self.buf.len() % align;
            if rem != 0 {
                self.buf.resize(self.buf.len() + (align - rem), 0);
            }
        }
    }
}

/// Serializes an [`Artifact`] into a complete ELF64 relocatable object file (`.o`, `ET_REL`).
///
/// ### Emitted Section Table Layout
/// - `[0]` `NULL`
/// - `[1]` `.text` (`SHT_PROGBITS`, `SHF_ALLOC | SHF_EXECINSTR`)
/// - `[2]` `.rodata` (`SHT_PROGBITS`, `SHF_ALLOC`)
/// - `[3]` `.data` (`SHT_PROGBITS`, `SHF_ALLOC | SHF_WRITE`)
/// - `[4]` `.bss` (`SHT_NOBITS`, `SHF_ALLOC | SHF_WRITE`)
/// - `[5]` `.rela.text` (`SHT_RELA`, `sh_link = 6 (.symtab)`, `sh_info = 1 (.text)`)
/// - `[6]` `.symtab` (`SHT_SYMTAB`, `sh_link = 7 (.strtab)`, `sh_info = first_global_sym_idx`)
/// - `[7]` `.strtab` (`SHT_STRTAB`)
/// - `[8]` `.shstrtab` (`SHT_STRTAB`)
pub fn emit_relocatable_elf(art: &Artifact) -> Vec<u8> {
    let is_be = art.target.is_big_endian();
    let machine = elf_machine(art.target.arch);

    // 1. Build `.shstrtab` with fixed offsets:
    let mut shstrtab = Vec::with_capacity(96);
    shstrtab.push(0); // [0] = ""
    let sh_name_text = push_cstr(&mut shstrtab, ".text");
    let sh_name_rodata = push_cstr(&mut shstrtab, ".rodata");
    let sh_name_data = push_cstr(&mut shstrtab, ".data");
    let sh_name_bss = push_cstr(&mut shstrtab, ".bss");
    let sh_name_rela_text = push_cstr(&mut shstrtab, ".rela.text");
    let sh_name_symtab = push_cstr(&mut shstrtab, ".symtab");
    let sh_name_strtab = push_cstr(&mut shstrtab, ".strtab");
    let sh_name_shstrtab = push_cstr(&mut shstrtab, ".shstrtab");

    // 2. Partition `art.symbols` so all `STB_LOCAL` symbols precede all `STB_GLOBAL` symbols
    //    (a strict requirement of the System V ELF specification for `.symtab`!):
    let mut strtab = Vec::with_capacity(128);
    strtab.push(0); // [0] = ""

    let mut sym_remap = vec![0u32; art.symbols.len()];
    let mut ordered_syms: Vec<(usize, u32, u8, u16)> = Vec::with_capacity(art.symbols.len());

    // Pass 1: Local symbols (`Linkage::Local`)
    for (orig_idx, sym) in art.symbols.iter().enumerate() {
        if sym.linkage == Linkage::Local {
            let elf_sym_idx = (ordered_syms.len() + 1) as u32; // +1 for NULL symbol [0]
            sym_remap[orig_idx] = elf_sym_idx;
            let name_off = push_cstr(&mut strtab, &sym.name);
            let st_info = (STB_LOCAL << 4) | map_sym_type(sym.kind);
            let shndx = map_sec_shndx(sym.section);
            ordered_syms.push((orig_idx, name_off, st_info, shndx));
        }
    }
    let first_global_sym_idx = (ordered_syms.len() + 1) as u32;

    // Pass 2: Global / Extern symbols (`Linkage::Export` | `Linkage::Extern`)
    for (orig_idx, sym) in art.symbols.iter().enumerate() {
        if sym.linkage != Linkage::Local {
            let elf_sym_idx = (ordered_syms.len() + 1) as u32;
            sym_remap[orig_idx] = elf_sym_idx;
            let name_off = push_cstr(&mut strtab, &sym.name);
            let st_info = (STB_GLOBAL << 4) | map_sym_type(sym.kind);
            let shndx = map_sec_shndx(sym.section);
            ordered_syms.push((orig_idx, name_off, st_info, shndx));
        }
    }

    // 3. Serialize `.symtab` (`24 bytes` per Elf64_Sym):
    let mut symtab_buf = EndianBuf::new((ordered_syms.len() + 1) * 24, is_be);
    // Symbol [0]: STN_UNDEF (24 zero bytes)
    for _ in 0..24 {
        symtab_buf.push_u8(0);
    }
    for &(orig_idx, name_off, st_info, shndx) in &ordered_syms {
        let sym = &art.symbols[orig_idx];
        symtab_buf.push_u32(name_off);
        symtab_buf.push_u8(st_info);
        symtab_buf.push_u8(0); // st_other (STV_DEFAULT)
        symtab_buf.push_u16(shndx);
        symtab_buf.push_u64(sym.offset as u64);
        symtab_buf.push_u64(sym.size as u64);
    }

    // 4. Serialize `.rela.text` (`24 bytes` per Elf64_Rela):
    let mut rela_buf = EndianBuf::new(art.relocs.len() * 24, is_be);
    for rel in &art.relocs {
        if rel.section != SectionId::Text {
            continue;
        }
        let elf_sym_idx = sym_remap[rel.sym.index()] as u64;
        let r_type = elf_reloc_type(art.target.arch, rel.kind) as u64;
        let r_info = (elf_sym_idx << 32) | r_type;
        rela_buf.push_u64(rel.offset as u64);
        rela_buf.push_u64(r_info);
        rela_buf.push_u64(rel.addend as u64);
    }

    // 5. Assemble the complete ELF64 file:
    let mut out = EndianBuf::new(512 + art.text.len() + art.rodata.len(), is_be);

    // Reserve 64-byte Elf64_Ehdr placeholder (we'll fill `e_shoff` once sections are written):
    out.buf.resize(64, 0);

    out.align_to(16);
    let off_text = out.buf.len() as u64;
    out.push_bytes(&art.text);

    out.align_to(8);
    let off_rodata = out.buf.len() as u64;
    out.push_bytes(&art.rodata);

    out.align_to(8);
    let off_data = out.buf.len() as u64;
    out.push_bytes(&art.data);

    let off_bss = out.buf.len() as u64;

    out.align_to(8);
    let off_rela = out.buf.len() as u64;
    out.push_bytes(&rela_buf.buf);

    out.align_to(8);
    let off_symtab = out.buf.len() as u64;
    out.push_bytes(&symtab_buf.buf);

    let off_strtab = out.buf.len() as u64;
    out.push_bytes(&strtab);

    let off_shstrtab = out.buf.len() as u64;
    out.push_bytes(&shstrtab);

    out.align_to(8);
    let shoff = out.buf.len() as u64;

    // 6. Write the 9 Section Headers (`64 bytes` per Elf64_Shdr):
    // [0] SHT_NULL
    write_shdr64(&mut out, 0, SHT_NULL, 0, 0, 0, 0, 0, 0, 0, 0);
    // [1] .text
    write_shdr64(&mut out, sh_name_text, SHT_PROGBITS, SHF_ALLOC | SHF_EXECINSTR, 0, off_text, art.text.len() as u64, 0, 0, 16, 0);
    // [2] .rodata
    write_shdr64(&mut out, sh_name_rodata, SHT_PROGBITS, SHF_ALLOC, 0, off_rodata, art.rodata.len() as u64, 0, 0, 8, 0);
    // [3] .data
    write_shdr64(&mut out, sh_name_data, SHT_PROGBITS, SHF_ALLOC | SHF_WRITE, 0, off_data, art.data.len() as u64, 0, 0, 8, 0);
    // [4] .bss
    write_shdr64(&mut out, sh_name_bss, SHT_NOBITS, SHF_ALLOC | SHF_WRITE, 0, off_bss, art.bss_size, 0, 0, 8, 0);
    // [5] .rela.text (sh_link = 6 [.symtab], sh_info = 1 [.text], entsize = 24)
    write_shdr64(&mut out, sh_name_rela_text, SHT_RELA, SHF_INFO_LINK, 0, off_rela, rela_buf.buf.len() as u64, 6, 1, 8, 24);
    // [6] .symtab (sh_link = 7 [.strtab], sh_info = first_global_sym_idx, entsize = 24)
    write_shdr64(&mut out, sh_name_symtab, SHT_SYMTAB, 0, 0, off_symtab, symtab_buf.buf.len() as u64, 7, first_global_sym_idx, 8, 24);
    // [7] .strtab
    write_shdr64(&mut out, sh_name_strtab, SHT_STRTAB, 0, 0, off_strtab, strtab.len() as u64, 0, 0, 1, 0);
    // [8] .shstrtab
    write_shdr64(&mut out, sh_name_shstrtab, SHT_STRTAB, 0, 0, off_shstrtab, shstrtab.len() as u64, 0, 0, 1, 0);

    // 7. Populate the 64-byte Elf64_Ehdr at the start of `out.buf`:
    let mut ehdr = EndianBuf::new(64, is_be);
    ehdr.push_bytes(&ELF_MAGIC);
    ehdr.push_u8(ELFCLASS64);
    ehdr.push_u8(if is_be { ELFDATA2MSB } else { ELFDATA2LSB });
    ehdr.push_u8(EV_CURRENT);
    ehdr.push_u8(ELFOSABI_NONE);
    ehdr.push_u64(0); // e_ident[8..16] padding
    ehdr.push_u16(ET_REL);
    ehdr.push_u16(machine);
    ehdr.push_u32(EV_CURRENT as u32);
    ehdr.push_u64(0);     // e_entry
    ehdr.push_u64(0);     // e_phoff
    ehdr.push_u64(shoff); // e_shoff
    ehdr.push_u32(0);     // e_flags
    ehdr.push_u16(64);    // e_ehsize
    ehdr.push_u16(0);     // e_phentsize
    ehdr.push_u16(0);     // e_phnum
    ehdr.push_u16(64);    // e_shentsize
    ehdr.push_u16(9);     // e_shnum
    ehdr.push_u16(8);     // e_shstrndx = [8] (.shstrtab)

    out.buf[0..64].copy_from_slice(&ehdr.buf);
    out.buf
}

/// Links internal relocations and emits a standalone 64-bit ELF executable (`ET_EXEC`)
/// with entry point `entry_sym_name` (e.g., `"_start"` or `"main"`).
pub fn emit_executable_elf(art: &Artifact, entry_sym_name: &str) -> Option<Vec<u8>> {
    let mut linked = art.clone();
    let is_be = linked.target.is_big_endian();
    let machine = elf_machine(linked.target.arch);

    // Find the entry symbol offset in `.text`:
    let entry_offset = linked
        .symbols
        .iter()
        .find(|s| s.name == entry_sym_name && s.section == Some(SectionId::Text))?
        .offset as u64;

    // Standard ELF64 base virtual address `0x0040_0000`:
    // File layout:
    //   0x000..0x040: Elf64_Ehdr (64 bytes)
    //   0x040..0x0B0: 2 x Elf64_Phdr (112 bytes: RX segment + RW segment)
    //   0x0B0..     : .text + .rodata (RX segment)
    let base_vaddr: u64 = 0x0040_0000;
    let text_file_off: u64 = 64 + 2 * 56; // 176 (0xB0)
    let text_vaddr = base_vaddr + text_file_off;
    let rodata_file_off = align_up_u64(text_file_off + (linked.text.len() as u64), 8);
    let rodata_vaddr = base_vaddr + rodata_file_off;

    let rx_filesz = rodata_file_off + (linked.rodata.len() as u64);
    let rw_file_off = align_up_u64(rx_filesz, 8);
    // Place RW data segment on the next 4 KiB page (`+ 0x1000`):
    let data_vaddr = base_vaddr + 0x1000 + (rw_file_off & 0xFFF);
    let bss_vaddr = data_vaddr + (linked.data.len() as u64);

    let section_vaddrs = [
        text_vaddr,
        rodata_vaddr,
        data_vaddr,
        bss_vaddr,
        0,
        0,
    ];
    if !linked.resolve_internal_relocs(&section_vaddrs) {
        return None;
    }

    let entry_vaddr = text_vaddr + entry_offset;
    let mut out = EndianBuf::new((rw_file_off as usize) + linked.data.len(), is_be);

    // 1. Elf64_Ehdr (64 bytes):
    out.push_bytes(&ELF_MAGIC);
    out.push_u8(ELFCLASS64);
    out.push_u8(if is_be { ELFDATA2MSB } else { ELFDATA2LSB });
    out.push_u8(EV_CURRENT);
    out.push_u8(ELFOSABI_NONE);
    out.push_u64(0);
    out.push_u16(ET_EXEC);
    out.push_u16(machine);
    out.push_u32(EV_CURRENT as u32);
    out.push_u64(entry_vaddr);
    out.push_u64(64); // e_phoff = 64
    out.push_u64(0);  // e_shoff = 0 (no section headers needed in standalone executable)
    out.push_u32(0);
    out.push_u16(64); // e_ehsize
    out.push_u16(56); // e_phentsize
    out.push_u16(2);  // e_phnum = 2
    out.push_u16(64);
    out.push_u16(0);
    out.push_u16(0);

    // 2. Program Header [0]: PT_LOAD (Read + Execute: Ehdr + Phdrs + .text + .rodata):
    write_phdr64(&mut out, PT_LOAD, PF_R | PF_X, 0, base_vaddr, base_vaddr, rx_filesz, rx_filesz, 0x1000);

    // 3. Program Header [1]: PT_LOAD (Read + Write: .data + .bss):
    let rw_filesz = linked.data.len() as u64;
    let rw_memsz = rw_filesz + linked.bss_size;
    write_phdr64(&mut out, PT_LOAD, PF_R | PF_W, rw_file_off, data_vaddr, data_vaddr, rw_filesz, rw_memsz, 0x1000);

    // 4. Write `.text`, `.rodata`, and `.data`:
    out.push_bytes(&linked.text);
    out.align_to(8);
    out.push_bytes(&linked.rodata);
    out.align_to(8);
    out.push_bytes(&linked.data);

    Some(out.buf)
}

#[allow(clippy::too_many_arguments)]
fn write_shdr64(
    out: &mut EndianBuf,
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
) {
    out.push_u32(sh_name);
    out.push_u32(sh_type);
    out.push_u64(sh_flags);
    out.push_u64(sh_addr);
    out.push_u64(sh_offset);
    out.push_u64(sh_size);
    out.push_u32(sh_link);
    out.push_u32(sh_info);
    out.push_u64(sh_addralign);
    out.push_u64(sh_entsize);
}

#[allow(clippy::too_many_arguments)]
fn write_phdr64(
    out: &mut EndianBuf,
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
) {
    out.push_u32(p_type);
    out.push_u32(p_flags);
    out.push_u64(p_offset);
    out.push_u64(p_vaddr);
    out.push_u64(p_paddr);
    out.push_u64(p_filesz);
    out.push_u64(p_memsz);
    out.push_u64(p_align);
}

fn push_cstr(strtab: &mut Vec<u8>, s: &str) -> u32 {
    let off = strtab.len() as u32;
    strtab.extend_from_slice(s.as_bytes());
    strtab.push(0);
    off
}

const fn map_sym_type(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::Function => STT_FUNC,
        SymbolKind::Data     => STT_OBJECT,
        SymbolKind::Tls      => STT_TLS,
        SymbolKind::Extern   => STT_NOTYPE,
    }
}

const fn map_sec_shndx(sec: Option<SectionId>) -> u16 {
    match sec {
        Some(SectionId::Text)   => 1,
        Some(SectionId::Rodata) => 2,
        Some(SectionId::Data)   => 3,
        Some(SectionId::Bss)    => 4,
        _                       => SHN_UNDEF,
    }
}

const fn align_up_u64(val: u64, align: u64) -> u64 {
    if align <= 1 {
        val
    } else {
        (val + align - 1) & !(align - 1)
    }
}
