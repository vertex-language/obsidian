//! Compiled binary artifact container (`Artifact`, `SymbolRecord`, `SymbolKind`).

use alloc::string::String;
use alloc::vec::Vec;
use obsidian_ir::{Linkage, Sym, Target};
use crate::reloc::{Reloc, SectionId};

/// Classification of a symbol inside an [`Artifact`].
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SymbolKind {
    /// Executable function in `.text`.
    Function = 0,
    /// Read-only or mutable data object in `.rodata`, `.data`, or `.bss`.
    Data     = 1,
    /// Thread-local storage object in `.tdata` or `.tbss`.
    Tls      = 2,
    /// Undefined external symbol imported from another object file or shared library.
    Extern   = 3,
}

/// A defined or imported symbol inside an [`Artifact`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolRecord {
    /// Unprefixed symbol name (e.g., `"max_pair_at"`, `"write"`).
    /// Note: Mach-O writers automatically prepend `'_'` (`"_max_pair_at"`) per Darwin ABI rules.
    pub name: String,
    pub kind: SymbolKind,
    pub linkage: Linkage,
    /// Section where this symbol is defined (`None` for `SymbolKind::Extern`).
    pub section: Option<SectionId>,
    /// Byte offset from the start of `section` (`0` for `Extern`).
    pub offset: u32,
    /// Size in bytes of the function or data object (`0` for `Extern`).
    pub size: u32,
    /// Required byte alignment of the symbol (`1`, `4`, `8`, `16`, etc.).
    pub align: u32,
}

/// Format-agnostic compiled module artifact produced by `obsidian-codegen-*` and consumed by
/// `obsidian-elf`, `obsidian-macho`, `obsidian-coff`, `obsidian-wasm-bin`, `obsidian-ld`, and `obsidian-jit`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    /// Module name (`"buffer_mod"`).
    pub name: String,
    /// Target architecture, OS, ABI, endianness, and object format.
    pub target: Target,
    /// Executable machine code bytes (`.text` / `__TEXT,__text`).
    pub text: Vec<u8>,
    /// Read-only constant bytes (`.rodata` / `__TEXT,__const` / `.rdata`).
    pub rodata: Vec<u8>,
    /// Initialized mutable global bytes (`.data` / `__DATA,__data`).
    pub data: Vec<u8>,
    /// Total byte size of zero-initialized globals (`.bss` / `__DATA,__bss`).
    pub bss_size: u64,
    /// Initialized thread-local storage bytes (`.tdata`).
    pub tdata: Vec<u8>,
    /// Total byte size of zero-initialized thread-local storage (`.tbss`).
    pub tbss_size: u64,
    /// Symbol table indexed by [`Sym`].
    pub symbols: Vec<SymbolRecord>,
    /// Relocations to be emitted to `.o` or resolved by `obsidian-ld` / `obsidian-jit`.
    pub relocs: Vec<Reloc>,
}

impl Artifact {
    /// Creates an empty [`Artifact`] for `target`.
    pub fn new(name: &str, target: Target) -> Self {
        Self {
            name: String::from(name),
            target,
            text: Vec::new(),
            rodata: Vec::new(),
            data: Vec::new(),
            bss_size: 0,
            tdata: Vec::new(),
            tbss_size: 0,
            symbols: Vec::new(),
            relocs: Vec::new(),
        }
    }

    /// Appends a compiled function's machine bytes to `.text` (aligned to `align` using `pad_byte`)
    /// and registers its [`SymbolRecord`].
    pub fn define_function(
        &mut self,
        name: &str,
        linkage: Linkage,
        align: u32,
        pad_byte: u8,
        code_bytes: &[u8],
    ) -> Sym {
        let align = align.max(1);
        align_vec(&mut self.text, align as usize, pad_byte);
        let offset = self.text.len() as u32;
        self.text.extend_from_slice(code_bytes);
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Function,
            linkage,
            section: Some(SectionId::Text),
            offset,
            size: code_bytes.len() as u32,
            align,
        });
        sym
    }

    /// Appends read-only constant bytes to `.rodata` and registers its [`SymbolRecord`].
    pub fn define_rodata(
        &mut self,
        name: &str,
        linkage: Linkage,
        align: u32,
        bytes: &[u8],
    ) -> Sym {
        let align = align.max(1);
        align_vec(&mut self.rodata, align as usize, 0);
        let offset = self.rodata.len() as u32;
        self.rodata.extend_from_slice(bytes);
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Data,
            linkage,
            section: Some(SectionId::Rodata),
            offset,
            size: bytes.len() as u32,
            align,
        });
        sym
    }

    /// Appends initialized mutable global bytes to `.data` (or reserves `.bss` if all zeros)
    /// and registers its [`SymbolRecord`].
    pub fn define_data(
        &mut self,
        name: &str,
        linkage: Linkage,
        align: u32,
        bytes: &[u8],
    ) -> Sym {
        let align = align.max(1);
        if bytes.iter().all(|&b| b == 0) {
            return self.define_bss(name, linkage, align, bytes.len() as u32);
        }
        align_vec(&mut self.data, align as usize, 0);
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(bytes);
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Data,
            linkage,
            section: Some(SectionId::Data),
            offset,
            size: bytes.len() as u32,
            align,
        });
        sym
    }

    /// Reserves `size` zero-initialized bytes in `.bss` and registers its [`SymbolRecord`].
    pub fn define_bss(
        &mut self,
        name: &str,
        linkage: Linkage,
        align: u32,
        size: u32,
    ) -> Sym {
        let align_u64 = (align.max(1)) as u64;
        self.bss_size = (self.bss_size + align_u64 - 1) & !(align_u64 - 1);
        let offset = self.bss_size as u32;
        self.bss_size += size as u64;
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Data,
            linkage,
            section: Some(SectionId::Bss),
            offset,
            size,
            align: align.max(1),
        });
        sym
    }

    /// Appends thread-local data to `.tdata` and registers its [`SymbolRecord`].
    pub fn define_tls_data(
        &mut self,
        name: &str,
        linkage: Linkage,
        align: u32,
        bytes: &[u8],
    ) -> Sym {
        let align = align.max(1);
        align_vec(&mut self.tdata, align as usize, 0);
        let offset = self.tdata.len() as u32;
        self.tdata.extend_from_slice(bytes);
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Tls,
            linkage,
            section: Some(SectionId::Tdata),
            offset,
            size: bytes.len() as u32,
            align,
        });
        sym
    }

    /// Declares an undefined external symbol (`SymbolKind::Extern`, `Linkage::Extern`).
    pub fn declare_import(&mut self, name: &str) -> Sym {
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push(SymbolRecord {
            name: String::from(name),
            kind: SymbolKind::Extern,
            linkage: Linkage::Extern,
            section: None,
            offset: 0,
            size: 0,
            align: 1,
        });
        sym
    }

    /// Records a relocation (`Reloc`) inside this artifact.
    #[inline]
    pub fn add_reloc(&mut self, reloc: Reloc) {
        self.relocs.push(reloc);
    }

    /// Returns the byte slice of `sec` (`&[]` for `Bss` / `Tbss`).
    #[inline]
    pub fn section_bytes(&self, sec: SectionId) -> &[u8] {
        match sec {
            SectionId::Text   => &self.text,
            SectionId::Rodata => &self.rodata,
            SectionId::Data   => &self.data,
            SectionId::Tdata  => &self.tdata,
            SectionId::Bss | SectionId::Tbss => &[],
        }
    }

    /// Returns a mutable byte slice for `sec` (`None` for zero-backed `Bss` / `Tbss`).
    #[inline]
    pub fn section_bytes_mut(&mut self, sec: SectionId) -> Option<&mut [u8]> {
        match sec {
            SectionId::Text   => Some(&mut self.text),
            SectionId::Rodata => Some(&mut self.rodata),
            SectionId::Data   => Some(&mut self.data),
            SectionId::Tdata  => Some(&mut self.tdata),
            SectionId::Bss | SectionId::Tbss => None,
        }
    }

    /// Resolves all internal (same-artifact defined) relocations given the base virtual address
    /// assigned to each section (`section_vaddrs[sec as usize]`).
    ///
    /// Used by `obsidian-ld` and `obsidian-jit` to link and patch code in place.
    pub fn resolve_internal_relocs(&mut self, section_vaddrs: &[u64; 6]) -> bool {
        let is_be = self.target.is_big_endian();
        for i in 0..self.relocs.len() {
            let rel = self.relocs[i];
            let sym_rec = &self.symbols[rel.sym.index()];
            let Some(target_sec) = sym_rec.section else {
                continue; // External symbol; resolved by dynamic/external linker
            };
            let target_vaddr = section_vaddrs[target_sec as usize] + (sym_rec.offset as u64);
            let patch_vaddr = section_vaddrs[rel.section as usize] + (rel.offset as u64);
            if let Some(buf) = self.section_bytes_mut(rel.section) {
                if !rel.kind.apply_in_place(
                    buf,
                    rel.offset as usize,
                    patch_vaddr,
                    target_vaddr,
                    rel.addend,
                    is_be,
                ) {
                    return false;
                }
            }
        }
        true
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
