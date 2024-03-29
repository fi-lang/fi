#![allow(dead_code)]

use std::ops::RangeInclusive;
use std::sync::Arc;

use target_lexicon::{PointerWidth, Triple};

use crate::db::MirDatabase;
use crate::repr::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReprAndLayout {
    pub repr: Repr,
    pub layout: Layout,
}

impl std::ops::Deref for ReprAndLayout {
    type Target = Layout;

    fn deref(&self) -> &Self::Target {
        &self.layout
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Layout {
    pub size: Size,
    pub align: Align,
    pub stride: Size,
    pub abi: Abi,
    pub fields: Fields,
    pub variants: Variants,
    pub largest_niche: Option<Niche>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Size {
    raw: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Align {
    pow2: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Abi {
    Uninhabited,
    Scalar(Scalar),
    ScalarPair(Scalar, Scalar),
    Aggregate { sized: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Fields {
    Primitive,
    Union(usize),
    Array { stride: Size, count: usize },
    Arbitrary { offsets: Vec<Size> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Variants {
    Single {
        index: usize,
    },
    Multiple {
        tag: Scalar,
        tag_encoding: TagEncoding,
        tag_field: usize,
        variants: Vec<Arc<ReprAndLayout>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagEncoding {
    Direct,
    Niche {
        dataful_variant: usize,
        niche_variants: RangeInclusive<usize>,
        niche_start: u128,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Niche {
    pub offset: Size,
    pub scalar: Scalar,
}

pub fn repr_and_layout(db: &dyn MirDatabase, repr: Repr) -> Arc<ReprAndLayout> {
    let layout = layout_of(db, &repr);

    Arc::new(ReprAndLayout { repr, layout })
}

pub fn layout_of(db: &dyn MirDatabase, repr: &Repr) -> Layout {
    let target = db.target();
    let triple = target.triple();

    _layout_of(db, &triple, repr)
}

pub fn _layout_of(db: &dyn MirDatabase, triple: &Triple, repr: &Repr) -> Layout {
    match repr {
        | Repr::Opaque => {
            let mut layout = Layout::UNIT;
            layout.abi = Abi::Aggregate { sized: true };
            layout
        },
        | Repr::Uninhabited | Repr::TypeVar(_) => {
            let mut layout = Layout::UNIT;
            layout.abi = Abi::Uninhabited;
            layout
        },
        | Repr::Scalar(scalar) => Layout::scalar(scalar.clone(), &triple),
        | Repr::ReprOf(ty) => _layout_of(db, triple, &db.repr_of(*ty)),
        | Repr::Ptr(_, false, false) => Layout::scalar(Scalar::new(Primitive::Pointer, triple), triple),
        | Repr::Ptr(_, true, nonnull) => {
            let mut scalar = Scalar::new(Primitive::Pointer, triple);
            scalar.valid_range = *nonnull as u128..=*scalar.valid_range.end();
            let meta = Scalar::new(Primitive::Int(Integer::Int, false), triple);
            scalar_pair(scalar, meta, triple)
        },
        | Repr::Ptr(_, false, true) | Repr::Box(_) | Repr::Func(_, false) => {
            let mut scalar = Scalar::new(Primitive::Pointer, triple);
            scalar.valid_range = 1..=*scalar.valid_range.end();
            Layout::scalar(scalar, triple)
        },
        | Repr::Func(_, true) => todo!(),
        | Repr::Array(ArrayLen::Const(len), el) => {
            let len = *len as u64;
            let elem = _layout_of(db, triple, el);

            Layout {
                size: elem.stride * len,
                align: elem.align,
                stride: elem.stride * len,
                abi: Abi::Aggregate { sized: true },
                fields: Fields::Array {
                    stride: elem.stride,
                    count: len as usize,
                },
                variants: Variants::Single { index: 0 },
                largest_niche: None,
            }
        },
        | Repr::Array(ArrayLen::TypeVar(_), el) => {
            let elem = _layout_of(db, triple, el);

            Layout {
                size: Size::ZERO,
                align: elem.align,
                stride: Size::ZERO,
                abi: Abi::Aggregate { sized: false },
                fields: Fields::Array {
                    stride: elem.stride,
                    count: 0,
                },
                variants: Variants::Single { index: 0 },
                largest_niche: None,
            }
        },
        | Repr::Struct(fields) => {
            let layouts = fields
                .iter()
                .map(|f| {
                    let layout = _layout_of(db, triple, f);
                    ReprAndLayout {
                        layout,
                        repr: f.clone(),
                    }
                })
                .collect();

            struct_layout(layouts, triple)
        },
        | Repr::Enum(variants) => {
            let layouts = variants
                .iter()
                .map(|v| {
                    let layout = _layout_of(db, triple, v);
                    (v.clone(), layout)
                })
                .collect();

            enum_layout(layouts, triple)
        },
        | Repr::Discr(repr) => {
            let layout = db.layout_of((**repr).clone());

            match layout.variants {
                | Variants::Multiple { ref tag, .. } => Layout::scalar(tag.clone(), triple),
                | _ => todo!(),
            }
        },
    }
}

fn scalar_pair(a: Scalar, b: Scalar, triple: &Triple) -> Layout {
    let b_align = b.value.align(triple);
    let align = a.value.align(triple).max(b_align);
    let b_offset = a.value.size(triple).align_to(b_align);
    let size = b_offset + b.value.size(triple);
    let largest_niche = Niche::from_scalar(triple, b_offset, b.clone())
        .into_iter()
        .chain(Niche::from_scalar(triple, Size::ZERO, a.clone()))
        .max_by_key(|n| n.available(triple));

    Layout {
        size,
        align,
        stride: size.align_to(align),
        abi: Abi::ScalarPair(a, b),
        fields: Fields::Arbitrary {
            offsets: vec![Size::ZERO, b_offset],
        },
        variants: Variants::Single { index: 0 },
        largest_niche,
    }
}

fn struct_layout(lyts: Vec<ReprAndLayout>, triple: &Triple) -> Layout {
    let abi = Abi::Aggregate { sized: true };

    // match (lyts.get(0), lyts.get(1), lyts.get(2)) {
    //     | (Some(a), Some(b), None) => match (&a.abi, &b.abi) {
    //         | (Abi::Scalar(a), Abi::Scalar(b)) => {
    //             let pair = scalar_pair(a.clone(), b.clone(), triple);

    //             abi = pair.abi;
    //         },
    //         | (_, _) => {},
    //     },
    //     | (Some(s), None, None) => match &s.abi {
    //         | Abi::Scalar(_) | Abi::ScalarPair(_, _) => {
    //             abi = s.abi.clone();
    //         },
    //         | _ => {},
    //     },
    //     | (_, _, _) => {},
    // }

    let mut align = Align::ONE;
    let mut fields = lyts.iter().map(|lyt| (Size::ZERO, lyt.clone())).collect::<Vec<_>>();
    let mut offset = Size::ZERO;
    let mut niches = Vec::new();

    for (i, lyt) in lyts.into_iter().enumerate() {
        if let Some(niche) = &lyt.largest_niche {
            niches.push(niche.clone());
        }

        offset = offset.align_to(lyt.align);
        align = align.max(lyt.align);
        fields[i].0 = offset;
        offset = offset + lyt.size;
    }

    let size = offset;
    let stride = offset.align_to(align);
    let offsets = fields.into_iter().map(|f| f.0).collect();
    let largest_niche = niches.into_iter().max_by_key(|n| n.available(triple));

    Layout {
        size,
        align,
        stride,
        abi,
        fields: Fields::Arbitrary { offsets },
        variants: Variants::Single { index: 0 },
        largest_niche,
    }
}

fn enum_layout(mut lyts: Vec<(Repr, Layout)>, triple: &Triple) -> Layout {
    if lyts.is_empty() {
        Layout::default()
    } else if lyts.len() == 1 {
        lyts.pop().unwrap().1
    } else {
        let largest_niche = lyts
            .iter()
            .filter_map(|v| v.1.largest_niche.clone())
            .max_by_key(|n| n.available(triple));

        for (i, lyt) in lyts.iter_mut().enumerate() {
            lyt.1.variants = Variants::Single { index: i };
        }

        let largest = &lyts.iter().max_by_key(|l| l.1.size).unwrap().1;
        let align = largest.align;
        let mut size = largest.size;
        let mut no_niche = |mut variants: Vec<(Repr, Layout)>| {
            let tag_size = Size::from_bits(variants.len()).align_to(align);
            let tag = Scalar {
                value: Primitive::Int(
                    match tag_size.bytes() {
                        | 1 => Integer::I8,
                        | 2 => Integer::I16,
                        | 3 | 4 => Integer::I32,
                        | 5 | 6 | 7 | 8 => Integer::I64,
                        | _ => Integer::I128,
                    },
                    false,
                ),
                valid_range: 0..=variants.len() as u128 - 1,
            };

            for (_, variant) in &mut variants {
                if let Fields::Arbitrary { offsets } = &mut variant.fields {
                    for offset in offsets {
                        *offset = *offset + tag_size;
                    }
                }
            }

            size = size + tag_size;
            let tag_encoding = TagEncoding::Direct;
            let offsets = vec![Size::ZERO];
            let variants = variants
                .into_iter()
                .map(|(repr, mut layout)| {
                    layout.size = size;
                    layout.stride = size.align_to(align);
                    Arc::new(ReprAndLayout { repr, layout })
                })
                .collect::<Vec<_>>();

            if size == tag_size {
                (
                    tag.clone(),
                    Fields::Arbitrary {
                        offsets: vec![Size::ZERO],
                    },
                    Variants::Multiple {
                        tag,
                        tag_encoding,
                        variants,
                        tag_field: 0,
                    },
                )
            } else {
                (tag.clone(), Fields::Arbitrary { offsets }, Variants::Multiple {
                    tag,
                    tag_encoding,
                    variants,
                    tag_field: 0,
                })
            }
        };

        let (tag, fields, variants) = if let Some(niche) = largest_niche {
            if niche.available(triple) >= lyts.len() as u128 {
                // @TODO: implement niches
                no_niche(lyts)
            } else {
                no_niche(lyts)
            }
        } else {
            no_niche(lyts)
        };

        let stride = size.align_to(align);

        if tag.value.size(triple) == size {
            Layout {
                size,
                align,
                stride,
                abi: Abi::Scalar(tag),
                fields,
                variants,
                largest_niche: None,
            }
        } else {
            Layout {
                size,
                align,
                stride,
                abi: Abi::Aggregate { sized: true },
                fields,
                variants,
                largest_niche: None,
            }
        }
    }
}

impl ReprAndLayout {
    pub fn elem(&self, db: &dyn MirDatabase) -> Option<Arc<ReprAndLayout>> {
        let el = match &self.repr {
            | Repr::Ptr(el, _, _) | Repr::Box(el) | Repr::Array(_, el) => el,
            | _ => return None,
        };

        Some(db.layout_of((**el).clone()))
    }

    #[track_caller]
    pub fn field(&self, db: &dyn MirDatabase, field: usize) -> Option<Arc<ReprAndLayout>> {
        assert!(field < self.fields.count());

        match &self.repr {
            | Repr::Array(_, el) => Some(db.layout_of((**el).clone())),
            | Repr::Struct(reprs) => Some(db.layout_of(reprs[field].clone())),
            | Repr::Ptr(el, true, nn) => match field {
                | 0 => Some(db.layout_of(Repr::Ptr(el.clone(), false, *nn))),
                | 1 => Some(db.layout_of(Repr::usize())),
                | _ => unreachable!(),
            },
            | Repr::Enum(reprs) => match self.variants {
                | Variants::Single { index } => db.layout_of(reprs[index].clone()).field(db, field),
                | Variants::Multiple { ref tag, .. } => {
                    assert_eq!(field, 0);
                    Some(Arc::new(ReprAndLayout {
                        repr: Repr::Scalar(tag.clone()),
                        layout: Layout::scalar(tag.clone(), db.target().triple()),
                    }))
                },
            },
            | _ => None,
        }
    }

    pub fn variant(self: &Arc<ReprAndLayout>, variant: usize) -> Arc<ReprAndLayout> {
        match self.variants {
            | Variants::Single { index } if variant == index && self.fields != Fields::Primitive => self.clone(),
            | Variants::Single { .. } => unreachable!(),
            // | Variants::Single { index } => Arc::new(Layout {
            //     size: Size::ZERO,
            //     stride: Size::ZERO,
            //     align: Align::ONE,
            //     elem: None,
            //     abi: Abi::Uninhabited,
            //     fields: Fields::Arbitrary { fields: Vec::new() },
            //     variants: Variants::Single { index },
            //     largest_niche: None,
            // }),
            | Variants::Multiple { ref variants, .. } => variants[variant].clone(),
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::UNIT
    }
}

impl Layout {
    pub const UNIT: Self = Self {
        size: Size::ZERO,
        align: Align::ONE,
        stride: Size::ZERO,
        abi: Abi::Aggregate { sized: true },
        fields: Fields::Arbitrary { offsets: Vec::new() },
        variants: Variants::Single { index: 0 },
        largest_niche: None,
    };

    pub fn scalar(scalar: Scalar, triple: &Triple) -> Self {
        let size = scalar.value.size(triple);
        let align = Align::from_bytes(size.bytes());
        let largest_niche = Niche::from_scalar(triple, Size::ZERO, scalar.clone());

        Self {
            size,
            align,
            stride: size.align_to(align),
            abi: Abi::Scalar(scalar),
            fields: Fields::Primitive,
            variants: Variants::Single { index: 0 },
            largest_niche,
        }
    }

    pub fn is_zst(&self) -> bool {
        match self.abi {
            | Abi::Scalar(_) | Abi::ScalarPair(_, _) => false,
            | Abi::Uninhabited => self.size.bytes() == 0,
            | Abi::Aggregate { sized } => sized && self.size.bytes() == 0,
        }
    }

    pub fn is_bool(&self) -> bool {
        match self.abi {
            | Abi::Scalar(ref s) => match s.value {
                | Primitive::Int(_, _) => s.valid_range == (0..=1),
                | _ => false,
            },
            | _ => false,
        }
    }

    pub fn is_float(&self) -> bool {
        match self.abi {
            | Abi::Scalar(ref s) => match s.value {
                | Primitive::Float | Primitive::Double => true,
                | _ => false,
            },
            | _ => false,
        }
    }

    pub fn is_signed(&self) -> bool {
        match self.abi {
            | Abi::Scalar(ref s) => match s.value {
                | Primitive::Int(_, s) => s,
                | _ => false,
            },
            | _ => false,
        }
    }
}

impl Size {
    pub const ZERO: Self = Self { raw: 0 };

    pub fn from_bits(bits: impl TryInto<u64>) -> Self {
        let bits = bits.try_into().ok().unwrap();

        Self::from_bytes(bits / 8 + ((bits % 8) + 7) / 8)
    }

    pub fn from_bytes(bytes: impl TryInto<u64>) -> Self {
        Self {
            raw: bytes.try_into().ok().unwrap(),
        }
    }

    pub fn bytes(self) -> u64 {
        self.raw
    }

    pub fn bits(self) -> u64 {
        self.bytes() * 8
    }

    pub fn align_to(self, align: Align) -> Self {
        let mask = align.bytes() - 1;

        Self::from_bytes((self.bytes() + mask) & !mask)
    }

    pub fn is_aligned(self, align: Align) -> bool {
        let mask = align.bytes() - 1;

        self.bytes() & mask == 0
    }
}

impl std::ops::Add for Size {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            raw: self.raw + rhs.raw,
        }
    }
}

impl std::ops::Sub for Size {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            raw: self.raw - rhs.raw,
        }
    }
}

impl std::ops::Mul<u64> for Size {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self::Output {
        Self { raw: self.raw * rhs }
    }
}

impl Align {
    pub const ONE: Self = Self { pow2: 0 };

    pub fn from_bits(bits: u64) -> Self {
        Self::from_bytes(Size::from_bits(bits).bytes())
    }

    pub fn from_bytes(mut bytes: u64) -> Self {
        if bytes == 0 {
            return Self::ONE;
        }

        let mut pow2 = 0u8;

        while (bytes & 1) == 0 {
            pow2 += 1;
            bytes >>= 1;
        }

        Self { pow2 }
    }

    pub fn bytes(self) -> u64 {
        1 << self.pow2
    }

    pub fn bits(self) -> u64 {
        self.bytes() * 8
    }

    pub fn max_for_offset(offset: Size) -> Self {
        Self {
            pow2: offset.bytes().trailing_zeros() as u8,
        }
    }

    pub fn restrict_for_offset(self, offset: Size) -> Self {
        self.min(Self::max_for_offset(offset))
    }
}

impl Abi {
    pub fn is_unsized(&self) -> bool {
        match self {
            | Abi::Uninhabited | Abi::Scalar(_) | Abi::ScalarPair(_, _) => false,
            | Abi::Aggregate { sized } => !sized,
        }
    }

    pub fn is_uninhabited(&self) -> bool {
        match self {
            | Abi::Uninhabited => true,
            | _ => false,
        }
    }
}

impl Fields {
    pub fn count(&self) -> usize {
        match self {
            | Fields::Primitive => 0,
            | Fields::Union(count) => *count,
            | Fields::Array { count, .. } => *count,
            | Fields::Arbitrary { offsets: fields } => fields.len(),
        }
    }

    pub fn offset(&self, idx: usize) -> Size {
        match self {
            | Fields::Primitive => unreachable!(),
            | Fields::Array { stride, .. } => {
                let i = idx as u64;

                *stride * i
            },
            | Fields::Union { .. } => Size::ZERO,
            | Fields::Arbitrary { offsets } => offsets[idx],
        }
    }

    pub fn min_offset(&self) -> Size {
        match self {
            | Fields::Arbitrary { offsets } => offsets.iter().copied().min().unwrap_or(Size::ZERO),
            | _ => Size::ZERO,
        }
    }
}

impl Niche {
    pub fn from_scalar(triple: &Triple, offset: Size, scalar: Scalar) -> Option<Self> {
        let niche = Self { offset, scalar };

        if niche.available(triple) > 0 {
            Some(niche)
        } else {
            None
        }
    }

    pub fn available(&self, triple: &Triple) -> u128 {
        let Scalar {
            value,
            valid_range: ref v,
        } = self.scalar;

        let bits = value.size(triple).bits();
        assert!(bits <= 128);
        let max_value = !0u128 >> (128 - bits);
        let niche = v.end().wrapping_add(1)..*v.start();

        niche.end.wrapping_sub(niche.start) & max_value
    }

    pub fn reserve(&self, triple: &Triple, count: u128) -> Option<(u128, Scalar)> {
        assert!(count > 0);
        let Scalar {
            value,
            valid_range: ref v,
        } = self.scalar;

        let bits = value.size(triple).bits();
        assert!(bits <= 128);
        let max_value = !0128 >> (128 - bits);

        if count > max_value {
            return None;
        }

        let start = v.end().wrapping_add(1) & max_value;
        let end = v.end().wrapping_add(count) & max_value;
        let valid_range_contains = |x| {
            if v.start() <= v.end() {
                *v.start() <= x && x <= *v.end()
            } else {
                *v.start() <= x || x <= *v.end()
            }
        };

        if valid_range_contains(end) {
            None
        } else {
            Some((start, Scalar {
                value,
                valid_range: *v.start()..=end,
            }))
        }
    }
}

impl Scalar {
    pub fn new(value: Primitive, triple: &Triple) -> Self {
        let bits = value.size(&triple).bits();

        Scalar {
            value,
            valid_range: 0..=(!0 >> (128 - bits)),
        }
    }
}

impl Primitive {
    pub fn size(self, triple: &Triple) -> Size {
        match self {
            | Primitive::Int(int, _) => int.size(triple),
            | Primitive::Float => Size::from_bits(32),
            | Primitive::Double => Size::from_bits(64),
            | Primitive::Pointer => match triple.pointer_width() {
                | Ok(PointerWidth::U16) => Size::from_bits(16),
                | Ok(PointerWidth::U32) => Size::from_bits(32),
                | Ok(PointerWidth::U64) => Size::from_bits(64),
                | Err(_) => Size::from_bits(32),
            },
        }
    }

    pub fn align(self, triple: &Triple) -> Align {
        Align::from_bytes(self.size(triple).bytes())
    }
}

impl Integer {
    pub fn size(self, triple: &Triple) -> Size {
        match self {
            | Integer::Int => match triple.pointer_width() {
                | Ok(PointerWidth::U16) => Size::from_bits(16),
                | Ok(PointerWidth::U32) => Size::from_bits(32),
                | Ok(PointerWidth::U64) => Size::from_bits(64),
                | _ => Size::from_bits(32),
            },
            | Integer::I8 => Size::from_bits(8),
            | Integer::I16 => Size::from_bits(16),
            | Integer::I32 => Size::from_bits(32),
            | Integer::I64 => Size::from_bits(64),
            | Integer::I128 => Size::from_bits(128),
        }
    }

    pub fn align(self, triple: &Triple) -> Align {
        Align::from_bytes(self.size(triple).bytes())
    }
}
