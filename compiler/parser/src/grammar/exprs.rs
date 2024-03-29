use super::*;
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::*;

pub(crate) fn expr(p: &mut Parser) {
    expr_(p, true, TokenSet::EMPTY);
}

fn expr_(p: &mut Parser, allow_do: bool, disallow: impl Into<TokenSet> + Copy) {
    if let Some(m) = infix(p, allow_do, disallow) {
        if p.at(DBL_COLON) {
            let expr = m.precede(p);
            let _ = p.bump(DBL_COLON);
            let _ = types::ty(p);

            expr.complete(p, EXPR_TYPED);
        }
    }
}

pub(crate) fn infix(p: &mut Parser, allow_do: bool, disallow: impl Into<TokenSet> + Copy) -> Option<CompletedMarker> {
    let mut m = app(p, allow_do)?;

    while p.eat(TICK) {
        let expr = m.precede(p);

        paths::path(p);
        p.expect(TICK);
        app(p, allow_do);
        m = expr.complete(p, EXPR_INFIX);
    }

    if peek_operator(p, disallow) {
        let expr = m.precede(p);

        while peek_operator(p, disallow) {
            p.bump_any();
            app(p, allow_do);
        }

        m = expr.complete(p, EXPR_INFIX);
    }

    Some(m)
}

pub(crate) fn app(p: &mut Parser, allow_do: bool) -> Option<CompletedMarker> {
    let mut m = postfix(p, true, allow_do)?;

    while peek(p, 0, allow_do) || p.at(DOT) {
        let expr = m.precede(p);

        if p.eat(DOT) {
            let _ = atom(p, false);

            m = expr.complete(p, EXPR_METHOD);
        } else {
            let _ = postfix(p, false, allow_do);

            m = expr.complete(p, EXPR_APP);
        }
    }

    Some(m)
}

pub(crate) fn postfix(p: &mut Parser, allow_op: bool, allow_do: bool) -> Option<CompletedMarker> {
    let mut m = prefix(p, allow_do)?;

    loop {
        match p.current() {
            | FIELD_DOT => {
                let expr = m.precede(p);

                p.bump(FIELD_DOT);

                match p.current() {
                    | IDENT => {
                        paths::name_ref(p);
                        m = expr.complete(p, EXPR_FIELD);
                    },
                    | _ => {
                        p.error("expected an identifier");
                        expr.abandon(p);
                        return None;
                    },
                }
            },
            | OPERATOR if allow_op && !peek(p, 1, allow_do) => {
                let expr = m.precede(p);

                p.bump(OPERATOR);
                m = expr.complete(p, EXPR_POSTFIX);
            },
            | _ => break,
        }
    }

    Some(m)
}

pub(crate) fn prefix(p: &mut Parser, allow_do: bool) -> Option<CompletedMarker> {
    if p.at(OPERATOR) {
        let m = p.start();

        p.bump(OPERATOR);
        prefix(p, allow_do);

        Some(m.complete(p, EXPR_PREFIX))
    } else {
        atom(p, allow_do)
    }
}

pub(crate) fn atom(p: &mut Parser, allow_do: bool) -> Option<CompletedMarker> {
    let m = p.start();

    match p.current() {
        | UNDERSCORE => {
            p.bump(UNDERSCORE);
            Some(m.complete(p, EXPR_HOLE))
        },
        | IDENT | SYMBOL => {
            paths::path(p);
            Some(m.complete(p, EXPR_PATH))
        },
        | INT | FLOAT | CHAR | STRING => {
            literal(p);
            Some(m.complete(p, EXPR_LITERAL))
        },
        | DO_KW if allow_do => {
            p.bump(DO_KW);
            block(p, false);
            Some(m.complete(p, EXPR_DO))
        },
        | TRY_KW if allow_do => {
            p.bump(TRY_KW);
            block(p, true);
            Some(m.complete(p, EXPR_TRY))
        },
        | DO_KW => {
            p.error("do blocks are not allowed in this position");
            p.bump_any();
            m.abandon(p);
            None
        },
        | FN_KW => {
            p.bump(FN_KW);

            while !p.at(EOF) && !p.at(ARROW) {
                patterns::atom(p);
            }

            p.expect(ARROW);
            expr(p);
            Some(m.complete(p, EXPR_CLOS))
        },
        | IF_KW => {
            p.bump_any();
            expr_(p, false, TokenSet::EMPTY);

            match p.current() {
                | THEN_KW => {
                    p.bump(THEN_KW);
                    expr(p);

                    if p.eat(ELSE_KW) {
                        expr(p);
                    }
                },
                | DO_KW => {
                    let do_expr = p.start();

                    p.bump(DO_KW);
                    block(p, false);
                    do_expr.complete(p, EXPR_DO);

                    if p.eat(ELSE_KW) {
                        expr(p);
                    }
                },
                | TRY_KW => {
                    let do_expr = p.start();

                    p.bump(DO_KW);
                    block(p, true);
                    do_expr.complete(p, EXPR_TRY);

                    if p.eat(ELSE_KW) {
                        expr(p);
                    }
                },
                | _ => {
                    p.error("expected 'then' or 'do'");
                    m.abandon(p);
                    return None;
                },
            }

            Some(m.complete(p, EXPR_IF))
        },
        | CASE_KW => {
            p.bump(CASE_KW);
            expr(p);
            p.expect(OF_KW);
            p.expect(LYT_START);

            while !p.at(EOF) && !p.at(LYT_END) {
                case_arm(p);

                if !p.at(LYT_END) && !p.expect(LYT_SEP) {
                    break;
                }
            }

            p.expect(LYT_END);

            Some(m.complete(p, EXPR_CASE))
        },
        | RECUR_KW => {
            p.bump(RECUR_KW);

            Some(m.complete(p, EXPR_RECUR))
        },
        | RETURN_KW => {
            p.bump(RETURN_KW);
            expr(p);

            Some(m.complete(p, EXPR_RETURN))
        },
        | L_PAREN => {
            p.bump(L_PAREN);

            if p.eat(R_PAREN) {
                Some(m.complete(p, EXPR_UNIT))
            } else {
                let _ = expr(p);
                p.expect(R_PAREN);

                Some(m.complete(p, EXPR_PARENS))
            }
        },
        | L_BRACKET => {
            p.bump(L_BRACKET);

            while !p.at(EOF) && !p.at(R_BRACKET) {
                expr_(p, true, COMMA);

                if !p.at(R_BRACKET) && !p.expect(COMMA) {
                    break;
                }
            }

            p.expect(R_BRACKET);
            Some(m.complete(p, EXPR_ARRAY))
        },
        | L_BRACE => {
            p.bump(L_BRACE);
            record_fields(p);
            p.expect(R_BRACE);
            Some(m.complete(p, EXPR_RECORD))
        },
        | _ => {
            p.error("expected an expression");
            p.bump_any();
            m.abandon(p);
            None
        },
    }
}

fn peek(p: &Parser, n: usize, allow_do: bool) -> bool {
    match p.nth(n) {
        | DO_KW | TRY_KW => allow_do,
        | FN_KW | IDENT | SYMBOL | INT | FLOAT | CHAR | STRING | L_PAREN | L_BRACE | L_BRACKET | CASE_KW
        | UNDERSCORE | RECUR_KW | RETURN_KW => true,
        | _ => false,
    }
}

pub(crate) fn literal(p: &mut Parser) {
    let m = p.start();

    if p.eat(INT) {
        m.complete(p, LIT_INT);
    } else if p.eat(FLOAT) {
        m.complete(p, LIT_FLOAT);
    } else if p.eat(CHAR) {
        m.complete(p, LIT_CHAR);
    } else if p.eat(STRING) {
        m.complete(p, LIT_STRING);
    } else {
        p.error("expected a literal");
        m.abandon(p);
    }
}

pub(crate) fn record_fields(p: &mut Parser) {
    while !p.at(R_BRACE) {
        let field = p.start();

        if p.at(IDENT) && p.nth_at(1, EQUALS) {
            paths::name(p);
            p.bump(EQUALS);
            infix(p, true, COMMA | PIPE);
            field.complete(p, FIELD_NORMAL);
        } else {
            paths::name(p);
            field.complete(p, FIELD_PUN);
        }

        if !p.at(R_BRACE) && !p.expect(COMMA) {
            break;
        }
    }
}

pub(crate) fn block(p: &mut Parser, allow_bind: bool) {
    let m = p.start();

    p.expect(LYT_START);
    p.eat(LYT_SEP);

    while !p.at(EOF) && !p.at(LYT_END) {
        // p.eat(LYT_SEP);
        stmt(p, allow_bind);

        if !p.at(LYT_END) && !p.expect(LYT_SEP) {
            break;
        }
    }

    p.expect(LYT_END);
    m.complete(p, BLOCK);
}

pub(crate) fn stmt(p: &mut Parser, allow_bind: bool) {
    let m = p.start();

    if p.eat(LET_KW) {
        patterns::pattern(p);
        p.expect(EQUALS);
        expr(p);
        m.complete(p, STMT_LET);
        return;
    } else if allow_bind {
        for i in 0..100 {
            if p.nth_at(i, LEFT_ARROW) {
                patterns::app(p);
                p.expect(LEFT_ARROW);
                expr(p);
                m.complete(p, STMT_BIND);
                return;
            } else if p.nth_at(i, LYT_START) || p.nth_at(i, LYT_SEP) || p.nth_at(i, LYT_END) {
                break;
            }
        }
    }

    while p.eat(LYT_SEP) {}

    expr(p);
    m.complete(p, STMT_EXPR);
}

fn case_arm(p: &mut Parser) {
    let m = p.start();

    patterns::pattern(p);
    case_value(p);
    m.complete(p, CASE_ARM);
}

fn case_value(p: &mut Parser) {
    let m = p.start();

    if p.eat(ARROW) {
        expr(p);
        m.complete(p, CASE_VALUE);
        return;
    }

    case_guard(p, ARROW);

    while p.at_ts(IF_KW | ELSE_KW) {
        case_guard(p, ARROW);
    }

    m.complete(p, CASE_GUARDED);
}

pub(crate) fn case_guard(p: &mut Parser, sep: SyntaxKind) {
    let m = p.start();

    if p.eat(ELSE_KW) {
        p.expect(sep);
        expr(p);
        m.complete(p, CASE_GUARD);
        return;
    }

    p.bump(IF_KW);
    expr_(p, true, sep);
    p.expect(sep);
    expr(p);
    m.complete(p, CASE_GUARD);
}
