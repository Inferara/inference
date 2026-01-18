//! LLVM-based WebAssembly code generation.
//!
//! This module implements the core compiler that translates Inference's typed AST into
//! WebAssembly bytecode via LLVM IR. It handles standard WASM instructions as well as
//! custom intrinsics for non-deterministic operations (uzumaki, forall, exists, assume, unique).
//!
//! # Architecture
//!
//! The compiler operates in several stages:
//!
//! 1. **Function lowering** - Convert AST function definitions to LLVM function declarations
//! 2. **Statement lowering** - Translate control flow and non-deterministic blocks
//! 3. **Expression lowering** - Generate LLVM IR for expressions and literals
//! 4. **Intrinsic injection** - Insert LLVM intrinsic calls for non-deterministic operations
//! 5. **WASM emission** - Compile LLVM IR to WebAssembly object files via inf-llc
//! 6. **Linking** - Link object files into final WASM module via rust-lld
//!
//! # Type Mapping
//!
//! Inference types are mapped to LLVM types and ultimately to WebAssembly types:
//!
//! | Inference Type | LLVM Type | WASM Type |
//! |----------------|-----------|-----------|
//! | `unit`         | void      | -         |
//! | `bool`         | i1        | i32       |
//! | `i8`, `u8`     | i8        | i32       |
//! | `i16`, `u16`   | i16       | i32       |
//! | `i32`, `u32`   | i32       | i32       |
//! | `i64`, `u64`   | i64       | i64       |
//!
//! Note: WebAssembly only supports i32, i64, f32, and f64 as value types. Smaller integer
//! types use i32 with appropriate truncation/extension.
//!
//! # Non-Deterministic Operations
//!
//! The compiler emits LLVM intrinsic calls for non-deterministic operations. These intrinsics
//! are recognized by inf-llc and compiled to custom WASM instructions with binary encoding
//! in the 0xfc prefix space:
//!
//! - `uzumaki()` - Non-deterministic value generation (0xfc 0x3a for i32, 0xfc 0x3c for i64)
//! - `forall { ... }` - Universal quantification block (0xfc 0x3a start, 0xfc 0x3b end)
//! - `exists { ... }` - Existential quantification block (0xfc 0x3c start, 0xfc 0x3d end)
//! - `assume { ... }` - Assumption block for preconditions (0xfc 0x3e start, 0xfc 0x3f end)
//! - `unique { ... }` - Uniqueness constraint block (0xfc 0x40 start, 0xfc 0x41 end)
//!
//! See the [language spec](https://github.com/Inferara/inference-language-spec) for details
//! on non-deterministic semantics.
//!
//! # Optimization Barriers
//!
//! Functions containing non-deterministic blocks receive `optnone` and `noinline` attributes
//! to prevent LLVM from optimizing away the intrinsic calls, which would break formal
//! verification guarantees.

//TODO: don't forget to remove
#![allow(dead_code)]
use crate::utils;
use inference_ast::nodes::{
    BlockType, Expression, FunctionDefinition, Literal, SimpleTypeKind, Statement, Type,
};
use inference_type_checker::{
    type_info::{NumberType, TypeInfoKind},
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

/// LLVM intrinsic for non-deterministic i32 value generation.
/// Compiles to WASM instruction 0xfc 0x3a.
const UZUMAKI_I32_INTRINSIC: &str = "llvm.wasm.uzumaki.i32";

/// LLVM intrinsic for non-deterministic i64 value generation.
/// Compiles to WASM instruction 0xfc 0x3c.
const UZUMAKI_I64_INTRINSIC: &str = "llvm.wasm.uzumaki.i64";

/// LLVM intrinsic marking the start of a forall (universal quantification) block.
/// Compiles to WASM instruction 0xfc 0x3a.
const FORALL_START_INTRINSIC: &str = "llvm.wasm.forall.start";

/// LLVM intrinsic marking the end of a forall block.
/// Compiles to WASM instruction 0xfc 0x3b.
const FORALL_END_INTRINSIC: &str = "llvm.wasm.forall.end";

/// LLVM intrinsic marking the start of an exists (existential quantification) block.
/// Compiles to WASM instruction 0xfc 0x3c.
const EXISTS_START_INTRINSIC: &str = "llvm.wasm.exists.start";

/// LLVM intrinsic marking the end of an exists block.
/// Compiles to WASM instruction 0xfc 0x3d.
const EXISTS_END_INTRINSIC: &str = "llvm.wasm.exists.end";

/// LLVM intrinsic marking the start of an assume (precondition) block.
/// Compiles to WASM instruction 0xfc 0x3e.
const ASSUME_START_INTRINSIC: &str = "llvm.wasm.assume.start";

/// LLVM intrinsic marking the end of an assume block.
/// Compiles to WASM instruction 0xfc 0x3f.
const ASSUME_END_INTRINSIC: &str = "llvm.wasm.assume.end";

/// LLVM intrinsic marking the start of a unique (uniqueness constraint) block.
/// Compiles to WASM instruction 0xfc 0x40.
const UNIQUE_START_INTRINSIC: &str = "llvm.wasm.unique.start";

/// LLVM intrinsic marking the end of a unique block.
/// Compiles to WASM instruction 0xfc 0x41.
const UNIQUE_END_INTRINSIC: &str = "llvm.wasm.unique.end";

/// LLVM-based compiler for generating WebAssembly bytecode from typed AST.
///
/// The compiler maintains LLVM context, module, and builder state throughout the
/// compilation process. It uses Inkwell (Rust bindings for LLVM) to generate LLVM IR,
/// which is then compiled to WebAssembly via external tools (inf-llc and rust-lld).
///
/// # Lifetime
///
/// The `'ctx` lifetime ties the compiler to the LLVM context. All LLVM values and types
/// created during compilation share this lifetime.
///
/// # Variable Storage
///
/// Local variables and constants are stored in a `RefCell<HashMap>` mapping names to
/// (pointer, type) pairs. This allows mutation during IR generation while maintaining
/// Rust's borrowing rules through interior mutability.
pub(crate) struct Compiler<'ctx> {
    /// LLVM context for creating types and values.
    context: &'ctx Context,

    /// LLVM module containing all generated functions and globals.
    module: Module<'ctx>,

    /// LLVM instruction builder for emitting IR.
    builder: Builder<'ctx>,

    /// Variable storage mapping names to stack-allocated pointers and their types.
    variables: RefCell<HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>>,
}

impl<'ctx> Compiler<'ctx> {
    /// Creates a new compiler instance with an empty LLVM module.
    ///
    /// # Parameters
    ///
    /// - `context` - LLVM context for creating types and values
    /// - `module_name` - Name for the generated LLVM module (typically `wasm_module`)
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

    /// Adds optimization barriers to a function to prevent LLVM from optimizing away
    /// non-deterministic intrinsic calls.
    ///
    /// This applies the `optnone` and `noinline` attributes, which are critical for
    /// preserving the semantics of non-deterministic blocks. Without these barriers,
    /// LLVM might inline or eliminate intrinsic calls, breaking formal verification.
    ///
    /// # Parameters
    ///
    /// - `function` - LLVM function value to annotate with barrier attributes
    fn add_optimization_barriers(&self, function: FunctionValue<'ctx>) {
        let attr_kind_optnone = Attribute::get_named_enum_kind_id("optnone");
        let attr_kind_noinline = Attribute::get_named_enum_kind_id("noinline");

        let optnone = self.context.create_enum_attribute(attr_kind_optnone, 0);
        let noinline = self.context.create_enum_attribute(attr_kind_noinline, 0);

        function.add_attribute(AttributeLoc::Function, optnone);
        function.add_attribute(AttributeLoc::Function, noinline);
    }

    /// Translates an AST function definition to LLVM IR.
    ///
    /// This is the main entry point for function compilation. It performs several steps:
    ///
    /// 1. **Type mapping** - Maps the return type from `Type::Simple(SimpleTypeKind)` to
    ///    corresponding LLVM types (void, bool, i8, i16, i32, i64)
    /// 2. **Function creation** - Declares the function in the LLVM module with the
    ///    appropriate signature
    /// 3. **Export annotation** - Adds `wasm-export-name` attribute to make the function
    ///    accessible from WebAssembly
    /// 4. **Optimization barriers** - If the function contains non-deterministic blocks,
    ///    applies `optnone` and `noinline` attributes to prevent optimization
    /// 5. **Body lowering** - Recursively lowers the function body statements to LLVM IR
    /// 6. **Return handling** - Inserts implicit void return for functions without explicit
    ///    return statements
    ///
    /// # Type Safety Improvement
    ///
    /// Prior to the introduction of `SimpleTypeKind`, this method used string-based type
    /// matching (`simple_type.name.to_lowercase().as_str()`), which was error-prone and
    /// lacked compile-time verification. The refactoring to use `SimpleTypeKind` enum
    /// provides type-safe pattern matching and ensures all primitive types are explicitly
    /// handled.
    ///
    /// # Parameters
    ///
    /// - `function_definition` - AST node representing the function to compile
    /// - `ctx` - Typed context containing type information for all AST nodes
    ///
    /// # Panics
    ///
    /// This method will panic if it encounters unsupported type constructs (arrays,
    /// generics, function types, qualified names, custom types) in return positions,
    /// as these are not yet implemented. The `todo!()` markers indicate planned future
    /// support.
    pub(crate) fn visit_function_definition(
        &self,
        function_definition: &Rc<FunctionDefinition>,
        ctx: &TypedContext,
    ) {
        let fn_name = function_definition.name();
        let fn_type = match &function_definition.returns {
            Some(ret_type) => match ret_type {
                Type::Simple(SimpleTypeKind::Unit) => self.context.void_type().fn_type(&[], false),
                Type::Simple(SimpleTypeKind::Bool) => self.context.bool_type().fn_type(&[], false),
                Type::Simple(SimpleTypeKind::I8 | SimpleTypeKind::U8) => {
                    self.context.i8_type().fn_type(&[], false)
                }
                Type::Simple(SimpleTypeKind::I16 | SimpleTypeKind::U16) => {
                    self.context.i16_type().fn_type(&[], false)
                }
                Type::Simple(SimpleTypeKind::I32 | SimpleTypeKind::U32) => {
                    self.context.i32_type().fn_type(&[], false)
                }
                Type::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => {
                    self.context.i64_type().fn_type(&[], false)
                }
                Type::Array(_array_type) => todo!(),
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

    /// Recursively lowers AST statements to LLVM IR instructions.
    ///
    /// This method handles all statement types including control flow, blocks, and
    /// non-deterministic constructs. It maintains a stack of parent blocks to track
    /// nesting context, which is needed for semantic analysis and code generation.
    ///
    /// # Statement Types
    ///
    /// - **Block types** (regular, forall, exists, assume, unique) - Recursively lower
    ///   nested statements with appropriate intrinsic calls
    /// - **Expression statements** - Evaluate expressions and handle side effects
    /// - **Return statements** - Generate LLVM return instructions
    /// - **Constant definitions** - Allocate stack storage and initialize values
    ///
    /// # Non-Deterministic Blocks
    ///
    /// For non-deterministic block types (forall, exists, assume, unique), this method:
    /// 1. Emits the start intrinsic call
    /// 2. Recursively lowers nested statements
    /// 3. Emits the end intrinsic call
    ///
    /// The intrinsic pairs ensure proper scoping in the generated WASM and are preserved
    /// by optimization barriers.
    ///
    /// # Parameters
    ///
    /// - `statements_iterator` - Iterator over statements to lower
    /// - `parent_blocks_stack` - Stack tracking enclosing block contexts
    /// - `ctx` - Typed context for type information lookup
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
            Statement::ConstantDefinition(constant_definition) => {
                match ctx
                    .get_node_typeinfo(constant_definition.id)
                    .expect("Constant definition must have a type info")
                    .kind
                {
                    TypeInfoKind::Unit => todo!(),
                    TypeInfoKind::Bool => todo!(),
                    TypeInfoKind::String => todo!(),
                    TypeInfoKind::Number(number_type_kind_number_type) => {
                        match number_type_kind_number_type {
                            NumberType::I8 => todo!(),
                            NumberType::I16 => todo!(),
                            NumberType::I32 => {
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
                            NumberType::I64 => todo!(),
                            NumberType::U8 => todo!(),
                            NumberType::U16 => todo!(),
                            NumberType::U32 => todo!(),
                            NumberType::U64 => todo!(),
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
        }
    }

    /// Lowers an AST expression to an LLVM integer value.
    ///
    /// This method recursively evaluates expressions and produces LLVM IR that computes
    /// the expression's value at runtime. Currently, the compiler only supports integer
    /// expressions, hence the return type is `IntValue`.
    ///
    /// # Supported Expressions
    ///
    /// - **Literals** - Compile-time constants (numbers, booleans)
    /// - **Identifiers** - Load values from local variables
    /// - **Uzumaki** - Non-deterministic value generation via intrinsics
    ///
    /// # Type Context
    ///
    /// The `TypedContext` is used to query type information for expressions, particularly
    /// for uzumaki expressions which can produce different integer types (i32, i64).
    ///
    /// # Parameters
    ///
    /// - `expression` - AST expression node to lower
    /// - `ctx` - Typed context for type lookups
    ///
    /// # Returns
    ///
    /// LLVM integer value representing the expression result
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

    /// Converts an AST literal to an LLVM constant integer value.
    ///
    /// Literals are compile-time constants that get embedded directly into the LLVM IR
    /// as constant integers. This method handles the conversion from Inference's literal
    /// representation to LLVM's constant values.
    ///
    /// # Literal Types
    ///
    /// - **Bool** - Converted to i32 (0 for false, 1 for true) per WASM convention
    /// - **Number** - Parsed from string and converted to i32 constant
    ///
    /// # Parameters
    ///
    /// - `literal` - AST literal node to convert
    ///
    /// # Returns
    ///
    /// LLVM constant integer value
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

    /// Generates LLVM IR for a 32-bit non-deterministic value (uzumaki expression).
    ///
    /// Emits a call to the `llvm.wasm.uzumaki.i32` intrinsic, which compiles to the
    /// custom WASM instruction 0xfc 0x3a. This instruction produces a non-deterministic
    /// i32 value at runtime.
    ///
    /// # Returns
    ///
    /// LLVM integer value (i32) representing the non-deterministic result
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

    /// Generates LLVM IR for a 64-bit non-deterministic value (uzumaki expression).
    ///
    /// Emits a call to the `llvm.wasm.uzumaki.i64` intrinsic, which compiles to the
    /// custom WASM instruction 0xfc 0x3c. This instruction produces a non-deterministic
    /// i64 value at runtime.
    ///
    /// # Returns
    ///
    /// LLVM integer value (i64) representing the non-deterministic result
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

    /// Retrieves or declares the i32 uzumaki intrinsic function.
    ///
    /// This method ensures the intrinsic function is declared in the LLVM module.
    /// If already declared, returns the existing function; otherwise, declares it
    /// with the signature `() -> i32`.
    ///
    /// # Returns
    ///
    /// LLVM function value for the uzumaki.i32 intrinsic
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

    /// Retrieves or declares the i64 uzumaki intrinsic function.
    ///
    /// This method ensures the intrinsic function is declared in the LLVM module.
    /// If already declared, returns the existing function; otherwise, declares it
    /// with the signature `() -> i64`.
    ///
    /// # Returns
    ///
    /// LLVM function value for the uzumaki.i64 intrinsic
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

    /// Retrieves or declares the forall start intrinsic function.
    ///
    /// This void-returning intrinsic marks the beginning of a universal quantification block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the forall.start intrinsic
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

    /// Retrieves or declares the forall end intrinsic function.
    ///
    /// This void-returning intrinsic marks the end of a universal quantification block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the forall.end intrinsic
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

    /// Retrieves or declares the exists start intrinsic function.
    ///
    /// This void-returning intrinsic marks the beginning of an existential quantification block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the exists.start intrinsic
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

    /// Retrieves or declares the exists end intrinsic function.
    ///
    /// This void-returning intrinsic marks the end of an existential quantification block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the exists.end intrinsic
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

    /// Retrieves or declares the assume start intrinsic function.
    ///
    /// This void-returning intrinsic marks the beginning of an assumption block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the assume.start intrinsic
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

    /// Retrieves or declares the assume end intrinsic function.
    ///
    /// This void-returning intrinsic marks the end of an assumption block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the assume.end intrinsic
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

    /// Retrieves or declares the unique start intrinsic function.
    ///
    /// This void-returning intrinsic marks the beginning of a uniqueness constraint block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the unique.start intrinsic
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

    /// Retrieves or declares the unique end intrinsic function.
    ///
    /// This void-returning intrinsic marks the end of a uniqueness constraint block.
    ///
    /// # Returns
    ///
    /// LLVM function value for the unique.end intrinsic
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

    /// Compiles the LLVM module to WebAssembly bytecode.
    ///
    /// This method orchestrates the final compilation stages:
    /// 1. Emit LLVM IR to a temporary file
    /// 2. Invoke inf-llc to compile IR to WASM object file
    /// 3. Invoke rust-lld to link object file into final WASM module
    /// 4. Read the resulting WASM bytes
    ///
    /// The actual compilation work is delegated to the `utils::compile_to_wasm` function,
    /// which handles toolchain invocation and temporary file management.
    ///
    /// # Parameters
    ///
    /// - `output_fname` - Base filename for intermediate files (extension will be added)
    /// - `optimization_level` - LLVM optimization level (0-3, higher is more optimized)
    ///
    /// # Returns
    ///
    /// WebAssembly bytecode as a byte vector
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - inf-llc or rust-lld executables are not found
    /// - Compilation or linking fails
    /// - File I/O operations fail
    pub(crate) fn compile_to_wasm(
        &self,
        output_fname: &str,
        optimization_level: u32,
    ) -> anyhow::Result<Vec<u8>> {
        utils::compile_to_wasm(&self.module, output_fname, optimization_level)
    }
}
