use std::sync::Arc;

use inkwell::types::BasicType;
use inkwell::values::BasicValue;
use inkwell::{values, AddressSpace};
use mir::layout::{Abi, ReprAndLayout, TagEncoding, Variants};
use mir::repr::Repr;

use crate::ctx::CodegenCtx;
use crate::operand::{OperandRef, OperandValue};

#[derive(Debug, Clone)]
pub struct PlaceRef<'ctx> {
    pub layout: Arc<ReprAndLayout>,
    pub ptr: values::PointerValue<'ctx>,
    pub extra: Option<values::BasicValueEnum<'ctx>>,
}

impl<'ctx> PlaceRef<'ctx> {
    #[allow(dead_code)]
    pub fn new_uninit(ctx: &mut CodegenCtx<'_, 'ctx>, layout: Arc<ReprAndLayout>) -> Self {
        let ty = ctx.basic_type_for_ral(&layout).ptr_type(AddressSpace::default());

        Self {
            layout,
            ptr: ty.const_null(),
            extra: None,
        }
    }

    pub fn new(
        layout: Arc<ReprAndLayout>,
        ptr: values::PointerValue<'ctx>,
        extra: Option<values::BasicValueEnum<'ctx>>,
    ) -> Self {
        Self { layout, ptr, extra }
    }

    pub fn new_alloca(ctx: &mut CodegenCtx<'_, 'ctx>, layout: Arc<ReprAndLayout>) -> Self {
        let ty = ctx.basic_type_for_ral(&layout);
        let ptr = ctx.builder.build_alloca(ty, "");

        Self {
            layout,
            ptr,
            extra: None,
        }
    }

    #[allow(dead_code)]
    pub fn cast(&self, ctx: &mut CodegenCtx<'_, 'ctx>, layout: Arc<ReprAndLayout>) -> Self {
        if self.layout == layout {
            return self.clone();
        }

        let ads = self.ptr.get_type().get_address_space();
        let ty = ctx.basic_type_for_ral(&layout).ptr_type(ads);
        let ptr = ctx.builder.build_pointer_cast(self.ptr, ty, "");

        Self {
            layout,
            ptr,
            extra: self.extra,
        }
    }

    pub fn deref(self, ctx: &mut CodegenCtx<'_, 'ctx>) -> Self {
        self.load_operand(ctx).deref(ctx)
    }

    pub fn field(&self, ctx: &mut CodegenCtx<'_, 'ctx>, index: usize) -> Self {
        let triple = ctx.triple;
        let field = self.layout.field(ctx.db, index).unwrap();
        let offset = self.layout.fields.offset(index);

        let ptr = match self.layout.abi {
            | _ if offset.bytes() == 0 => self.ptr,
            | Abi::ScalarPair(ref a, ref b) if offset == a.value.size(triple).align_to(b.value.align(triple)) => {
                ctx.builder.build_struct_gep(self.ptr, 1, "").unwrap()
            },
            | Abi::Scalar(_) | Abi::ScalarPair(..) if field.is_zst() => {
                let ptr = ctx.context.i8_type().ptr_type(AddressSpace::default());
                let ptr = ctx.builder.build_pointer_cast(self.ptr, ptr, "");
                let usize = ctx.context.ptr_sized_int_type(&ctx.target_data, None);
                let offset = usize.const_int(offset.bytes(), false);
                unsafe { ctx.builder.build_gep(ptr, &[offset], "") }
            },
            | Abi::Scalar(_) | Abi::ScalarPair(..) => unreachable!(),
            | _ => {
                let min_offset = self.layout.fields.min_offset();
                let index = if min_offset.bytes() != 0 { index + 1 } else { index };

                ctx.builder.build_struct_gep(self.ptr, index as u32, "").unwrap()
            },
        };

        let ty = ctx.basic_type_for_ral(&field).ptr_type(AddressSpace::default());
        let ptr = ctx.builder.build_pointer_cast(ptr, ty, "");

        PlaceRef::new(field, ptr, None)
    }

    pub fn index(&self, ctx: &mut CodegenCtx<'_, 'ctx>, index: values::BasicValueEnum<'ctx>) -> Self {
        let layout = self.layout.elem(ctx.db).unwrap();
        let index = index.into_int_value();
        let zero = ctx
            .context
            .ptr_sized_int_type(&ctx.target_data, None)
            .const_int(0, false);
        let ptr = unsafe { ctx.builder.build_in_bounds_gep(self.ptr, &[zero, index], "") };

        Self::new(layout, ptr, None)
    }

    pub fn slice(
        &self,
        ctx: &mut CodegenCtx<'_, 'ctx>,
        lo: values::BasicValueEnum<'ctx>,
        hi: values::BasicValueEnum<'ctx>,
    ) -> Self {
        let repr = Repr::Ptr(Box::new(self.layout.elem(ctx.db).unwrap().repr.clone()), true, false);
        let layout = ctx.db.layout_of(repr);
        let mut slice = self.index(ctx, lo);
        let lo = lo.into_int_value();
        let hi = hi.into_int_value();
        let len = ctx.builder.build_int_sub(hi, lo, "");

        slice.layout = layout;
        slice.extra = Some(len.as_basic_value_enum());
        slice
    }

    pub fn downcast(&self, ctx: &mut CodegenCtx<'_, 'ctx>, ctor: hir::Ctor) -> Self {
        let mut downcast = self.clone();
        let ctors = ctor.type_ctor().ctors(ctx.db.upcast());
        let index = ctors.iter().position(|&c| c == ctor).unwrap();

        downcast.layout = self.layout.variant(index);

        let ty = ctx.basic_type_for_ral(&downcast.layout);
        let ty = ty.ptr_type(AddressSpace::default());

        downcast.ptr = ctx.builder.build_pointer_cast(downcast.ptr, ty, "");
        downcast
    }

    pub fn get_discr(&self, ctx: &mut CodegenCtx<'_, 'ctx>, layout: &Arc<ReprAndLayout>) -> values::IntValue<'ctx> {
        let discr_ty = ctx.basic_type_for_ral(&layout).into_int_type();
        let (_tag_scalar, tag_encoding, tag_field) = match self.layout.variants {
            | Variants::Single { index } => {
                return discr_ty.const_int(index as u64, false);
            },
            | Variants::Multiple {
                ref tag,
                ref tag_encoding,
                tag_field,
                ..
            } => (tag, tag_encoding, tag_field),
        };

        let tag = self.field(ctx, tag_field);
        let tag_op = tag.load_operand(ctx);
        let tag_imm = tag_op.immediate().into_int_value();

        match *tag_encoding {
            | TagEncoding::Direct => ctx.builder.build_int_cast(tag_imm, discr_ty, ""),
            | TagEncoding::Niche { .. } => todo!(),
        }
    }

    pub fn set_discr(&self, ctx: &mut CodegenCtx<'_, 'ctx>, idx: usize) {
        if self.layout.variant(idx).abi.is_uninhabited() {
            return;
        }

        match self.layout.variants {
            | Variants::Single { index } => {
                assert_eq!(index, idx);
            },
            | Variants::Multiple {
                tag_encoding: TagEncoding::Direct,
                tag_field,
                ref tag,
                ..
            } => {
                let ptr = self.field(ctx, tag_field).ptr;
                let ty = ctx.basic_type_for_scalar(tag).into_int_type();
                let val = ty.const_int(idx as u64, false);

                ctx.builder.build_store(ptr, val);
            },
            | Variants::Multiple {
                tag_encoding:
                    TagEncoding::Niche {
                        dataful_variant,
                        ref niche_variants,
                        niche_start,
                    },
                tag_field,
                ..
            } => {
                if idx != dataful_variant {
                    let niche = self.field(ctx, tag_field);
                    let niche_ty = ctx.basic_type_for_ral(&niche.layout).into_int_type();
                    let niche_val = idx - *niche_variants.start();
                    let niche_val = (niche_val as u128).wrapping_add(niche_start);
                    let niche_val = niche_ty.const_int(niche_val as u64, false);

                    ctx.builder.build_store(niche.ptr, niche_val);
                }
            },
        }
    }

    pub fn load_operand(self, ctx: &mut CodegenCtx<'_, 'ctx>) -> OperandRef<'ctx> {
        if self.layout.is_zst() {
            return OperandRef::new_zst(ctx, self.layout);
        }

        let val = if let Some(extra) = self.extra {
            OperandValue::Pair(self.ptr.as_basic_value_enum(), extra)
        } else {
            match &self.layout.abi {
                | Abi::Scalar(_) => {
                    let load = ctx.builder.build_load(self.ptr, "");

                    OperandValue::Imm(load)
                },
                | Abi::ScalarPair(_, _) => {
                    let load = |i| {
                        let ptr = ctx.builder.build_struct_gep(self.ptr, i, "").unwrap();
                        ctx.builder.build_load(ptr, "")
                    };

                    OperandValue::Pair(load(0), load(1))
                },
                | _ => OperandValue::Ref(self.ptr, self.extra),
            }
        };

        OperandRef {
            layout: self.layout,
            val,
        }
    }
}
