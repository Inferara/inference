//TODO: don't forget to remove
#![allow(dead_code)]
use crate::utils;
use inference_ast::nodes::{BlockType, Expression, FunctionDefinition, Literal, Statement, Type};
use inkwell::{
    attributes::{Attribute, AttributeLoc},
    builder::Builder,
    context::Context,
    module::Module,
    values::FunctionValue,
};
use std::rc::Rc;

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
}

impl<'ctx> Compiler<'ctx> {
    pub(crate) fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
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

    pub(crate) fn visit_function_definition(&self, function_definition: &Rc<FunctionDefinition>) {
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
        self.visit_statement(Statement::Block(function_definition.body.clone()));
    }

    fn visit_statement(&self, statement: Statement) {
        match statement {
            Statement::Block(block_type) => match block_type {
                BlockType::Block(block) => {
                    for stmt in block.statements.clone() {
                        self.visit_statement(stmt);
                    }
                }
                BlockType::Forall(forall_block) => {
                    let forall_start = self.forall_start_intrinsic();
                    self.builder
                        .build_call(forall_start, &[], "")
                        .expect("Failed to build forall intrinsic call");
                    for stmt in forall_block.statements.clone() {
                        self.visit_statement(stmt);
                    }
                    let forall_end = self.forall_end_intrinsic();
                    self.builder
                        .build_call(forall_end, &[], "")
                        .expect("Failed to build forall end intrinsic call");
                }
                _ => todo!(),
            },
            Statement::Expression(_expression) => todo!(),
            Statement::Assign(_assign_statement) => todo!(),
            Statement::Return(return_statement) => {
                let ret = match &*return_statement.expression.borrow() {
                    Expression::ArrayIndexAccess(_array_index_access_expression) => todo!(),
                    Expression::Binary(_binary_expression) => todo!(),
                    Expression::MemberAccess(_member_access_expression) => todo!(),
                    Expression::TypeMemberAccess(_type_member_access_expression) => todo!(),
                    Expression::FunctionCall(_function_call_expression) => todo!(),
                    Expression::Struct(_struct_expression) => todo!(),
                    Expression::PrefixUnary(_prefix_unary_expression) => todo!(),
                    Expression::Parenthesized(_parenthesized_expression) => todo!(),
                    Expression::Literal(literal) => match literal {
                        Literal::Array(_array_literal) => todo!(),
                        Literal::Bool(_bool_literal) => todo!(),
                        Literal::String(_string_literal) => todo!(),
                        Literal::Number(number_literal) => self
                            .context
                            .i32_type()
                            .const_int(number_literal.value.parse::<u64>().unwrap_or(0), false),
                        Literal::Unit(_unit_literal) => todo!(),
                    },
                    Expression::Identifier(_identifier) => todo!(),
                    Expression::Type(_) => todo!(),
                    Expression::Uzumaki(_uzumaki_expression) => {
                        let uzumaki_i32_intr = self.uzumaki_i32_intrinsic();
                        let call = self
                            .builder
                            .build_call(uzumaki_i32_intr, &[], "uz_i32")
                            .expect("Failed to build uzumaki_i32_intrinsic call");
                        let call_kind = call.try_as_basic_value();
                        let basic = call_kind.unwrap_basic();
                        basic.into_int_value()
                    }
                };
                self.builder.build_return(Some(&ret)).unwrap();
            }
            Statement::Loop(_loop_statement) => todo!(),
            Statement::Break(_break_statement) => todo!(),
            Statement::If(_if_statement) => todo!(),
            Statement::VariableDefinition(_variable_definition_statement) => todo!(),
            Statement::TypeDefinition(_type_definition_statement) => todo!(),
            Statement::Assert(_assert_statement) => todo!(),
            Statement::ConstantDefinition(_constant_definition) => todo!(),
        }
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
