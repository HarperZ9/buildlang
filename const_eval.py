"""Constant initializers: evaluate operators, casts and parentheses; refuse what cannot be evaluated."""
import io

N = "\r\n"


def crlf(t):
    return t.replace("\r\n", "\n").replace("\n", N)


p = "compiler/src/codegen/lower/types.rs"
s = io.open(p, newline="").read()
old = crlf(
    """            ExprKind::Ident(id) => self.const_values.get(&id.name).cloned(),
            ExprKind::Path(p) => p
                .last_ident()
                .and_then(|i| self.const_values.get(&i.name).cloned()),
            _ => None,
        }
    }
"""
)
new = crlf(
    """            ExprKind::Ident(id) => self.const_values.get(&id.name).cloned(),
            ExprKind::Path(p) => p
                .last_ident()
                .and_then(|i| self.const_values.get(&i.name).cloned()),
            ExprKind::Paren(inner) => self.try_const_eval(inner),
            ExprKind::Unary {
                op: ast::UnaryOp::Not,
                expr: inner,
            } => match self.try_const_eval(inner)? {
                MirConst::Bool(b) => Some(MirConst::Bool(!b)),
                _ => None,
            },
            ExprKind::Cast { expr: inner, ty } => {
                let value = self.try_const_eval(inner)?;
                Self::const_cast(value, &self.lower_type_from_ast(ty))
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.try_const_eval(left)?;
                let r = self.try_const_eval(right)?;
                Self::const_binary(*op, l, r)
            }
            _ => None,
        }
    }

    /// The integer held by a constant, with its type.
    fn const_int(c: &MirConst) -> Option<(i128, MirType)> {
        match c {
            MirConst::Int(v, t) => Some((*v, t.clone())),
            MirConst::Uint(v, t) => Some((i128::try_from(*v).ok()?, t.clone())),
            _ => None,
        }
    }

    /// `value as ty` at compile time, for number types. An integer is wrapped
    /// to the width of `ty`, as the cast does at run time.
    fn const_cast(value: MirConst, ty: &MirType) -> Option<MirConst> {
        match ty {
            MirType::Float(_) => match &value {
                MirConst::Float(v, _) => Some(MirConst::Float(*v, ty.clone())),
                _ => {
                    let (v, _) = Self::const_int(&value)?;
                    Some(MirConst::Float(v as f64, ty.clone()))
                }
            },
            MirType::Int(size, signed) => {
                let v = match &value {
                    MirConst::Float(f, _) => *f as i128,
                    MirConst::Bool(b) => i128::from(*b),
                    other => Self::const_int(other)?.0,
                };
                let bits: u32 = match size {
                    IntSize::I8 => 8,
                    IntSize::I16 => 16,
                    IntSize::I32 => 32,
                    IntSize::I64 | IntSize::ISize => 64,
                    IntSize::I128 => return Some(MirConst::Int(v, ty.clone())),
                };
                let low = v & ((1i128 << bits) - 1);
                let wrapped = if *signed && low >= (1i128 << (bits - 1)) {
                    low - (1i128 << bits)
                } else {
                    low
                };
                Some(MirConst::Int(wrapped, ty.clone()))
            }
            _ => None,
        }
    }

    /// `l op r` at compile time. Two integers, two floats or two booleans; an
    /// overflow, a division by zero or a mixed pair gives `None`, and the
    /// caller reports the initializer.
    fn const_binary(op: ast::BinOp, l: MirConst, r: MirConst) -> Option<MirConst> {
        use ast::BinOp as B;
        if let (MirConst::Bool(a), MirConst::Bool(b)) = (&l, &r) {
            return match op {
                B::And => Some(MirConst::Bool(*a && *b)),
                B::Or => Some(MirConst::Bool(*a || *b)),
                B::Eq => Some(MirConst::Bool(a == b)),
                B::Ne => Some(MirConst::Bool(a != b)),
                _ => None,
            };
        }
        if let (MirConst::Float(a, ty), MirConst::Float(b, _)) = (&l, &r) {
            let (a, b) = (*a, *b);
            return match op {
                B::Add => Some(MirConst::Float(a + b, ty.clone())),
                B::Sub => Some(MirConst::Float(a - b, ty.clone())),
                B::Mul => Some(MirConst::Float(a * b, ty.clone())),
                B::Div => Some(MirConst::Float(a / b, ty.clone())),
                B::Eq => Some(MirConst::Bool(a == b)),
                B::Ne => Some(MirConst::Bool(a != b)),
                B::Lt => Some(MirConst::Bool(a < b)),
                B::Le => Some(MirConst::Bool(a <= b)),
                B::Gt => Some(MirConst::Bool(a > b)),
                B::Ge => Some(MirConst::Bool(a >= b)),
                _ => None,
            };
        }
        let (a, ta) = Self::const_int(&l)?;
        let (b, tb) = Self::const_int(&r)?;
        // A literal has no type of its own yet (`i32` stands for that); the
        // other side's type is the result's.
        let ty = if ta == MirType::i32() { tb } else { ta };
        let int = |v: Option<i128>| v.map(|v| MirConst::Int(v, ty.clone()));
        match op {
            B::Add => int(a.checked_add(b)),
            B::Sub => int(a.checked_sub(b)),
            B::Mul => int(a.checked_mul(b)),
            B::Div => int(a.checked_div(b)),
            B::Rem => int(a.checked_rem(b)),
            B::BitAnd => int(Some(a & b)),
            B::BitOr => int(Some(a | b)),
            B::BitXor => int(Some(a ^ b)),
            B::Shl => int(u32::try_from(b).ok().filter(|s| *s < 127).and_then(|s| a.checked_shl(s))),
            B::Shr => int(u32::try_from(b).ok().filter(|s| *s < 127).and_then(|s| a.checked_shr(s))),
            B::Eq => Some(MirConst::Bool(a == b)),
            B::Ne => Some(MirConst::Bool(a != b)),
            B::Lt => Some(MirConst::Bool(a < b)),
            B::Le => Some(MirConst::Bool(a <= b)),
            B::Gt => Some(MirConst::Bool(a > b)),
            B::Ge => Some(MirConst::Bool(a >= b)),
            _ => None,
        }
    }
"""
)
assert s.count(old) == 1
s = s.replace(old, new)
io.open(p, "w", newline="").write(s)

p = "compiler/src/codegen/lower/global_init.rs"
s = io.open(p, newline="").read()
nl = "\r\n" if "\r\n" in s else "\n"
old = """        if !is_aggregate(ty) {
            return Ok(self.try_const_eval(expr));
        }
""".replace("\n", nl)
new = """        if !is_aggregate(ty) {
            return self.scalar_initializer(name, expr, ty);
        }
""".replace("\n", nl)
assert s.count(old) == 1
s = s.replace(old, new)
old = """    /// `expr` as a constant of type `ty`, or `None` when it is not one.
    fn const_eval_typed(""".replace("\n", nl)
new = """    /// The initializer of a global that is not an aggregate. A number or a
    /// boolean gets its value or a compile error: with no constant the C global
    /// was zero, so `const N: i32 = 3 + 4;` read as 0 with no diagnostic. Other
    /// types keep the earlier evaluation.
    fn scalar_initializer(
        &self,
        name: &str,
        expr: &ast::Expr,
        ty: &MirType,
    ) -> CodegenResult<Option<MirConst>> {
        let value = self.try_const_eval(expr);
        let refuse = |why: &str| {
            let span = expr.span;
            Err(CodegenError::Rejected {
                message: format!("the initializer of `{name}` {why}"),
                location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
                help: Some(
                    "use literals, other constants, arithmetic, comparisons and `as` casts between number types"
                        .to_string(),
                ),
            })
        };
        match ty {
            MirType::Bool => match value {
                Some(MirConst::Bool(_)) => Ok(value),
                _ => refuse("is not a boolean the compiler can evaluate"),
            },
            MirType::Float(_) => match value {
                Some(MirConst::Float(v, _)) => Ok(Some(MirConst::Float(v, ty.clone()))),
                Some(MirConst::Int(v, _)) => Ok(Some(MirConst::Float(v as f64, ty.clone()))),
                Some(MirConst::Uint(v, _)) => Ok(Some(MirConst::Float(v as f64, ty.clone()))),
                _ => refuse("is not a number the compiler can evaluate"),
            },
            MirType::Int(..) => match value {
                Some(MirConst::Int(..)) | Some(MirConst::Uint(..)) => Ok(value),
                _ => refuse("is not an integer the compiler can evaluate"),
            },
            _ => Ok(value),
        }
    }

    /// `expr` as a constant of type `ty`, or `None` when it is not one.
    fn const_eval_typed(""".replace("\n", nl)
assert s.count(old) == 1
s = s.replace(old, new)
io.open(p, "w", newline="").write(s)
print("applied")
