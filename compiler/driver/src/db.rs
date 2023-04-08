use parking_lot::RwLock;

use crate::Options;

#[salsa::db(
    vfs::Jar,
    ::diagnostics::Jar,
    base_db::Jar,
    hir_def::Jar,
    hir_ty::Jar,
    hir::Jar,
    mir::Jar,
    codegen::Jar
)]
pub struct Database {
    storage: salsa::Storage<Self>,
    syntax_interner: RwLock<syntax::Interner>,
    type_cache: hir_ty::ctx::Cache,
    libs: base_db::libs::LibSet,
    options: Options,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            storage: Default::default(),
            syntax_interner: RwLock::new(syntax::new_interner()),
            type_cache: Default::default(),
            libs: Default::default(),
            options: Default::default(),
        }
    }
}

impl Database {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            ..Default::default()
        }
    }
}

impl salsa::Database for Database {
}

impl base_db::Db for Database {
    fn syntax_interner(&self) -> &RwLock<syntax::Interner> {
        &self.syntax_interner
    }

    fn libs(&self) -> &base_db::libs::LibSet {
        &self.libs
    }
}

impl hir_ty::Db for Database {
    fn type_cache(&self) -> &hir_ty::ctx::Cache {
        &self.type_cache
    }
}

impl codegen::Db for Database {
    fn target(&self) -> &codegen::target::Target {
        &self.options.target
    }

    fn target_dir(&self) -> &std::path::Path {
        &self.options.target_dir
    }

    fn optimization_level(&self) -> codegen::OptimizationLevel {
        self.options.optimization_level
    }
}
