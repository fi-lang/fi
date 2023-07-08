use arena::Idx;
use base_db::Error;
use hir::display::HirDisplay;
use inkwell::values::{self, BasicValue};
use inkwell::{types, IntPredicate};
use mir::instance::Instance;
use mir::ir::{self, Local, LocalKind};
use mir::repr::{Repr, ReprKind};

use crate::abi::{ArgAbi, EmptySinglePair, PassMode};
use crate::ctx::BodyCtx;
use crate::layout::{repr_and_layout, Abi, ReprAndLayout};
use crate::local::LocalRef;
use crate::operand::{OperandRef, OperandValue};
use crate::place::PlaceRef;

impl<'ctx> BodyCtx<'_, '_, 'ctx> {
    pub fn codegen(&mut self) {
        tracing::debug!("{}", self.func);
        let by_ref_locals = crate::ssa::analyze(self);
        tracing::debug!("{:?}", by_ref_locals);
        let entry = self.context.append_basic_block(self.func, "entry");
        let first_block = Idx::from_raw(0u32.into());
        self.builder.position_at_end(entry);

        for (arg, local_ref) in self.body.blocks[first_block]
            .params
            .iter()
            .zip(self.arg_local_refs(&by_ref_locals))
        {
            self.locals.insert(arg.0, local_ref);
        }

        if self.fn_abi.ret.is_indirect() {
            let ptr = self.func.get_first_param().unwrap().into_pointer_value();
            let val = PlaceRef::new(self.fn_abi.ret.layout.clone(), ptr, None);

            self.ret_ptr = Some(val);
        }

        for (local, data) in self.body.locals.iter() {
            if !self.body.blocks[first_block].params.contains(&ir::Local(local)) {
                let repr = self.instance.subst_repr(self.db, data.repr);
                let layout = repr_and_layout(self.db, repr);
                let value = if by_ref_locals.contains(&ir::Local(local)) {
                    if layout.abi.is_unsized() {
                        todo!();
                    } else {
                        LocalRef::Place(PlaceRef::new_alloca(self.cx, layout))
                    }
                } else if data.kind == LocalKind::Arg {
                    continue;
                } else {
                    LocalRef::new_operand(self.cx, layout)
                };

                self.locals.insert(local, value);
            }
        }

        for (block, data) in self.body.blocks.iter() {
            let name = format!("block{}", u32::from(block.into_raw()));
            let bb = self.context.append_basic_block(self.func, &name);
            self.builder.position_at_end(bb);
            self.blocks.insert(block, bb);

            if block != first_block {
                for local in data.params.iter() {
                    if !by_ref_locals.contains(local) {
                        let repr = self.instance.subst_repr(self.db, self.body.locals[local.0].repr);
                        let layout = repr_and_layout(self.db, repr);

                        if layout.is_zst() {
                            let zst = OperandRef::new_zst(self.cx, layout);
                            self.locals.insert(local.0, LocalRef::Operand(Some(zst)));
                        } else {
                            let ty = self.basic_type_for_ral(&layout);
                            let phi = self.builder.build_phi(ty, "");
                            let phi = OperandRef::new_phi(layout, phi);
                            self.locals.insert(local.0, LocalRef::Operand(Some(phi)));
                        }
                    }
                }
            }
        }

        let first_block = self.blocks[first_block];
        self.builder.position_at_end(entry);
        self.builder.build_unconditional_branch(first_block);

        for (block, data) in self.body.blocks.iter() {
            self.codegen_block(ir::Block(block), data);
        }

        if self.func.verify(true) {
            self.fpm.run_on(&self.func);
        } else {
            eprintln!();
            self.func.print_to_stderr();
            eprintln!();

            unsafe {
                self.func.delete();
            }

            panic!("generated invalid function");
        }
    }

    pub fn codegen_block(&mut self, block: ir::Block, data: &ir::BlockData) {
        let bb = self.blocks[block.0];
        self.builder.position_at_end(bb);

        for stmt in &data.statements {
            self.codegen_statement(stmt);
        }

        self.codegen_terminator(&data.terminator);
    }

    pub fn codegen_terminator(&mut self, terminator: &ir::Terminator) {
        match terminator {
            | ir::Terminator::None => unreachable!(),
            | ir::Terminator::Unreachable => {
                // let msg = self.alloc_string("unreachable code reached");
                // self.build_puts(msg.as_pointer_value());
                self.builder.build_unreachable();
            },
            | ir::Terminator::Abort => {
                let exit = match self.module.get_function("exit") {
                    | Some(func) => func,
                    | None => {
                        let i32t = self.context.i32_type().into();
                        let ty = self.context.void_type().fn_type(&[i32t], false);
                        self.module.add_function("exit", ty, None)
                    },
                };

                let one = self.context.i32_type().const_int(1, true);
                self.builder.build_call(exit, &[one.into()], "");
                self.builder.build_unreachable();
            },
            | ir::Terminator::Return(op) => self.codegen_return(op),
            | ir::Terminator::Jump(target) => {
                self.codegen_jump_target(target);
                let block = self.blocks[target.block.0];
                self.builder.build_unconditional_branch(block);
            },
            | ir::Terminator::Switch { discr, values, targets } => self.codegen_switch(discr, values, targets),
        }
    }

    pub fn codegen_return(&mut self, op: &ir::Operand) {
        match self.fn_abi.ret.mode {
            | PassMode::NoPass => {
                self.builder.build_return(None);
            },
            | PassMode::ByVal(_) => {
                let op = self.codegen_operand(op);
                let value = op.load(self.cx);

                self.builder.build_return(Some(&value));
            },
            | PassMode::ByValPair(_, _) => {
                let op = self.codegen_operand(op);
                let (a, b) = op.pair();

                self.builder.build_aggregate_return(&[a, b]);
            },
            | PassMode::ByRef { size: Some(_) } => {
                let ret_ptr = self.ret_ptr.clone().unwrap();
                let op = self.codegen_operand(op);

                op.store(self.cx, &ret_ptr);
                self.builder.build_return(None);
            },
            | PassMode::ByRef { size: None } => todo!(),
        }
    }

    pub fn codegen_switch(&mut self, discr: &ir::Operand, values: &[i128], targets: &[ir::JumpTarget]) {
        let mut targets = targets.iter();
        let discr = self.codegen_operand(discr);
        let discr_val = discr.load(self.cx).into_int_value();
        let discr_ty = discr_val.get_type();

        if values.len() == 1 {
            let val = discr_ty.const_int(values[0] as u64, false);
            let then = targets.next().unwrap();
            self.codegen_jump_target(then);
            let then = self.blocks[then.block.0];
            let else_ = targets.next().unwrap();
            self.codegen_jump_target(else_);
            let else_ = self.blocks[else_.block.0];

            if discr.layout.is_bool() {
                let i1_type = self.context.bool_type();
                let discr_val = self.builder.build_int_truncate(discr_val, i1_type, "");

                match values[0] {
                    | 0 => self.builder.build_conditional_branch(discr_val, else_, then),
                    | 1 => self.builder.build_conditional_branch(discr_val, then, else_),
                    | _ => unreachable!(),
                };
            } else {
                let cmp = self.builder.build_int_compare(IntPredicate::EQ, discr_val, val, "");
                self.builder.build_conditional_branch(cmp, then, else_);
            }

            return;
        }

        let cases = values
            .iter()
            .zip(&mut targets)
            .map(|(&val, target)| {
                let val = discr_ty.const_int(val as u64, false);
                self.codegen_jump_target(target);
                (val, self.blocks[target.block.0])
            })
            .collect::<Vec<_>>();

        let else_ = targets.next().unwrap();
        self.codegen_jump_target(else_);
        let else_ = self.blocks[else_.block.0];
        self.builder.build_switch(discr_val, else_, &cases);
    }

    pub fn codegen_jump_target(&mut self, target: &ir::JumpTarget) {
        let data = &self.body.blocks[target.block.0];
        let block = self.builder.get_insert_block().unwrap();

        for (param, arg) in data.params.iter().zip(target.args.iter()) {
            let op = self.codegen_operand(arg);

            match self.locals[param.0].clone() {
                | LocalRef::Place(place) => {
                    op.store(self.cx, &place);
                },
                | LocalRef::Operand(Some(phi)) => {
                    if !phi.layout.is_zst() {
                        let phi = phi.phi();
                        let val = op.load(self.cx);
                        phi.add_incoming(&[(&val, block)]);
                    }
                },
                | LocalRef::Operand(None) => unreachable!(),
            }
        }
    }

    pub fn codegen_statement(&mut self, stmt: &ir::Statement) {
        match stmt {
            | ir::Statement::Init(local) => self.codegen_init(*local),
            | ir::Statement::Drop(place) => self.codegen_drop(place),
            | ir::Statement::Assign(place, rvalue) => self.codegen_assign(place, rvalue),
            | ir::Statement::Call { place, func, args } => self.codegen_call(place, func, args),
            | ir::Statement::Intrinsic { place, name, args } => self.codegen_intrinsic(place, name, args),
            | ir::Statement::SetDiscriminant(place, ctor) => self.codegen_set_discriminant(place, *ctor),
        }
    }

    pub fn codegen_init(&mut self, local: Local) {
        let repr = self.instance.subst_repr(self.db, self.body.locals[local.0].repr);

        if let ReprKind::Box(inner) = repr.kind(self.db) {
            let layout = repr_and_layout(self.db, repr);
            let inner_layout = repr_and_layout(self.db, *inner);
            let box_alloc = if let Some(func) = self.module.get_function("box_alloc") {
                func
            } else {
                let arg = self.usize_type().into();
                let ty = self.ptr_type().fn_type(&[arg], false);
                self.module.add_function("box_alloc", ty, None)
            };

            let size = self.usize_type().const_int(inner_layout.size.bytes(), false);
            let call = self.builder.build_direct_call(box_alloc, &[size.into()], "");
            let ptr = call.try_as_basic_value().unwrap_left();
            let op = OperandRef::new_imm(layout, ptr);

            match &self.locals[local.0] {
                | LocalRef::Place(place) => op.store(self.cx, place),
                | LocalRef::Operand(None) => self.locals[local.0] = LocalRef::Operand(Some(op)),
                | LocalRef::Operand(Some(_)) => unreachable!(),
            }
        }
    }

    pub fn codegen_drop(&mut self, place: &ir::Place) {
        let layout = self.place_layout(place);

        if let ReprKind::Box(repr) = layout.repr.kind(self.db) {
            assert!(place.projection.is_empty());

            if let LocalRef::Operand(Some(op)) = &self.locals[place.local.0] {
                let ptr = op.load(self.cx).into_pointer_value();
                self.gen_drop_box(ptr, false, *repr);
            }
        }
    }

    pub fn codegen_assign(&mut self, place: &ir::Place, rvalue: &ir::RValue) {
        if place.projection.is_empty() {
            match &self.locals[place.local.0] {
                | LocalRef::Place(place) => self.codegen_rvalue(place.clone(), rvalue),
                | LocalRef::Operand(None) => {
                    let repr = self.instance.subst_repr(self.db, self.body.locals[place.local.0].repr);
                    let layout = repr_and_layout(self.db, repr);
                    let op = self.codegen_rvalue_operand(layout, rvalue);
                    self.locals[place.local.0] = LocalRef::Operand(Some(op));
                },
                | LocalRef::Operand(Some(op)) => {
                    assert!(op.layout.is_zst());
                    let repr = self.instance.subst_repr(self.db, self.body.locals[place.local.0].repr);
                    let layout = repr_and_layout(self.db, repr);
                    self.codegen_rvalue_operand(layout, rvalue);
                },
            }
        } else {
            let place = self.codegen_place(place);
            self.codegen_rvalue(place, rvalue);
        }
    }

    pub fn codegen_call(&mut self, place: &ir::Place, func: &ir::Operand, args: &[ir::Operand]) {
        let func = self.codegen_operand(func);
        let (func_sig, env) = match func.layout.repr.kind(self.db) {
            | ReprKind::Func(sig, env) => (sig, *env),
            | _ => unreachable!(),
        };

        tracing::debug!("{}, {env}", func_sig.display(self.db));
        let func_abi = self.compute_fn_abi(func_sig, env);
        let func_ty = self.fn_type_for_abi(&func_abi);
        let (func, env) = match func.val {
            | OperandValue::Ref(ptr, None) => (
                self.builder.build_load(func_ty.ptr_type(Default::default()), ptr, ""),
                None,
            ),
            | OperandValue::Pair(ptr, env) => (ptr, Some(env)),
            | _ => (func.immediate(), None),
        };

        let ret_ptr = if func_abi.ret.is_indirect() {
            Some(self.codegen_place(place).ptr.as_basic_value_enum().into())
        } else {
            None
        };

        let e = env.is_some() as usize;
        let args = ret_ptr
            .into_iter()
            .chain(env.map(Into::into))
            .chain(
                args.iter()
                    .zip(func_abi.args[e..].iter())
                    .flat_map(|(arg, abi)| self.pass_arg(arg, abi))
                    .collect::<Vec<_>>(),
            )
            .chain(
                args[func_abi.args.len() - e..]
                    .iter()
                    .flat_map(|arg| self.pass_extra_arg(arg)),
            )
            .collect::<Vec<_>>();

        let func = func.into_pointer_value();
        let call = self.builder.build_indirect_call(func_ty, func, &args, "");
        let call = call.try_as_basic_value();

        match func_abi.ret.mode {
            | PassMode::ByVal(_) => {
                let res = OperandRef::new_imm(func_abi.ret.layout, call.left().unwrap());
                self.store_return(place, res);
            },
            | PassMode::ByValPair(_, _) => {
                let val = call.left().unwrap().into_struct_value();
                let a = self.builder.build_extract_value(val, 0, "").unwrap();
                let b = self.builder.build_extract_value(val, 1, "").unwrap();
                let res = OperandRef::new_pair(func_abi.ret.layout, a, b);
                self.store_return(place, res);
            },
            | _ => {},
        }
    }

    pub fn pass_arg(
        &mut self,
        arg: &ir::Operand,
        abi: &ArgAbi<'ctx>,
    ) -> EmptySinglePair<values::BasicMetadataValueEnum<'ctx>> {
        tracing::debug!("{}: {:?}", abi.layout.repr.display(self.db), abi.mode);
        match abi.mode {
            | PassMode::NoPass => EmptySinglePair::Empty,
            | PassMode::ByVal(_) => EmptySinglePair::Single(self.codegen_operand(arg).load(self.cx).into()),
            | PassMode::ByValPair(_, _) => {
                let op = self.codegen_operand(arg);
                let (a, b) = op.pair();
                EmptySinglePair::Pair(a.into(), b.into())
            },
            | PassMode::ByRef { size: Some(_) } => {
                let op = self.codegen_operand(arg);
                let ptr = self.make_ref(op).ptr.as_basic_value_enum();
                EmptySinglePair::Single(ptr.into())
            },
            | PassMode::ByRef { size: None } => todo!(),
        }
    }

    pub fn pass_extra_arg(&mut self, arg: &ir::Operand) -> EmptySinglePair<values::BasicMetadataValueEnum<'ctx>> {
        let op = self.codegen_operand(arg);
        let abi = self.compute_layout_abi(op.layout.clone());

        match abi.mode {
            | PassMode::NoPass => EmptySinglePair::Empty,
            | PassMode::ByVal(_) => EmptySinglePair::Single(op.load(self.cx).into()),
            | PassMode::ByValPair(_, _) => {
                let (a, b) = op.pair();
                EmptySinglePair::Pair(a.into(), b.into())
            },
            | PassMode::ByRef { size: Some(_) } => {
                let ptr = self.make_ref(op).ptr.as_basic_value_enum();
                EmptySinglePair::Single(ptr.into())
            },
            | PassMode::ByRef { size: None } => todo!(),
        }
    }

    pub fn store_return(&mut self, place: &ir::Place, op: OperandRef<'ctx>) {
        if !place.projection.is_empty() {
            let place = self.codegen_place(place);
            op.store(self.cx, &place);
        }

        match self.locals[place.local.0].clone() {
            | LocalRef::Place(place) => op.store(self.cx, &place),
            | LocalRef::Operand(None) => self.locals[place.local.0] = LocalRef::Operand(Some(op)),
            | LocalRef::Operand(Some(_)) => unreachable!(),
        }
    }

    pub fn codegen_set_discriminant(&mut self, place: &ir::Place, ctor: hir::id::CtorId) {
        let place = self.codegen_place(place);
        let ctors = hir::Ctor::from(ctor).type_ctor(self.db).ctors(self.db);
        let index = ctors.iter().position(|c| c.id() == ctor).unwrap();

        place.set_discr(self.cx, index);
    }

    pub fn codegen_get_discriminant(&mut self, layout: ReprAndLayout, place: &ir::Place) -> OperandRef<'ctx> {
        let place = self.codegen_place(place);
        let discr = place.get_discr(self.cx, &layout).as_basic_value_enum();

        OperandRef::new_imm(layout, discr)
    }

    pub fn rvalue_creates_operand(&self, place: &ir::Place, rvalue: &ir::RValue) -> bool {
        match rvalue {
            | ir::RValue::Cast(ir::CastKind::Bitcast, op) => {
                let op_lyt = self.operand_layout(op);
                let cast_lyt = self.place_layout(place);

                if op_lyt.size != cast_lyt.size {
                    return false;
                }

                match (&op_lyt.abi, &cast_lyt.abi) {
                    | (Abi::Uninhabited, _) => false,
                    | (_, Abi::Uninhabited) => false,
                    | (Abi::Aggregate { sized: _ }, _) => true,
                    | (Abi::Scalar(_) | Abi::ScalarPair(_, _), Abi::Aggregate { sized: _ }) => false,
                    | (Abi::Scalar(_), Abi::Scalar(_)) => true,
                    | (Abi::ScalarPair(_, _), Abi::ScalarPair(_, _)) => true,
                    | (Abi::Scalar(_), Abi::ScalarPair(_, _)) => false,
                    | (Abi::ScalarPair(_, _), Abi::Scalar(_)) => false,
                }
            },
            | _ => true,
        }
    }

    pub fn codegen_rvalue(&mut self, place: PlaceRef<'ctx>, rvalue: &ir::RValue) {
        match rvalue {
            | ir::RValue::Use(op) => {
                let value = self.codegen_operand(op);
                value.store(self.cx, &place);
            },
            | _ => {
                let op = self.codegen_rvalue_operand(place.layout.clone(), rvalue);
                op.store(self.cx, &place);
            },
        }
    }

    pub fn codegen_rvalue_operand(&mut self, layout: ReprAndLayout, rvalue: &ir::RValue) -> OperandRef<'ctx> {
        match rvalue {
            | ir::RValue::Use(op) => self.codegen_operand(op),
            | ir::RValue::AddrOf(place) => {
                let ptr = self.codegen_place(place).ptr.as_basic_value_enum();
                OperandRef::new_imm(layout, ptr)
            },
            | ir::RValue::Cast(kind, op) => self.codegen_cast(layout, *kind, op),
            | ir::RValue::BinOp(op, lhs, rhs) => self.codegen_binop(layout, *op, lhs, rhs),
            | ir::RValue::NullOp(op, repr) => self.codegen_nullop(layout, *op, *repr),
            | ir::RValue::Discriminant(place) => self.codegen_get_discriminant(layout, place),
        }
    }

    pub fn codegen_cast(&mut self, layout: ReprAndLayout, kind: ir::CastKind, op: &ir::Operand) -> OperandRef<'ctx> {
        let value = self.codegen_operand(op);

        match kind {
            | ir::CastKind::Bitcast => {
                if value.layout.size != layout.size {
                    Error::throw("bitcast to type of different size");
                }

                value.bitcast(self.cx, layout)
            },
            | ir::CastKind::Pointer => {
                let ty = self.basic_type_for_ral(&layout).into_pointer_type();
                let ptr = value.load(self.cx).into_pointer_value();
                let ptr = self.builder.build_pointer_cast(ptr, ty, "");
                OperandRef::new_imm(layout, ptr.as_basic_value_enum())
            },
            | ir::CastKind::IntToInt => {
                let ty = self.basic_type_for_ral(&layout).into_int_type();
                let val = value.load(self.cx).into_int_value();
                let val = self.builder.build_int_cast(val, ty, "");
                OperandRef::new_imm(layout, val.as_basic_value_enum())
            },
            | ir::CastKind::FloatToFloat => {
                let ty = self.basic_type_for_ral(&layout).into_float_type();
                let val = value.load(self.cx).into_float_value();
                let val = self.builder.build_float_cast(val, ty, "");
                OperandRef::new_imm(layout, val.as_basic_value_enum())
            },
            | ir::CastKind::IntToFloat => {
                let ty = self.basic_type_for_ral(&layout).into_float_type();
                let val = value.load(self.cx).into_int_value();
                let val = if value.layout.is_signed() {
                    self.builder.build_signed_int_to_float(val, ty, "")
                } else {
                    self.builder.build_unsigned_int_to_float(val, ty, "")
                };

                OperandRef::new_imm(layout, val.as_basic_value_enum())
            },
            | ir::CastKind::FloatToInt => {
                let ty = self.basic_type_for_ral(&layout).into_int_type();
                let val = value.load(self.cx).into_float_value();
                let val = if layout.is_signed() {
                    self.builder.build_float_to_signed_int(val, ty, "")
                } else {
                    self.builder.build_float_to_unsigned_int(val, ty, "")
                };

                OperandRef::new_imm(layout, val.as_basic_value_enum())
            },
        }
    }

    pub fn codegen_binop(
        &mut self,
        layout: ReprAndLayout,
        op: ir::BinOp,
        lhs: &ir::Operand,
        rhs: &ir::Operand,
    ) -> OperandRef<'ctx> {
        let is_float = layout.is_float();
        let is_signed = layout.is_signed();
        let lhs = self.codegen_operand(lhs).load(self.cx);
        let rhs = self.codegen_operand(rhs).load(self.cx);

        if let ir::BinOp::Offset = op {
            let lhs = lhs.into_pointer_value();
            let rhs = rhs.into_int_value();
            let ty = self.basic_type_for_ral(&layout.elem(self.db).unwrap());
            let value = unsafe { self.builder.build_gep(ty, lhs, &[rhs], "") };

            OperandRef::new_imm(layout, value.as_basic_value_enum())
        } else if is_float {
            let lhs = lhs.into_float_value();
            let rhs = rhs.into_float_value();
            let val = match op {
                | ir::BinOp::Add => self.builder.build_float_add(lhs, rhs, ""),
                | ir::BinOp::Sub => self.builder.build_float_sub(lhs, rhs, ""),
                | ir::BinOp::Mul => self.builder.build_float_mul(lhs, rhs, ""),
                | ir::BinOp::Div => self.builder.build_float_div(lhs, rhs, ""),
                | ir::BinOp::Rem => self.builder.build_float_rem(lhs, rhs, ""),
                // | ir::BinOp::Eq => self.builder.build_float_compare(FloatPredicate::OEQ, lhs, rhs, ""),
                // | ir::BinOp::Ne => self.builder.build_float_compare(FloatPredicate::UNE, lhs, rhs, ""),
                // | ir::BinOp::Lt => self.builder.build_float_compare(FloatPredicate::OLT, lhs, rhs, ""),
                // | ir::BinOp::Le => self.builder.build_float_compare(FloatPredicate::OLE, lhs, rhs, ""),
                // | ir::BinOp::Gt => self.builder.build_float_compare(FloatPredicate::OGT, lhs, rhs, ""),
                // | ir::BinOp::Ge => self.builder.build_float_compare(FloatPredicate::OGE, lhs, rhs, ""),
                | _ => unreachable!(),
            };

            OperandRef::new_imm(layout, val.as_basic_value_enum())
        } else {
            let lhs = lhs.into_int_value();
            let rhs = rhs.into_int_value();
            let val = match op {
                | ir::BinOp::Add => self.builder.build_int_add(lhs, rhs, ""),
                | ir::BinOp::Sub => self.builder.build_int_sub(lhs, rhs, ""),
                | ir::BinOp::Mul => self.builder.build_int_mul(lhs, rhs, ""),
                | ir::BinOp::Div if is_signed => self.builder.build_int_signed_div(lhs, rhs, ""),
                | ir::BinOp::Div => self.builder.build_int_unsigned_div(lhs, rhs, ""),
                | ir::BinOp::Rem if is_signed => self.builder.build_int_signed_rem(lhs, rhs, ""),
                | ir::BinOp::Rem => self.builder.build_int_unsigned_rem(lhs, rhs, ""),
                | ir::BinOp::Lsh => self.builder.build_left_shift(lhs, rhs, ""),
                | ir::BinOp::Rsh => self.builder.build_right_shift(lhs, rhs, is_signed, ""),
                | ir::BinOp::And => self.builder.build_and(lhs, rhs, ""),
                | ir::BinOp::Or => self.builder.build_or(lhs, rhs, ""),
                | ir::BinOp::Xor => self.builder.build_xor(lhs, rhs, ""),
                | ir::BinOp::Eq | ir::BinOp::Ne | ir::BinOp::Lt | ir::BinOp::Le | ir::BinOp::Gt | ir::BinOp::Ge => {
                    let pred = match op {
                        | ir::BinOp::Eq => IntPredicate::EQ,
                        | ir::BinOp::Ne => IntPredicate::NE,
                        | ir::BinOp::Lt if is_signed => IntPredicate::SLT,
                        | ir::BinOp::Lt => IntPredicate::ULT,
                        | ir::BinOp::Le if is_signed => IntPredicate::SLE,
                        | ir::BinOp::Le => IntPredicate::ULE,
                        | ir::BinOp::Gt if is_signed => IntPredicate::SGT,
                        | ir::BinOp::Gt => IntPredicate::UGT,
                        | ir::BinOp::Ge if is_signed => IntPredicate::SGE,
                        | ir::BinOp::Ge => IntPredicate::UGE,
                        | _ => unreachable!(),
                    };

                    self.builder.build_int_compare(pred, lhs, rhs, "")
                },
                | ir::BinOp::Offset => unreachable!(),
            };

            OperandRef::new_imm(layout, val.as_basic_value_enum())
        }
    }

    pub fn codegen_nullop(&mut self, layout: ReprAndLayout, op: ir::NullOp, repr: Repr) -> OperandRef<'ctx> {
        let repr = repr_and_layout(self.db, self.instance.subst_repr(self.db, repr));
        let ty = self.basic_type_for_ral(&layout).into_int_type();
        let val = match op {
            | ir::NullOp::SizeOf => ty.const_int(repr.size.bytes(), layout.is_signed()),
            | ir::NullOp::AlignOf => ty.const_int(repr.align.bytes(), layout.is_signed()),
            | ir::NullOp::StrideOf => ty.const_int(repr.stride.bytes(), layout.is_signed()),
        };

        OperandRef::new_imm(layout, val.as_basic_value_enum())
    }

    pub fn codegen_operand(&mut self, op: &ir::Operand) -> OperandRef<'ctx> {
        match op {
            | ir::Operand::Move(p) => self.codegen_consume(p),
            | ir::Operand::Copy(p) => {
                self.codegen_copy(p);
                self.codegen_consume(p)
            },
            | ir::Operand::Const(c, r) => self.codegen_const(c, *r),
        }
    }

    pub fn codegen_consume(&mut self, place: &ir::Place) -> OperandRef<'ctx> {
        if let Some(o) = self.maybe_codegen_consume(place) {
            return o;
        }

        let place = self.codegen_place(place);
        place.load_operand(self.cx)
    }

    pub fn maybe_codegen_consume(&mut self, place: &ir::Place) -> Option<OperandRef<'ctx>> {
        match self.locals[place.local.0].clone() {
            | LocalRef::Operand(Some(mut o)) => {
                for proj in place.projection.iter() {
                    match *proj {
                        | ir::Projection::Field(index) => {
                            o = o.field(self.cx, index);
                        },
                        | _ => return None,
                    }
                }

                Some(o)
            },
            | LocalRef::Operand(None) => unreachable!(),
            | LocalRef::Place(_) => None,
        }
    }

    pub fn codegen_copy(&mut self, place: &ir::Place) -> OperandRef<'ctx> {
        if let Some(o) = self.maybe_codegen_consume(place) {
            // assert!(!matches!(o.layout.repr.kind(self.db), ReprKind::Box(_)));
            return o;
        }

        let place = self.codegen_place(place);

        if let ReprKind::Box(_) = place.layout.repr.kind(self.db) {
            let ptr = place.deref(self.cx).ptr;
            // let ptr = place.deref(self.cx).field(self.cx, 0).ptr;
            // let one = self.usize_type().const_int(1, false);
            // let count = self.builder.build_load(self.usize_type(), ptr, "").into_int_value();
            // let count = self.builder.build_int_add(count, one, "");
            // self.builder.build_store(ptr, count);

            let box_copy = if let Some(func) = self.module.get_function("box_copy") {
                func
            } else {
                let arg = self.ptr_type().into();
                let ty = self.context.void_type().fn_type(&[arg], false);
                self.module.add_function("box_copy", ty, None)
            };

            self.builder.build_direct_call(box_copy, &[ptr.into()], "");
        }

        place.load_operand(self.cx)
    }

    pub fn codegen_place(&mut self, place: &ir::Place) -> PlaceRef<'ctx> {
        let mut base = 0;
        let mut res = match &self.locals[place.local.0] {
            | LocalRef::Place(place) => place.clone(),
            | LocalRef::Operand(Some(_)) if matches!(place.projection.get(0), Some(ir::Projection::Deref)) => {
                base = 1;
                self.codegen_consume(&ir::Place::new(place.local)).deref(self.cx)
            },
            | LocalRef::Operand(_) => {
                self.func.print_to_stderr();
                unreachable!();
            },
        };

        for proj in place.projection[base..].iter() {
            res = match proj {
                | ir::Projection::Deref => res.deref(self.cx),
                | ir::Projection::Field(i) => res.field(self.cx, *i),
                | ir::Projection::Downcast(ctor) => res.downcast(self.cx, *ctor),
                | ir::Projection::Index(i) => {
                    let i = self.codegen_operand(i).load(self.cx);
                    res.index(self.cx, i)
                },
                | ir::Projection::Slice(lo, hi) => {
                    let lo = self.codegen_operand(lo).load(self.cx);
                    let hi = self.codegen_operand(hi).load(self.cx);
                    res.slice(self.cx, lo, hi)
                },
            };
        }

        res
    }

    pub fn codegen_const(&mut self, const_: &ir::Const, repr: Repr) -> OperandRef<'ctx> {
        let repr = self.instance.subst_repr(self.db, repr);
        let layout = repr_and_layout(self.db, repr);
        let ty = self.basic_type_for_ral(&layout);
        let value = match *const_ {
            | ir::Const::Undefined => match ty {
                | types::BasicTypeEnum::IntType(t) => t.get_undef().as_basic_value_enum(),
                | types::BasicTypeEnum::FloatType(t) => t.get_undef().as_basic_value_enum(),
                | types::BasicTypeEnum::PointerType(t) => t.get_undef().as_basic_value_enum(),
                | types::BasicTypeEnum::VectorType(t) => t.get_undef().as_basic_value_enum(),
                | types::BasicTypeEnum::ArrayType(t) => t.get_undef().as_basic_value_enum(),
                | types::BasicTypeEnum::StructType(t) => t.get_undef().as_basic_value_enum(),
            },
            | ir::Const::Zeroed => ty.const_zero(),
            | ir::Const::Unit => return OperandRef::new_zst(self.cx, layout),
            | ir::Const::Int(i) => ty
                .into_int_type()
                .const_int(i as u64, layout.is_signed())
                .as_basic_value_enum(),
            | ir::Const::Float(f) => ty
                .into_float_type()
                .const_float(f64::from_bits(f))
                .as_basic_value_enum(),
            | ir::Const::Char(c) => ty.into_int_type().const_int(c as u64, false).as_basic_value_enum(),
            | ir::Const::String(ref s) => return self.codegen_string(s, layout),
            | ir::Const::Instance(i) => self.codegen_instance(i),
            | ir::Const::Ctor(_) if layout.is_zst() => return OperandRef::new_zst(self.cx, layout),
            | ir::Const::Ctor(ctor) => {
                let type_ctor = hir::Ctor::from(ctor).type_ctor(self.db);
                let ctors = type_ctor.ctors(self.db);
                let idx = ctors.iter().position(|c| c.id() == ctor).unwrap();

                if idx == 0 {
                    ty.const_zero()
                } else {
                    ty.into_int_type().const_int(idx as u64, false).as_basic_value_enum()
                }
            },
        };

        OperandRef::new_imm(layout, value)
    }

    pub fn codegen_string(&mut self, string: &str, layout: ReprAndLayout) -> OperandRef<'ctx> {
        let ptr = if let Some(value) = self.strings.get(string) {
            value.clone()
        } else {
            let name = format!("string.{}", self.strings.len());
            let value = self.builder.build_global_string_ptr(string, &name);

            self.strings.insert(String::from(string), value);
            value
        };

        let len = self
            .context
            .ptr_sized_int_type(&self.target_data, None)
            .const_int(string.len() as u64, false)
            .as_basic_value_enum();

        OperandRef::new_pair(layout, ptr.as_basic_value_enum(), len)
    }

    pub fn alloc_string(&mut self, string: &str) -> values::GlobalValue<'ctx> {
        if let Some(value) = self.strings.get(string) {
            value.clone()
        } else {
            let name = format!("string.{}", self.strings.len());
            let value = self.builder.build_global_string_ptr(string, &name);

            self.strings.insert(String::from(string), value);
            value
        }
    }

    pub fn codegen_instance(&mut self, inst: Instance) -> values::BasicValueEnum<'ctx> {
        let inst = self.instance.subst_instance(self.db, inst);

        if inst.is_func(self.db) {
            let (value, _) = self.declare_func(inst);
            value.as_global_value().as_basic_value_enum()
        } else {
            todo!("statics");
        }
    }

    pub fn make_ref(&mut self, op: OperandRef<'ctx>) -> PlaceRef<'ctx> {
        match op.val {
            | OperandValue::Ref(ptr, extra) => PlaceRef::new(op.layout, ptr, extra),
            | _ => {
                let place = PlaceRef::new_alloca(self.cx, op.layout.clone());
                op.store(self.cx, &place);
                place
            },
        }
    }
}

// https://llvm.org/docs/LangRef.html#variable-argument-handling-intrinsics
#[allow(dead_code)]
impl<'ctx> BodyCtx<'_, '_, 'ctx> {
    pub fn build_puts(&mut self, msg: values::PointerValue<'ctx>) {
        let puts = if let Some(func) = self.module.get_function("puts") {
            func
        } else {
            let ptr = self.context.i8_type().ptr_type(Default::default());
            let ty = self.context.i32_type().fn_type(&[ptr.into()], false);
            self.module.add_function("puts", ty, None)
        };

        self.builder.build_call(puts, &[msg.into()], "");
    }

    fn va_list(&self) -> types::StructType<'ctx> {
        if let Some(ty) = self.context.get_struct_type("va_list") {
            return ty;
        }

        let ptr = self.context.i8_type().ptr_type(Default::default()).into();
        let i32 = self.context.i32_type().into();
        let ty = self.context.opaque_struct_type("va_list");

        if self.target.triple.architecture == target_lexicon::Architecture::X86_64 {
            ty.set_body(&[i32, i32, ptr, ptr], false);
        } else {
            ty.set_body(&[ptr], false);
        }

        ty
    }

    fn build_va_start(&self, arglist: values::PointerValue<'ctx>) {
        let llvm_va_start = match self.module.get_function("llvm.va_start") {
            | Some(func) => func,
            | None => {
                let ptr = self.context.i8_type().ptr_type(Default::default());
                let ty = self.context.void_type().fn_type(&[ptr.into()], false);
                self.module.add_function("llvm.va_start", ty, None)
            },
        };

        self.builder.build_call(llvm_va_start, &[arglist.into()], "");
    }

    fn build_va_end(&self, arglist: values::PointerValue<'ctx>) {
        let llvm_va_end = match self.module.get_function("llvm.va_end") {
            | Some(func) => func,
            | None => {
                let ptr = self.context.i8_type().ptr_type(Default::default());
                let ty = self.context.void_type().fn_type(&[ptr.into()], false);
                self.module.add_function("llvm.va_end", ty, None)
            },
        };

        self.builder.build_call(llvm_va_end, &[arglist.into()], "");
    }
}
