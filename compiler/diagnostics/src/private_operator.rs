use hir::diagnostic::Diagnostic as _;
use syntax::TextRange;

use super::*;

pub struct PrivateOperator<'db, 'd, DB: hir::db::HirDatabase> {
    _db: &'db DB,
    _diag: &'d hir::diagnostic::PrivateOperator,
    location: TextRange,
}

impl<'db, 'd, DB: hir::db::HirDatabase> Diagnostic for PrivateOperator<'db, 'd, DB> {
    fn title(&self) -> String {
        "private operator".into()
    }

    fn range(&self) -> TextRange {
        self.location
    }
}

impl<'db, 'd, DB: hir::db::HirDatabase> PrivateOperator<'db, 'd, DB> {
    pub fn new(db: &'db DB, diag: &'d hir::diagnostic::PrivateOperator) -> Self {
        let parse = db.parse(diag.file);
        let location = diag
            .src
            .to_node(&parse.syntax_node())
            .children_with_tokens()
            .find(|n| n.kind() == syntax::syntax_kind::OPERATOR)
            .map(|n| n.text_range())
            .unwrap_or_else(|| diag.display_source().value.range());

        Self {
            _db: db,
            _diag: diag,
            location,
        }
    }
}
