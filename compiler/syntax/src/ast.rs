mod def;
mod ext;
// mod groups;
mod tokens;

use std::marker::PhantomData;

pub use def::*;
pub use ext::*;
// pub use groups::*;
use parser::syntax_kind::SyntaxKind;
pub use tokens::*;

use crate::syntax_node::{SyntaxNode, SyntaxNodeChildren, SyntaxToken};

pub trait AstNode {
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxNode;

    fn clone_for_update(&self) -> Self
    where
        Self: Sized,
    {
        Self::cast(self.syntax().clone()).unwrap()
    }
}

pub trait AstToken {
    fn can_cast(token: SyntaxKind) -> bool;

    fn cast(token: SyntaxToken) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxToken;

    fn text(&self) -> &str {
        self.syntax().text()
    }
}

pub struct AstChildren<N> {
    inner: SyntaxNodeChildren,
    _marker: PhantomData<N>,
}

impl<N> Clone for AstChildren<N> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _marker: PhantomData,
        }
    }
}

impl<N> AstChildren<N> {
    fn new(parent: &SyntaxNode) -> Self {
        AstChildren {
            inner: parent.children(),
            _marker: PhantomData,
        }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.by_ref().find_map(N::cast)
    }
}

mod support {
    use super::{AstChildren, AstNode, SyntaxKind, SyntaxNode, SyntaxToken};

    pub(super) fn child<C: AstNode>(parent: &SyntaxNode) -> Option<C> {
        parent.children().find_map(C::cast)
    }

    pub(super) fn children<C: AstNode>(parent: &SyntaxNode) -> AstChildren<C> {
        AstChildren::new(parent)
    }

    pub(super) fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        parent
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == kind)
    }
}

#[macro_export]
macro_rules! ast_node {
    ($name:ident, $($kind:ident)|*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(pub(crate) $crate::syntax_node::SyntaxNode);

        impl $crate::ast::AstNode for $name {
            fn can_cast(kind: ::parser::syntax_kind::SyntaxKind) -> bool {
                matches!(kind, $($kind)|*)
            }

            fn cast(syntax: $crate::syntax_node::SyntaxNode) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self(syntax))
                } else {
                    None
                }
            }

            fn syntax(&self) -> &$crate::syntax_node::SyntaxNode {
                &self.0
            }
        }
    };

    ($name:ident { $($var:ident($varname:ident, $varkind:ident)),* $(,)? }) => {
        $crate::ast_node!(@ $name { $($var($varname, $varkind)),* });
        $($crate::ast_node!($varname, $varkind);)*
    };

    (@ $name:ident { $($var:ident($varname:ident, $varkind:ident)),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name {
            $($var($varname)),*
        }

        impl $crate::ast::AstNode for $name {
            fn can_cast(kind: ::parser::syntax_kind::SyntaxKind) -> bool {
                matches!(kind, $($varkind)|*)
            }

            fn cast(syntax: $crate::syntax_node::SyntaxNode) -> Option<Self> {
                match syntax.kind() {
                    $(
                        $varkind => Some($name::$var($varname(syntax))),
                    )*
                    _ => None,
                }
            }

            fn syntax(&self) -> &$crate::syntax_node::SyntaxNode {
                match self {
                    $(
                        $name::$var(n) => n.syntax(),
                    )*
                }
            }
        }

        $(
            impl From<$varname> for $name {
                fn from(src: $varname) -> Self {
                    $name::$var(src)
                }
            }
        )*
    };
}
