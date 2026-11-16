//! Human-readable `.obs` text formatter (`core::fmt::Display` for [`Function`] and [`Module`]).

use core::fmt::{self, Write};
use crate::func::Function;
use crate::meta::BranchHint;
use crate::module::Module;
use crate::node::{Sym, Value};
use crate::opcode::{IntCC, LoadExtKind, Opcode, TrapReason};
use crate::types::Type;

fn fmt_return_types(f: &mut fmt::Formatter<'_>, returns: &[Type]) -> fmt::Result {
    match returns.len() {
        0 => Ok(()),
        1 => write!(f, " -> {}", returns[0].as_str()),
        _ => {
            f.write_str(" -> (")?;
            for (i, ty) in returns.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                f.write_str(ty.as_str())?;
            }
            f.write_char(')')
        }
    }
}

fn fmt_block_args(f: &mut fmt::Formatter<'_>, args: &[Value]) -> fmt::Result {
    if args.is_empty() {
        return Ok(());
    }
    f.write_char('(')?;
    for (i, v) in args.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "%v{}", v.0)?;
    }
    f.write_char(')')
}

impl Function {
    /// Formats this function into `.obs` text, optionally resolving `@symbol` names via `module`.
    pub fn fmt_with_module(&self, f: &mut fmt::Formatter<'_>, module: Option<&Module>) -> fmt::Result {
        f.write_str(self.linkage.as_str())?;
        if let Some(cc) = self.call_conv.as_obs_str() {
            write!(f, " {cc}")?;
        }
        if let Some(inl) = self.attrs.inline.as_obs_str() {
            write!(f, " {inl}")?;
        }
        if self.attrs.cold {
            f.write_str(" cold")?;
        }
        if self.attrs.noreturn {
            f.write_str(" noreturn")?;
        }
        if self.attrs.align > 0 {
            write!(f, " align({})", self.attrs.align)?;
        }
        write!(f, " fn @{}(", self.name)?;
        let entry_params = &self.blocks[0].params;
        for (i, (&val, &ty)) in entry_params.iter().zip(self.param_types.iter()).enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "%v{}: {}", val.0, ty.as_str())?;
        }
        f.write_char(')')?;
        fmt_return_types(f, &self.return_types)?;
        f.write_str(" {\n")?;

        for (b_idx, block) in self.blocks.iter().enumerate() {
            if b_idx > 0 {
                f.write_char('\n')?;
            }
            f.write_str(&block.name)?;
            if b_idx > 0 && !block.params.is_empty() {
                f.write_char('(')?;
                for (i, &p) in block.params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    let ty = self.insts[p.index()].ty;
                    write!(f, "%v{}: {}", p.0, ty.as_str())?;
                }
                f.write_char(')')?;
            }
            f.write_str(":\n")?;

            for &inst_val in &block.insts {
                let node = self.insts[inst_val.index()];
                if node.op == Opcode::Tombstone || node.op == Opcode::ResultProj {
                    continue;
                }
                f.write_str("    ")?;

                // Left-hand side assignment (`%vN = ` or `%vN, %vM = `):
                if node.ty != Type::Void {
                    if node.has_second_result() {
                        write!(f, "%v{}, %v{} = ", inst_val.0, inst_val.0 + 1)?;
                    } else {
                        write!(f, "%v{} = ", inst_val.0)?;
                    }
                }

                match node.op {
                    Opcode::Const => {
                        let bits = self.const_bits(inst_val).unwrap_or(0);
                        write!(f, "{}.const {}", node.ty.as_str(), bits as i64)?;
                    }
                    Opcode::PtrSym => {
                        let sym = Sym(node.args[0]);
                        let offset = node.args[1] as i32;
                        if let Some(m) = module {
                            write!(f, "ptr.sym @{}", m.symbol_name(sym))?;
                        } else {
                            write!(f, "ptr.sym @sym{}", sym.0)?;
                        }
                        if offset != 0 {
                            write!(f, ", offset={offset}")?;
                        }
                    }
                    Opcode::PtrAdd => {
                        if node.has_inline_imm1() {
                            write!(f, "ptr.add %v{}, {}", node.args[0], node.inline_imm1())?;
                        } else {
                            write!(f, "ptr.add %v{}, %v{}", node.args[0], node.args[1])?;
                        }
                    }
                    Opcode::StackSlot => {
                        write!(f, "stack.slot size={}, align={}", node.args[0], node.args[1])?;
                    }
                    Opcode::ICmp => {
                        let cc = IntCC::from_raw(node.subop).unwrap_or(IntCC::Eq);
                        let lhs_ty = self.insts[node.args[0] as usize].ty;
                        if node.has_inline_imm1() {
                            write!(
                                f,
                                "{}.cmp.{} %v{}, {}",
                                lhs_ty.as_str(),
                                cc.as_str(),
                                node.args[0],
                                node.inline_imm1()
                            )?;
                        } else {
                            write!(
                                f,
                                "{}.cmp.{} %v{}, %v{}",
                                lhs_ty.as_str(),
                                cc.as_str(),
                                node.args[0],
                                node.args[1]
                            )?;
                        }
                    }
                    Opcode::Select => {
                        write!(
                            f,
                            "{}.select %v{}, %v{}, %v{}",
                            node.ty.as_str(),
                            node.aux16 as u16,
                            node.args[0],
                            node.args[1]
                        )?;
                    }
                    Opcode::Load => {
                        write!(f, "{}.load %v{}", node.ty.as_str(), node.args[0])?;
                        if node.aux16 != 0 {
                            write!(f, ", offset={}", node.aux16)?;
                        }
                    }
                    Opcode::LoadExt => {
                        let ext = match node.subop {
                            0 => LoadExtKind::U8,
                            1 => LoadExtKind::S8,
                            2 => LoadExtKind::U16,
                            3 => LoadExtKind::S16,
                            4 => LoadExtKind::U32,
                            _ => LoadExtKind::S32,
                        };
                        write!(
                            f,
                            "{}.load.{} %v{}",
                            node.ty.as_str(),
                            ext.as_str(),
                            node.args[0]
                        )?;
                        if node.aux16 != 0 {
                            write!(f, ", offset={}", node.aux16)?;
                        }
                    }
                    Opcode::Store => {
                        let val_ty = self.insts[node.args[1] as usize].ty;
                        write!(
                            f,
                            "{}.store %v{}, %v{}",
                            val_ty.as_str(),
                            node.args[0],
                            node.args[1]
                        )?;
                        if node.aux16 != 0 {
                            write!(f, ", offset={}", node.aux16)?;
                        }
                    }
                    Opcode::Call => {
                        let sym = Sym(node.args[0]);
                        let args = &self.value_lists[node.args[1] as usize];
                        if let Some(m) = module {
                            write!(f, "call @{}", m.symbol_name(sym))?;
                        } else {
                            write!(f, "call @sym{}", sym.0)?;
                        }
                        fmt_block_args(f, args)?;
                    }
                    Opcode::Br => {
                        let target_bb = &self.blocks[node.args[0] as usize];
                        let args = &self.value_lists[node.args[1] as usize];
                        write!(f, "br {}", target_bb.name)?;
                        fmt_block_args(f, args)?;
                    }
                    Opcode::BrIf => {
                        let hint = BranchHint::from_raw(node.subop);
                        let targets = &self.br_if_targets[node.args[1] as usize];
                        let then_name = &self.blocks[targets.then_bb.index()].name;
                        let else_name = &self.blocks[targets.else_bb.index()].name;
                        write!(f, "br_if{} %v{}, {}", hint.suffix_str(), node.args[0], then_name)?;
                        fmt_block_args(f, &targets.then_args)?;
                        write!(f, ", {else_name}")?;
                        fmt_block_args(f, &targets.else_args)?;
                    }
                    Opcode::Ret => {
                        let vals = &self.value_lists[node.args[0] as usize];
                        f.write_str("ret")?;
                        for (i, v) in vals.iter().enumerate() {
                            if i == 0 {
                                write!(f, " %v{}", v.0)?;
                            } else {
                                write!(f, ", %v{}", v.0)?;
                            }
                        }
                    }
                    Opcode::Trap => {
                        let reason = match node.subop {
                            0 => TrapReason::Unreachable,
                            1 => TrapReason::BoundsCheck,
                            2 => TrapReason::IntOverflow,
                            3 => TrapReason::DivByZero,
                            _ => TrapReason::Abort,
                        };
                        write!(f, "trap {}", reason.as_str())?;
                    }
                    Opcode::Clz | Opcode::Ctz | Opcode::Popcnt | Opcode::ByteSwap | Opcode::BitReverse | Opcode::Neg | Opcode::Not => {
                        write!(f, "{}.{} %v{}", node.ty.as_str(), node.op.op_name(), node.args[0])?;
                    }
                    _ => {
                        if node.has_inline_imm1() {
                            write!(
                                f,
                                "{}.{} %v{}, {}",
                                node.ty.as_str(),
                                node.op.op_name(),
                                node.args[0],
                                node.inline_imm1()
                            )?;
                        } else {
                            write!(
                                f,
                                "{}.{} %v{}, %v{}",
                                node.ty.as_str(),
                                node.op.op_name(),
                                node.args[0],
                                node.args[1]
                            )?;
                        }
                    }
                }
                f.write_char('\n')?;
            }
        }
        f.write_str("}\n")
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_with_module(f, None)
    }
}

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "module @{}", self.name)?;
        writeln!(
            f,
            "target arch={}, os={}, abi={}",
            self.target.arch.as_str(),
            self.target.os.as_str(),
            self.target.abi.as_str()
        )?;

        if !self.consts.is_empty() || !self.globals.is_empty() || !self.tls_vars.is_empty() || !self.extern_fns.is_empty() {
            f.write_char('\n')?;
        }

        for c in &self.consts {
            writeln!(
                f,
                "const @{}: [{} x i8], align={}",
                c.name,
                c.bytes.len(),
                c.attrs.align
            )?;
        }
        for g in &self.globals {
            writeln!(
                f,
                "global @{}: {} = {}, align={}",
                g.name,
                g.ty.as_str(),
                g.init_bits as i64,
                g.attrs.align
            )?;
        }
        for t in &self.tls_vars {
            writeln!(
                f,
                "tls @{}: {} = {}, align={}",
                t.name,
                t.ty.as_str(),
                t.init_bits as i64,
                t.attrs.align
            )?;
        }
        for ext in &self.extern_fns {
            f.write_str("extern")?;
            if let Some(cc) = ext.call_conv.as_obs_str() {
                write!(f, " {cc}")?;
            }
            write!(f, " fn @{}(", ext.name)?;
            for (i, ty) in ext.params.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "%p{i}: {}", ty.as_str())?;
            }
            if ext.is_variadic {
                if !ext.params.is_empty() {
                    f.write_str(", ")?;
                }
                f.write_str("...")?;
            }
            f.write_char(')')?;
            fmt_return_types(f, &ext.returns)?;
            f.write_char('\n')?;
        }

        for func in &self.functions {
            f.write_char('\n')?;
            func.fmt_with_module(f, Some(self))?;
        }
        Ok(())
    }
}
