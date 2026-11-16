//! From-scratch Mach-O 64 relocatable object (`.o` `MH_OBJECT`) and Universal Fat Binary (`FAT_MAGIC`) serializer.

use alloc::vec;
use alloc::vec::Vec;
use obsidian_ir::{Linkage, Os};
use obsidian_obj::{Artifact, SectionId, SymbolKind};
use crate::defs::*;

/// Serializes an [`Artifact`] into a 64-bit Mach-O relocatable object file (`.o`, `MH_OBJECT`).
///
/// ### Emitted Load Commands & Sections
/// 1. `mach_header_64` (`32 bytes`)
/// 2. `LC_SEGMENT_64` (`72 bytes` + `4 * 80 bytes` = `392 bytes`) containing 4 sections:
///    - `sect 1`: `("__TEXT", "__text")`
///    - `sect 2`: `("__TEXT", "__const")`
///    - `sect 3`: `("__DATA", "__data")`
///    - `sect 4`: `("__DATA", "__bss")`
/// 3. `LC_BUILD_VERSION` (`24 bytes`)
/// 4. `LC_SYMTAB` (`24 bytes`)
/// 5. `LC_DYSYMTAB` (`80 bytes`, partitioning local defined, external defined, and undefined symbols)
pub fn emit_relocatable_macho(art: &Artifact) -> Vec<u8> {
    let (cputype, cpusubtype) = macho_cpu_type(art.target.arch);
    let platform = if matches!(art.target.os, Os::Ios) {
        PLATFORM_IOS
    } else {
        PLATFORM_MACOS
    };

    // Total load commands size: 392 (LC_SEGMENT_64 + 4 sect64) + 24 (LC_BUILD_VERSION) + 24 (LC_SYMTAB) + 80 (LC_DYSYMTAB) = 520 bytes.
    let sizeofcmds: u32 = 392 + 24 + 24 + 80;
    let header_and_cmds_size: usize = 32 + (sizeofcmds as usize); // 552 bytes

    // 1. Partition symbols into the 3 groups required by `LC_DYSYMTAB`:
    //    Group A: Local defined symbols (`Linkage::Local`)
    //    Group B: External defined symbols (`Linkage::Export`, `kind != Extern`)
    //    Group C: Undefined external symbols (`kind == Extern`)
    let mut strtab = Vec::with_capacity(128);
    strtab.push(0); // byte 0 is empty string

    let mut sym_remap = vec![0u32; art.symbols.len()];
    let mut nlists: Vec<(u32, u8, u8, u16, u64)> = Vec::with_capacity(art.symbols.len());

    // Compute virtual addresses of the 4 sections inside the single `.o` segment:
    let text_addr: u64 = 0;
    let rodata_addr: u64 = align_up_u64(text_addr + (art.text.len() as u64), 8);
    let data_addr: u64 = align_up_u64(rodata_addr + (art.rodata.len() as u64), 8);
    let bss_addr: u64 = align_up_u64(data_addr + (art.data.len() as u64), 8);
    let vmsize: u64 = bss_addr + art.bss_size;

    let sec_base_addr = |sec: SectionId| -> u64 {
        match sec {
            SectionId::Text   => text_addr,
            SectionId::Rodata => rodata_addr,
            SectionId::Data   => data_addr,
            SectionId::Bss    => bss_addr,
            _                 => 0,
        }
    };

    // Pass A: Local defined symbols
    for (orig_idx, sym) in art.symbols.iter().enumerate() {
        if sym.linkage == Linkage::Local && sym.kind != SymbolKind::Extern {
            sym_remap[orig_idx] = nlists.len() as u32;
            let strx = push_darwin_sym(&mut strtab, &sym.name);
            let n_sect = map_macho_sect(sym.section);
            let n_value = sym.section.map(sec_base_addr).unwrap_or(0) + (sym.offset as u64);
            nlists.push((strx, N_SECT, n_sect, 0, n_value));
        }
    }
    let nlocalsym = nlists.len() as u32;

    // Pass B: External defined symbols (`N_SECT | N_EXT`)
    for (orig_idx, sym) in art.symbols.iter().enumerate() {
        if sym.linkage == Linkage::Export && sym.kind != SymbolKind::Extern {
            sym_remap[orig_idx] = nlists.len() as u32;
            let strx = push_darwin_sym(&mut strtab, &sym.name);
            let n_sect = map_macho_sect(sym.section);
            let n_value = sym.section.map(sec_base_addr).unwrap_or(0) + (sym.offset as u64);
            nlists.push((strx, N_SECT | N_EXT, n_sect, 0, n_value));
        }
    }
    let nextdefsym = (nlists.len() as u32) - nlocalsym;

    // Pass C: Undefined external symbols (`N_UNDF | N_EXT`)
    for (orig_idx, sym) in art.symbols.iter().enumerate() {
        if sym.kind == SymbolKind::Extern {
            sym_remap[orig_idx] = nlists.len() as u32;
            let strx = push_darwin_sym(&mut strtab, &sym.name);
            nlists.push((strx, N_UNDF | N_EXT, 0, 0, 0));
        }
    }
    let nundefsym = (nlists.len() as u32) - nlocalsym - nextdefsym;

    // 2. Build section data & compute file offsets:
    let mut body = Vec::with_capacity(art.text.len() + art.rodata.len() + art.data.len() + 256);

    let off_text = (header_and_cmds_size + body.len()) as u32;
    body.extend_from_slice(&art.text);
    align_vec(&mut body, 8);

    let off_rodata = (header_and_cmds_size + body.len()) as u32;
    body.extend_from_slice(&art.rodata);
    align_vec(&mut body, 8);

    let off_data = (header_and_cmds_size + body.len()) as u32;
    body.extend_from_slice(&art.data);
    align_vec(&mut body, 8);

    let file_seg_size = body.len() as u64;

    // 3. Serialize relocations for `__TEXT,__text` (`8 bytes` per `relocation_info`):
    let reloff_text = if art.relocs.is_empty() {
        0u32
    } else {
        (header_and_cmds_size + body.len()) as u32
    };
    let mut nreloc_text = 0u32;
    for rel in &art.relocs {
        if rel.section != SectionId::Text {
            continue;
        }
        let sym_num = sym_remap[rel.sym.index()] & 0x00FF_FFFF;
        let (r_type, r_pcrel, r_len) = macho_reloc_info(art.target.arch, rel.kind);
        let r_extern = 1u32;
        let packed_info = sym_num
            | ((r_pcrel as u32) << 24)
            | (((r_len & 0x3) as u32) << 25)
            | (r_extern << 27)
            | (((r_type & 0xF) as u32) << 28);
        body.extend_from_slice(&(rel.offset as i32).to_le_bytes());
        body.extend_from_slice(&packed_info.to_le_bytes());
        nreloc_text += 1;
    }
    align_vec(&mut body, 8);

    // 4. Serialize `nlist_64` symbol table (`16 bytes` per entry):
    let symoff = (header_and_cmds_size + body.len()) as u32;
    for &(strx, n_type, n_sect, n_desc, n_value) in &nlists {
        body.extend_from_slice(&strx.to_le_bytes());
        body.push(n_type);
        body.push(n_sect);
        body.extend_from_slice(&n_desc.to_le_bytes());
        body.extend_from_slice(&n_value.to_le_bytes());
    }

    // 5. Append string table (`strtab`):
    let stroff = (header_and_cmds_size + body.len()) as u32;
    body.extend_from_slice(&strtab);
    let strsize = strtab.len() as u32;

    // 6. Assemble final Mach-O 64 file:
    let mut out = Vec::with_capacity(header_and_cmds_size + body.len());

    // --- mach_header_64 (32 bytes) ---
    push_u32(&mut out, MH_MAGIC_64);
    push_u32(&mut out, cputype);
    push_u32(&mut out, cpusubtype);
    push_u32(&mut out, MH_OBJECT);
    push_u32(&mut out, 4); // ncmds = 4 (LC_SEGMENT_64, LC_BUILD_VERSION, LC_SYMTAB, LC_DYSYMTAB)
    push_u32(&mut out, sizeofcmds);
    push_u32(&mut out, MH_SUBSECTIONS_VIA_SYMBOLS);
    push_u32(&mut out, 0); // reserved

    // --- Load Command 1: LC_SEGMENT_64 (72 bytes + 4 * 80 bytes = 392 bytes) ---
    push_u32(&mut out, LC_SEGMENT_64);
    push_u32(&mut out, 392);
    push_fixed_str16(&mut out, ""); // unnamed segment in MH_OBJECT
    push_u64(&mut out, 0);          // vmaddr
    push_u64(&mut out, vmsize);     // vmsize
    push_u64(&mut out, header_and_cmds_size as u64); // fileoff
    push_u64(&mut out, file_seg_size);               // filesize
    push_u32(&mut out, VM_PROT_READ | VM_PROT_WRITE | VM_PROT_EXECUTE); // maxprot
    push_u32(&mut out, VM_PROT_READ | VM_PROT_WRITE | VM_PROT_EXECUTE); // initprot
    push_u32(&mut out, 4); // nsects = 4
    push_u32(&mut out, 0); // flags

    // Section 1: ("__TEXT", "__text")
    write_sect64(
        &mut out,
        "__text",
        "__TEXT",
        text_addr,
        art.text.len() as u64,
        off_text,
        4, // align = 2^4 = 16 bytes
        reloff_text,
        nreloc_text,
        S_REGULAR | S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS,
    );
    // Section 2: ("__TEXT", "__const")
    write_sect64(
        &mut out,
        "__const",
        "__TEXT",
        rodata_addr,
        art.rodata.len() as u64,
        off_rodata,
        3, // align = 2^3 = 8 bytes
        0,
        0,
        S_REGULAR,
    );
    // Section 3: ("__DATA", "__data")
    write_sect64(
        &mut out,
        "__data",
        "__DATA",
        data_addr,
        art.data.len() as u64,
        off_data,
        3,
        0,
        0,
        S_REGULAR,
    );
    // Section 4: ("__DATA", "__bss")
    write_sect64(
        &mut out,
        "__bss",
        "__DATA",
        bss_addr,
        art.bss_size,
        0,
        3,
        0,
        0,
        S_ZEROFILL,
    );

    // --- Load Command 2: LC_BUILD_VERSION (24 bytes) ---
    push_u32(&mut out, LC_BUILD_VERSION);
    push_u32(&mut out, 24);
    push_u32(&mut out, platform);
    push_u32(&mut out, 0x000E_0000); // minos = 14.0.0
    push_u32(&mut out, 0x000E_0000); // sdk   = 14.0.0
    push_u32(&mut out, 0);           // ntools = 0

    // --- Load Command 3: LC_SYMTAB (24 bytes) ---
    push_u32(&mut out, LC_SYMTAB);
    push_u32(&mut out, 24);
    push_u32(&mut out, symoff);
    push_u32(&mut out, nlists.len() as u32);
    push_u32(&mut out, stroff);
    push_u32(&mut out, strsize);

    // --- Load Command 4: LC_DYSYMTAB (80 bytes) ---
    push_u32(&mut out, LC_DYSYMTAB);
    push_u32(&mut out, 80);
    push_u32(&mut out, 0);                        // ilocalsym
    push_u32(&mut out, nlocalsym);                // nlocalsym
    push_u32(&mut out, nlocalsym);                // iextdefsym
    push_u32(&mut out, nextdefsym);               // nextdefsym
    push_u32(&mut out, nlocalsym + nextdefsym);   // iundefsym
    push_u32(&mut out, nundefsym);                // nundefsym
    for _ in 0..12 {
        push_u32(&mut out, 0); // toc, modtab, extrefsyms, indirectsyms, extrel, locrel
    }

    debug_assert_eq!(out.len(), header_and_cmds_size);
    out.extend_from_slice(&body);
    out
}

/// Combines an `x86_64` Mach-O binary and an `arm64` Mach-O binary into a single
/// Apple Universal Fat Binary (`FAT_MAGIC = 0xCAFEBABE`, 16 KiB page-aligned slices).
pub fn emit_universal_macho(x86_64_macho: &[u8], arm64_macho: &[u8]) -> Vec<u8> {
    // 16 KiB (`0x4000`, `2^14`) alignment required by macOS ARM64 kernel loader:
    let page_align: usize = 0x4000;
    let x86_off = page_align;
    let arm_off = align_up_usize(x86_off + x86_64_macho.len(), page_align);
    let total_len = arm_off + arm64_macho.len();

    let mut out = Vec::with_capacity(total_len);

    // `fat_header` (8 bytes, Big-Endian!):
    out.extend_from_slice(&FAT_MAGIC.to_be_bytes());
    out.extend_from_slice(&2u32.to_be_bytes()); // nfat_arch = 2

    // `fat_arch[0]` (`x86_64`, 20 bytes, Big-Endian!):
    out.extend_from_slice(&CPU_TYPE_X86_64.to_be_bytes());
    out.extend_from_slice(&CPU_SUBTYPE_X86_64_ALL.to_be_bytes());
    out.extend_from_slice(&(x86_off as u32).to_be_bytes());
    out.extend_from_slice(&(x86_64_macho.len() as u32).to_be_bytes());
    out.extend_from_slice(&14u32.to_be_bytes()); // align = 2^14 (16384)

    // `fat_arch[1]` (`arm64`, 20 bytes, Big-Endian!):
    out.extend_from_slice(&CPU_TYPE_ARM64.to_be_bytes());
    out.extend_from_slice(&CPU_SUBTYPE_ARM64_ALL.to_be_bytes());
    out.extend_from_slice(&(arm_off as u32).to_be_bytes());
    out.extend_from_slice(&(arm64_macho.len() as u32).to_be_bytes());
    out.extend_from_slice(&14u32.to_be_bytes()); // align = 2^14 (16384)

    out.resize(x86_off, 0);
    out.extend_from_slice(x86_64_macho);
    out.resize(arm_off, 0);
    out.extend_from_slice(arm64_macho);
    out
}

#[allow(clippy::too_many_arguments)]
fn write_sect64(
    out: &mut Vec<u8>,
    sectname: &str,
    segname: &str,
    addr: u64,
    size: u64,
    offset: u32,
    align_log2: u32,
    reloff: u32,
    nreloc: u32,
    flags: u32,
) {
    push_fixed_str16(out, sectname);
    push_fixed_str16(out, segname);
    push_u64(out, addr);
    push_u64(out, size);
    push_u32(out, offset);
    push_u32(out, align_log2);
    push_u32(out, reloff);
    push_u32(out, nreloc);
    push_u32(out, flags);
    push_u32(out, 0); // reserved1
    push_u32(out, 0); // reserved2
    push_u32(out, 0); // reserved3 (64-bit padding)
}

/// Pushes a Darwin C symbol name with leading `'_'` prefix to `strtab`.
fn push_darwin_sym(strtab: &mut Vec<u8>, name: &str) -> u32 {
    let off = strtab.len() as u32;
    strtab.push(b'_');
    strtab.extend_from_slice(name.as_bytes());
    strtab.push(0);
    off
}

fn push_fixed_str16(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(16);
    out.extend_from_slice(&bytes[..len]);
    for _ in len..16 {
        out.push(0);
    }
}

#[inline]
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

const fn map_macho_sect(sec: Option<SectionId>) -> u8 {
    match sec {
        Some(SectionId::Text)   => 1,
        Some(SectionId::Rodata) => 2,
        Some(SectionId::Data)   => 3,
        Some(SectionId::Bss)    => 4,
        _                       => 0,
    }
}

fn align_vec(buf: &mut Vec<u8>, align: usize) {
    let rem = buf.len() % align;
    if rem != 0 {
        buf.resize(buf.len() + (align - rem), 0);
    }
}

const fn align_up_u64(val: u64, align: u64) -> u64 {
    (val + align - 1) & !(align - 1)
}

const fn align_up_usize(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}
