use std::cmp::max;
use std::convert::TryInto;

use cranelift::codegen::{
    ir::{
        types::{self, Type as IrType},
        AbiParam, Signature,
    },
    isa::CallConv,
};
use target_lexicon::Triple;

use crate::data::{
    types::{
        ArrayType, FunctionType, StructType,
        Type::{self, *},
    },
    Symbol,
};

// NOTE: this is required by the standard to always be one
const CHAR_SIZE: u16 = 1;

// TODO: allow this to be configured at runtime
lazy_static! {
    // TODO: make this `const` when
    // https://github.com/CraneStation/target-lexicon/pull/19 is merged
    pub static ref TARGET: Triple = Triple::host();
    pub static ref CALLING_CONVENTION: CallConv = CallConv::triple_default(&TARGET);
}
mod x64;
pub use x64::*;

pub fn union_size(symbols: &[Symbol]) -> Result<SIZE_T, &'static str> {
    symbols
        .iter()
        .map(|symbol| symbol.ctype.sizeof())
        // max of member sizes
        .try_fold(1, |n, size| Ok(max(n, size?)))
}

pub fn struct_size(symbols: &[Symbol]) -> Result<SIZE_T, &'static str> {
    // TODO: this doesn't handle padding
    symbols
        .iter()
        .map(|symbol| symbol.ctype.sizeof())
        // sum of member sizes
        .try_fold(0, |n, size| Ok(n + size?))
}

pub fn struct_align(members: &[Symbol]) -> Result<SIZE_T, &'static str> {
    members.iter().try_fold(0, |max, member| {
        Ok(std::cmp::max(member.ctype.alignof()?, max))
    })
}

impl Type {
    pub fn can_represent(&self, other: &Type) -> bool {
        self == other
            || *self == Type::Double && *other == Type::Float
            || (self.is_integral() && other.is_integral())
                && (self.sizeof() > other.sizeof()
                    || self.sizeof() == other.sizeof() && self.is_signed() == other.is_signed())
    }

    pub fn sizeof(&self) -> Result<SIZE_T, &'static str> {
        match self {
            Bool => Ok(BOOL_SIZE.into()),
            Char(_) => Ok(CHAR_SIZE.into()),
            Short(_) => Ok(SHORT_SIZE.into()),
            Int(_) => Ok(INT_SIZE.into()),
            Long(_) => Ok(LONG_SIZE.into()),
            Float => Ok(FLOAT_SIZE.into()),
            Double => Ok(DOUBLE_SIZE.into()),
            Pointer(_, _) => Ok(PTR_SIZE.into()),
            // now for the hard ones
            Array(t, ArrayType::Fixed(l)) => t.sizeof().and_then(|n| Ok(n * l)),
            Array(_, ArrayType::Unbounded) => Err("cannot take sizeof variable length array"),
            Enum(_, symbols) => {
                let uchar = CHAR_BIT as usize;
                // integer division, but taking the ceiling instead of the floor
                // https://stackoverflow.com/a/17974/7669110
                Ok(match (symbols.len() + uchar - 1) / uchar {
                    0..=8 => 8,
                    9..=16 => 16,
                    17..=32 => 32,
                    33..=64 => 64,
                    _ => return Err("enum cannot be represented in SIZE_T bits"),
                })
            }
            Union(StructType::Named(_, size, _, _)) | Struct(StructType::Named(_, size, _, _)) => {
                Ok(*size)
            }
            Struct(StructType::Anonymous(symbols)) => struct_size(&symbols),
            Union(StructType::Anonymous(symbols)) => union_size(&symbols),
            Bitfield(_) => unimplemented!("sizeof(bitfield)"),
            // illegal operations
            Function(_) => Err("cannot take `sizeof` a function"),
            Void => Err("cannot take `sizeof` void"),
            VaList => Err("cannot take `sizeof` va_list"),
        }
    }
    pub fn alignof(&self) -> Result<SIZE_T, &'static str> {
        match self {
            Bool
            | Char(_)
            | Short(_)
            | Int(_)
            | Long(_)
            | Float
            | Double
            | Pointer(_, _)
            // TODO: is this correct? still need to worry about padding
            | Union(_)
            | Enum(_, _) => self.sizeof(),
            Array(t, _) => t.alignof(),
            // Clang uses the largest alignment of any element as the alignment of the whole
            // Not sure why, but who am I to argue
            // Anyway, Faerie panics if the alignment isn't a power of two so it's probably for the best
            Struct(StructType::Named(_, _, align, _)) => Ok(*align),
            Struct(StructType::Anonymous(members)) => struct_align(members),
            Bitfield(_) => unimplemented!("alignof bitfield"),
            Function(_) => Err("cannot take `alignof` function"),
            Void => Err("cannot take `alignof` void"),
            VaList => Err("cannot take `alignof` va_list"),
        }
    }
    pub fn ptr_type() -> IrType {
        IrType::int(CHAR_BIT * PTR_SIZE).expect("pointer size should be valid")
    }
    pub fn struct_offset(&self, members: &[Symbol], member: &str) -> u64 {
        let mut current_offset = 0;
        for formal in members {
            if formal.id == member {
                return current_offset;
            }
            let mut size = formal
                .ctype
                .sizeof()
                .expect("struct members should have complete object type");
            let align = self.alignof().expect("struct should have valid alignment");
            // round up to the nearest multiple of align
            if size % align != 0 {
                size += (align - size) % align;
            }
            current_offset += size;
        }
        unreachable!("cannot call struct_offset for member not in struct");
    }
    pub fn as_ir_type(&self) -> IrType {
        match self {
            // Integers
            Bool => types::B1,
            Char(_) | Short(_) | Int(_) | Long(_) | Pointer(_, _) | Enum(_, _) => {
                let int_size = SIZE_T::from(CHAR_BIT)
                    * self
                        .sizeof()
                        .expect("integers should always have a valid size");
                IrType::int(int_size.try_into().unwrap_or_else(|_| {
                    panic!(
                        "integers should never have a size larger than {}",
                        i16::max_value()
                    )
                }))
                .unwrap_or_else(|| panic!("unsupported size for IR: {}", int_size))
            }

            // Floats
            // TODO: this is hard-coded for x64
            Float => types::F32,
            Double => types::F64,

            // Aggregates
            // arrays and functions decay to pointers
            Function(_) | Array(_, _) => IrType::int(PTR_SIZE * CHAR_BIT)
                .unwrap_or_else(|| panic!("unsupported size of IR: {}", PTR_SIZE)),
            // void cannot be loaded or stored
            _ => types::INVALID,
        }
    }
}

impl FunctionType {
    pub fn signature(&self) -> Signature {
        let params = if self.params.len() == 1 && self.params[0].ctype == Type::Void {
            // no arguments
            Vec::new()
        } else {
            self.params
                .iter()
                .map(|param| AbiParam::new(param.ctype.as_ir_type()))
                .collect()
        };
        let return_type = if !self.should_return() {
            vec![]
        } else {
            vec![AbiParam::new(self.return_type.as_ir_type())]
        };
        Signature {
            call_conv: *CALLING_CONVENTION,
            params,
            returns: return_type,
        }
    }
}

/// short-circuiting version of iter.max_by_key
///
/// partially taken from https://doc.rust-lang.org/src/core/iter/traits/iterator.rs.html#2591
///
/// Example:
///
/// ```
/// use rcc::backend::try_max_by_key;
/// let list = [[1, 2, 3], [5, 4, 3], [1, 1, 4]];
/// assert_eq!(try_max_by_key(list.iter(), |vec| vec.last().map(|&x| x).ok_or(())), Some(Ok(&[1, 1, 4])));
///
/// let lengths = [vec![], vec![1], vec![1, 2]];
/// assert_eq!(try_max_by_key(lengths.iter(), |vec| vec.last().map(|&x| x).ok_or(())), Some(Err(())));
/// ```
pub fn try_max_by_key<'a, I, T, C, R, F>(mut iter: I, mut f: F) -> Option<Result<&'a T, R>>
where
    I: Iterator<Item = &'a T>,
    C: std::cmp::Ord,
    F: FnMut(&T) -> Result<C, R>,
{
    iter.next().map(|initial| {
        // if this gives an error, return it immediately
        // avoids f not being called if there's only one element
        f(&initial)?;
        iter.try_fold(initial, |current, next| {
            if f(current)? >= f(next)? {
                Ok(current)
            } else {
                Ok(next)
            }
        })
    })
}
