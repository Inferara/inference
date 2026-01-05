//TODO: don't forget to remove
#![allow(dead_code)]
use crate::utils;
use inference_ast::nodes::{BlockType, Expression, FunctionDefinition, Literal, Statement, Type};
use inference_type_checker::{
    type_info::{NumberTypeKindNumberType, TypeInfoKind},
    typed_context::TypedContext,
};
use inkwell::{
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    module::Module,
    types::BasicTypeEnum,
    values::{FunctionValue, PointerValue},
};
use std::{cell::RefCell, collections::HashMap, iter::Peekable, rc::Rc};

const UZUMAKI_I32_INTRINSIC: &str = "llvm.wasm.uzumaki.i32";
const UZUMAKI_I64_INTRINSIC: &str = "llvm.wasm.uzumaki.i64";
const FORALL_START_INTRINSIC: &str = "llvm.wasm.forall.start";
const FORALL_END_INTRINSIC: &str = "llvm.wasm.forall.end";
const EXISTS_START_INTRINSIC: &str = "llvm.wasm.exists.start";
const EXISTS_END_INTRINSIC: &str = "llvm.wasm.exists.end";
const ASSUME_START_INTRINSIC: &str = "llvm.wasm.assume.start";
const ASSUME_END_INTRINSIC: &str = "llvm.wasm.assume.end";
const UNIQUE_START_INTRINSIC: &str = "llvm.wasm.unique.start";
const UNIQUE_END_INTRINSIC: &str = "llvm.wasm.unique.end";

pub(crate) struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    variables: RefCell<HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>>,
}

impl<'ctx> Compiler<'ctx> {
    pub(crate) fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
            variables: RefCell::new(HashMap::new()),
        }
    }

    fn add_optimization_barriers(&self, function: FunctionValue<'ctx>) {
        let attr_kind_optnone = Attribute::get_named_enum_kind_id("optnone");
        let attr_kind_noinline = Attribute::get_named_enum_kind_id("noinline");

        let optnone = self.context.create_enum_attribute(attr_kind_optnone, 0);
        let noinline = self.context.create_enum_attribute(attr_kind_noinline, 0);

        function.add_attribute(AttributeLoc::Function, optnone);
        function.add_attribute(AttributeLoc::Function, noinline);
    }

    pub(crate) fn visit_function_definition(
        &self,
        function_definition: &Rc<FunctionDefinition>,
        ctx: &TypedContext,
    ) {
        let fn_name = function_definition.name();
        let fn_type = match &function_definition.returns {
            Some(ret_type) => match ret_type {
                Type::Array(_array_type) => todo!(),
                Type::Simple(simple_type) => match simple_type.name.to_lowercase().as_str() {
                    "i32" => self.context.i32_type().fn_type(&[], false),
                    "i64" => self.context.i64_type().fn_type(&[], false),
                    "u32" => todo!(),
                    "u64" => todo!(),
                    _ => panic!("Unsupported return type: {}", simple_type.name),
                },
                Type::Generic(_generic_type) => todo!(),
                Type::Function(_function_type) => todo!(),
                Type::QualifiedName(_qualified_name) => todo!(),
                Type::Qualified(_type_qualified_name) => todo!(),
                Type::Custom(_identifier) => todo!(),
            },
            None => self.context.void_type().fn_type(&[], false),
        };
        let function = self.module.add_function(fn_name.as_str(), fn_type, None);

        let export_name_attr = self
            .context
            .create_string_attribute("wasm-export-name", fn_name.as_str());
        function.add_attribute(AttributeLoc::Function, export_name_attr);
        if function_definition.is_non_det() {
            self.add_optimization_barriers(function);
        }
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.lower_statement(
            std::iter::once(Statement::Block(function_definition.body.clone())).peekable(),
            &mut vec![function_definition.body.clone()],
            ctx,
        );
        if function_definition.is_void() {
            self.builder.build_return(None).unwrap();
        }
    }

    #[allow(clippy::too_many_lines)]
    fn lower_statement<I: Iterator<Item = Statement>>(
        &self,
        mut statements_iterator: Peekable<I>,
        parent_blocks_stack: &mut Vec<BlockType>,
        ctx: &TypedContext,
    ) {
        let statement = statements_iterator.next().unwrap();
        match statement {
            Statement::Block(block_type) => match block_type {
                BlockType::Block(block) => {
                    parent_blocks_stack.push(BlockType::Block(block.clone()));
                    for stmt in block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                        );
                    }
                    parent_blocks_stack.pop();
                }
                BlockType::Forall(forall_block) => {
                    let forall_start = self.forall_start_intrinsic();
                    self.builder
                        .build_call(forall_start, &[], "")
                        .expect("Failed to build forall intrinsic call");
                    parent_blocks_stack.push(BlockType::Forall(forall_block.clone()));
                    for stmt in forall_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                        );
                    }
                    let forall_end = self.forall_end_intrinsic();
                    self.builder
                        .build_call(forall_end, &[], "")
                        .expect("Failed to build forall end intrinsic call");
                    parent_blocks_stack.pop();
                }
                BlockType::Assume(assume_block) => {
                    let assume_start = self.assume_start_intrinsic();
                    self.builder
                        .build_call(assume_start, &[], "")
                        .expect("Failed to build assume intrinsic call");
                    parent_blocks_stack.push(BlockType::Assume(assume_block.clone()));
                    for stmt in assume_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                        );
                    }
                    let assume_end = self.assume_end_intrinsic();
                    self.builder
                        .build_call(assume_end, &[], "")
                        .expect("Failed to build assume end intrinsic call");
                    parent_blocks_stack.pop();
                }
                BlockType::Exists(exists_block) => {
                    let exists_start = self.exists_start_intrinsic();
                    self.builder
                        .build_call(exists_start, &[], "")
                        .expect("Failed to build exists intrinsic call");
                    parent_blocks_stack.push(BlockType::Exists(exists_block.clone()));
                    for stmt in exists_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                        );
                    }
                    let exists_end = self.exists_end_intrinsic();
                    self.builder
                        .build_call(exists_end, &[], "")
                        .expect("Failed to build exists end intrinsic call");
                    parent_blocks_stack.pop();
                }
                BlockType::Unique(unique_block) => {
                    let unique_start = self.unique_start_intrinsic();
                    self.builder
                        .build_call(unique_start, &[], "")
                        .expect("Failed to build unique intrinsic call");
                    parent_blocks_stack.push(BlockType::Unique(unique_block.clone()));
                    for stmt in unique_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                        );
                    }
                    let unique_end = self.unique_end_intrinsic();
                    self.builder
                        .build_call(unique_end, &[], "")
                        .expect("Failed to build unique end intrinsic call");
                    parent_blocks_stack.pop();
                }
            },
            Statement::Expression(expression) => {
                let expr = self.lower_expression(&expression, ctx);
                //FIXME: revisit this logic #45
                if statements_iterator.peek().is_none()
                    && parent_blocks_stack.first().unwrap().is_non_det()
                    && parent_blocks_stack.first().unwrap().is_void()
                {
                    let local = self.builder.build_alloca(expr.get_type(), "temp").unwrap();
                    self.builder.build_store(local, expr).unwrap();
                }
            }
            Statement::Assign(_assign_statement) => todo!(),
            Statement::Return(return_statement) => {
                let ret = self.lower_expression(&return_statement.expression.borrow(), ctx);
                self.builder.build_return(Some(&ret)).unwrap();
            }
            Statement::Loop(_loop_statement) => todo!(),
            Statement::Break(_break_statement) => todo!(),
            Statement::If(_if_statement) => todo!(),
            Statement::VariableDefinition(_variable_definition_statement) => {
                // let ctx_type = self.context.i32_type(); //TODO: support other types
                // if let Some(value) = &variable_definition_statement.value {
                //     if matches!(*value.borrow(), Expression::Uzumaki(_))
                //         || matches!(*value.borrow(), Expression::Literal(_))
                //     {
                //     } else {
                //         todo!()
                //     }
                // }
            }
            Statement::TypeDefinition(_type_definition_statement) => todo!(),
            Statement::Assert(_assert_statement) => todo!(),
            Statement::ConstantDefinition(constant_definition) => match &constant_definition.ty {
                Type::Array(_type_array) => todo!(),
                Type::Simple(simple_type) => {
                    match ctx
                        .get_node_typeinfo(simple_type.id)
                        .expect("Constant definition must have a type info")
                        .kind
                    {
                        TypeInfoKind::Unit => todo!(),
                        TypeInfoKind::Bool => todo!(),
                        TypeInfoKind::String => todo!(),
                        TypeInfoKind::Number(number_type_kind_number_type) => {
                            match number_type_kind_number_type {
                                NumberTypeKindNumberType::I8 => todo!(),
                                NumberTypeKindNumberType::I16 => todo!(),
                                NumberTypeKindNumberType::I32 => {
                                    let ctx_type = self.context.i32_type();
                                    match &constant_definition.value {
                                        Literal::Number(number_literal) => {
                                            let val = ctx_type.const_int(
                                                number_literal.value.parse::<u64>().unwrap_or(0),
                                                false,
                                            );
                                            let local = self
                                                .builder
                                                .build_alloca(ctx_type, &constant_definition.name())
                                                .unwrap();
                                            self.builder.build_store(local, val).unwrap();
                                            self.variables.borrow_mut().insert(
                                                constant_definition.name(),
                                                (local, ctx_type.into()),
                                            );
                                        }
                                        _ => panic!(
                                            "Constant value for i32 should be a number literal. Found: {:?}",
                                            constant_definition.value
                                        ),
                                    }
                                }
                                NumberTypeKindNumberType::I64 => todo!(),
                                NumberTypeKindNumberType::U8 => todo!(),
                                NumberTypeKindNumberType::U16 => todo!(),
                                NumberTypeKindNumberType::U32 => todo!(),
                                NumberTypeKindNumberType::U64 => todo!(),
                            }
                        }
                        TypeInfoKind::Custom(_) => todo!(),
                        TypeInfoKind::Array(_type_info, _) => todo!(),
                        TypeInfoKind::Generic(_) => todo!(),
                        TypeInfoKind::QualifiedName(_) => todo!(),
                        TypeInfoKind::Qualified(_) => todo!(),
                        TypeInfoKind::Function(_) => todo!(),
                        TypeInfoKind::Struct(_) => todo!(),
                        TypeInfoKind::Enum(_) => todo!(),
                        TypeInfoKind::Spec(_) => todo!(),
                    }
                }
                Type::Generic(_generic_type) => todo!(),
                Type::Function(_function_type) => todo!(),
                Type::QualifiedName(_qualified_name) => todo!(),
                Type::Qualified(_type_qualified_name) => todo!(),
                Type::Custom(_identifier) => todo!(),
            },
        }
    }

    fn lower_expression(
        &self,
        expression: &Expression,
        ctx: &TypedContext,
    ) -> inkwell::values::IntValue<'ctx> {
        match expression {
            Expression::ArrayIndexAccess(_array_index_access_expression) => todo!(),
            Expression::Binary(_binary_expression) => todo!(),
            Expression::MemberAccess(_member_access_expression) => todo!(),
            Expression::TypeMemberAccess(_type_member_access_expression) => todo!(),
            Expression::FunctionCall(_function_call_expression) => todo!(),
            Expression::Struct(_struct_expression) => todo!(),
            Expression::PrefixUnary(_prefix_unary_expression) => todo!(),
            Expression::Parenthesized(_parenthesized_expression) => todo!(),
            Expression::Literal(literal) => self.lower_literal(literal),
            Expression::Identifier(identifier) => {
                let (ptr, ty) = self
                    .variables
                    .borrow()
                    .get(&identifier.name)
                    .copied()
                    .expect("Variable not found");
                self.builder
                    .build_load(ty, ptr, &identifier.name)
                    .unwrap()
                    .into_int_value()
            }
            Expression::Type(_) => todo!(),
            Expression::Uzumaki(uzumaki_expression) => {
                if ctx.is_node_i32(uzumaki_expression.id) {
                    return self.lower_uzumaki_i32_expression();
                }
                if ctx.is_node_i64(uzumaki_expression.id) {
                    return self.lower_uzumaki_i64_expression();
                }
                panic!("Unsupported Uzumaki expression type: {uzumaki_expression:?}");
            }
        }
    }

    fn lower_literal(&self, literal: &Literal) -> inkwell::values::IntValue<'ctx> {
        match literal {
            Literal::Array(_array_literal) => todo!(),
            Literal::Bool(bool_literal) => self
                .context
                .i32_type()
                .const_int(u64::from(bool_literal.value), false),
            Literal::String(_string_literal) => todo!(),
            Literal::Number(number_literal) => self
                .context
                .i32_type()
                .const_int(number_literal.value.parse::<u64>().unwrap_or(0), false),
            Literal::Unit(_unit_literal) => todo!(),
        }
    }

    fn lower_uzumaki_i32_expression(&self) -> inkwell::values::IntValue<'ctx> {
        let uzumaki_i32_intr = self.uzumaki_i32_intrinsic();
        let call = self
            .builder
            .build_call(uzumaki_i32_intr, &[], "uz_i32")
            .expect("Failed to build uzumaki_i32_intrinsic call");
        let call_kind = call.try_as_basic_value();
        let basic = call_kind.unwrap_basic();
        basic.into_int_value()
    }

    fn lower_uzumaki_i64_expression(&self) -> inkwell::values::IntValue<'ctx> {
        let uzumaki_i64_intr = self.uzumaki_i64_intrinsic();
        let call = self
            .builder
            .build_call(uzumaki_i64_intr, &[], "uz_i64")
            .expect("Failed to build uzumaki_i64_intrinsic call");
        let call_kind = call.try_as_basic_value();
        let basic = call_kind.unwrap_basic();
        basic.into_int_value()
    }

    fn uzumaki_i32_intrinsic(&self) -> FunctionValue<'ctx> {
        let i32_type = self.context.i32_type();
        let fn_type = i32_type.fn_type(&[], false);
        self.module
            .get_function(UZUMAKI_I32_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UZUMAKI_I32_INTRINSIC, fn_type, None)
            })
    }

    fn uzumaki_i64_intrinsic(&self) -> FunctionValue<'ctx> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        self.module
            .get_function(UZUMAKI_I64_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UZUMAKI_I64_INTRINSIC, fn_type, None)
            })
    }

    fn forall_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(FORALL_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(FORALL_START_INTRINSIC, fn_type, None)
            })
    }

    fn forall_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(FORALL_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(FORALL_END_INTRINSIC, fn_type, None)
            })
    }

    fn exists_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(EXISTS_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(EXISTS_START_INTRINSIC, fn_type, None)
            })
    }

    fn exists_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(EXISTS_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(EXISTS_END_INTRINSIC, fn_type, None)
            })
    }

    fn assume_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(ASSUME_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(ASSUME_START_INTRINSIC, fn_type, None)
            })
    }

    fn assume_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(ASSUME_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(ASSUME_END_INTRINSIC, fn_type, None)
            })
    }

    fn unique_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(UNIQUE_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UNIQUE_START_INTRINSIC, fn_type, None)
            })
    }

    fn unique_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(UNIQUE_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UNIQUE_END_INTRINSIC, fn_type, None)
            })
    }

    pub(crate) fn compile_to_wasm(
        &self,
        output_fname: &str,
        optimization_level: u32,
    ) -> anyhow::Result<Vec<u8>> {
        utils::compile_to_wasm(&self.module, output_fname, optimization_level)
    }
}
