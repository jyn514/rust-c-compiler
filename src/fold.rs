use crate::arch::CHAR_BIT;
use crate::data::prelude::*;
use std::ops::{Add, Div, Mul, Sub};
use Literal::*;

macro_rules! fold_int_bin_op {
    ($op: tt) => {
        |a: &Literal, b: &Literal, _| match (a, b) {
            (Int(a), Int(b)) => Ok(Some(Int(a $op b))),
            (UnsignedInt(a), UnsignedInt(b)) => Ok(Some(UnsignedInt(a $op b))),
            (Char(a), Char(b)) => Ok(Some(Char(a $op b))),
            (_, _) => Ok(None),
        }
    }
}

#[inline]
fn fold_scalar_bin_op(
    simple: fn(f64, f64) -> f64,
    overflowing: fn(i64, i64) -> (i64, bool),
    wrapping: fn(u64, u64) -> u64,
) -> impl Fn(&Literal, &Literal, &Type) -> Result<Option<Literal>, SemanticError> {
    move |a: &Literal, b: &Literal, _ctype| match (a, b) {
        (Int(a), Int(b)) => {
            // overflowing returns the wrapped value, so if we had a negative
            // value, it would be a positive overflow.
            let (value, overflowed) = overflowing(*a, *b);
            if overflowed {
                Err(SemanticError::ConstOverflow {
                    is_positive: value.is_negative(),
                })
            } else {
                Ok(Some(Int(value)))
            }
        }
        (UnsignedInt(a), UnsignedInt(b)) => Ok(Some(UnsignedInt(wrapping(*a, *b)))),
        (Float(a), Float(b)) => Ok(Some(Float(simple(*a, *b)))),
        // TODO: find a way to do this that allows `"hello" + 2 - 1`
        //(Str(s), Int(i)) | (Int(i), Str(s)) => {
        (_, _) => Ok(None),
    }
}

macro_rules! fold_compare_op {
($left: expr, $right: expr, $constructor: ident, $op: tt, $compare: expr) => {{
        let (left, right) = ($left.const_fold()?, $right.const_fold()?);
        match (&left.expr, &right.expr) {
            (ExprType::Literal(a), ExprType::Literal(b)) => {
                match (a, b) {
                    (Int(a), Int(b)) => ExprType::Literal(Int((a $op b) as i64)),
                    (UnsignedInt(a), UnsignedInt(b)) => ExprType::Literal(Int((a $op b) as i64)),
                    #[allow(clippy::float_cmp)]
                    (Float(a), Float(b)) => ExprType::Literal(Int((a $op b) as i64)),
                    (Char(a), Char(b)) => ExprType::Literal(Int((a $op b) as i64)),
                    (_, _) => ExprType::$constructor(Box::new(left), Box::new(right), $compare),
                }
            }
            _ => ExprType::$constructor(Box::new(left), Box::new(right), $compare),
        }
    }}
}

impl Expr {
    pub fn is_zero(&self) -> bool {
        if let ExprType::Literal(token) = &self.expr {
            match *token {
                Int(i) => i == 0,
                UnsignedInt(u) => u == 0,
                Float(f) => f == 0.0,
                Char(c) => c == 0,
                _ => false,
            }
        } else {
            false
        }
    }
    pub fn is_negative(&self) -> bool {
        if let ExprType::Literal(token) = &self.expr {
            match *token {
                Int(i) => i < 0,
                Float(f) => f < 0.0,
                _ => false,
            }
        } else {
            false
        }
    }
    // first result: whether the expression itself is erroneous
    // second result: whether the expression was constexpr
    pub fn constexpr(self) -> CompileResult<Locatable<(Literal, Type)>> {
        let folded = self.const_fold()?;
        match folded.expr {
            ExprType::Literal(token) => Ok(Locatable {
                data: (token, folded.ctype),
                location: folded.location,
            }),
            _ => Err(folded.location.error(SemanticError::NotConstant(folded))),
        }
    }
    pub fn const_fold(self) -> CompileResult<Expr> {
        use crate::data::lex::ComparisonToken::*;
        let location = self.location;
        let folded = match self.expr {
            ExprType::Literal(_) => self.expr,
            ExprType::Id(ref name) => match &self.ctype {
                Type::Enum(_, members) => match members.iter().find(|member| member.0 == name.id) {
                    Some(enum_literal) => ExprType::Literal(Int(enum_literal.1)),
                    _ => self.expr,
                },
                // TODO: if a variable were const, could we const fold Ids?
                _ => self.expr,
            },
            ExprType::Sizeof(ctype) => {
                let sizeof = ctype.sizeof().map_err(|data| Locatable {
                    data: data.to_string(),
                    location,
                })?;
                ExprType::Literal(UnsignedInt(sizeof))
            }
            ExprType::Negate(expr) => expr.const_fold()?.map_literal(
                &location,
                |token| match token {
                    Int(i) => {
                        let (value, overflowed) = i.overflowing_neg();
                        if overflowed {
                            Err(SemanticError::ConstOverflow {
                                is_positive: value.is_negative(),
                            })
                        } else {
                            Ok(Int(value))
                        }
                    }
                    UnsignedInt(u) => Ok(UnsignedInt(u.wrapping_neg())),
                    Char(c) => Ok(Char(c.wrapping_neg())),
                    Float(f) => Ok(Float(-f)),
                    _ => Ok(token),
                },
                ExprType::Negate,
            )?,
            ExprType::BitwiseNot(expr) => expr.const_fold()?.map_literal(
                &location,
                |token| match token {
                    Int(i) => Ok(Int(!i)),
                    UnsignedInt(u) => Ok(UnsignedInt(!u)),
                    Char(c) => Ok(Char(!c)),
                    _ => Ok(token),
                },
                ExprType::BitwiseNot,
            )?,
            ExprType::Comma(left, right) => {
                let (left, right) = (left.const_fold()?, right.const_fold()?);
                // check if we can ignore left or it has side effects
                if left.constexpr {
                    right.expr
                } else {
                    ExprType::Comma(Box::new(left), Box::new(right))
                }
            }
            ExprType::Noop(inner) => {
                let inner = inner.const_fold()?;
                ExprType::Noop(Box::new(inner))
            }
            ExprType::Deref(expr) => {
                let folded = expr.const_fold()?;
                if let ExprType::Literal(Int(0)) = folded.expr {
                    semantic_err!("cannot dereference NULL pointer".into(), folded.location);
                }
                ExprType::Deref(Box::new(folded))
            }
            ExprType::Add(left, right) => left.literal_bin_op(
                *right,
                &location,
                fold_scalar_bin_op(f64::add, i64::overflowing_add, u64::wrapping_add),
                ExprType::Add,
            )?,
            ExprType::Sub(left, right) => left.literal_bin_op(
                *right,
                &location,
                fold_scalar_bin_op(f64::sub, i64::overflowing_sub, u64::wrapping_sub),
                ExprType::Sub,
            )?,
            ExprType::Mul(left, right) => left.literal_bin_op(
                *right,
                &location,
                fold_scalar_bin_op(f64::mul, i64::overflowing_mul, u64::wrapping_mul),
                ExprType::Mul,
            )?,
            ExprType::Div(left, right) => {
                let right = right.const_fold()?;
                if right.is_zero() {
                    return Err(location.error(SemanticError::DivideByZero));
                }
                left.literal_bin_op(
                    right,
                    &location,
                    fold_scalar_bin_op(f64::div, i64::overflowing_div, u64::wrapping_div),
                    ExprType::Div,
                )?
            }
            ExprType::Mod(left, right) => {
                let right = right.const_fold()?;
                if right.is_zero() {
                    return Err(location.error(SemanticError::DivideByZero));
                }
                left.literal_bin_op(
                    right,
                    &location,
                    |a: &Literal, b: &Literal, _| match (a, b) {
                        (Int(a), Int(b)) => {
                            let (value, overflowed) = a.overflowing_rem(*b);

                            if overflowed {
                                Err(SemanticError::ConstOverflow {
                                    is_positive: value.is_negative(),
                                })
                            } else {
                                Ok(Some(Int(value)))
                            }
                        }
                        (UnsignedInt(a), UnsignedInt(b)) => {
                            Ok(Some(UnsignedInt(a.wrapping_rem(*b))))
                        }
                        (_, _) => Ok(None),
                    },
                    ExprType::Mod,
                )?
            }
            ExprType::Xor(left, right) => {
                left.literal_bin_op(*right, &location, fold_int_bin_op!(^), ExprType::Xor)?
            }
            ExprType::BitwiseAnd(left, right) => {
                left.literal_bin_op(*right, &location, fold_int_bin_op!(&), ExprType::BitwiseAnd)?
            }
            ExprType::BitwiseOr(left, right) => {
                left.literal_bin_op(*right, &location, fold_int_bin_op!(|), ExprType::BitwiseOr)?
            }
            ExprType::Shift(left, right, true) => {
                shift_left(*left, *right, &self.ctype, &location)?
            }
            ExprType::Shift(left, right, false) => {
                shift_right(*left, *right, &self.ctype, &location)?
            }
            ExprType::Compare(left, right, Less) => fold_compare_op!(left, right, Compare, <, Less),
            ExprType::Compare(left, right, LessEqual) => {
                fold_compare_op!(left, right, Compare, <=, LessEqual)
            }
            ExprType::Compare(left, right, Greater) => {
                fold_compare_op!(left, right, Compare, >, Greater)
            }
            ExprType::Compare(left, right, GreaterEqual) => {
                fold_compare_op!(left, right, Compare, >=, GreaterEqual)
            }
            ExprType::Compare(left, right, EqualEqual) => {
                fold_compare_op!(left, right, Compare, ==, EqualEqual)
            }
            ExprType::Compare(left, right, NotEqual) => {
                fold_compare_op!(left, right, Compare, !=, NotEqual)
            }
            ExprType::Ternary(condition, then, otherwise) => {
                let (condition, then, otherwise) = (
                    condition.const_fold()?,
                    then.const_fold()?,
                    otherwise.const_fold()?,
                );
                match condition.expr {
                    ExprType::Literal(Int(0)) => otherwise.expr,
                    ExprType::Literal(Int(_)) => then.expr,
                    _ => {
                        ExprType::Ternary(Box::new(condition), Box::new(then), Box::new(otherwise))
                    }
                }
            }
            ExprType::FuncCall(func, params) => {
                let func = func.const_fold()?;
                #[rustfmt::skip]
                let params: Vec<Expr> = params
                    .into_iter()
                    .map(Self::const_fold)
                    .collect::<CompileResult<_>>()?;
                // function calls are always non-constant
                // TODO: if we have access to the full source of a function, could we try to
                // TODO: fold across function boundaries?
                ExprType::FuncCall(Box::new(func), params)
            }
            ExprType::Member(expr, member) => {
                let expr = expr.const_fold()?;
                ExprType::Member(Box::new(expr), member)
            }
            ExprType::Assign(target, value, token) => {
                let (target, value) = (target.const_fold()?, value.const_fold()?);
                // TODO: could we propagate this information somehow?
                // e.g. fold `int main() { int x = 1; return x; }` to `return 1;`
                ExprType::Assign(Box::new(target), Box::new(value), token)
            }
            ExprType::PostIncrement(expr, increase) => {
                let expr = expr.const_fold()?;
                // this isn't constant for the same reason assignment isn't constant
                ExprType::PostIncrement(Box::new(expr), increase)
            }
            ExprType::Cast(expr) => cast(*expr, &self.ctype)?,
            ExprType::LogicalAnd(left, right) => left.literal_bin_op(
                *right,
                &location,
                |left, right, _| match (left, right) {
                    (Int(1), Int(1)) => Ok(Some(Int(1))),
                    (Int(0), _) | (_, Int(0)) => Ok(Some(Int(0))),
                    _ => Ok(None),
                },
                ExprType::LogicalAnd,
            )?,
            ExprType::LogicalOr(left, right) => left.literal_bin_op(
                *right,
                &location,
                |left, right, _| match (left, right) {
                    (Int(0), Int(0)) => Ok(Some(Int(0))),
                    (Int(1), _) | (_, Int(1)) => Ok(Some(Int(1))),
                    _ => Ok(None),
                },
                ExprType::LogicalOr,
            )?,
            ExprType::StaticRef(inner) => ExprType::StaticRef(Box::new(inner.const_fold()?)),
        };
        let is_constexpr = match folded {
            ExprType::Literal(_) => true,
            _ => false,
        };
        //assert_eq!(self.constexpr, is_constexpr);
        Ok(Expr {
            expr: folded,
            constexpr: is_constexpr,
            location,
            ..self
        })
    }
    ///
    /// fold_func return values:
    /// `Ok(Some(_))`: Successfuly folded
    /// `Ok(None)`: Non-foldable expression
    /// `Err(_)`: Error while folding
    fn literal_bin_op<F, C>(
        self,
        other: Expr,
        location: &Location,
        fold_func: F,
        constructor: C,
    ) -> CompileResult<ExprType>
    where
        F: FnOnce(&Literal, &Literal, &Type) -> Result<Option<Literal>, SemanticError>,
        C: FnOnce(Box<Expr>, Box<Expr>) -> ExprType,
    {
        let (left, right) = (self.const_fold()?, other.const_fold()?);
        let literal: Option<ExprType> = match (&left.expr, &right.expr) {
            (ExprType::Literal(left_token), ExprType::Literal(right_token)) => {
                match fold_func(left_token, right_token, &left.ctype) {
                    Err(err) => {
                        return Err(location.error(err));
                    }
                    Ok(token) => token.map(ExprType::Literal),
                }
            }
            _ => None,
        };
        Ok(literal.unwrap_or_else(|| constructor(Box::new(left), Box::new(right))))
    }
    fn map_literal<F, C>(
        self,
        location: &Location,
        literal_func: F,
        constructor: C,
    ) -> CompileResult<ExprType>
    where
        F: FnOnce(Literal) -> Result<Literal, SemanticError>,
        C: FnOnce(Box<Expr>) -> ExprType,
    {
        match self.expr {
            ExprType::Literal(token) => match literal_func(token) {
                Ok(literal) => Ok(ExprType::Literal(literal)),
                Err(error) => Err(location.error(error)),
            },
            _ => Ok(constructor(Box::new(self))),
        }
    }
}

impl Literal {
    fn non_negative_int(&self) -> Result<u64, ()> {
        match *self {
            Int(i) if i >= 0 => Ok(i as u64),
            UnsignedInt(u) => Ok(u),
            Char(c) => Ok(u64::from(c)),
            _ => Err(()),
        }
    }
}

fn cast(expr: Expr, ctype: &Type) -> CompileResult<ExprType> {
    let expr = expr.const_fold()?;
    Ok(if let ExprType::Literal(ref token) = expr.expr {
        if let Some(token) = const_cast(token, ctype) {
            ExprType::Literal(token)
        } else {
            ExprType::Cast(Box::new(expr))
        }
    } else {
        ExprType::Cast(Box::new(expr))
    })
}

/// since we only have Int and Float for literals,
/// all this does is make sure the folded value is in a valid range
/// TODO: when we add suffix literals, that will have type information
/// and we can use that to store the new type
fn const_cast(token: &Literal, ctype: &Type) -> Option<Literal> {
    let token = match (token, ctype) {
        (Int(i), Type::Bool) => Int((*i != 0) as i64),
        (Int(i), Type::Double) | (Int(i), Type::Float) => Float(*i as f64),
        (Int(i), ty) if ty.is_integral() && ty.is_signed() => Int(*i),
        (Int(i), ty) if ty.is_integral() => UnsignedInt(*i as u64),
        (UnsignedInt(u), Type::Bool) => Int((*u != 0) as i64),
        (UnsignedInt(u), Type::Double) | (UnsignedInt(u), Type::Float) => Float(*u as f64),
        (UnsignedInt(u), ty) if ty.is_integral() && ty.is_signed() => Int(*u as i64),
        (UnsignedInt(u), ty) if ty.is_integral() => UnsignedInt(*u),
        (Float(f), Type::Bool) => Int((*f != 0.0) as i64),
        (Float(f), Type::Double) | (Float(f), Type::Float) => Float(*f),
        (Float(f), ty) if ty.is_integral() && ty.is_signed() => Int(*f as i64),
        (Float(f), ty) if ty.is_integral() => UnsignedInt(*f as u64),
        (Int(i), _) if ctype.is_pointer() && *i >= 0 => UnsignedInt(*i as u64),
        (UnsignedInt(u), _) if ctype.is_pointer() => UnsignedInt(*u),
        (Char(c), _) if ctype.is_pointer() => UnsignedInt(u64::from(*c)),
        _ => return None,
    };
    Some(token)
}

fn shift_right(
    left: Expr,
    right: Expr,
    ctype: &Type,
    location: &Location,
) -> CompileResult<ExprType> {
    let (left, right) = (left.const_fold()?, right.const_fold()?);
    if let ExprType::Literal(token) = right.expr {
        let shift = match token.non_negative_int() {
            Ok(u) => u,
            Err(_) => {
                return Err(location.error(SemanticError::NegativeShift { is_left: false }));
            }
        };
        let sizeof = ctype.sizeof().map_err(|err| Locatable {
            data: err.to_string(),
            location: *location,
        })?;
        // Rust panics if the shift is greater than the size of the type
        if shift >= sizeof {
            return Ok(ExprType::Literal(if ctype.is_signed() {
                Int(0)
            } else {
                UnsignedInt(0)
            }));
        }
        if let ExprType::Literal(token) = left.expr {
            Ok(match token {
                Int(i) => ExprType::Literal(Int(i.wrapping_shr(shift as u32))),
                UnsignedInt(u) => ExprType::Literal(UnsignedInt(u.wrapping_shr(shift as u32))),
                _ => unreachable!("only ints and unsigned ints can be right shifted"),
            })
        } else {
            Ok(ExprType::Shift(
                Box::new(left),
                Box::new(Expr {
                    expr: ExprType::Literal(token),
                    ..right
                }),
                false,
            ))
        }
    } else {
        Ok(ExprType::Shift(Box::new(left), Box::new(right), false))
    }
}

fn shift_left(
    left: Expr,
    right: Expr,
    ctype: &Type,
    location: &Location,
) -> CompileResult<ExprType> {
    let (left, right) = (left.const_fold()?, right.const_fold()?);
    if let ExprType::Literal(token) = right.expr {
        let shift = match token.non_negative_int() {
            Ok(u) => u,
            Err(_) => {
                return Err(location.error(SemanticError::NegativeShift { is_left: true }));
            }
        };

        if left.ctype.is_signed() {
            let size = match left.ctype.sizeof() {
                Ok(s) => s,
                Err(err) => semantic_err!(err.into(), *location),
            };
            let max_shift = u64::from(CHAR_BIT) * size;
            if shift >= max_shift {
                return Err(location.error(SemanticError::TooManyShiftBits {
                    is_left: true,
                    current: shift,
                    ctype: ctype.clone(),
                    maximum: max_shift,
                }));
            }
        }
        Ok(match left.expr {
            ExprType::Literal(Int(i)) => {
                let (result, overflow) = i.overflowing_shl(shift as u32);
                if overflow {
                    return Err(location.error(SemanticError::ConstOverflow { is_positive: true }));
                }
                ExprType::Literal(Int(result))
            }
            ExprType::Literal(UnsignedInt(u)) => {
                ExprType::Literal(UnsignedInt(u.wrapping_shl(shift as u32)))
            }
            _ => ExprType::Shift(
                Box::new(left),
                Box::new(Expr {
                    expr: ExprType::Literal(token),
                    ..right
                }),
                false,
            ),
        })
    } else {
        Ok(ExprType::Shift(Box::new(left), Box::new(right), false))
    }
}

#[cfg(test)]
mod tests {
    use crate::data::prelude::*;
    use crate::parse::tests::parse_expr;

    fn test_const_fold(s: &str) -> CompileResult<Expr> {
        parse_expr(s).unwrap().const_fold()
    }

    // I will be including the test cases from https://github.com/jyn514/rcc/issues/38#issue-491407941
    // as well as a working case for each operator

    #[test]
    fn test_addition() {
        assert_eq!(
            test_const_fold("3 + 4").unwrap().expr,
            parse_expr("7").unwrap().expr
        );
        assert_eq!(
            test_const_fold("0x7fffffffffffffffL + 1").unwrap_err().data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
        assert_eq!(
            test_const_fold("-0x7fffffffffffffffL + -2")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: false }.into()
        );
    }

    #[test]
    fn test_subtraction() {
        assert_eq!(
            test_const_fold("9 - 3").unwrap().expr,
            parse_expr("6").unwrap().expr
        );
        assert_eq!(
            test_const_fold("-0x7fffffffffffffffL - 2")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: false }.into()
        );
        assert_eq!(
            test_const_fold("0x7fffffffffffffffL - -1")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
    }

    #[test]
    fn test_multiplication() {
        assert_eq!(
            test_const_fold("3 * 5").unwrap().expr,
            parse_expr("15").unwrap().expr
        );
        assert_eq!(
            test_const_fold("0x7fffffffffffffffL * 2").unwrap_err().data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
        assert_eq!(
            test_const_fold("(-0x7fffffffffffffffL - 1) * -1")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
    }

    #[test]
    fn test_division() {
        assert_eq!(
            test_const_fold("6 / 3").unwrap().expr,
            parse_expr("2").unwrap().expr
        );
        assert_eq!(
            test_const_fold("6 / -3").unwrap().expr,
            test_const_fold("-2").unwrap().expr
        );
        assert_eq!(
            test_const_fold("1 / 0").unwrap_err().data,
            SemanticError::DivideByZero.into()
        );
        assert_eq!(
            test_const_fold("1 / (2 - 2)").unwrap_err().data,
            SemanticError::DivideByZero.into()
        );
        assert_eq!(
            test_const_fold("(-0x7fffffffffffffffL - 1) / -1")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
    }

    #[test]
    fn test_modulo() {
        assert_eq!(
            test_const_fold("5 % 3").unwrap().expr,
            parse_expr("2").unwrap().expr
        );
        assert_eq!(
            test_const_fold("-7 % 2").unwrap().expr,
            test_const_fold("-1").unwrap().expr
        );
        assert_eq!(
            test_const_fold("1%0").unwrap_err().data,
            SemanticError::DivideByZero.into()
        );
        assert_eq!(
            test_const_fold("(-0x7fffffffffffffffL - 1) % -1")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: false }.into()
        );
    }

    #[test]
    fn test_negation() {
        assert_eq!(
            test_const_fold("-0").unwrap().expr,
            parse_expr("0").unwrap().expr
        );

        assert_eq!(
            test_const_fold("-(-0x7fffffffffffffffL - 1L)")
                .unwrap_err()
                .data,
            SemanticError::ConstOverflow { is_positive: true }.into()
        );
    }

    #[test]
    fn test_left_shift() {
        assert_eq!(
            test_const_fold("8 << 0").unwrap().expr,
            parse_expr("8").unwrap().expr
        );

        assert_eq!(
            test_const_fold("1 << 4").unwrap().expr,
            parse_expr("16").unwrap().expr
        );

        assert_eq!(
            test_const_fold("1 << 65").unwrap_err().data,
            SemanticError::TooManyShiftBits {
                is_left: true,
                current: 65,
                ctype: Type::Long(true),
                maximum: 64
            }
            .into()
        );

        assert_eq!(
            test_const_fold("8 << -1").unwrap_err().data,
            SemanticError::NegativeShift { is_left: true }.into()
        );
    }

    #[test]
    fn test_right_shift() {
        assert_eq!(
            test_const_fold("8 >> 0").unwrap().expr,
            parse_expr("8").unwrap().expr
        );

        assert_eq!(
            test_const_fold("32 >> 5").unwrap().expr,
            parse_expr("1").unwrap().expr
        );

        assert_eq!(
            test_const_fold("8 >> -1").unwrap_err().data,
            SemanticError::NegativeShift { is_left: false }.into()
        );
    }
}
