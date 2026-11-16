//! From-scratch Windows/UEFI COFF relocatable object (`.obj`) and PE32+ executable (`.exe`) serializer.

use alloc::vec::Vec;
use obsidian_ir::Linkage;
use obsidian_obj::{Artifact, SectionId, SymbolKind};
use crate::defs::*;

/// Serializes an [`Artifact`] into a Microsoft COFF relocatable object file (`.obj`).
///
/// ### Emitted COFF Structure
/// 1. `IMAGE_FILE_HEADER` (`20 bytes`)
/// 2. 4 `IMAGE_SECTION_HEADER` entries (`4 * 40 = 160 bytes`):
///    - `[1]` `.text` (`IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ`)
///    - `[2]` `.rdata` (`IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ`)
///    - `[3]` `.data` (`IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE`)
///    - `[4]` `.bss` (`IMAGE_SCN_CNT_UNINITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE`)
/// 3. Section raw data (`.text`, `.rdata`, `.data`)
/// 4. 10-byte `IMAGE_RELOCATION` entries for `.text`
/// 5. 18-byte `IMAGE_SYMBOL` symbol table entries
/// 6. COFF string table (`4-byte` length prefix + null-terminated long names)
pub fn emit_relocatable_coff(art: &Artifact) -> Vec<u8> {
    let machine = coff_machine(art.target.arch);
    let headers_size: usize = 20 + 4 * 40; // 180 bytes

    let mut body = Vec::with_capacity(art.text.len() + art.rodata.len() + art.data.len() + 256);

    let ptr_text = if art.text.is_empty() {
        0u32
    } else {
        (headers_size + body.len()) as u32
    };
    body.extend_from_slice(&art.text);
    align_vec(&mut body, 8);

    let ptr_rdata = if art.rodata.is_empty() {
        0u32
    } else {
        (headers_size + body.len()) as u32
    };
    body.extend_from_slice(&art.rodata);
    align_vec(&mut body, 8);

    let ptr_data = if art.data.is_empty() {
        0u32
    } else {
        (headers_size + body.len()) as u32
    };
    body.extend_from_slice(&art.data);
    align_vec(&mut body, 8);

    // Serialize 10-byte COFF relocation entries (`VirtualAddress: u32, SymbolTableIndex: u32, Type: u16`):
    let ptr_relocs_text = if art.relocs.is_empty() {
        0u32
    } else {
        (headers_size + body.len()) as u32
    };
    let mut nrelocs_text = 0u16;
    for rel in &art.relocs {
        if rel.section != SectionId::Text {
            continue;
        }
        let r_type = coff_reloc_type(art.target.arch, rel.kind);
        body.extend_from_slice(&rel.offset.to_le_bytes());
        body.extend_from_slice(&rel.sym.0.to_le_bytes());
        body.extend_from_slice(&r_type.to_le_bytes());
        nrelocs_text += 1;
    }
    align_vec(&mut body, 4);

    // Serialize 18-byte COFF symbol records & string table (starts with 4-byte total size):
    let ptr_symtab = (headers_size + body.len()) as u32;
    let mut strtab = Vec::with_capacity(64);
    strtab.extend_from_slice(&4u32.to_le_bytes()); // initial size = 4 bytes

    for sym in &art.symbols {
        let name_bytes = sym.name.as_bytes();
        if name_bytes.len() <= 8 {
            let mut short_name = [0u8; 8];
            short_name[..name_bytes.len()].copy_from_slice(name_bytes);
            body.extend_from_slice(&short_name);
        } else {
            let str_off = strtab.len() as u32;
            strtab.extend_from_slice(name_bytes);
            strtab.push(0);
            body.extend_from_slice(&0u32.to_le_bytes()); // Zeroes == 0 means long name in string table!
            body.extend_from_slice(&str_off.to_le_bytes());
        }

        let value = sym.offset;
        let sec_num: i16 = match sym.section {
            Some(SectionId::Text)   => 1,
            Some(SectionId::Rodata) => 2,
            Some(SectionId::Data)   => 3,
            Some(SectionId::Bss)    => 4,
            _                       => 0, // IMAGE_SYM_UNDEFINED
        };
        let sym_type: u16 = if sym.kind == SymbolKind::Function {
            IMAGE_SYM_DTYPE_FUNCTION
        } else {
            0
        };
        let storage_class: u8 = if sym.linkage == Linkage::Local {
            IMAGE_SYM_CLASS_STATIC
        } else {
            IMAGE_SYM_CLASS_EXTERNAL
        };

        body.extend_from_slice(&value.to_le_bytes());
        body.extend_from_slice(&sec_num.to_le_bytes());
        body.extend_from_slice(&sym_type.to_le_bytes());
        body.push(storage_class);
        body.push(0); // NumberOfAuxSymbols = 0
    }

    // Patch final string-table byte size into `strtab[0..4]` and append to `body`:
    let strtab_len = strtab.len() as u32;
    strtab[0..4].copy_from_slice(&strtab_len.to_le_bytes());
    body.extend_from_slice(&strtab);

    let mut out = Vec::with_capacity(headers_size + body.len());

    // 1. IMAGE_FILE_HEADER (20 bytes):
    push_u16(&mut out, machine);
    push_u16(&mut out, 4); // NumberOfSections = 4
    push_u32(&mut out, 0); // TimeDateStamp
    push_u32(&mut out, ptr_symtab);
    push_u32(&mut out, art.symbols.len() as u32);
    push_u16(&mut out, 0); // SizeOfOptionalHeader = 0 for .obj
    push_u16(&mut out, 0); // Characteristics

    // 2. 4 x IMAGE_SECTION_HEADER (40 bytes each):
    // [1] .text
    write_coff_shdr(
        &mut out,
        ".text",
        0,
        0,
        art.text.len() as u32,
        ptr_text,
        ptr_relocs_text,
        nrelocs_text,
        IMAGE_SCN_CNT_CODE | IMAGE_SCN_ALIGN_16BYTES | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ,
    );
    // [2] .rdata
    write_coff_shdr(
        &mut out,
        ".rdata",
        0,
        0,
        art.rodata.len() as u32,
        ptr_rdata,
        0,
        0,
        IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_ALIGN_8BYTES | IMAGE_SCN_MEM_READ,
    );
    // [3] .data
    write_coff_shdr(
        &mut out,
        ".data",
        0,
        0,
        art.data.len() as u32,
        ptr_data,
        0,
        0,
        IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_ALIGN_8BYTES | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE,
    );
    // [4] .bss
    write_coff_shdr(
        &mut out,
        ".bss",
        0,
        0,
        art.bss_size as u32,
        0,
        0,
        0,
        IMAGE_SCN_CNT_UNINITIALIZED_DATA | IMAGE_SCN_ALIGN_8BYTES | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE,
    );

    debug_assert_eq!(out.len(), headers_size);
    out.extend_from_slice(&body);
    out
}

/// Links internal relocations and emits a standalone 64-bit Windows/UEFI PE32+ executable (`.exe`).
pub fn emit_executable_pe(art: &Artifact, entry_sym_name: &str) -> Option<Vec<u8>> {
    let mut linked = art.clone();
    let machine = coff_machine(linked.target.arch);

    let entry_offset = linked
        .symbols
        .iter()
        .find(|s| s.name == entry_sym_name && s.section == Some(SectionId::Text))?
        .offset;

    let image_base: u64 = 0x0000_0001_4000_0000;
    let sect_align: u32 = 0x1000;
    let file_align: u32 = 0x0200; // 512 bytes

    // Virtual RVA layout:
    //   0x1000: .text
    //   0x2000: .rdata
    //   0x3000: .data
    let text_rva: u32 = 0x1000;
    let rdata_rva: u32 = 0x2000;
    let data_rva: u32 = 0x3000;

    let section_vaddrs = [
        image_base + (text_rva as u64),
        image_base + (rdata_rva as u64),
        image_base + (data_rva as u64),
        image_base + 0x4000,
        0,
        0,
    ];
    if !linked.resolve_internal_relocs(&section_vaddrs) {
        return None;
    }

    let text_raw_size = align_up_u32(linked.text.len() as u32, file_align);
    let rdata_raw_size = align_up_u32(linked.rodata.len() as u32, file_align);
    let data_raw_size = align_up_u32(linked.data.len() as u32, file_align);

    let headers_raw_size: u32 = 0x0200; // 512 bytes covers DOS header + PE sig + COFF hdr + OptHdr64 + 3 Shdrs
    let text_file_ptr = headers_raw_size;
    let rdata_file_ptr = text_file_ptr + text_raw_size;
    let data_file_ptr = rdata_file_ptr + rdata_raw_size;
    let size_of_image: u32 = 0x4000;

    let mut out = Vec::with_capacity((data_file_ptr + data_raw_size) as usize);

    // 1. Minimal 64-byte MS-DOS Header (`MZ`, `e_lfanew = 64` at offset `0x3C`):
    push_u16(&mut out, MZ_MAGIC);
    out.resize(0x3C, 0);
    push_u32(&mut out, 64); // e_lfanew = 64

    // 2. PE Signature (`"PE\0\0"`, 4 bytes) + IMAGE_FILE_HEADER (`20 bytes`):
    out.extend_from_slice(&PE_SIGNATURE);
    push_u16(&mut out, machine);
    push_u16(&mut out, 3); // NumberOfSections = 3 (.text, .rdata, .data)
    push_u32(&mut out, 0); // TimeDateStamp
    push_u32(&mut out, 0); // PointerToSymbolTable
    push_u32(&mut out, 0); // NumberOfSymbols
    push_u16(&mut out, 240); // SizeOfOptionalHeader = 240 (IMAGE_OPTIONAL_HEADER64)
    push_u16(&mut out, IMAGE_FILE_EXECUTABLE_IMAGE | IMAGE_FILE_LARGE_ADDRESS_AWARE);

    // 3. IMAGE_OPTIONAL_HEADER64 (240 bytes):
    push_u16(&mut out, PE32_PLUS_MAGIC);
    out.push(14); // MajorLinkerVersion
    out.push(0);  // MinorLinkerVersion
    push_u32(&mut out, text_raw_size);
    push_u32(&mut out, rdata_raw_size + data_raw_size);
    push_u32(&mut out, 0); // SizeOfUninitializedData
    push_u32(&mut out, text_rva + entry_offset); // AddressOfEntryPoint
    push_u32(&mut out, text_rva);                // BaseOfCode
    push_u64(&mut out, image_base);              // ImageBase
    push_u32(&mut out, sect_align);              // SectionAlignment
    push_u32(&mut out, file_align);              // FileAlignment
    push_u16(&mut out, 6); // MajorOperatingSystemVersion
    push_u16(&mut out, 0);
    push_u16(&mut out, 0);
    push_u16(&mut out, 0);
    push_u16(&mut out, 6); // MajorSubsystemVersion
    push_u16(&mut out, 0);
    push_u32(&mut out, 0); // Win32VersionValue
    push_u32(&mut out, size_of_image);
    push_u32(&mut out, headers_raw_size);
    push_u32(&mut out, 0); // CheckSum
    push_u16(&mut out, IMAGE_SUBSYSTEM_WINDOWS_CUI);
    push_u16(&mut out, 0x8160); // DllCharacteristics (HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT | TERMINAL_SERVER_AWARE)
    push_u64(&mut out, 0x10_0000); // SizeOfStackReserve
    push_u64(&mut out, 0x1000);    // SizeOfStackCommit
    push_u64(&mut out, 0x10_0000); // SizeOfHeapReserve
    push_u64(&mut out, 0x1000);    // SizeOfHeapCommit
    push_u32(&mut out, 0);         // LoaderFlags
    push_u32(&mut out, 16);        // NumberOfRvaAndSizes = 16
    for _ in 0..16 {
        push_u64(&mut out, 0);     // 16 x IMAGE_DATA_DIRECTORY (128 bytes)
    }

    // 4. 3 x IMAGE_SECTION_HEADER (120 bytes):
    write_coff_shdr(
        &mut out,
        ".text",
        linked.text.len() as u32,
        text_rva,
        text_raw_size,
        text_file_ptr,
        0,
        0,
        IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ,
    );
    write_coff_shdr(
        &mut out,
        ".rdata",
        linked.rodata.len() as u32,
        rdata_rva,
        rdata_raw_size,
        rdata_file_ptr,
        0,
        0,
        IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ,
    );
    write_coff_shdr(
        &mut out,
        ".data",
        linked.data.len() as u32,
        data_rva,
        data_raw_size,
        data_file_ptr,
        0,
        0,
        IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE,
    );

    // Pad headers to `headers_raw_size` (`0x200` = 512 bytes) and write sections:
    out.resize(headers_raw_size as usize, 0);
    out.extend_from_slice(&linked.text);
    out.resize((rdata_file_ptr) as usize, 0);
    out.extend_from_slice(&linked.rodata);
    out.resize((data_file_ptr) as usize, 0);
    out.extend_from_slice(&linked.data);
    out.resize((data_file_ptr + data_raw_size) as usize, 0);

    Some(out)
}

#[allow(clippy::too_many_arguments)]
fn write_coff_shdr(
    out: &mut Vec<u8>,
    name: &str,
    virtual_size: u32,
    virtual_addr: u32,
    raw_data_size: u32,
    ptr_raw_data: u32,
    ptr_relocs: u32,
    num_relocs: u16,
    characteristics: u32,
) {
    let mut name8 = [0u8; 8];
    let bytes = name.as_bytes();
    let len = bytes.len().min(8);
    name8[..len].copy_from_slice(&bytes[..len]);
    out.extend_from_slice(&name8);
    push_u32(out, virtual_size);
    push_u32(out, virtual_addr);
    push_u32(out, raw_data_size);
    push_u32(out, ptr_raw_data);
    push_u32(out, ptr_relocs);
    push_u32(out, 0); // PointerToLinenumbers
    push_u16(out, num_relocs);
    push_u16(out, 0); // NumberOfLinenumbers
    push_u32(out, characteristics);
}

#[inline]
fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline]
fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn align_vec(buf: &mut Vec<u8>, align: usize) {
    let rem = buf.len() % align;
    if rem != 0 {
        buf.resize(buf.len() + (align - rem), 0);
    }
}

const fn align_up_u32(val: u32, align: u32) -> u32 {
    if align <= 1 {
        val
    } else {
        (val + align - 1) & !(align - 1)
    }
}
