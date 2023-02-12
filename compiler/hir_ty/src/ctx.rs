use std::collections::HashMap;

use arena::ArenaMap;
use either::Either;
use hir_def::body::Body;
use hir_def::expr::ExprId;
use hir_def::id::{HasModule, TypeCtorId, TypeVarId, TypedItemId};
use hir_def::lang_item::{self, LangItem};
use hir_def::name::AsName;
use hir_def::pat::PatId;
use hir_def::type_ref::TypeRefId;
use parking_lot::RwLock;
use ra_ap_stdx::hash::{NoHashHashMap, NoHashHashSet};
use triomphe::Arc;

use crate::ty::{GeneralizedType, Ty, TyKind, Unknown};
use crate::unify::{Substitution, UnkLevel};
use crate::Db;

pub struct Ctx<'db> {
    pub(crate) db: &'db dyn Db,
    pub(crate) result: InferResult,
    pub(crate) subst: Substitution,
    pub(crate) owner: TypedItemId,
    pub(crate) level: UnkLevel,
    pub(crate) ret_ty: Ty,
}

pub struct BodyCtx<'db, 'ctx> {
    pub(crate) ctx: &'ctx mut Ctx<'db>,
    pub(crate) body: Arc<Body>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct InferResult {
    pub ty: GeneralizedType,
    pub type_of_expr: ArenaMap<ExprId, Ty>,
    pub type_of_pat: ArenaMap<PatId, Ty>,
    pub kind_of_ty: ArenaMap<TypeRefId, Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    None,
    HasType(Ty),
}

#[derive(Default, Debug)]
pub struct Cache(RwLock<CacheInner>);

#[derive(Default, Debug)]
pub struct CacheInner {}

impl<'db> Ctx<'db> {
    pub fn new(db: &'db dyn Db, owner: TypedItemId) -> Self {
        let ty = Ty::new(db, TyKind::Error);

        Self {
            db,
            owner,
            result: InferResult {
                ty: GeneralizedType::Mono(ty),
                type_of_expr: Default::default(),
                type_of_pat: Default::default(),
                kind_of_ty: Default::default(),
            },
            subst: Substitution::default(),
            level: UnkLevel(1),
            ret_ty: ty,
        }
    }

    pub fn with_body(&mut self, body: Arc<Body>) -> BodyCtx<'db, '_> {
        BodyCtx { ctx: self, body }
    }

    pub fn finish(self) -> Arc<InferResult> {
        Arc::new(self.result)
    }

    pub fn error(&self) -> Ty {
        Ty::new(self.db, TyKind::Error)
    }

    pub fn type_kind(&self) -> Ty {
        let type_kind = self.lang_ctor(lang_item::TYPE_KIND).unwrap();
        Ty::new(self.db, TyKind::Ctor(type_kind))
    }

    pub fn unit_type(&self) -> Ty {
        let unit_type = self.lang_ctor(lang_item::UNIT_TYPE).unwrap();
        Ty::new(self.db, TyKind::Ctor(unit_type))
    }

    pub fn bool_type(&self) -> Ty {
        let bool_type = self.lang_ctor(lang_item::BOOL_TYPE).unwrap();
        Ty::new(self.db, TyKind::Ctor(bool_type))
    }

    fn lang_item(&self, name: &'static str) -> Option<LangItem> {
        let lib = self.owner.module(self.db).lib(self.db);
        lang_item::query(self.db, lib, name)
    }

    fn lang_ctor(&self, name: &'static str) -> Option<TypeCtorId> {
        self.lang_item(name).and_then(LangItem::as_type_ctor)
    }

    pub fn instantiate(&mut self, ty: GeneralizedType) -> Ty {
        match ty {
            | GeneralizedType::Mono(ty) => ty,
            | GeneralizedType::Poly(vars, ty) => {
                let mut replacements = HashMap::default();

                for &var in vars.iter() {
                    replacements.insert(var, self.fresh_type(self.level));
                }

                ty.replace_vars(self.db, &replacements)
            },
        }
    }

    pub fn generalize(&mut self, ty: Ty, type_vars: &[TypeVarId]) -> GeneralizedType {
        let mut vars = NoHashHashSet::default();
        self.find_all_unknowns(ty, &mut vars);
        let vars = self.new_type_vars(vars);

        if vars.is_empty() && type_vars.is_empty() {
            GeneralizedType::Mono(ty)
        } else {
            let mut type_vars = type_vars.to_vec();

            for (u, var) in vars {
                self.subst.solved.insert(u, Ty::new(self.db, TyKind::Var(var)));
                type_vars.push(var);
            }

            GeneralizedType::Poly(type_vars.into(), ty)
        }
    }

    pub fn find_all_unknowns(&mut self, ty: Ty, res: &mut NoHashHashSet<Unknown>) {
        match ty.kind(self.db) {
            | TyKind::Unknown(u) => match self.find_binding(*u) {
                | Ok(t) => self.find_all_unknowns(t, res),
                | Err((level, _)) => {
                    if level >= self.level {
                        res.insert(*u);
                    }
                },
            },
            | TyKind::App(base, args) => {
                self.find_all_unknowns(*base, res);
                for &arg in args.iter() {
                    self.find_all_unknowns(arg, res);
                }
            },
            | TyKind::Func(func) => {
                for &param in func.params.iter() {
                    self.find_all_unknowns(param, res);
                }

                self.find_all_unknowns(func.ret, res);
                self.find_all_unknowns(func.env, res);
            },
            | _ => {},
        }
    }

    fn new_type_vars(&self, unknowns: NoHashHashSet<Unknown>) -> NoHashHashMap<Unknown, TypeVarId> {
        let mut res = NoHashHashMap::default();

        for u in unknowns {
            let mut name = String::with_capacity(2);
            let mut n = u.0;

            unsafe {
                name.push('\'');
                while n >= 24 {
                    name.push(char::from_u32_unchecked('a' as u32 + n % 24));
                    n -= 24;
                }
                name.push(char::from_u32_unchecked('a' as u32 + n));
            }

            let name = name.as_name(self.db);
            let var = TypeVarId::new(self.db, self.owner, Either::Right(name));

            res.insert(u, var);
        }

        res
    }
}

impl<'db> std::ops::Deref for BodyCtx<'db, '_> {
    type Target = Ctx<'db>;

    fn deref(&self) -> &Self::Target {
        self.ctx
    }
}

impl<'db> std::ops::DerefMut for BodyCtx<'db, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ctx
    }
}

impl Cache {
    pub fn clear(&self) {
        self.0.write().clear();
    }
}

impl CacheInner {
    fn clear(&mut self) {
    }
}

impl Expectation {
    pub fn adjust_for_branches(self, db: &dyn Db) -> Self {
        match self {
            | Self::HasType(ty) => {
                if let TyKind::Unknown(_) = ty.kind(db) {
                    Self::None
                } else {
                    Self::HasType(ty)
                }
            },
            | _ => Self::None,
        }
    }

    pub fn to_option(self) -> Option<Ty> {
        match self {
            | Self::HasType(ty) => Some(ty),
            | _ => None,
        }
    }
}