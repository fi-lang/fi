use std::fmt::Write;

pub use hir_ty::display::*;

use crate::*;

impl HirDisplay for Module {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        write!(f, "module {}", self.name(f.db))
    }
}

impl HirDisplay for Fixity {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        let func_name = self.func(f.db).path(f.db);
        let name = self.name(f.db);

        match self.kind(f.db) {
            | FixityKind::Infix { assoc, prec } => {
                match assoc {
                    | Assoc::None => write!(f, "infix")?,
                    | Assoc::Left => write!(f, "infixl")?,
                    | Assoc::Right => write!(f, "infixr")?,
                }

                write!(f, " {} {} as {}", prec, func_name, name)
            },
            | FixityKind::Prefix => write!(f, "prefix {} as {}", func_name, name),
            | FixityKind::Postfix => write!(f, "postfix {} as {}", func_name, name),
        }
    }
}

impl HirDisplay for Func {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        if self.is_foreign(f.db) {
            write!(f, "foreign ")?;
        }

        let infer = f.db.infer(self.id.into());

        write!(f, "{} :: {}", self.name(f.db), infer.self_type.ty.display(f.db))
    }
}

impl HirDisplay for TypeAlias {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        let lower = f.db.type_for_alias(self.id);
        let ty = lower.ty.ty.display(f.db);

        write!(f, "type {} = {}", self.name(f.db), ty)
    }
}

impl HirDisplay for TypeCtor {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        let kind = f.db.kind_for_ctor(self.id);
        let kind = kind.ty.ty.display(f.db);

        if self.is_foreign(f.db) {
            write!(f, "foreign ")?;
        }

        write!(f, "type {} :: {}", self.name(f.db), kind)
    }
}

impl HirDisplay for Class {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        write!(f, "class {}", self.name(f.db))
    }
}

impl HirDisplay for TypeVar {
    fn hir_fmt(&self, f: &mut HirFormatter) -> std::fmt::Result {
        let kind = self.kind(f.db);
        let kind = kind.display(f.db);

        write!(f, "type {} :: {}", self.name(f.db), kind)
    }
}
