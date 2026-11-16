//! Zero-heap function, symbol, branch, and source-location metadata (`ir::FnAttrs`, `ir::SymAttrs`, `ir::SourceLoc`).

use alloc::string::String;

/// Function inlining directive (`inline(hint)`, `inline(always)`, `inline(never)`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum InlineAttr {
    #[default]
    Auto   = 0,
    Hint   = 1,
    Always = 2,
    Never  = 3,
}

impl InlineAttr {
    pub const fn as_obs_str(self) -> Option<&'static str> {
        match self {
            Self::Auto   => None,
            Self::Hint   => Some("inline(hint)"),
            Self::Always => Some("inline(always)"),
            Self::Never  => Some("inline(never)"),
        }
    }
}

/// Branch prediction and block-layout hint stored in `InstNode::subop` of `Opcode::BrIf`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum BranchHint {
    #[default]
    Neutral      = 0,
    /// `br_if.likely`: `<then_bb>` is the hot path.
    LikelyThen   = 1,
    /// `br_if.unlikely`: `<else_bb>` is the hot fallthrough path.
    UnlikelyThen = 2,
}

impl BranchHint {
    #[inline]
    pub const fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::LikelyThen,
            2 => Self::UnlikelyThen,
            _ => Self::Neutral,
        }
    }

    pub const fn suffix_str(self) -> &'static str {
        match self {
            Self::Neutral      => "",
            Self::LikelyThen   => ".likely",
            Self::UnlikelyThen => ".unlikely",
        }
    }
}

/// Compact attributes attached directly to an [`ir::Function`](crate::Function).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FnAttrs {
    pub inline: InlineAttr,
    pub cold: bool,
    pub noreturn: bool,
    pub naked: bool,
    /// Required function entry alignment in bytes (`0` = default target alignment).
    pub align: u16,
    /// Optional custom linker section name (e.g. `".text.boot"`).
    pub section: Option<String>,
}

/// Compact attributes attached to module-level globals and constants (`ConstDef`, `GlobalDef`, `TlsDef`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SymAttrs {
    /// Required byte alignment (power of two; `1` = unaligned).
    pub align: u32,
    /// Optional custom linker section name (e.g. `".boot.rodata"`).
    pub section: Option<String>,
    /// Prevents the linker from garbage-collecting this symbol (`#[used]`).
    pub used: bool,
}

/// Compact 8-byte source location (`@loc(file_id, line, col)`).
///
/// Stored in an optional parallel `Vec<SourceLoc>` on [`ir::Function`](crate::Function)
/// indexed by `Value(i)`, adding zero bytes to `InstNode`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SourceLoc {
    pub line: u32,
    pub file_id: u16,
    pub col: u16,
}

impl SourceLoc {
    pub const NONE: Self = Self {
        line: 0,
        file_id: 0,
        col: 0,
    };

    #[inline]
    pub const fn new(file_id: u16, line: u32, col: u16) -> Self {
        Self { line, file_id, col }
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        self.line != 0
    }
}
