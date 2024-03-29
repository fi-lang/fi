use base_db::libs::LibId;
use expr::Arg;
use hir::id::HasModule;
use hir_def::lang_item;

use super::*;
use crate::repr::{ArrayLen, Repr};

impl BodyLowerCtx<'_> {
    fn lib(&self) -> LibId {
        self.builder.origin().def.module(self.db.upcast()).lib
    }

    pub fn lower_intrinsic(
        &mut self,
        expr: hir::ExprId,
        name: &str,
        args: Vec<Arg>,
        store_in: &mut Option<Place>,
    ) -> Operand {
        let mut args = args.into_iter();

        match name {
            // | "partial" => self.lower_arg(args.next().unwrap(), store_in),
            // | "unsafe" => self.lower_arg(args.next().unwrap(), store_in),
            | "apply" => {
                let base = args.next().unwrap();

                self.lower_app(expr, base, args.collect(), store_in)
            },
            | "crash" => {
                let _msg = args.next().unwrap();
                self.builder.abort();
                Operand::Const(Const::Unit, Repr::unit())
            },
            | "drop" => {
                let arg = self.lower_arg(args.next().unwrap(), &mut None);
                let arg = match arg {
                    | Operand::Copy(place) | Operand::Move(place) => place,
                    | _ => unreachable!(),
                };

                assert!(arg.projection.is_empty());
                self.builder.drop(arg.local);
                Operand::Const(Const::Unit, Repr::unit())
            },
            | "copy" => {
                let arg = self.lower_arg(args.next().unwrap(), &mut None);

                match arg {
                    | Operand::Move(place) => Operand::Copy(place),
                    | _ => arg,
                }
            },
            | "size_of" => self.lower_intrinsic_nullop(expr, NullOp::SizeOf, args, store_in),
            | "align_of" => self.lower_intrinsic_nullop(expr, NullOp::AlignOf, args, store_in),
            | "stride_of" => self.lower_intrinsic_nullop(expr, NullOp::StrideOf, args, store_in),
            | "zeroed" => {
                let proxy = match args.next().unwrap() {
                    | Arg::ExprId(e) => self.infer.type_of_expr[e],
                    | Arg::Op(_) => unreachable!(),
                };

                let proxy = self.get_proxy_type(proxy);
                let repr = self.db.repr_of(proxy);

                Operand::Const(Const::Zeroed, repr)
            },
            | "addr_of" => {
                let place = self.lower_arg(args.next().unwrap(), &mut None);
                let place = self.place_op(place);
                let repr = self.db.repr_of(self.infer.type_of_expr[expr]);
                let res = self.builder.add_local(LocalKind::Tmp, repr);

                self.builder.init(res);
                self.builder.ref_(Place::new(res), place);
                Operand::Move(Place::new(res))
            },
            | "ptr_read" => {
                let arg = self.lower_arg(args.next().unwrap(), &mut None);
                let place = self.place_op(arg);

                Operand::Move(place.deref())
            },
            | "ptr_write" => {
                let place = self.lower_arg(args.next().unwrap(), &mut None);
                let place = self.place_op(place);
                let op = self.lower_arg(args.next().unwrap(), &mut None);

                self.builder.assign(place.deref(), op);
                Operand::Const(Const::Unit, Repr::unit())
            },
            | "ptr_offset" => {
                let ptr = self.lower_arg(args.next().unwrap(), &mut None);
                let offset = self.lower_arg(args.next().unwrap(), &mut None);
                let ty = self.infer.type_of_expr[expr];
                let res = self.store_in(store_in, ty);

                self.builder.binop(res.clone(), BinOp::Offset, ptr, offset);
                Operand::Move(res)
            },
            | "array_index" => {
                let arr = self.lower_arg(args.next().unwrap(), &mut None);
                let arr = self.place_op(arr);
                let idx = self.lower_arg(args.next().unwrap(), &mut None);

                Operand::Copy(arr.index(idx))
            },
            | "array_slice" => {
                let arr = self.lower_arg(args.next().unwrap(), &mut None);
                let arr = self.place_op(arr);
                let lo = self.lower_arg(args.next().unwrap(), &mut None);
                let hi = self.lower_arg(args.next().unwrap(), &mut None);

                Operand::Copy(arr.slice(lo, hi))
            },
            | "array_len" => {
                let arr = self.lower_arg(args.next().unwrap(), &mut None);
                let repr = self.builder.operand_repr(&arr);
                let len = match repr {
                    | Repr::Array(len, _) => len,
                    | _ => unreachable!(),
                };

                let len = match len {
                    | ArrayLen::Const(l) => Const::Int(l as i128),
                    | ArrayLen::TypeVar(v) => Const::TypeVar(v),
                };

                Operand::Const(len, Repr::isize())
            },
            | "iadd" => self.lower_intrinsic_binop(expr, BinOp::Add, args, store_in),
            | "isub" => self.lower_intrinsic_binop(expr, BinOp::Sub, args, store_in),
            | "ieq" => self.lower_intrinsic_binop(expr, BinOp::Eq, args, store_in),
            | "ilt" => self.lower_intrinsic_binop(expr, BinOp::Lt, args, store_in),
            | "iconvert" => {
                let val = self.lower_arg(args.next().unwrap(), &mut None);
                let ty = self.infer.type_of_expr[expr];
                let res = self.store_in(store_in, ty);

                self.builder.cast(res.clone(), CastKind::IntToInt, val);
                Operand::Move(res)
            },
            | "transmute" => {
                let arg = self.lower_arg(args.next().unwrap(), &mut None);
                let ty = self.infer.type_of_expr[expr];
                let res = self.store_in(store_in, ty);

                self.builder.cast(res.clone(), CastKind::Bitcast, arg);
                Operand::Move(res)
            },
            | _ => {
                let args = args.map(|a| self.lower_arg(a, &mut None)).collect::<Vec<_>>();
                let ty = self.infer.type_of_expr[expr];
                let res = self.store_in(store_in, ty);

                self.builder.intrinsic(res.clone(), name.to_string(), args);
                Operand::Move(res)
            },
        }
    }

    fn lower_intrinsic_binop(
        &mut self,
        expr: hir::ExprId,
        op: BinOp,
        mut args: impl Iterator<Item = Arg>,
        store_in: &mut Option<Place>,
    ) -> Operand {
        let lhs = self.lower_arg(args.next().unwrap(), &mut None);
        let rhs = self.lower_arg(args.next().unwrap(), &mut None);
        let ty = self.infer.type_of_expr[expr];
        let res = self.store_in(store_in, ty);

        self.builder.binop(res.clone(), op, lhs, rhs);
        Operand::Move(res)
    }

    fn lower_intrinsic_nullop(
        &mut self,
        expr: hir::ExprId,
        op: NullOp,
        mut args: impl Iterator<Item = Arg>,
        store_in: &mut Option<Place>,
    ) -> Operand {
        let proxy = match args.next().unwrap() {
            | Arg::ExprId(e) => self.infer.type_of_expr[e],
            | Arg::Op(_) => unreachable!(),
        };

        let proxy = self.get_proxy_type(proxy);
        let repr = self.db.repr_of(proxy);
        let res = self.store_in(store_in, self.infer.type_of_expr[expr]);

        self.builder.nullop(res.clone(), op, repr);
        Operand::Move(res)
    }

    fn get_proxy_type(&self, proxy: Ty) -> Ty {
        let lib = self.lib();
        let proxy_id = self.db.lang_item(lib, lang_item::PROXY_TYPE).unwrap();
        let proxy_id = proxy_id.as_type_ctor().unwrap();

        proxy.match_ctor(self.db.upcast(), proxy_id).unwrap()[0]
    }
}
