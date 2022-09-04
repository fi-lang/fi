use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use base_db::input::SourceRootId;
use hir::cfg::CfgOptions;
use hir::db::HirDatabase;
use hir::id::LibId;
use relative_path::RelativePath;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Metadata {
    version: usize,

    #[serde(skip, default)]
    lib: LibId,
    files: FxHashMap<String, SystemTime>,
}

#[derive(Default, Debug)]
pub struct InputPaths {
    pub target_dir: PathBuf,
    pub source_roots: FxHashMap<SourceRootId, PathBuf>,
}

impl Metadata {
    const EXTENSION: &'static str = "metadata";

    pub fn has_changed(&self, db: &dyn HirDatabase, paths: &InputPaths) -> bool {
        if self.version != 0 {
            return true;
        }

        let source_root = db.libs()[self.lib].source_root;
        let dir = &paths.source_roots[&source_root];
        let source_root = db.source_root(source_root);
        let mut checked = FxHashSet::default();

        if self.file_changed(&mut checked, dir, RelativePath::new("shadow.toml")) {
            return true;
        }

        for (_, path) in source_root.iter() {
            if self.file_changed(&mut checked, dir, path) {
                return true;
            }
        }

        let files = self.files.keys().map(|f| f.as_str()).collect();
        let mut diff = checked.difference(&files);

        diff.next().is_some()
    }

    fn file_changed<'a>(&self, checked: &mut FxHashSet<&'a str>, dir: &PathBuf, path: &'a RelativePath) -> bool {
        match self.files.get(path.as_str()) {
            | None => return true,
            | Some(&timestamp) => {
                let path_buf = path.to_logical_path(dir);
                let meta = match path_buf.metadata() {
                    | Err(_) => return true,
                    | Ok(m) => m,
                };

                let t = match meta.modified() {
                    | Err(_) => return true,
                    | Ok(t) => t,
                };

                if t != timestamp {
                    return true;
                }
            },
        }

        checked.insert(path.as_str());
        false
    }
}

pub fn read_metadata(db: &dyn HirDatabase, lib: LibId, paths: &InputPaths) -> Option<Arc<Metadata>> {
    let metadata_dir = paths.target_dir.join(metadata_name(db, lib));

    if let Ok(mut file) = File::open(metadata_dir) {
        let config = bincode::config::standard();
        let mut meta: Metadata = bincode::serde::decode_from_std_read(&mut file, config).ok()?;

        meta.lib = lib;

        Some(Arc::new(meta))
    } else {
        None
    }
}

pub fn write_metadata(db: &dyn HirDatabase, lib: LibId, paths: &InputPaths) -> io::Result<()> {
    let source_root = db.libs()[lib].source_root;
    let dir = &paths.source_roots[&source_root];
    let source_root = db.source_root(source_root);
    let metadata_dir = paths.target_dir.join(metadata_name(db, lib));
    let mut metadata = Metadata::default();

    {
        let path = RelativePath::new("shadow.toml");
        let path_buf = path.to_logical_path(dir);
        let meta = path_buf.metadata()?;
        let timestamp = meta.modified()?;

        metadata.files.insert(path.to_string(), timestamp);
    }

    for (_, path) in source_root.iter() {
        let path_buf = path.to_logical_path(dir);
        let meta = path_buf.metadata()?;
        let timestamp = meta.modified()?;

        metadata.files.insert(path.to_string(), timestamp);
    }

    let mut file = File::create(metadata_dir)?;
    let config = bincode::config::standard();

    match bincode::serde::encode_into_std_write(&metadata, &mut file, config) {
        | Ok(_) => Ok(()),
        | Err(e) => match e {
            | bincode::error::EncodeError::Io { error, .. } => Err(error),
            | _ => panic!("write_metadata: {}", e),
        },
    }
}

fn metadata_name(db: &dyn HirDatabase, lib: LibId) -> String {
    let libs = db.libs();
    let data = &libs[lib];
    let cfg_hash = hash_cfg(&data.cfg_options);

    format!("{}-{:X}.{}", data.name, cfg_hash, Metadata::EXTENSION)
}

fn hash_cfg(cfg: &CfgOptions) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();

    for flag in cfg.flags().iter() {
        flag.hash(&mut hasher);
    }

    for (key, value) in cfg.keys().iter() {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }

    hasher.finish()
}
