use std::str::Chars;

use text_size::TextSize;
use unicode_xid::UnicodeXID;
use SyntaxKind::*;

use crate::token::{SyntaxKind, Token};

pub struct Lexer<'input> {
    text: &'input str,
    chars: Chars<'input>,
    start: TextSize,
    pos: TextSize,
    current: char,
    next: char,
    indents: Vec<Indent>,
    current_indent: usize,
    should_indent: bool,
    insert_newline: bool,
}

struct Indent {
    level: usize,
    ignored: bool,
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if self.insert_newline {
            self.insert_newline = false;
            return self.token(LYT_SEP);
        }

        if let Some(last_indent) = self.indents.last() && self.current_indent < last_indent.level {
            return self.dedent(self.current_indent);
        }

        if self.current == '\r' || self.current == '\n' {
            return self.newline();
        }

        self.should_indent = false;

        match (self.current, self.next) {
            | (c, _) if c.is_whitespace() => self.whitespace(),
            | (c, _) if c.is_xid_start() => self.ident(),
            | (c, _) if c.is_ascii_digit() => self.number(),
            | ('\0', _) if self.current_indent > 0 => self.dedent(0),
            | ('\0', _) if self.pos > TextSize::of(self.text) => None,
            | ('\0', _) => self.emit_eof(),
            | ('/', '/') => self.comment(),
            | (':', ':') => self.advance2_with(DBL_COLON),
            | ('.', '.') => self.advance2_with(DBL_DOT),
            | ('-', '>') => self.advance2_with(ARROW),
            | ('<', '-') => self.advance2_with(LEFT_ARROW),
            | ('(', _) => self.advance_with(L_PAREN),
            | (')', _) => self.advance_with(R_PAREN),
            | ('{', _) => self.advance_with(L_BRACE),
            | ('}', _) => self.advance_with(R_BRACE),
            | ('[', _) => self.advance_with(L_BRACKET),
            | (']', _) => self.advance_with(R_BRACKET),
            | ('.', _) => self.advance_with(DOT),
            | (',', _) => self.advance_with(COMMA),
            | ('|', _) => self.advance_with(PIPE),
            | ('@', _) => self.advance_with(AT),
            | ('`', _) => self.advance_with(TICK),
            | ('_', _) => self.advance_with(UNDERSCORE),
            | ('=', _) => {
                self.should_indent = true;
                self.advance_with(EQUALS)
            },
            | _ => self.advance_with(ERROR),
        }
    }
}

impl<'input> Lexer<'input> {
    pub fn new(text: &'input str) -> Self {
        let mut chars = text.chars();

        Self {
            current: chars.next().unwrap_or('\0'),
            next: chars.next().unwrap_or('\0'),
            start: TextSize::from(0),
            pos: TextSize::from(0),
            indents: Vec::new(),
            current_indent: 0,
            should_indent: false,
            insert_newline: false,
            text,
            chars,
        }
    }

    fn eof(&self) -> bool {
        self.current == '\0'
    }

    fn current_text(&self) -> &'input str {
        &self.text[self.start.into()..self.pos.into()]
    }

    fn advance(&mut self) {
        self.pos += TextSize::of(self.current);
        self.current = self.next;
        self.next = self.chars.next().unwrap_or('\0');
    }

    fn advance_with(&mut self, kind: SyntaxKind) -> Option<Token> {
        self.advance();
        self.token(kind)
    }

    fn advance2_with(&mut self, kind: SyntaxKind) -> Option<Token> {
        self.advance();
        self.advance_with(kind)
    }

    fn emit_eof(&mut self) -> Option<Token> {
        let token = self.token(EOF);
        self.advance();
        token
    }

    fn token(&mut self, kind: SyntaxKind) -> Option<Token> {
        let len = self.pos - self.start;
        self.start = self.pos;
        Some(Token { kind, len })
    }

    fn whitespace(&mut self) -> Option<Token> {
        while !self.eof() && self.current.is_whitespace() {
            self.advance();
        }

        self.token(WHITESPACE)
    }

    fn comment(&mut self) -> Option<Token> {
        while !self.eof() && self.current != '\n' {
            self.advance();
        }

        self.token(COMMENT)
    }

    fn ident(&mut self) -> Option<Token> {
        let is_type = self.current.is_uppercase();

        while !self.eof() && self.current.is_xid_continue() {
            self.advance();
        }

        match self.current_text() {
            | "as" => self.token(AS_KW),
            | "const" => self.token(CONST_KW),
            | "do" => self.token(DO_KW),
            | "else" => self.token(ELSE_KW),
            | "fn" => self.token(FN_KW),
            | "foreign" => self.token(FOREIGN_KW),
            | "if" => self.token(IF_KW),
            | "impl" => self.token(IMPL_KW),
            | "import" => self.token(IMPORT_KW),
            | "infix" => self.token(INFIX_KW),
            | "infixl" => self.token(INFIXL_KW),
            | "infixr" => self.token(INFIXR_KW),
            | "let" => self.token(LET_KW),
            | "match" => self.token(MATCH_KW),
            | "module" => self.token(MODULE_KW),
            | "recur" => self.token(RECUR_KW),
            | "return" => self.token(RETURN_KW),
            | "static" => self.token(STATIC_KW),
            | "then" => self.token(THEN_KW),
            | "trait" => self.token(TRAIT_KW),
            | "try" => self.token(TRY_KW),
            | "type" => self.token(TYPE_KW),
            | "where" => self.token(WHERE_KW),
            | "with" => self.token(WITH_KW),
            | _ if is_type => self.token(TYPE),
            | _ => self.token(IDENT),
        }
    }

    fn number(&mut self) -> Option<Token> {
        while !self.eof() && (self.current.is_ascii_digit() || self.current == '_') {
            self.advance();
        }

        self.token(INT)
    }

    fn newline(&mut self) -> Option<Token> {
        self.advance();
        let start = self.pos;
        while !self.eof() && (self.current == ' ' || self.current == '\t') {
            self.advance();
        }

        let new_indent = self.text[start.into()..self.pos.into()].len();

        match (self.current, self.next) {
            | ('\r' | '\n', _) => self.newline(),
            | ('/', '/') => self.token(WHITESPACE),
            | _ if new_indent > self.current_indent => self.indent(new_indent),
            | _ if new_indent < self.current_indent => self.dedent(new_indent),
            | _ if self.newlines_ignored() => self.next(),
            | _ => self.token(LYT_SEP),
        }
    }

    fn newlines_ignored(&self) -> bool {
        self.indents.last().map(|i| i.ignored).unwrap_or(false)
    }

    fn indent(&mut self, new_indent: usize) -> Option<Token> {
        if self.should_indent {
            self.indents.push(Indent {
                level: new_indent,
                ignored: false,
            });
            self.current_indent = new_indent;
            self.token(LYT_START)
        } else {
            self.indents.push(Indent {
                level: new_indent,
                ignored: false,
            });
            self.current_indent = new_indent;
            self.next()
        }
    }

    fn dedent(&mut self, new_indent: usize) -> Option<Token> {
        let last_indent = self.indents.pop().unwrap();

        self.current_indent = new_indent;
        self.insert_newline = !self.newlines_ignored();

        if new_indent > last_indent.level {
            // TODO: report error
        }

        if last_indent.ignored {
            self.next()
        } else {
            self.token(LYT_END)
        }
    }
}

#[test]
fn test_lexer() {
    let input = r#"
        module Core.Cmp =

        main = 0

        type X =
            | Y
            | Z

        trait Iterator self it =
            next :: self -> Option it

        impl Iterator Iter Item =
            next self = _
    "#;
    let input = unindent::unindent(input.trim());
    let lexer = Lexer::new(&input);
    let tokens = lexer.collect::<Vec<_>>();

    insta::assert_ron_snapshot!(tokens);
}