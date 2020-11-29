pub mod abi;
pub mod analyze;
pub mod assembly;

use mir::ir as mir;
use std::collections::HashMap;
use std::sync::Arc;

pub trait Backend: Sized {
    type Module;
    type Context;
    type Builder;
    type Func: Copy;
    type Static: Copy;
    type Block: Copy;
    type Place: Place<Backend = Self> + Clone;
    type Value: Value<Backend = Self>;
    type Type: Type<Backend = Self>;

    fn create_module(&mut self, lib: source::LibId, db: &dyn ::mir::MirDatabase) -> Self::Module;
    fn create_ctx(&mut self, module: &mut Self::Module) -> Self::Context;

    fn declare_static(mcx: &mut ModuleCtx<Self>, body: &mir::Body) -> Self::Static;
    fn declare_func(mcx: &mut ModuleCtx<Self>, body: &mir::Body) -> Self::Func;

    fn trans_place(fx: &mut FunctionCtx<Self>, place: &mir::Place) -> Self::Place;
    fn trans_const(fx: &mut FunctionCtx<Self>, const_: &mir::Const, ty: &mir::Ty) -> Self::Value;
    fn trans_rvalue(fx: &mut FunctionCtx<Self>, place: Self::Place, rvalue: &mir::RValue);
    fn trans_term(fx: &mut FunctionCtx<Self>, term: &mir::Term);

    fn trans_op(fx: &mut FunctionCtx<Self>, op: &mir::Operand) -> Self::Value {
        match op {
            mir::Operand::Move(place) => Self::trans_place(fx, place).to_value(fx),
            mir::Operand::Copy(place) => Self::trans_place(fx, place).to_value(fx),
            mir::Operand::Const(const_, ty) => Self::trans_const(fx, const_, ty),
        }
    }
}

pub trait Place: Sized {
    type Backend: Backend;

    fn layout(&self) -> &layout::TyLayout<mir::Ty>;

    fn to_value(self, fx: &mut FunctionCtx<Self::Backend>) -> <Self::Backend as Backend>::Value;

    fn deref(self, fx: &mut FunctionCtx<Self::Backend>) -> Self;

    fn index(
        self,
        fx: &mut FunctionCtx<Self::Backend>,
        idx: <Self::Backend as Backend>::Value,
    ) -> Self;

    fn field(self, fx: &mut FunctionCtx<Self::Backend>, idx: usize) -> Self;

    fn store(self, fx: &mut FunctionCtx<Self::Backend>, from: <Self::Backend as Backend>::Value);

    fn write_place_ref(self, fx: &mut FunctionCtx<Self::Backend>, dest: Self);

    fn downcast_variant(self, fx: &mut FunctionCtx<Self::Backend>, variant: usize) -> Self;
}

pub trait Value: Sized {
    type Raw;
    type Backend: Backend;

    fn layout(&self) -> &layout::TyLayout<mir::Ty>;

    fn load_scalar(self, fx: &mut FunctionCtx<Self::Backend>) -> Self::Raw;

    fn load_scalar_pair(self, fx: &mut FunctionCtx<Self::Backend>) -> (Self::Raw, Self::Raw);

    fn cast(self, fx: &mut FunctionCtx<Self::Backend>, layout: layout::TyLayout<mir::Ty>) -> Self;

    fn field(self, fx: &mut FunctionCtx<Self::Backend>, idx: usize) -> Self;

    fn deref(self, fx: &mut FunctionCtx<Self::Backend>) -> Self;
}

pub trait Type: Sized {
    type Backend: Backend;
    type Raw: Copy;

    fn ir_type(
        layout: &layout::TyLayout<mir::Ty>,
        mcx: &ModuleCtx<Self::Backend>,
    ) -> Option<Self::Raw>;

    fn ir_pair_type(
        layout: &layout::TyLayout<mir::Ty>,
        mcx: &ModuleCtx<Self::Backend>,
    ) -> Option<(Self::Raw, Self::Raw)>;

    fn scalar_ty(scalar: &layout::Scalar, mcx: &ModuleCtx<Self::Backend>) -> Self::Raw;
}

pub struct ModuleCtx<'db, B: Backend> {
    pub db: &'db dyn ::mir::MirDatabase,
    pub backend: B,
    pub ctx: B::Context,
    pub module: B::Module,
    pub lib: source::LibId,
    pub mir: Arc<mir::Module>,
}

pub struct FunctionCtx<'db, 'mcx, B: Backend> {
    pub mcx: &'mcx mut ModuleCtx<'db, B>,
    pub bcx: B::Builder,
    pub body: &'db mir::Body,
    pub blocks: HashMap<mir::Block, B::Block>,
    pub locals: HashMap<mir::Local, B::Place>,
}

impl<'db, B: Backend> ModuleCtx<'db, B> {
    pub fn new(
        db: &'db dyn ::mir::MirDatabase,
        lib: source::LibId,
        mir: Arc<mir::Module>,
        mut backend: B,
    ) -> Self {
        let mut module = backend.create_module(lib, db);
        let ctx = backend.create_ctx(&mut module);

        ModuleCtx {
            db,
            lib,
            mir,
            ctx,
            module,
            backend,
        }
    }
}

impl<'db, 'mcx, B: Backend> FunctionCtx<'db, 'mcx, B> {
    pub fn ir_type(&self, layout: &layout::TyLayout<mir::Ty>) -> Option<<B::Type as Type>::Raw> {
        <B::Type as Type>::ir_type(layout, self.mcx)
    }

    pub fn ir_pair_type(
        &self,
        layout: &layout::TyLayout<mir::Ty>,
    ) -> Option<(<B::Type as Type>::Raw, <B::Type as Type>::Raw)> {
        <B::Type as Type>::ir_pair_type(layout, self.mcx)
    }

    pub fn scalar_ty(&self, scalar: &layout::Scalar) -> <B::Type as Type>::Raw {
        <B::Type as Type>::scalar_ty(scalar, self.mcx)
    }
}

impl<'db, B: Backend> std::ops::Deref for ModuleCtx<'db, B> {
    type Target = B;

    fn deref(&self) -> &Self::Target {
        &self.backend
    }
}

impl<'db, B: Backend> std::ops::DerefMut for ModuleCtx<'db, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend
    }
}

impl<'db, 'mcx, B: Backend> std::ops::Deref for FunctionCtx<'db, 'mcx, B> {
    type Target = ModuleCtx<'db, B>;

    fn deref(&self) -> &Self::Target {
        self.mcx
    }
}

impl<'db, 'mcx, B: Backend> std::ops::DerefMut for FunctionCtx<'db, 'mcx, B> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mcx
    }
}
