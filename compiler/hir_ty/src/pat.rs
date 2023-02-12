use hir_def::expr::Literal;
use hir_def::pat::{Pat, PatId};

use crate::ctx::{BodyCtx, Expectation};
use crate::lower::LowerCtx;
use crate::ty::{Ty, TyKind};

impl BodyCtx<'_, '_> {
    pub fn infer_pat(&mut self, id: PatId, expected: Expectation) -> Ty {
        let ty = self.infer_pat_inner(id, expected);

        if let Expectation::HasType(expected) = expected {
            self.unify_types(ty, expected, id.into());

            if matches!(self.resolve_type_shallow(ty).kind(self.db), TyKind::Error) {
                return expected;
            }
        }

        ty
    }

    fn infer_pat_inner(&mut self, id: PatId, expected: Expectation) -> Ty {
        let body = self.body.clone();
        let ty = match &body[id] {
            | Pat::Missing => self.error(),
            | Pat::Wildcard => self.ctx.fresh_type(self.level),
            | Pat::Bind { subpat: None, .. } => self.ctx.fresh_type(self.level),
            | Pat::Bind {
                subpat: Some(subpat), ..
            } => self.infer_pat(*subpat, expected),
            | Pat::Ctor { ctor: None, .. } => self.error(),
            | Pat::Ctor {
                ctor: Some(def), args, ..
            } => {
                let ty = crate::ctor_ty(self.db, *def);
                let ty = self.instantiate(ty);
                let _ = args;

                ty
            },
            | Pat::Lit { lit } => match lit {
                | Literal::Int(_) => {
                    let var = self.ctx.fresh_type(self.level);
                    // TODO: add AnyInt constraint
                    var
                },
                | l => todo!("{l:?}"),
            },
            | Pat::Typed { pat, ty } => {
                let (type_map, _, _) = self.owner.type_map(self.db);
                let mut lcx = LowerCtx::new(self, type_map);
                let ty = lcx.lower_type_ref(*ty);

                self.infer_pat(*pat, Expectation::HasType(ty))
            },
        };

        self.result.type_of_pat.insert(id, ty);
        ty
    }
}