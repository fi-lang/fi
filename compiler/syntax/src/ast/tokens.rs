use std::fmt;

use parser::syntax_kind::SyntaxKind;

use crate::ast::AstToken;
use crate::syntax_node::SyntaxToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Whitespace(pub(crate) SyntaxToken);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Comment(pub(crate) SyntaxToken);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Operator(pub(crate) SyntaxToken);

pub enum CommentKind {
    Doc,
    Normal,
}

impl Comment {
    pub fn kind(&self) -> CommentKind {
        CommentKind::from_text(self.text())
    }

    pub fn is_doc(&self) -> bool {
        matches!(self.kind(), CommentKind::Doc)
    }

    pub fn doc_comment(&self) -> Option<&str> {
        let kind = self.kind();

        match kind {
            | CommentKind::Doc => {
                let text = &self.text()[2..];
                let ws = text.chars().next().filter(|c| c.is_whitespace());
                let text = ws.map_or(text, |ws| &text[ws.len_utf8()..]);

                Some(text)
            },
            | CommentKind::Normal => None,
        }
    }
}

impl CommentKind {
    pub fn from_text(text: &str) -> Self {
        if text.starts_with(";;") {
            CommentKind::Doc
        } else {
            CommentKind::Normal
        }
    }
}

impl Operator {
    pub fn text(&self) -> &str {
        self.0.text()
    }
}

impl AstToken for Whitespace {
    fn can_cast(token: SyntaxKind) -> bool {
        token == SyntaxKind::WHITESPACE
    }

    fn cast(token: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(token.kind()) {
            Some(Whitespace(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }

    fn text(&self) -> &str {
        self.0.text()
    }
}

impl AstToken for Comment {
    fn can_cast(token: SyntaxKind) -> bool {
        token == SyntaxKind::COMMENT
    }

    fn cast(token: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(token.kind()) {
            Some(Comment(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }

    fn text(&self) -> &str {
        self.0.text()
    }
}

impl AstToken for Operator {
    fn can_cast(token: SyntaxKind) -> bool {
        token == SyntaxKind::OPERATOR
    }

    fn cast(token: SyntaxToken) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(token.kind()) {
            Some(Operator(token))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }

    fn text(&self) -> &str {
        self.0.text()
    }
}

impl fmt::Display for Whitespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for Comment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
