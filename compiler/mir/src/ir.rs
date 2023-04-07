use arena::{Arena, Idx};
use hir_def::id::CtorId;
use hir_ty::ty::Constraint;
use rustc_hash::FxHashMap;
use triomphe::Arc;

use crate::instance::Instance;
use crate::repr::Repr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub id: hir_def::id::ModuleId,
    pub functions: FxHashMap<MirValueId, ValueDef>,
    pub statics: FxHashMap<MirValueId, ValueDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirValueId {
    Lambda(hir_def::id::ValueId, hir_def::expr::ExprId),
    ValueId(hir_def::id::ValueId),
    CtorId(hir_def::id::CtorId),
    FieldId(hir_def::id::FieldId),
}

#[salsa::tracked]
pub struct ValueDef {
    pub linkage: Linkage,
    pub name: String,
    pub body: Option<Body>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Linkage {
    #[default]
    Local,
    Import,
    Export,
}

#[salsa::tracked]
pub struct Body {
    #[return_ref]
    pub constraints: Vec<Constraint>,
    #[return_ref]
    pub locals: Arena<LocalData>,
    #[return_ref]
    pub blocks: Arena<BlockData>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub Idx<LocalData>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalData {
    pub kind: LocalKind,
    pub repr: Arc<Repr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalKind {
    Arg,
    Var,
    Tmp,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Block(pub Idx<BlockData>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockData {
    pub params: Vec<Local>,
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Location {
    pub block: Block,
    pub statement: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Terminator {
    None,
    Unreachable,
    Abort,
    Return(Operand),
    Jump(JumpTarget),
    Switch {
        discr: Operand,
        values: Vec<i128>,
        targets: Vec<JumpTarget>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JumpTarget {
    pub block: Block,
    pub args: Vec<Operand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Statement {
    Init(Local),
    Drop(Place),
    Assign(Place, RValue),
    SetDiscriminant(Place, CtorId),
    Intrinsic {
        place: Place,
        name: String,
        args: Vec<Operand>,
    },
    Call {
        place: Place,
        func: Operand,
        args: Vec<Operand>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RValue {
    Use(Operand),
    AddrOf(Place),
    Cast(CastKind, Operand),
    BinOp(BinOp, Operand, Operand),
    NullOp(NullOp, Arc<Repr>),
    Discriminant(Place),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CastKind {
    Bitcast,
    Pointer,
    IntToInt,
    FloatToFloat,
    IntToFloat,
    FloatToInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Eq,
    Ne,

    Lt,
    Le,
    Gt,
    Ge,

    Lsh,
    Rsh,
    And,
    Or,
    Xor,

    Add,
    Sub,
    Mul,
    Div,
    Rem,

    Offset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NullOp {
    SizeOf,
    AlignOf,
    StrideOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operand {
    Copy(Place),
    Move(Place),
    Const(Const, Arc<Repr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<Projection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Projection {
    Deref,
    Field(usize),
    Index(Operand),
    Slice(Operand, Operand),
    Downcast(CtorId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Const {
    Undefined,
    Zeroed,
    Unit,
    Int(i128),
    Float(u64),
    Char(char),
    String(String),
    Ctor(CtorId),
    Instance(Instance),
}

impl Block {
    pub const ENTRY: Self = Self(Idx::DUMMY);

    pub fn start_location(self) -> Location {
        Location {
            block: self,
            statement: 0,
        }
    }
}

impl Location {
    pub const START: Self = Self {
        block: Block::ENTRY,
        statement: 0,
    };

    pub fn next_stmt(self) -> Self {
        Self {
            statement: self.statement + 1,
            ..self
        }
    }
}

impl std::fmt::Debug for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Local").field(&u32::from(self.0.into_raw())).finish()
    }
}

impl std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Block").field(&u32::from(self.0.into_raw())).finish()
    }
}