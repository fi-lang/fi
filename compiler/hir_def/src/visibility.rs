use std::fmt;

use crate::db::DefDatabase;
use crate::def_map::DefMap;
use crate::id::{LocalModuleId, ModuleId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Module(ModuleId),
    Public,
}

impl Visibility {
    pub fn is_visible_from(self, db: &dyn DefDatabase, from: ModuleId) -> bool {
        let to = match self {
            | Visibility::Module(m) => m,
            | Visibility::Public => return true,
        };

        if from.lib != to.lib {
            return false;
        }

        let def_map = db.def_map(from.lib);

        self.is_visible_from_def_map(&def_map, from.local_id)
    }

    pub(crate) fn is_visible_from_def_map(self, def_map: &DefMap, from: LocalModuleId) -> bool {
        match self {
            | Visibility::Module(m) => m == def_map.module_id(from),
            | Visibility::Public => true,
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::Public => write!(f, "public"),
            | Self::Module(id) => {
                let local: u32 = id.local_id.into_raw().into();
                write!(f, "{}:{}", id.lib.0, local)
            },
        }
    }
}
