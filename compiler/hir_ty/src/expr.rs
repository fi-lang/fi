use hir_def::expr::{Expr, ExprId, Literal, Stmt};
use hir_def::id::ValueDefId;
use hir_def::name::Name;
use hir_def::pat::PatId;

use crate::ctx::{BodyCtx, Expectation};
use crate::lower::LowerCtx;
use crate::ty::{ConstraintOrigin, FuncType, Ty, TyKind};

impl BodyCtx<'_, '_> {
    pub fn infer_expr(&mut self, id: ExprId, expected: Expectation) -> Ty {
        let ty = self.infer_expr_inner(id, expected);

        if let Expectation::HasType(expected) = expected {
            self.unify_types(ty, expected, id.into());

            if let TyKind::Error = self.resolve_type_shallow(ty).kind(self.db) {
                return expected;
            }
        }

        ty
    }

    fn infer_expr_inner(&mut self, id: ExprId, expected: Expectation) -> Ty {
        if let Some(&ty) = self.result.type_of_expr.get(id) {
            return ty;
        }

        let body = self.body.clone();
        let ty = match &body[id] {
            | Expr::Missing => self.error(),
            | Expr::Typed { expr, ty } => {
                let (type_map, _, _) = self.owner.type_map(self.db);
                let mut lcx = LowerCtx::new(self, type_map);
                let ty = lcx.lower_type_ref(*ty, false);

                self.infer_expr(*expr, Expectation::HasType(ty));
                ty
            },
            | Expr::Lit { lit } => match lit {
                | Literal::Int(_) => {
                    let kind = self.int_tag_kind();
                    let var = self.ctx.fresh_type_with_kind(self.level, kind, false);
                    let int = self.int_type();

                    Ty::new(self.db, TyKind::App(int, Box::new([var])))
                },
                | Literal::Float(_) => {
                    let kind = self.float_tag_kind();
                    let var = self.ctx.fresh_type_with_kind(self.level, kind, false);
                    let float = self.float_type();

                    Ty::new(self.db, TyKind::App(float, Box::new([var])))
                },
                | Literal::Char(_) => self.char_type(),
                | Literal::String(_) => self.str_type(),
            },
            | Expr::Recur => {
                self.recursive_calls.push(id);
                match self.lambdas.last() {
                    | Some(ty) => *ty,
                    | None => {
                        let ty = self.result.ty.clone();
                        self.instantiate(ty, Vec::new(), Some(id), false).0
                    },
                }
            },
            | Expr::Block { stmts, expr } => self.infer_block(stmts, *expr, expected),
            | Expr::Path { def: None, .. } => self.error(),
            | Expr::Path { def: Some(def), path } => self.infer_value_def_id(id, *def, path.segments().last().copied()),
            | Expr::Array { exprs } => self.infer_array(id, exprs, expected),
            | Expr::Lambda { env, params, body } => self.infer_lambda(id, env, params, *body, expected),
            | Expr::App { base, args } => self.infer_app(id, *base, args),
            | Expr::If { cond, then, else_ } => self.infer_if(id, *cond, *then, *else_, expected),
            | Expr::Match {
                expr,
                branches,
                decision_tree: _,
            } => {
                let expected = expected.adjust_for_branches(self.db);
                let pred = self.infer_expr(*expr, Expectation::None);
                let res = self.ctx.fresh_type(self.level, false);

                for &(pat, branch) in branches.iter() {
                    self.infer_pat(pat, Expectation::HasType(pred));
                    let ty = self.infer_expr_inner(branch, expected);
                    self.coerce(ty, res, branch.into());
                }

                res
            },
            | Expr::Return { expr } => {
                let ret_ty = *self.ret_ty.last().unwrap();
                self.infer_expr(*expr, Expectation::HasType(ret_ty));
                self.never_type()
            },
            | e => todo!("{e:?}"),
        };

        self.result.type_of_expr.insert(id, ty);
        ty
    }

    fn resolve_value_def_id(&mut self, def: ValueDefId, name: Option<Name>) -> Option<(ValueDefId, Option<Name>)> {
        match def {
            | ValueDefId::FixityId(id) => {
                let data = hir_def::data::fixity_data(self.db, id);
                let name = data.def_path(self.db).segments().last().copied();

                match data.def(self.db) {
                    | Some(def) => self.resolve_value_def_id(def.unwrap_left(), name),
                    | None => None,
                }
            },
            | _ => Some((def, name)),
        }
    }

    fn infer_value_def_id(&mut self, expr: ExprId, def: ValueDefId, name: Option<Name>) -> Ty {
        let (ty, name, constraints) = match self.resolve_value_def_id(def, name) {
            | Some((def, name)) => match def {
                | ValueDefId::ValueId(id) if self.owner == id.into() => {
                    self.recursive_calls.push(expr);
                    (self.result.ty.clone(), name, Vec::new())
                },
                | ValueDefId::ValueId(id) => {
                    let infer = crate::infer(self.db, id);
                    (infer.ty.clone(), name, infer.constraints.clone())
                },
                | ValueDefId::FixityId(_) => unreachable!(),
                | ValueDefId::CtorId(id) => (crate::ctor_ty(self.db, id), name, Vec::new()),
                | ValueDefId::PatId(id) => return self.result.type_of_pat[id],
                | d => todo!("{d:?}"),
            },
            | None => return self.error(),
        };

        let (ty, constraints) = self.instantiate(ty, constraints, Some(expr), false);

        for constraint in constraints {
            self.constrain(constraint, ConstraintOrigin::ExprId(expr, name));
        }

        ty
    }

    fn infer_array(&mut self, id: ExprId, exprs: &[ExprId], expected: Expectation) -> Ty {
        let len = Ty::new(self.db, TyKind::Literal(Literal::Int(exprs.len() as i128)));
        let array = self.array_type();

        if let Expectation::HasType(ty) = expected {
            if let &TyKind::App(base, ref args) = ty.kind(self.db) {
                if base == array {
                    self.unify_types(len, args[0], id.into());
                    for &expr in exprs {
                        self.infer_expr(expr, Expectation::HasType(args[1]));
                    }
                    return ty;
                } else if base == self.slice_type() {
                    for &expr in exprs {
                        self.infer_expr(expr, Expectation::HasType(args[0]));
                    }
                    return ty;
                }
            }
        }

        let elem = self.ctx.fresh_type(self.ctx.level, false);
        let args = Box::new([len, elem]);

        for &expr in exprs {
            self.infer_expr(expr, Expectation::HasType(elem));
        }

        Ty::new(self.db, TyKind::App(array, args))
    }

    fn infer_lambda(&mut self, id: ExprId, env: &[PatId], params: &[PatId], body: ExprId, expected: Expectation) -> Ty {
        let env = env.iter().map(|&p| self.result.type_of_pat[p]).collect();
        let env = self.tuple_type(env);

        if let Expectation::HasType(ty) = expected {
            if let TyKind::Func(func) = ty.kind(self.db) {
                for (&param, &ty) in params.iter().zip(func.params.iter()) {
                    self.infer_pat(param, Expectation::HasType(ty));
                }

                self.unify_types(env, func.env, id.into());
                self.ret_ty.push(func.ret);
                self.lambdas.push(ty);
                self.infer_expr(body, Expectation::HasType(func.ret));
                self.ret_ty.pop().unwrap();
                self.lambdas.pop().unwrap();
                return ty;
            }
        }

        let params = params.iter().map(|&p| self.infer_pat(p, Expectation::None)).collect();
        let ret = self.ctx.fresh_type(self.ctx.level, false);
        let ty = Ty::new(
            self.db,
            TyKind::Func(FuncType {
                is_varargs: false,
                env,
                params,
                ret,
            }),
        );

        self.ret_ty.push(ret);
        self.lambdas.push(ty);
        self.infer_expr_inner(body, Expectation::HasType(ret));
        self.ret_ty.pop().unwrap();
        self.lambdas.pop().unwrap();
        ty
    }

    fn infer_app(&mut self, id: ExprId, base: ExprId, args: &[ExprId]) -> Ty {
        let func_ty = self.infer_expr_inner(base, Expectation::None);

        if let TyKind::Func(func) = func_ty.kind(self.db) {
            return self.infer_call(id, base, func, args);
        }

        let params = args
            .iter()
            .map(|a| self.infer_expr_inner(*a, Expectation::None))
            .collect();
        let ret = self.ctx.fresh_type(self.level, false);
        let new_func = Ty::new(
            self.db,
            TyKind::Func(FuncType {
                params,
                ret,
                env: self.ctx.fresh_type(self.level, false),
                is_varargs: false,
            }),
        );

        self.unify_types(func_ty, new_func, base.into());
        ret
    }

    fn infer_call(&mut self, id: ExprId, base: ExprId, func: &FuncType, args: &[ExprId]) -> Ty {
        let value = match self.body[base] {
            | Expr::Path { def: Some(def), .. } => match self.resolve_value_def_id(def, None) {
                | Some((ValueDefId::ValueId(id), _)) => Some(id),
                | _ => None,
            },
            | _ => None,
        };

        let attrs = value.map(|id| hir_def::attrs::query(self.db, id.into()));
        let deref = attrs.map(|a| a.by_key("deref").exists()).unwrap_or_default();

        for (&arg, &ty) in args.iter().zip(func.params.iter()) {
            self.infer_expr(arg, Expectation::HasType(ty));
        }

        if args.len() <= func.params.len() {
            return self.infer_ret(id, deref, func.ret);
        }

        if func.is_varargs {
            args[func.params.len()..]
                .iter()
                .map(|&e| self.infer_expr(e, Expectation::None))
                .count();

            return self.infer_ret(id, deref, func.ret);
        }

        let ret = self.ctx.fresh_type(self.ctx.level, false);
        let params = args[func.params.len()..]
            .iter()
            .map(|&e| self.infer_expr(e, Expectation::None))
            .collect::<Vec<_>>();
        let func2 = params.into_iter().rfold(ret, |ret, param| {
            let env = self.ctx.fresh_type(self.ctx.level, false);

            Ty::new(
                self.db,
                TyKind::Func(FuncType {
                    env,
                    ret,
                    params: Box::new([param]),
                    is_varargs: false,
                }),
            )
        });

        self.unify_types(func.ret, func2, id.into());
        ret
    }

    fn infer_ret(&mut self, id: ExprId, deref: bool, ty: Ty) -> Ty {
        match ty.kind(self.db) {
            | TyKind::Ref(_, to) if deref => *to,
            | _ if deref => {
                let var = self.ctx.fresh_type(self.ctx.level, false);
                let lt = self.ctx.fresh_lifetime(self.ctx.level);
                let ref_ty = Ty::new(self.db, TyKind::Ref(lt, var));
                self.unify_types(ty, ref_ty, id.into());
                var
            },
            | _ => ty,
        }
    }

    fn infer_if(&mut self, id: ExprId, cond: ExprId, then: ExprId, else_: Option<ExprId>, expected: Expectation) -> Ty {
        let expected = expected.adjust_for_branches(self.db);
        let bool_type = self.bool_type();

        self.infer_expr(cond, Expectation::HasType(bool_type));

        let result_ty = self.ctx.fresh_type(self.level, false);
        let then_ty = self.infer_expr_inner(then, expected);
        let else_ty = match else_ {
            | Some(else_) => self.infer_expr_inner(else_, expected),
            | None => self.unit_type(),
        };

        self.coerce(then_ty, result_ty, id.into());
        self.coerce(else_ty, result_ty, id.into());
        result_ty
    }

    fn infer_block(&mut self, stmts: &[Stmt], expr: Option<ExprId>, expected: Expectation) -> Ty {
        for stmt in stmts {
            match *stmt {
                | Stmt::Expr(e) => {
                    self.infer_expr_inner(e, Expectation::None);
                },
                | Stmt::Let(p, e) => {
                    let ty = self.infer_pat(p, Expectation::None);
                    self.infer_expr(e, Expectation::HasType(ty));
                },
            }
        }

        if let Some(expr) = expr {
            return self.infer_expr(expr, expected);
        }

        self.unit_type()
    }
}
