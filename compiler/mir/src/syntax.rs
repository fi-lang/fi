use arena::{Arena, Idx};
use hir::id::DefWithBodyId;
use hir::ty::TypeVar;
use hir::{Ctor, ExprId, Func};
use rustc_hash::FxHashMap;
use salsa::{InternId, InternKey};

use crate::instance::Instance;
use crate::repr::Repr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub hir: hir::Module,
    pub functions: FxHashMap<Func, Function>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Linkage {
    #[default]
    Local,
    Import,
    Export,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Function {
    pub linkage: Linkage,
    pub name: String,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Body(InternId);

impl InternKey for Body {
    fn from_intern_id(v: InternId) -> Self {
        Self(v)
    }

    fn as_intern_id(&self) -> InternId {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyOrigin {
    pub def: DefWithBodyId,
    pub expr: Option<ExprId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BodyData {
    pub origin: BodyOrigin,
    pub locals: Arena<LocalData>,
    pub blocks: Arena<BlockData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub Idx<LocalData>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalData {
    pub id: Local,
    pub kind: LocalKind,
    pub repr: Repr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalKind {
    Arg,
    Var,
    Tmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Block(pub Idx<BlockData>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockData {
    pub id: Block,
    pub params: Vec<Local>,
    pub stmts: Vec<Stmt>,
    pub term: Term,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    /// No terminator, must be replaced before codegen.
    None,

    /// Indicates that this code path should never be reached.
    Unreachable,

    /// Stop execution and exit the current process.
    Abort,

    /// Return the given value.
    Return(Operand),

    /// Jump to the given target.
    Jump(JumpTarget),

    /// switch over the given discriminant value.
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
pub enum Stmt {
    /// Creates a new instance of the type for the given local.
    /// Does nothing for trivial types.
    Init(Local),

    /// Deletes the value of the given local.
    /// Does nothing for trivial types.
    Drop(Local),

    /// Assign a value to the given place.
    Assign(Place, Rvalue),

    /// Set the discriminant value of the place.
    SetDiscriminant(Place, Ctor),

    /// Call the given intrinsic function with the arguments and store the result in place.
    Intrinsic {
        place: Place,
        func: String,
        args: Vec<Operand>,
    },

    /// Call the given function with the arguments and store the result in place.
    Call {
        place: Place,
        func: Operand,
        args: Vec<Operand>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Rvalue {
    /// Use the value of the given operand.
    Use(Operand),

    /// Get the address of the place.
    Ref(Place),

    /// Get the discriminant value of the place.
    Discriminant(Place),

    /// Cast to a different type.
    Cast(CastKind, Operand),

    /// Create a reference to the given instance.
    InstanceRef(Instance),

    /// A binary operation (add, sub, mul, div, etc.).
    BinOp(BinOp, Operand, Operand),

    /// A nullary operation (size_of, align_of, stride_of).
    NullOp(NullOp, Repr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CastKind {
    Bitcast,
    Pointer,
    IntToInt,
    FloatToFloat,
    IntToFloat,
    FloatToInt,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NullOp {
    SizeOf,
    AlignOf,
    StrideOf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Operand {
    /// Load the value at the place.
    /// This may perform extra operations for non-trivial types.
    Copy(Place),

    /// Load the value at the place.
    Move(Place),

    /// A constant value.
    Const(Const, Repr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<Projection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Projection {
    /// Dereference the pointer.
    Deref,

    /// Get a reference to a specific field of this place.
    Field(usize),

    /// Get a reference to a specific index of this place.
    Index(Operand),

    /// Get a subslice of this place.
    Slice(Operand, Operand),

    /// Downcast the type of this place to the given variant.
    Downcast(Ctor),
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
    Ctor(Ctor),
    TypeVar(TypeVar),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Location {
    pub block: Block,
    pub stmt: usize,
}

impl Place {
    pub fn has_deref(&self) -> bool {
        self.projection
            .first()
            .map(|f| matches!(f, Projection::Deref))
            .unwrap_or(false)
    }
}

impl Block {
    pub const ENTRY: Self = Self(Idx::DUMMY);

    pub fn start_location(self) -> Location {
        Location { block: self, stmt: 0 }
    }
}

impl Location {
    pub const START: Self = Self {
        block: Block::ENTRY,
        stmt: 0,
    };

    pub fn next_stmt(self) -> Self {
        Self {
            stmt: self.stmt + 1,
            ..self
        }
    }
}
