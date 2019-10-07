use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display, Formatter, Write};
use std::hash::Hash;

use crate::backend::SIZE_T;

pub type SemanticResult<T> = Result<T, Locatable<String>>;

pub mod lex;
pub mod types;
pub mod prelude {
    pub use super::{
        types::StructType, Declaration, Expr, ExprType, Locatable, Location, SemanticResult, Stmt,
        StmtType, Symbol, Token, Type,
    };
}
pub use lex::{Keyword, Locatable, Location, Token};
pub use types::Type;

pub type Stmt = Locatable<StmtType>;

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum StmtType {
    Compound(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Do(Box<Stmt>, Expr),
    While(Expr, Option<Box<Stmt>>),
    // for(int i = 1, j = 2; i < 4; ++i) body
    // for(i = 1; ; ++i) body
    // for (;;) ;
    For(
        Option<Box<Stmt>>,
        Option<Expr>,
        Option<Expr>,
        Option<Box<Stmt>>,
    ),
    Switch(Expr, Box<Stmt>),
    Label(String),
    Case(u64, Option<Box<Stmt>>),
    Default(Option<Box<Stmt>>),
    Expr(Expr),
    Goto(String),
    Continue,
    Break,
    Return(Option<Expr>),
    Decl(VecDeque<Locatable<Declaration>>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    pub symbol: Symbol,
    pub init: Option<Initializer>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Initializer {
    Scalar(Box<Expr>),                 // int i = 5;
    InitializerList(Vec<Initializer>), // int a[] = { 1, 2, 3 };
    FunctionBody(Vec<Stmt>),           // int f() { return 0; }
}

/// Holds the metadata for an expression.
///
/// This should be the datatype you use in APIs, etc.
/// because it is more useful than the raw ExprType.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// expr: holds the actual expression
    pub expr: ExprType,

    /// ctype: holds the type of the expression
    pub ctype: Type,

    /// constexpr: whether a value can be constant-folded at compile-time
    ///
    /// unrelated to the `const` keyword
    /// NOTE: can sometimes be true at the same time as `lval` (e.g. for constant arrays)
    pub constexpr: bool,

    /// lval: whether an expression can be assigned to
    ///
    /// for example, variables, array elements, and pointer dereferences are lvals,
    /// but literals, functions, and addresses cannot
    pub lval: bool,

    /// location: the best approximation of where the expression is
    ///
    /// usually points to the location of the operation symbol, or the literal if no
    /// operations is being performed
    /// implicit operations should point to the child expression
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprType {
    Id(Symbol),
    Literal(Token),
    FuncCall(Box<Expr>, Vec<Expr>),
    Member(Box<Expr>, String),
    // post increment/decrement
    PostIncrement(Box<Expr>, bool),
    Cast(Box<Expr>),
    Sizeof(Type),
    Deref(Box<Expr>),
    Negate(Box<Expr>),
    LogicalNot(Box<Expr>),
    BitwiseNot(Box<Expr>),
    LogicalOr(Box<Expr>, Box<Expr>),
    BitwiseOr(Box<Expr>, Box<Expr>),
    LogicalAnd(Box<Expr>, Box<Expr>),
    BitwiseAnd(Box<Expr>, Box<Expr>),
    Xor(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    // bool: left or right
    Shift(Box<Expr>, Box<Expr>, bool),
    // Token: make >, <, <=, ... part of the same variant
    Compare(Box<Expr>, Box<Expr>, Token),
    // Token: allow extended assignment
    Assign(Box<Expr>, Box<Expr>, Token),
    // Ternary: if ? then : else
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Comma(Box<Expr>, Box<Expr>),
    // &expr in static context
    // requires cooperation with the linker
    StaticRef(Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageClass {
    Static = Keyword::Static as isize,
    Extern = Keyword::Extern as isize,
    Auto = Keyword::Auto as isize,
    Register = Keyword::Register as isize,
    Typedef = Keyword::Typedef as isize,
}

/* structs */
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: String,
    pub ctype: Type,
    pub qualifiers: Qualifiers,
    pub storage_class: StorageClass,
    pub init: bool,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Qualifiers {
    pub volatile: bool,
    pub c_const: bool,
}

#[derive(Debug)]
pub struct Scope<K: Hash + Eq, V>(Vec<HashMap<K, V>>);

#[allow(dead_code)]
impl Qualifiers {
    pub const NONE: Qualifiers = Qualifiers {
        c_const: false,
        volatile: false,
    };
    pub const VOLATILE: Qualifiers = Qualifiers {
        c_const: false,
        volatile: true,
    };
    pub const CONST: Qualifiers = Qualifiers {
        c_const: true,
        volatile: false,
    };
    pub const CONST_VOLATILE: Qualifiers = Qualifiers {
        c_const: true,
        volatile: true,
    };
}

lazy_static! {
    pub static ref INT_POINTER: Type =
        { Type::Pointer(Box::new(Type::Int(true)), Qualifiers::NONE) };
}

pub enum LengthError {
    Unbounded,
    Dynamic,
    NonIntegral,
    Negative,
}

impl Expr {
    pub fn const_int(self) -> SemanticResult<SIZE_T> {
        use std::convert::TryInto;
        if !self.ctype.is_integral() {
            return Err(Locatable {
                data: LengthError::NonIntegral.into(),
                location: self.location.clone(),
            });
        }
        let literal = self.constexpr()?.map_err(|location| Locatable {
            data: LengthError::Dynamic.into(),
            location,
        })?;
        match literal.data.0 {
            Token::UnsignedInt(u) => Ok(u),
            Token::Int(x) => x.try_into().map_err(|_| Locatable {
                data: LengthError::Negative.into(),
                location: literal.location,
            }),
            x => unreachable!("should have been caught already: {:?}", x),
        }
    }
    pub fn zero() -> Expr {
        Expr {
            ctype: Type::Int(true),
            constexpr: true,
            expr: ExprType::Literal(Token::Int(0)),
            lval: false,
            location: Default::default(),
        }
    }
}

impl From<LengthError> for String {
    fn from(err: LengthError) -> String {
        let s: &'static str = err.into();
        s.to_string()
    }
}

impl From<LengthError> for &'static str {
    fn from(err: LengthError) -> &'static str {
        use LengthError::*;
        match err {
            Unbounded => "Cannot take the length of unbounded array type",
            Dynamic => "Length of variable-length array cannot be known at compile time",
            NonIntegral => "The length of an array must be an integer",
            Negative => "The length of an array must not be negative",
        }
    }
}

impl<K: Hash + Eq, V> Scope<K, V> {
    #[inline]
    pub fn new() -> Self {
        Self(vec![HashMap::new()])
    }
    #[inline]
    pub fn enter_scope(&mut self) {
        self.0.push(HashMap::<K, V>::new())
    }
    #[inline]
    pub fn leave_scope(&mut self) {
        self.0.pop();
    }
    pub fn get(&self, name: &K) -> Option<&V> {
        debug_assert!(!self.0.is_empty());
        for map in self.0.iter().rev() {
            let current = map.get(name);
            if current.is_some() {
                return current;
            }
        }
        None
    }
    // returns whether the _immediate_ scope contains `name`
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        debug_assert!(!self.0.is_empty());
        self.0.last_mut().unwrap().insert(key, value)
    }
    #[inline]
    pub fn get_immediate(&self, name: &K) -> Option<&V> {
        debug_assert!(!self.0.is_empty());
        self.0.last().unwrap().get(name)
    }
    #[inline]
    pub fn get_all_immediate(&mut self) -> &mut HashMap<K, V> {
        debug_assert!(!self.0.is_empty());
        self.0.last_mut().unwrap()
    }
    #[inline(always)]
    pub fn depth(&self) -> usize {
        self.0.len()
    }
    pub fn is_global(&self) -> bool {
        debug_assert!(!self.0.is_empty());
        self.0.len() == 1
    }
    pub fn _remove(&mut self, key: &K) -> Option<V> {
        debug_assert!(!self.0.is_empty());
        self.0.last_mut().unwrap().remove(key)
    }
}

impl<K: Eq + Hash, V> Default for Scope<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<Keyword> for StorageClass {
    type Error = Keyword;
    fn try_from(value: Keyword) -> Result<StorageClass, Keyword> {
        use StorageClass::*;
        match value {
            Keyword::Extern => Ok(Extern),
            Keyword::Static => Ok(Static),
            Keyword::Auto => Ok(Auto),
            Keyword::Register => Ok(Register),
            Keyword::Typedef => Ok(Typedef),
            _ => Err(value),
        }
    }
}

impl Default for StorageClass {
    fn default() -> StorageClass {
        StorageClass::Auto
    }
}

impl Display for StorageClass {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", &format!("{:?}", self).to_lowercase())
    }
}

impl Display for Qualifiers {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match (self.c_const, self.volatile) {
                (true, true) => "'const volatile' type qualifiers",
                (true, false) => "'const' type qualifier",
                (false, true) => "'volatile' type qualifier",
                (false, false) => "",
            }
        )
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.expr {
            ExprType::Comma(left, right) => write!(f, "{}, {}", *left, *right),
            ExprType::Literal(token) => write!(f, "{}", token),
            ExprType::Id(symbol) => write!(f, "{}", symbol.id),
            ExprType::Add(left, right) => write!(f, "({}) + ({})", left, right),
            ExprType::Sub(left, right) => write!(f, "({}) - ({})", left, right),
            ExprType::Mul(left, right) => write!(f, "({}) * ({})", left, right),
            ExprType::Div(left, right) => write!(f, "({}) / ({})", left, right),
            ExprType::Mod(left, right) => write!(f, "({}) % ({})", left, right),
            ExprType::Xor(left, right) => write!(f, "({}) ^ ({})", left, right),
            ExprType::BitwiseOr(left, right) => write!(f, "({}) | ({})", left, right),
            ExprType::BitwiseAnd(left, right) => write!(f, "({}) & ({})", left, right),
            ExprType::BitwiseNot(expr) => write!(f, "(~{})", expr),
            ExprType::Deref(expr) => write!(f, "*({})", expr),
            ExprType::Negate(expr) => write!(f, "-({})", expr),
            ExprType::LogicalNot(expr) => write!(f, "!({})", expr),
            ExprType::LogicalOr(left, right) => write!(f, "({}) || ({})", left, right),
            ExprType::LogicalAnd(left, right) => write!(f, "({}) && ({})", left, right),
            ExprType::Shift(val, by, left) => {
                write!(f, "({}) {} ({})", val, if *left { "<<" } else { ">>" }, by)
            }
            ExprType::Compare(left, right, token) => write!(f, "({}) {} ({})", left, token, right),
            ExprType::Assign(left, right, token) => write!(f, "({}) {} ({})", left, token, right),
            ExprType::Ternary(cond, left, right) => {
                write!(f, "({}) ? ({}) : ({})", cond, left, right)
            }
            ExprType::FuncCall(left, params) => {
                let varargs = if let Type::Function(ftype) = &left.ctype {
                    ftype.varargs
                } else {
                    unreachable!("parser should catch illegal function calls");
                };
                write!(
                    f,
                    "({})({})",
                    left,
                    print_func_call(params.as_slice(), varargs, |expr| {
                        let mut s = String::new();
                        write!(s, "{}", expr).unwrap();
                        s
                    })
                )
            }
            ExprType::Cast(expr) => write!(f, "({})({})", self.ctype, expr),
            ExprType::Sizeof(ty) => write!(f, "sizeof({})", ty),
            ExprType::Member(compound, id) => write!(f, "({}).{}", compound, id),
            ExprType::PostIncrement(expr, inc) => {
                write!(f, "({}){}", expr, if *inc { "++" } else { "--" })
            }
            ExprType::StaticRef(expr) => write!(f, "&{}", expr),
        }
    }
}

fn print_func_call<T, F: Fn(&T) -> String>(params: &[T], varargs: bool, print_func: F) -> String {
    // https://stackoverflow.com/a/30325430
    let mut comma_separated = String::new();
    for param in params {
        comma_separated.push_str(&print_func(param));
        comma_separated.push_str(", ");
    }
    if varargs {
        comma_separated.push_str("...");
    } else if !params.is_empty() {
        comma_separated.pop();
        comma_separated.pop();
    }
    comma_separated
}

impl Display for Initializer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Initializer::Scalar(expr) => write!(f, "{}", expr),
            Initializer::InitializerList(list) => {
                write!(f, "{{ ")?;
                write!(
                    f,
                    "{}",
                    print_func_call(list, false, |init| { format!("{}", init) })
                )?;
                write!(f, " }}")
            }
            Initializer::FunctionBody(body) => {
                writeln!(f, "{{")?;
                for stmt in body {
                    writeln!(f, "{}", stmt.data)?;
                }
                write!(f, "}}")
            }
        }
    }
}

impl Display for StmtType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StmtType::Expr(expr) => write!(f, "{};", expr),
            StmtType::Return(None) => write!(f, "return;"),
            StmtType::Return(Some(expr)) => write!(f, "return {};", expr),
            StmtType::Break => write!(f, "break;"),
            StmtType::Continue => write!(f, "continue;"),
            StmtType::Default(stmt) => write!(
                f,
                "default:{}",
                if let Some(stmt) = stmt {
                    format!("\n{}", stmt.data)
                } else {
                    " ;".into()
                }
            ),
            StmtType::Case(expr, stmt) => write!(
                f,
                "case {}:{}",
                expr,
                if let Some(stmt) = stmt {
                    format!("\n{}", stmt.data)
                } else {
                    " ;".into()
                }
            ),
            StmtType::Goto(id) => write!(f, "goto {};", id),
            StmtType::Label(id) => write!(f, "{}: ", id),
            StmtType::While(condition, None) => write!(f, "while ({}) {{}}", condition),
            StmtType::While(condition, Some(body)) => {
                write!(f, "while ({}) {}", condition, body.data)
            }
            StmtType::If(condition, body, None) => write!(f, "if ({}) {}", condition, body.data),
            StmtType::If(condition, body, Some(otherwise)) => write!(
                f,
                "if ({}) {} else {}",
                condition, body.data, otherwise.data
            ),
            StmtType::Do(body, condition) => {
                write!(f, "do {:?} while ({:?});", body.data, condition)
            }
            StmtType::For(decls, condition, post_loop, body) => {
                write!(f, "for (")?;
                if let Some(init) = decls {
                    match &init.data {
                        StmtType::Decl(decls) => {
                            let len = decls.len();
                            for (i, decl) in decls.iter().enumerate() {
                                write!(f, "{}", decl.data)?;
                                if i != len - 1 {
                                    write!(f, ", ")?;
                                }
                            }
                        }
                        StmtType::Expr(expr) => write!(f, "{}", expr)?,
                        _ => unreachable!("for loop initialization other than decl or expr"),
                    }
                }
                match condition {
                    Some(condition) => write!(f, "; {}; ", condition)?,
                    None => write!(f, "; ; ")?,
                };
                match post_loop {
                    Some(condition) => write!(f, " {})", condition)?,
                    None => write!(f, ")")?,
                };
                write!(
                    f,
                    " {}",
                    if let Some(body) = body {
                        format!("{}", body.data)
                    } else {
                        ";".into()
                    }
                )
            }
            StmtType::Decl(decls) => {
                for decl in decls {
                    writeln!(f, "{};", decl.data)?;
                }
                Ok(())
            }
            StmtType::Compound(stmts) => {
                writeln!(f, "{{")?;
                for stmt in stmts {
                    writeln!(f, "{}", stmt.data)?;
                }
                write!(f, "}}")
            }
            StmtType::Switch(condition, body) => write!(f, "switch ({}) {}", condition, body.data),
        }
    }
}

impl Display for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // TODO: this is not right
        write!(
            f,
            "{} {} {}: {}",
            self.symbol.storage_class, self.symbol.qualifiers, self.symbol.id, self.symbol.ctype
        )?;
        match &self.init {
            Some(Initializer::FunctionBody(body)) => {
                writeln!(f, " {{")?;
                for stmt in body {
                    writeln!(f, "{}", stmt.data)?;
                }
                writeln!(f, "}}")
            }
            Some(Initializer::Scalar(expr)) => write!(f, " = {};", expr),
            Some(Initializer::InitializerList(inits)) => {
                write!(f, " = {{")?;
                for init in inits {
                    write!(f, "{}, ", init)?;
                }
                write!(f, "}};")
            }
            None => write!(f, ";"),
        }
    }
}

impl PartialEq for Symbol {
    // don't require both symbols to be `init` to be equal
    fn eq(&self, other: &Self) -> bool {
        self.ctype == other.ctype
            && self.id == other.id
            && self.qualifiers == other.qualifiers
            && self.storage_class == other.storage_class
    }
}

impl Eq for Symbol {}

mod tests {
    #[test]
    fn type_display() {
        for ty in [
            "int",
            "int *",
            "int[1][2][3]",
            "int *(*)(int)",
            "int *(*)[1][2][3]",
        ]
        .iter()
        {
            assert_eq!(
                &format!(
                    "{}",
                    crate::Parser::new(
                        crate::Lexer::new("<integration-test>".into(), ty.chars(), false),
                        false
                    )
                    .type_name()
                    .unwrap()
                    .data
                    .0
                ),
                ty
            );
        }
    }
}
