use super::{ExprOrPatId, InferenceContext, InferenceDiagnostic, MethodSource};
use crate::class::{ClassEnvScope, Members};
use crate::info::CtntInfo;

impl InferenceContext<'_> {
    pub fn solve_constraints(&mut self) {
        let n_constraints = self.constraints.len();
        let mut unsolved = std::mem::replace(&mut self.constraints, Vec::with_capacity(n_constraints));
        let mut n_solved = 1;

        while !unsolved.is_empty() && n_solved > 0 {
            n_solved = 0;

            for (ctnt, id, scope) in unsolved {
                if self.solve_constraint(ctnt, id, scope) {
                    n_solved += 1;
                }
            }

            unsolved = std::mem::replace(&mut self.constraints, Vec::with_capacity(n_constraints));
        }

        for (ctnt, id, scope) in unsolved {
            if ctnt.can_be_generalized(&self.types) {
                self.constraints.push((ctnt, id, scope));

                if let ExprOrPatId::ExprId(expr) = id {
                    self.result
                        .methods
                        .insert(expr, MethodSource::Record(self.member_records));
                    self.member_records += 1;
                }
            } else {
                self.report(InferenceDiagnostic::UnsolvedConstraint { id, ctnt });
            }
        }
    }

    fn solve_constraint(&mut self, ctnt: CtntInfo, id: ExprOrPatId, scope: Option<ClassEnvScope>) -> bool {
        let ctnt = self.subst_ctnt(&ctnt);
        let src = self.source(id);

        if let Some(res) = self.class_env.solve(self.db, &self.types, ctnt.clone(), scope) {
            for (&u, &ty) in res.subst.iter() {
                self.solve_type(u, ty);
            }

            let entry = &self.class_env[res.scope];

            if entry.is_method() {
                if let ExprOrPatId::ExprId(expr) = id {
                    self.result
                        .methods
                        .insert(expr, MethodSource::Record(self.member_records));
                    self.member_records += 1;
                }
            }

            true
        } else if let Some(res) = Members::solve_constraint(self.db, &mut self.types, &ctnt, src) {
            res.apply(self);

            if let ExprOrPatId::ExprId(expr) = id {
                self.result.methods.insert(expr, MethodSource::Member(res.member));
            }

            true
        } else {
            self.constraints.push((ctnt, id, scope));
            false
        }
    }
}
