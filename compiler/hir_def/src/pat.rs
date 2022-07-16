use arena::Idx;

use crate::expr::{Literal, RecordField};
use crate::name::Name;
use crate::path::Path;
use crate::type_ref::LocalTypeRefId;

pub type PatId = Idx<Pat>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat {
    Missing,
    Wildcard,
    Typed {
        pat: PatId,
        ty: LocalTypeRefId,
    },
    Infix {
        pats: Box<[PatId]>,
        ops: Box<[Path]>,
    },
    App {
        base: PatId,
        args: Box<[PatId]>,
    },
    Path {
        path: Path,
    },
    Bind {
        name: Name,
        subpat: Option<PatId>,
    },
    Tuple {
        pats: Box<[PatId]>,
    },
    Record {
        fields: Box<[RecordField<PatId>]>,
        has_rest: bool,
    },
    Lit {
        lit: Literal,
    },
}

impl Pat {
    pub fn walk(&self, mut f: impl FnMut(PatId)) {
        match self {
            | Pat::Missing | Pat::Wildcard | Pat::Lit { .. } | Pat::Path { .. } | Pat::Bind { subpat: None, .. } => {},
            | Pat::Typed { pat, .. } => f(*pat),
            | Pat::Infix { pats, .. } => {
                pats.iter().copied().for_each(f);
            },
            | Pat::App { base, args } => {
                f(*base);
                args.iter().copied().for_each(f);
            },
            | Pat::Bind {
                subpat: Some(subpat), ..
            } => f(*subpat),
            | Pat::Tuple { pats } => {
                pats.iter().copied().for_each(f);
            },
            | Pat::Record { fields, .. } => {
                fields.iter().map(|f| f.val).for_each(f);
            },
        }
    }
}
