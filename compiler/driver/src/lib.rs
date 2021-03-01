pub mod db;
pub mod diagnostics;
pub mod manifest;

use base_db::input::SourceRoot;
use base_db::libs::LibSet;
use base_db::SourceDatabase;
use base_db::SourceDatabaseExt;
use hir::db::DefDatabase;
use syntax::ast::{self, AstNode, NameOwner};

pub fn build() {
    let mut rdb = db::RootDatabase::default();
    let mut libs = LibSet::default();
    let path = std::path::PathBuf::from("test");

    match manifest::load_project(&mut rdb, &mut libs, &mut 0, &mut 0, &path) {
        | Ok(_lib) => {
            rdb.set_libs(libs.into());

            let start = std::time::Instant::now();

            for lib in rdb.libs().toposort() {
                let lib_data = &rdb.libs()[lib];

                println!("  \x1B[1;32m\x1B[1mCompiling\x1B[0m {}", lib_data.name);

                diagnostics::emit_diagnostics(&rdb, lib, &mut std::io::stderr());

                // let def_map = rdb.def_map(lib);
                //
                // def_map.dump(&mut std::io::stdout()).unwrap();
            }

            let elapsed = start.elapsed();

            println!("   \x1B[1;32m\x1B[1mFinished\x1B[0m in {:?}", elapsed);
        },
        | Err(e) => {
            eprintln!("{}", e);
        },
    }
}
