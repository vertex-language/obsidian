//! Multi-artifact section merging, cross-module symbol resolution, and executable linking.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use obsidian_coff::{emit_executable_pe, emit_relocatable_coff};
use obsidian_elf::{emit_executable_elf, emit_relocatable_elf};
use obsidian_ir::{Arch, Linkage, ObjFormat, Sym};
use obsidian_macho::emit_relocatable_macho;
use obsidian_obj::{Artifact, Reloc, SectionId, SymbolKind, SymbolRecord};

/// Errors returned by [`merge_artifacts`] and [`link_executable`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkError {
    /// No input artifacts were provided.
    EmptyArtifacts,
    /// Input artifacts have mismatched target architectures or endianness.
    TargetMismatch,
    /// Two artifacts exported a global symbol with the same name.
    DuplicateSymbol(String),
    /// An imported `Extern` symbol was not defined by any input artifact.
    UndefinedSymbol(String),
    /// The requested entry point symbol (e.g., `"_start"` or `"main"`) was not found in `.text`.
    MissingEntryPoint(String),
    /// A relocation displacement exceeded the hardware bitfield range.
    RelocationOverflow,
}

/// Emits a relocatable object file (`.o` / `.obj`) for `art` by dispatching to
/// `obsidian-elf`, `obsidian-macho`, or `obsidian-coff` according to `art.target.obj_format`.
pub fn emit_relocatable_object(art: &Artifact) -> Vec<u8> {
    match art.target.obj_format {
        ObjFormat::MachO => emit_relocatable_macho(art),
        ObjFormat::Coff  => emit_relocatable_coff(art),
        _                => emit_relocatable_elf(art),
    }
}

/// Merges multiple [`Artifact`]s into a single combined [`Artifact`], resolving
/// cross-module symbol references (`Extern` imports in one artifact matched against
/// `Export` definitions in another).
///
/// - If `require_all_resolved` is `true`, returns [`LinkError::UndefinedSymbol`] if any
///   imported symbol remains undefined after merging.
/// - If `require_all_resolved` is `false` (partial linking, equivalent to `ld -r`),
///   unresolved imports are preserved in the merged symbol table.
pub fn merge_artifacts(
    artifacts: &[Artifact],
    require_all_resolved: bool,
) -> Result<Artifact, LinkError> {
    let Some(first) = artifacts.first() else {
        return Err(LinkError::EmptyArtifacts);
    };

    let target = first.target;
    for art in &artifacts[1..] {
        if art.target.arch != target.arch || art.target.endian != target.endian {
            return Err(LinkError::TargetMismatch);
        }
    }

    let mut merged = Artifact::new(&first.name, target);
    let pad_byte = if target.arch == Arch::X86_64 || target.arch == Arch::X86_32 {
        0x90
    } else {
        0x00
    };

    // Per-artifact base offsets for each section `[Text, Rodata, Data, Bss, Tdata, Tbss]`:
    let mut sec_bases: Vec<[u32; 6]> = Vec::with_capacity(artifacts.len());

    for art in artifacts {
        align_vec(&mut merged.text, 16, pad_byte);
        let text_base = merged.text.len() as u32;
        merged.text.extend_from_slice(&art.text);

        align_vec(&mut merged.rodata, 8, 0);
        let rodata_base = merged.rodata.len() as u32;
        merged.rodata.extend_from_slice(&art.rodata);

        align_vec(&mut merged.data, 8, 0);
        let data_base = merged.data.len() as u32;
        merged.data.extend_from_slice(&art.data);

        merged.bss_size = (merged.bss_size + 7) & !7;
        let bss_base = merged.bss_size as u32;
        merged.bss_size += art.bss_size;

        align_vec(&mut merged.tdata, 8, 0);
        let tdata_base = merged.tdata.len() as u32;
        merged.tdata.extend_from_slice(&art.tdata);

        merged.tbss_size = (merged.tbss_size + 7) & !7;
        let tbss_base = merged.tbss_size as u32;
        merged.tbss_size += art.tbss_size;

        sec_bases.push([
            text_base,
            rodata_base,
            data_base,
            bss_base,
            tdata_base,
            tbss_base,
        ]);
    }

    // Pass 1: Collect all defined symbols (`kind != SymbolKind::Extern`) across all artifacts:
    // Also build a remap table `sym_remap[art_idx][old_sym_idx] -> new_sym: Sym`.
    let mut sym_remap: Vec<Vec<Sym>> = Vec::with_capacity(artifacts.len());

    for (art_idx, art) in artifacts.iter().enumerate() {
        let bases = sec_bases[art_idx];
        let mut remap = vec![Sym(u32::MAX); art.symbols.len()];

        for (sym_idx, sym) in art.symbols.iter().enumerate() {
            if sym.kind == SymbolKind::Extern {
                continue;
            }
            if sym.linkage == Linkage::Export {
                // Check for duplicate exported symbol definitions:
                if merged
                    .symbols
                    .iter()
                    .any(|s| s.linkage == Linkage::Export && s.kind != SymbolKind::Extern && s.name == sym.name)
                {
                    return Err(LinkError::DuplicateSymbol(sym.name.clone()));
                }
            }
            let sec_base = sym.section.map(|s| bases[s as usize]).unwrap_or(0);
            let new_sym = Sym(merged.symbols.len() as u32);
            merged.symbols.push(SymbolRecord {
                name: sym.name.clone(),
                kind: sym.kind,
                linkage: sym.linkage,
                section: sym.section,
                offset: sym.offset + sec_base,
                size: sym.size,
                align: sym.align,
            });
            remap[sym_idx] = new_sym;
        }
        sym_remap.push(remap);
    }

    // Pass 2: Resolve `SymbolKind::Extern` imports against exported symbols in `merged.symbols`:
    for (art_idx, art) in artifacts.iter().enumerate() {
        for (sym_idx, sym) in art.symbols.iter().enumerate() {
            if sym.kind != SymbolKind::Extern {
                continue;
            }
            if let Some(found_idx) = merged
                .symbols
                .iter()
                .position(|s| s.kind != SymbolKind::Extern && s.linkage == Linkage::Export && s.name == sym.name)
            {
                sym_remap[art_idx][sym_idx] = Sym(found_idx as u32);
            } else if require_all_resolved {
                return Err(LinkError::UndefinedSymbol(sym.name.clone()));
            } else {
                // Deduplicate unresolved `Extern` symbols in partial linking (`ld -r`):
                if let Some(ext_idx) = merged
                    .symbols
                    .iter()
                    .position(|s| s.kind == SymbolKind::Extern && s.name == sym.name)
                {
                    sym_remap[art_idx][sym_idx] = Sym(ext_idx as u32);
                } else {
                    let new_sym = merged.declare_import(&sym.name);
                    sym_remap[art_idx][sym_idx] = new_sym;
                }
            }
        }
    }

    // Pass 3: Merge and rebase all relocations:
    for (art_idx, art) in artifacts.iter().enumerate() {
        let bases = sec_bases[art_idx];
        for rel in &art.relocs {
            let rebased_off = rel.offset + bases[rel.section as usize];
            let mapped_sym = sym_remap[art_idx][rel.sym.index()];
            merged.add_reloc(Reloc {
                section: rel.section,
                offset: rebased_off,
                sym: mapped_sym,
                kind: rel.kind,
                addend: rel.addend,
            });
        }
    }

    Ok(merged)
}

/// Merges `artifacts`, resolves all cross-module symbols and relocations, and emits a
/// standalone executable binary (`ELF` `ET_EXEC` or Windows `PE32+` `.exe`) with entry point `entry_sym_name`.
pub fn link_executable(
    artifacts: &[Artifact],
    entry_sym_name: &str,
) -> Result<Vec<u8>, LinkError> {
    let merged = merge_artifacts(artifacts, true)?;

    // Verify entry point exists in `.text`:
    if !merged
        .symbols
        .iter()
        .any(|s| s.name == entry_sym_name && s.section == Some(SectionId::Text))
    {
        return Err(LinkError::MissingEntryPoint(String::from(entry_sym_name)));
    }

    match merged.target.obj_format {
        ObjFormat::Coff => emit_executable_pe(&merged, entry_sym_name).ok_or(LinkError::RelocationOverflow),
        _ => emit_executable_elf(&merged, entry_sym_name).ok_or(LinkError::RelocationOverflow),
    }
}

fn align_vec(buf: &mut Vec<u8>, align: usize, pad_byte: u8) {
    if align > 1 {
        let rem = buf.len() % align;
        if rem != 0 {
            buf.resize(buf.len() + (align - rem), pad_byte);
        }
    }
}
