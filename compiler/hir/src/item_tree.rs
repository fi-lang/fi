mod lower;

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Index;

use arena::{Arena, Idx};
use base_db::input::File;
pub use lower::query;
use syntax::ast::{self, AstNode};
use vfs::InFile;

use crate::ast_id::FileAstId;
use crate::name::Name;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ItemTree {
    file: File,
    items: Vec<Item>,
    data: ItemTreeData,
}

#[derive(Default, Debug, PartialEq, Eq, Hash)]
pub struct ItemTreeData {
    modules: Arena<Module>,
    imports: Arena<Import>,
    fixities: Arena<Fixity>,
    values: Arena<Value>,
    type_aliases: Arena<TypeAlias>,
    type_ctors: Arena<TypeCtor>,
    ctors: Arena<Ctor>,
    traits: Arena<Trait>,
    impls: Arena<Impl>,
}

pub trait ItemTreeNode: Clone {
    type Source: AstNode + Into<ast::Item>;

    fn ast_id(&self) -> FileAstId<Self::Source>;
    fn lookup(tree: &ItemTree, index: Idx<Self>) -> &Self;
    fn id_from_item(item: Item) -> Option<LocalItemTreeId<Self>>;
    fn id_to_item(id: LocalItemTreeId<Self>) -> Item;
}

pub struct LocalItemTreeId<N: ItemTreeNode> {
    index: Idx<N>,
    _marker: PhantomData<N>,
}

pub type ItemTreeId<N> = InFile<LocalItemTreeId<N>>;

impl ItemTree {
    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

impl<N: ItemTreeNode> Index<LocalItemTreeId<N>> for ItemTree {
    type Output = N;

    fn index(&self, id: LocalItemTreeId<N>) -> &Self::Output {
        N::lookup(self, id.index)
    }
}

impl Index<Idx<Ctor>> for ItemTree {
    type Output = Ctor;

    fn index(&self, id: Idx<Ctor>) -> &Self::Output {
        &self.data.ctors[id]
    }
}

macro_rules! items {
    ($($typ:ident in $f_id:ident -> $ast:ty),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Item {
            $($typ(LocalItemTreeId<$typ>)),*
        }

        $(
            impl From<LocalItemTreeId<$typ>> for Item {
                fn from(id: LocalItemTreeId<$typ>) -> Self {
                    Self::$typ(id)
                }
            }

            impl ItemTreeNode for $typ {
                type Source = $ast;

                fn ast_id(&self) -> FileAstId<Self::Source> {
                    self.ast_id
                }

                fn lookup(tree: &ItemTree, index: Idx<Self>) -> &Self {
                    &tree.data.$f_id[index]
                }

                fn id_from_item(item: Item) -> Option<LocalItemTreeId<Self>> {
                    if let Item::$typ(id) = item {
                        Some(id)
                    } else {
                        None
                    }
                }

                fn id_to_item(id: LocalItemTreeId<Self>) -> Item {
                    Item::$typ(id)
                }
            }
        )*
    };
}

items! {
    Module in modules -> ast::ItemModule,
    Fixity in fixities -> ast::ItemFixity,
    Value in values -> ast::ItemValue,
    TypeAlias in type_aliases -> ast::ItemType,
    TypeCtor in type_ctors -> ast::ItemType,
    Trait in traits -> ast::ItemTrait,
    Impl in impls -> ast::ItemImpl,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Module {
    pub ast_id: FileAstId<ast::ItemModule>,
    pub name: Name,
    pub items: Box<[Item]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Import {
    pub ast_id: FileAstId<ast::ItemImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fixity {
    pub ast_id: FileAstId<ast::ItemFixity>,
    pub name: Name,
    pub is_type: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value {
    pub ast_id: FileAstId<ast::ItemValue>,
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeAlias {
    pub ast_id: FileAstId<ast::ItemType>,
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeCtor {
    pub ast_id: FileAstId<ast::ItemType>,
    pub name: Name,
    pub ctors: Box<[Idx<Ctor>]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ctor {
    pub ast_id: FileAstId<ast::Ctor>,
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Trait {
    pub ast_id: FileAstId<ast::ItemTrait>,
    pub name: Name,
    pub items: Box<[LocalItemTreeId<Value>]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Impl {
    pub ast_id: FileAstId<ast::ItemImpl>,
    pub items: Box<[LocalItemTreeId<Value>]>,
}

impl<N: ItemTreeNode> Clone for LocalItemTreeId<N> {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            _marker: PhantomData,
        }
    }
}

impl<N: ItemTreeNode> Copy for LocalItemTreeId<N> {
}

impl<N: ItemTreeNode> PartialEq for LocalItemTreeId<N> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<N: ItemTreeNode> Eq for LocalItemTreeId<N> {
}

impl<N: ItemTreeNode> Hash for LocalItemTreeId<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<N: ItemTreeNode> fmt::Debug for LocalItemTreeId<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LocalItemTreeId").field(&self.index).finish()
    }
}