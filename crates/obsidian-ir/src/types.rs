//! The 10 `.obs` Bedrock types (`ir::Type`), symbol linkage (`ir::Linkage`), and calling conventions (`ir::CallConv`).

/// The 10 built-in `.obs` Bedrock primitive types (plus `Void` for non-value instructions).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Type {
    /// No output value (`store`, `br`, `br_if`, `ret` with 0 values).
    Void = 0,
    /// `i1`: 1-bit boolean condition (`*.cmp.*`, `.ovf_*`, `.cas`).
    I1   = 1,
    /// `i8`: 8-bit signless integer.
    I8   = 2,
    /// `i16`: 16-bit signless integer.
    I16  = 3,
    /// `i32`: 32-bit signless integer.
    I32  = 4,
    /// `i64`: 64-bit signless integer.
    I64  = 5,
    /// `i128`: 128-bit signless integer.
    I128 = 6,
    /// `f32`: IEEE-754 binary32 single-precision float.
    F32  = 7,
    /// `f64`: IEEE-754 binary64 double-precision float.
    F64  = 8,
    /// `ptr`: Target-sized memory address.
    Ptr  = 9,
    /// `v128`: 128-bit SIMD vector register.
    V128 = 10,
}

impl Type {
    /// Returns the human-readable `.obs` type name (`"i64"`, `"ptr"`, `"v128"`, etc.).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::I1   => "i1",
            Self::I8   => "i8",
            Self::I16  => "i16",
            Self::I32  => "i32",
            Self::I64  => "i64",
            Self::I128 => "i128",
            Self::F32  => "f32",
            Self::F64  => "f64",
            Self::Ptr  => "ptr",
            Self::V128 => "v128",
        }
    }

    #[inline]
    pub const fn is_int(self) -> bool {
        matches!(
            self,
            Self::I1 | Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128
        )
    }

    #[inline]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// Returns the bit width for fixed-size types (`None` for `Void` or target-dependent `Ptr`).
    #[inline]
    pub const fn fixed_bits(self) -> Option<u16> {
        match self {
            Self::Void => None,
            Self::I1   => Some(1),
            Self::I8   => Some(8),
            Self::I16  => Some(16),
            Self::I32 | Self::F32 => Some(32),
            Self::I64 | Self::F64 => Some(64),
            Self::I128 | Self::V128 => Some(128),
            Self::Ptr  => None,
        }
    }
}

/// 128-bit SIMD lane interpretation (`i8x16`, `i16x8`, `i32x4`, `i64x2`, `f32x4`, `f64x2`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SimdLane {
    I8x16 = 0,
    I16x8 = 1,
    I32x4 = 2,
    I64x2 = 3,
    F32x4 = 4,
    F64x2 = 5,
}

impl SimdLane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I8x16 => "i8x16",
            Self::I16x8 => "i16x8",
            Self::I32x4 => "i32x4",
            Self::I64x2 => "i64x2",
            Self::F32x4 => "f32x4",
            Self::F64x2 => "f64x2",
        }
    }
}

/// Symbol linkage in `.obs` (`local`, `export`, or `extern`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Linkage {
    /// `local`: Internal to this module; eligible for inlining and dead-stripping.
    Local  = 0,
    /// `export`: Publicly visible symbol in the object file / binary.
    Export = 1,
    /// `extern`: Imported from another object file or shared library.
    Extern = 2,
}

impl Linkage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local  => "local",
            Self::Export => "export",
            Self::Extern => "extern",
        }
    }
}

/// Calling convention for functions and call instructions.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum CallConv {
    /// Default Obsidian fast register convention.
    #[default]
    Fast     = 0,
    /// Platform C ABI (`SysV64`, `AAPCS64`, `Win64`, etc.).
    C        = 1,
    /// Cold path (callee preserves most registers to keep hot caller register pressure low).
    Cold     = 2,
    /// Preserve-all (used for runtime hooks / profiling).
    Preserve = 3,
}

impl CallConv {
    pub const fn as_obs_str(self) -> Option<&'static str> {
        match self {
            Self::Fast     => None,
            Self::C        => Some("cc(c)"),
            Self::Cold     => Some("cc(cold)"),
            Self::Preserve => Some("cc(preserve)"),
        }
    }
}
