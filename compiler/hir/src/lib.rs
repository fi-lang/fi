pub mod db;
pub mod diagnostic;
mod from_id;
pub mod semantics;
pub mod source_analyzer;

use base_db::input::FileId;
use base_db::libs::LibId;
use hir_def::id::*;
use hir_def::name::AsName;
pub use hir_def::name::Name;
use hir_def::pat::PatId;
use hir_ty::db::HirDatabase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lib {
    pub(crate) id: LibId,
}

#[derive(Debug)]
pub struct LibDep {
    pub lib: Lib,
    pub name: Name,
}

impl Lib {
    pub fn dependencies(self, db: &dyn HirDatabase) -> Vec<LibDep> {
        let libs = db.libs();

        libs[self.id]
            .deps
            .iter()
            .map(|&dep| {
                let lib = Lib { id: dep };
                let name = libs[dep].name.as_name();

                LibDep { lib, name }
            })
            .collect()
    }

    pub fn root_module(self, db: &dyn HirDatabase) -> Module {
        let def_map = db.def_map(self.id);

        Module {
            id: def_map.module_id(def_map.root()),
        }
    }

    pub fn root_file(self, db: &dyn HirDatabase) -> FileId {
        db.libs()[self.id].root_file
    }

    pub fn name(self, db: &dyn HirDatabase) -> Name {
        db.libs()[self.id].name.as_name()
    }

    pub fn all(db: &dyn HirDatabase) -> Vec<Lib> {
        db.libs().toposort().map(|id| Lib { id }).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Module {
    pub(crate) id: ModuleId,
}

impl Module {
    pub fn name(self, db: &dyn HirDatabase) -> Name {
        let def_map = db.def_map(self.id.lib);

        def_map[self.id.local_id].name.clone()
    }

    pub fn lib(self) -> Lib {
        Lib { id: self.id.lib }
    }

    pub fn children(self, db: &dyn HirDatabase) -> Vec<Module> {
        let def_map = db.def_map(self.id.lib);

        def_map[self.id.local_id]
            .children
            .iter()
            .map(|(_, mid)| Module {
                id: def_map.module_id(*mid),
            })
            .collect::<Vec<_>>()
    }

    pub fn parent(self, db: &dyn HirDatabase) -> Option<Module> {
        let def_map = db.def_map(self.id.lib);
        let parent_id = def_map[self.id.local_id].parent?;

        Some(Module {
            id: def_map.module_id(parent_id),
        })
    }

    pub fn path_to_root(self, db: &dyn HirDatabase) -> Vec<Module> {
        let mut res = vec![self];
        let mut curr = self;

        while let Some(next) = curr.parent(db) {
            res.push(next);
            curr = next;
        }

        res
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolution {
    Def(ModuleDef),
    Local(Local),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleDef {
    Module(Module),
    Fixity(Fixity),
    Foreign(Foreign),
    Func(Func),
    Static(Static),
    Const(Const),
    Type(Type),
    Ctor(Ctor),
    Class(Class),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fixity {
    pub(crate) id: FixityId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Foreign {
    pub(crate) id: ForeignId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Func {
    pub(crate) id: FuncId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Static {
    pub(crate) id: StaticId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Const {
    pub(crate) id: ConstId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Type {
    pub(crate) id: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ctor {
    pub(crate) id: CtorId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Class {
    pub(crate) id: ClassId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local {
    pub(crate) parent: DefWithBodyId,
    pub(crate) pat_id: PatId,
}

macro_rules! impl_from {
    ($($variant:ident),* for $ty:ident) => {
        $(
            impl From<$variant> for $ty {
                fn from(src: $variant) -> Self {
                    Self::$variant(src)
                }
            }
        )*
    };
}

impl_from!(Fixity, Foreign, Func, Static, Const, Type, Ctor, Class for ModuleDef);
