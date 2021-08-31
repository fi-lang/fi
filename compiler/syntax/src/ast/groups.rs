use crate::ast::*;
use std::iter::Peekable;

impl Module {
    pub fn item_groups(&self) -> ItemGroups {
        ItemGroups {
            items: self.items().peekable(),
        }
    }
}

pub struct ItemGroups {
    items: Peekable<AstChildren<Item>>,
}

#[derive(Debug, Clone, Copy)]
enum ItemGroupKind {
    Import,
    Fixity,
    Func(bool),
    Const(bool),
    Static(bool),
    Type(bool),
    Class,
    Instance,
}

impl Iterator for ItemGroups {
    type Item = (Item, Vec<Item>);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.items.next()?;
        let name = item.group_name();
        let mut rest = Vec::new();
        let mut kind = item.group_kind();

        while let Some(next) = self.items.peek() {
            if name_eq(&next.group_name(), &name) && rest.len() != kind.max() {
                let kind2 = next.group_kind();

                if kind == kind2 {
                    kind = kind2;
                    rest.push(self.items.next()?);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some((item, rest))
    }
}

fn name_eq(a: &Option<Name>, b: &Option<Name>) -> bool {
    match (a, b) {
        | (Some(a), Some(b)) => a.text() == b.text(),
        | (None, None) => true,
        | _ => false,
    }
}

impl ItemGroupKind {
    fn max(self) -> usize {
        match self {
            | ItemGroupKind::Func(_) => usize::MAX,
            | ItemGroupKind::Static(_) => 1,
            | ItemGroupKind::Const(_) => 1,
            | ItemGroupKind::Type(_) => 1,
            | _ => 0,
        }
    }
}

impl PartialEq for ItemGroupKind {
    fn eq(&self, other: &Self) -> bool {
        use ItemGroupKind::*;

        match (self, other) {
            | (Import, Import) => true,
            | (Fixity, Fixity) => true,
            | (Func(true), Func(false)) => true,
            | (Func(false), Func(false)) => true,
            | (Const(true), Const(false)) => true,
            | (Const(false), Const(false)) => true,
            | (Static(true), Static(false)) => true,
            | (Static(false), Static(false)) => true,
            | (Type(true), Type(false)) => true,
            | (Type(false), Type(false)) => true,
            | (Class, Class) => true,
            | (Instance, Instance) => true,
            | _ => false,
        }
    }
}

impl Item {
    fn group_name(&self) -> Option<Name> {
        match self {
            | Item::Fun(it) => it.name(),
            | Item::Static(it) => it.name(),
            | Item::Const(it) => it.name(),
            | Item::Type(it) => it.name(),
            | _ => None,
        }
    }

    fn group_kind(&self) -> ItemGroupKind {
        match self {
            | Item::Import(_) => ItemGroupKind::Import,
            | Item::Fixity(_) => ItemGroupKind::Fixity,
            | Item::Fun(it) if it.is_foreign() => ItemGroupKind::Func(false),
            | Item::Fun(it) if it.ty().is_some() => ItemGroupKind::Func(true),
            | Item::Fun(_) => ItemGroupKind::Func(false),
            | Item::Static(it) if it.is_foreign() => ItemGroupKind::Static(false),
            | Item::Static(it) if it.ty().is_some() => ItemGroupKind::Static(true),
            | Item::Static(_) => ItemGroupKind::Static(false),
            | Item::Const(it) if it.ty().is_some() => ItemGroupKind::Const(true),
            | Item::Const(_) => ItemGroupKind::Const(false),
            | Item::Type(it) if it.kind().is_some() => ItemGroupKind::Type(true),
            | Item::Type(_) => ItemGroupKind::Type(false),
            | Item::Class(_) => ItemGroupKind::Class,
            | Item::Instance(_) => ItemGroupKind::Instance,
        }
    }
}

impl ItemFun {
    /// Returns an iterator of all items in this groups, including self
    pub fn group(&self) -> impl Iterator<Item = Self> {
        let name = self.name();

        std::iter::successors(Some(self.clone()), move |it| {
            let next = it.syntax().next_sibling().and_then(Self::cast)?;

            if name_eq(&next.name(), &name) {
                Some(next)
            } else {
                None
            }
        })
    }
}

impl ItemStatic {
    pub fn next(&self) -> Option<Self> {
        self.syntax()
            .next_sibling()
            .and_then(Self::cast)
            .filter(|n| name_eq(&n.name(), &self.name()))
    }
}

impl ItemConst {
    pub fn next(&self) -> Option<Self> {
        self.syntax()
            .next_sibling()
            .and_then(Self::cast)
            .filter(|n| name_eq(&n.name(), &self.name()))
    }
}

impl ItemType {
    pub fn next(&self) -> Option<Self> {
        self.syntax()
            .next_sibling()
            .and_then(Self::cast)
            .filter(|n| name_eq(&n.name(), &self.name()))
    }
}