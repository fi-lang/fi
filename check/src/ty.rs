pub use hir::Ident;
use hir::Symbol;
use std::fmt;

pub type Ty<'tcx> = &'tcx Type<'tcx>;
pub type Layout<'tcx> = crate::layout::TyLayout<'tcx, Ty<'tcx>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type<'tcx> {
    Error,
    Var(TypeVar),
    Never,
    Bool,
    Str,
    TypeId,
    VInt(TypeVar),
    VUInt(TypeVar),
    VFloat(TypeVar),
    Int(u8),
    UInt(u8),
    Float(u8),
    Ref(bool, Ty<'tcx>),
    Tuple(&'tcx [Ty<'tcx>]),
    Func(&'tcx [Param<'tcx>], Ty<'tcx>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeVar(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Param<'tcx> {
    pub name: Ident,
    pub ty: Ty<'tcx>,
}

impl<'tcx> Type<'tcx> {
    pub fn func(&self) -> Option<(&'tcx [Param<'tcx>], Ty<'tcx>)> {
        match self {
            Type::Func(params, ret) => Some((params, ret)),
            _ => None,
        }
    }

    pub fn fields(&self, tcx: &crate::tcx::Tcx<'tcx>) -> Vec<(Symbol, Ty<'tcx>)> {
        match self {
            Type::Str => vec![
                (Symbol::new("ptr"), tcx.builtin.ref_u8),
                (Symbol::new("len"), tcx.builtin.usize),
            ],
            Type::TypeId => vec![
                (Symbol::new("size"), tcx.builtin.usize),
                (Symbol::new("align"), tcx.builtin.usize),
            ],
            Type::Tuple(tys) => tys
                .iter()
                .enumerate()
                .map(|(i, ty)| (Symbol::new(i.to_string()), *ty))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn pointee(&self) -> Ty<'tcx> {
        match self {
            Type::Ref(_, to) => to,
            _ => panic!("type is not a reference"),
        }
    }

    pub fn idx(&self, tcx: &crate::tcx::Tcx<'tcx>) -> Ty<'tcx> {
        match self {
            Type::Str => tcx.builtin.u8,
            _ => panic!("type can't be indexed"),
        }
    }
}

impl fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Error => write!(f, "[type error]"),
            Type::Var(var) => var.fmt(f),
            Type::Never => write!(f, "never"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "str"),
            Type::TypeId => write!(f, "type"),
            Type::VInt(_) => write!(f, "int"),
            Type::VUInt(_) => write!(f, "uint"),
            Type::VFloat(_) => write!(f, "float"),
            Type::Int(0) => write!(f, "isize"),
            Type::UInt(0) => write!(f, "usize"),
            Type::Int(bits) => write!(f, "i{}", bits),
            Type::UInt(bits) => write!(f, "u{}", bits),
            Type::Float(bits) => write!(f, "f{}", bits),
            Type::Ref(true, to) => write!(f, "ref mut {}", to),
            Type::Ref(false, to) => write!(f, "ref {}", to),
            Type::Tuple(tys) => write!(
                f,
                "({})",
                tys.iter()
                    .map(|t| format!("{},", t))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Type::Func(params, ret) => write!(
                f,
                "fn ({}) -> {}",
                params
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                ret
            ),
        }
    }
}

impl fmt::Display for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

impl fmt::Display for Param<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.ty)
    }
}
