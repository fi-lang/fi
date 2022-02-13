mod attributes;
mod exprs;
mod items;
mod paths;
mod patterns;
mod types;

use crate::parser::Parser;
use crate::syntax_kind::*;

crate fn root(p: &mut Parser) {
    let m = p.start();

    while !p.at(EOF) {
        let m = p.start();

        p.eat(LYT_SEP);

        while !p.at(EOF) && p.at(AT) {
            attributes::attr(p);
            p.eat(LYT_SEP);
        }

        p.expect(MODULE_KW);
        paths::module_name(p);

        if p.at(L_PAREN) {
            exports(p);
        }

        p.expect(EQUALS);
        p.expect(LYT_START);
        p.eat(LYT_SEP);

        while !p.at(EOF) && !p.at(LYT_END) {
            items::any_item(p);

            if !p.at(LYT_END) && !p.at(EOF) {
                p.expect(LYT_SEP);
            }
        }

        p.eat(LYT_END);
        p.expect(EOF);
        m.complete(p, MODULE);
    }

    m.complete(p, SOURCE_FILE);
}

fn exports(p: &mut Parser) {
    let m = p.start();

    p.expect(L_PAREN);

    while !p.at(EOF) && !p.at(R_PAREN) {
        export(p);

        if !p.at(R_PAREN) {
            p.expect(COMMA);
        }
    }

    p.expect(R_PAREN);
    m.complete(p, EXPORTS);
}

fn export(p: &mut Parser) {
    let m = p.start();

    match p.current() {
        | IDENT if p.nth_at(1, L_PAREN) => {
            paths::name_or_symbol_ref(p);

            let inner = p.start();

            p.bump(L_PAREN);

            if p.eat(DBL_DOT) {
                p.expect(R_PAREN);
                inner.complete(p, EXPORT_GROUP_ALL);
            } else {
                while !p.at(EOF) && !p.at(R_PAREN) {
                    paths::name_ref(p);

                    if !p.at(R_PAREN) {
                        p.expect(COMMA);
                    }
                }

                p.expect(R_PAREN);
                inner.complete(p, EXPORT_GROUP_NAMED);
            }

            m.complete(p, EXPORT_GROUP);
        },
        | IDENT | SYMBOL => {
            paths::name_or_symbol_ref(p);
            m.complete(p, EXPORT_NAME);
        },
        | MODULE_KW => {
            p.bump(MODULE_KW);
            paths::path(p);
            m.complete(p, EXPORT_MODULE);
        },
        | _ => {
            p.error("exported an export");
            m.abandon(p);
        },
    }
}

crate mod fragments {
    use super::*;

    crate fn path(p: &mut Parser) {
        paths::path(p);
    }

    crate fn type_(p: &mut Parser) {
        types::ty(p);
    }
}
