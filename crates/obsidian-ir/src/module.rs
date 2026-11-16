//! Top-level `.obs` compilation unit (`ir::Module`) holding `Target`, constants, globals, TLS, externs, and functions.

use alloc::string::String;
use alloc::vec::Vec;
use crate::func::Function;
use crate::meta::SymAttrs;
use crate::node::Sym;
use crate::target::Target;
use crate::types::{CallConv, Type};

/// Read-only constant data declaration (`const @NAME: [N x i8] = ..., align=M`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstDef {
    pub name: String,
    pub bytes: Vec<u8>,
    pub attrs: SymAttrs,
}

/// Mutable global variable declaration (`global @NAME: <ty> = <init>, align=M`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalDef {
    pub name: String,
    pub ty: Type,
    pub init_bits: u64,
    pub attrs: SymAttrs,
}

/// Thread-local storage variable declaration (`tls @NAME: <ty> = <init>, align=M`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TlsDef {
    pub name: String,
    pub ty: Type,
    pub init_bits: u64,
    pub attrs: SymAttrs,
}

/// External function import declaration (`extern cc(c) fn @name(...) -> ...`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternFn {
    pub name: String,
    pub call_conv: CallConv,
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
    pub is_variadic: bool,
}

/// Discriminates which module table a [`Sym`] handle refers to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SymKind {
    Const(usize),
    Global(usize),
    Tls(usize),
    ExternFn(usize),
    Function(usize),
}

/// Top-level `.obs` module (`module @name` + `target ...` + declarations + functions).
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub target: Target,
    pub consts: Vec<ConstDef>,
    pub globals: Vec<GlobalDef>,
    pub tls_vars: Vec<TlsDef>,
    pub extern_fns: Vec<ExternFn>,
    pub functions: Vec<Function>,
    pub symbols: Vec<(String, SymKind)>,
}

impl Module {
    /// Creates a new `.obs` module targeting `target`.
    pub fn new(name: &str, target: Target) -> Self {
        Self {
            name: String::from(name),
            target,
            consts: Vec::new(),
            globals: Vec::new(),
            tls_vars: Vec::new(),
            extern_fns: Vec::new(),
            functions: Vec::new(),
            symbols: Vec::new(),
        }
    }

    /// Declares a read-only byte array constant (`const @name: [N x i8] = ...`) and returns its [`Sym`].
    pub fn add_const(&mut self, name: &str, bytes: &[u8], align: u32) -> Sym {
        let idx = self.consts.len();
        self.consts.push(ConstDef {
            name: String::from(name),
            bytes: bytes.to_vec(),
            attrs: SymAttrs {
                align,
                section: None,
                used: false,
            },
        });
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push((String::from(name), SymKind::Const(idx)));
        sym
    }

    /// Declares a mutable 64-bit integer global variable (`global @name: i64 = init`).
    pub fn add_global_i64(&mut self, name: &str, init: i64, align: u32) -> Sym {
        let idx = self.globals.len();
        self.globals.push(GlobalDef {
            name: String::from(name),
            ty: Type::I64,
            init_bits: init as u64,
            attrs: SymAttrs {
                align,
                section: None,
                used: false,
            },
        });
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push((String::from(name), SymKind::Global(idx)));
        sym
    }

    /// Declares a thread-local 32-bit integer variable (`tls @name: i32 = init`).
    pub fn add_tls_i32(&mut self, name: &str, init: i32, align: u32) -> Sym {
        let idx = self.tls_vars.len();
        self.tls_vars.push(TlsDef {
            name: String::from(name),
            ty: Type::I32,
            init_bits: (init as u32) as u64,
            attrs: SymAttrs {
                align,
                section: None,
                used: false,
            },
        });
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push((String::from(name), SymKind::Tls(idx)));
        sym
    }

    /// Declares an external function import (`extern fn @name(...)`) and returns its [`Sym`].
    pub fn add_extern_fn(
        &mut self,
        name: &str,
        call_conv: CallConv,
        params: &[Type],
        returns: &[Type],
        is_variadic: bool,
    ) -> Sym {
        let idx = self.extern_fns.len();
        self.extern_fns.push(ExternFn {
            name: String::from(name),
            call_conv,
            params: params.to_vec(),
            returns: returns.to_vec(),
            is_variadic,
        });
        let sym = Sym(self.symbols.len() as u32);
        self.symbols
            .push((String::from(name), SymKind::ExternFn(idx)));
        sym
    }

    /// Adds a defined [`Function`] to the module and returns its [`Sym`].
    pub fn add_function(&mut self, func: Function) -> Sym {
        let name = func.name.clone();
        let idx = self.functions.len();
        self.functions.push(func);
        let sym = Sym(self.symbols.len() as u32);
        self.symbols.push((name, SymKind::Function(idx)));
        sym
    }

    /// Returns the symbol name (`without @`) for `sym`.
    #[inline]
    pub fn symbol_name(&self, sym: Sym) -> &str {
        &self.symbols[sym.index()].0
    }
}
