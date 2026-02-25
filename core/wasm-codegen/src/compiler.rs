//! WebAssembly code generation via wasm-encoder.
//!
//! This module implements the core compiler that translates Inference's typed AST
//! into WebAssembly binary format using `wasm-encoder`. It handles standard
//! WASM instructions as well as custom non-deterministic operations (uzumaki, forall,
//! exists, assume, unique).
//!
//! # Prerequisites
//!
//! Before reading this documentation, you should be familiar with:
//! - WebAssembly binary format and module structure
//! - WebAssembly execution model (stack machine, locals, structured control flow)
//! - Inference language syntax and semantics (see language specification)
//! - The concept of non-deterministic computation in formal verification
//!
//! # Architecture
//!
//! The compiler operates in several stages:
//!
//! 1. **Function lowering** - Convert AST function definitions to WASM function bodies;
//!    emit `unreachable` before function `end` for non-void functions so that WASM
//!    validators accept functions whose all paths exit via explicit `return`
//! 2. **Statement lowering** - Translate control flow (if/else), non-deterministic blocks,
//!    variable and constant definitions, return, and expression statements
//! 3. **Expression lowering** - Generate WASM instructions for expressions and literals
//! 4. **Non-det emission** - Emit custom 0xfc-prefixed instructions for non-deterministic ops
//! 5. **Module assembly** - Combine all sections into a valid WASM binary
//!
//! # Type Mapping
//!
//! Inference types are mapped to WebAssembly types:
//!
//! | Inference Type | WASM Type |
//! |----------------|-----------|
//! | `unit`         | (none)    |
//! | `bool`         | i32       |
//! | `i8`, `u8`     | i32       |
//! | `i16`, `u16`   | i32       |
//! | `i32`, `u32`   | i32       |
//! | `i64`, `u64`   | i64       |
//!
//! Note: WebAssembly only supports i32, i64, f32, and f64 as value types. Smaller integer
//! types use i32 with appropriate truncation/extension.
//!
//! # Non-Deterministic Operations
//!
//! The compiler emits custom WASM instructions with binary encoding in the 0xfc prefix
//! space. Ground truth opcodes from `tools/inf-wasmparser/src/binary_reader.rs`:
//!
//! - `i32.uzumaki` - 0xfc 0x31 (standalone)
//! - `i64.uzumaki` - 0xfc 0x32 (standalone)
//! - `forall { ... }` - 0xfc 0x3a + blocktype(0x40) + body + end(0x0b)
//! - `exists { ... }` - 0xfc 0x3b + blocktype(0x40) + body + end(0x0b)
//! - `assume { ... }` - 0xfc 0x3c + blocktype(0x40) + body + end(0x0b)
//! - `unique { ... }` - 0xfc 0x3d + blocktype(0x40) + body + end(0x0b)
//!
//! Non-det blocks are structured blocks (like `block`/`loop`/`if`), terminated by a
//! regular `end` instruction (0x0b).

use crate::errors::CodegenError;
use rustc_hash::FxHashMap;
use std::iter::Peekable;
use std::rc::Rc;

use inference_ast::nodes::{
    ArgumentType, AssignStatement, BinaryExpression, BlockType, Expression, FunctionDefinition,
    Literal, OperatorKind, PrefixUnaryExpression, SimpleTypeKind, Statement, Type,
    UnaryOperatorKind, Visibility,
};
use inference_type_checker::{
    type_info::{NumberType, TypeInfoKind},
    typed_context::TypedContext,
};
use wasm_encoder::{
    BlockType as WasmBlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection,
    IndirectNameMap, Instruction, Module, NameMap, NameSection, TypeSection, ValType,
};

// Custom opcode constants for non-deterministic operations.
// Ground truth: tools/inf-wasmparser/src/binary_reader.rs lines 1372-1388.
const OPCODE_PREFIX: u8 = 0xfc;
const UZUMAKI_I32_OPCODE: u8 = 0x31;
const UZUMAKI_I64_OPCODE: u8 = 0x32;
const FORALL_OPCODE: u8 = 0x3a;
const EXISTS_OPCODE: u8 = 0x3b;
const ASSUME_OPCODE: u8 = 0x3c;
const UNIQUE_OPCODE: u8 = 0x3d;
const BLOCK_TYPE_VOID: u8 = 0x40;
const END_OPCODE: u8 = 0x0b;

/// WASM compiler for generating WebAssembly binary from typed AST.
///
/// The compiler builds a complete WASM module in-process using `wasm-encoder`. Each function
/// definition from the AST is compiled into a WASM function body with proper
/// type signatures, exports, and debug names.
///
/// # Variable Storage
///
/// Local variables, constants, and function parameters are stored as WASM locals mapped by name
/// to (`local_index`, `ValType`) pairs. Parameters occupy indices `0..param_count`; regular
/// locals follow at `param_count`.. Function bodies pre-scan for local declarations before
/// emitting instructions, since WASM requires all locals to be declared at the start of a
/// function body.
///
/// # Internal Usage Example
///
/// ```ignore
/// let mut compiler = Compiler::new("output");
///
/// for func_def in typed_context.source_files()[0].function_definitions() {
///     compiler.visit_function_definition(&func_def, &typed_context);
/// }
///
/// let wasm_bytes = compiler.finish();
/// ```
pub(crate) struct Compiler {
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
    functions: Vec<u32>,
    exports: Vec<(String, ExportKind, u32)>,
    bodies: Vec<Function>,
    func_names: Vec<(u32, String)>,
    local_names: Vec<(u32, Vec<(u32, String)>)>,
    func_idx: u32,
    has_main: bool,
    module_name: String,
    /// Maps function names to their WASM function section indices.
    ///
    /// Built by `build_func_name_to_idx` before the main compilation pass so that
    /// forward references (callee defined after caller) resolve correctly.
    func_name_to_idx: FxHashMap<String, u32>,
}

impl Compiler {
    /// Creates a new compiler instance for building a WASM module.
    ///
    /// # Parameters
    ///
    /// - `module_name` - Name for the generated WASM module (used in the name section)
    pub(crate) fn new(module_name: &str) -> Self {
        Self {
            types: Vec::new(),
            functions: Vec::new(),
            exports: Vec::new(),
            bodies: Vec::new(),
            func_names: Vec::new(),
            local_names: Vec::new(),
            func_idx: 0,
            has_main: false,
            module_name: module_name.to_string(),
            func_name_to_idx: FxHashMap::default(),
        }
    }

    /// Builds the function name-to-WASM-index map from the source file's function definitions.
    ///
    /// Must be called before `visit_function_definition` so that forward references
    /// (a caller defined before its callee) resolve correctly during call lowering.
    /// The traversal order must match the order used in `visit_function_definition`.
    ///
    /// # Parameters
    ///
    /// - `funcs` - Ordered list of function definitions for one source file
    pub(crate) fn build_func_name_to_idx(&mut self, funcs: &[Rc<FunctionDefinition>]) {
        #[allow(clippy::cast_possible_truncation)]
        for (idx, func_def) in funcs.iter().enumerate() {
            self.func_name_to_idx
                .insert(func_def.name(), idx as u32 + self.func_idx);
        }
    }

    /// Maps an Inference `Type` to the corresponding WASM `ValType`.
    ///
    /// Returns `None` for `Type::Simple(Unit)` because unit functions produce no WASM value.
    /// Panics for complex types (arrays, generics, function types, custom types) not yet supported.
    fn val_type_from_type(ty: &Type) -> Option<ValType> {
        match ty {
            Type::Simple(SimpleTypeKind::Unit) => None,
            Type::Simple(
                SimpleTypeKind::Bool
                | SimpleTypeKind::I8
                | SimpleTypeKind::U8
                | SimpleTypeKind::I16
                | SimpleTypeKind::U16
                | SimpleTypeKind::I32
                | SimpleTypeKind::U32,
            ) => Some(ValType::I32),
            Type::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => Some(ValType::I64),
            Type::Array(_array_type) => todo!(),
            Type::Generic(_generic_type) => todo!(),
            Type::Function(_function_type) => todo!(),
            Type::QualifiedName(_qualified_name) => todo!(),
            Type::Qualified(_type_qualified_name) => todo!(),
            Type::Custom(_identifier) => todo!(),
        }
    }

    /// Translates an AST function definition to a WASM function body.
    ///
    /// This is the main entry point for function compilation. It performs several steps:
    ///
    /// 1. **Type mapping** - Maps return type and parameter types to WASM `ValType`
    /// 2. **Parameter lowering** - Registers parameters in `locals_map` at indices 0..n
    /// 3. **Type registration** - Registers the function signature in the type section
    /// 4. **Export annotation** - Marks public functions for WASM export
    /// 5. **Local pre-scan** - Scans the function body to determine regular locals (indices n..)
    /// 6. **Body lowering** - Recursively lowers the function body statements to WASM
    /// 7. **Return handling** - Inserts implicit `end` for function body termination
    ///
    /// # WASM Parameter Semantics
    ///
    /// Parameters occupy local slots `0..param_count`. The WASM function body declares only
    /// additional locals (via `Function::new`); params are implicit from the type signature.
    /// `pre_scan_locals` starts indexing regular locals at `param_count` so there is no
    /// collision.
    ///
    /// # Parameters
    ///
    /// - `function_definition` - AST node representing the function to compile
    /// - `ctx` - Typed context containing type information for all AST nodes
    ///
    /// # Panics
    ///
    /// This method will panic if it encounters unsupported type constructs (arrays,
    /// generics, function types, qualified names, custom types) in parameter or return
    /// positions, as these are not yet implemented.
    pub(crate) fn visit_function_definition(
        &mut self,
        function_definition: &Rc<FunctionDefinition>,
        ctx: &TypedContext,
    ) {
        let fn_name = function_definition.name();
        let results: Vec<ValType> = function_definition
            .returns
            .as_ref()
            .and_then(Self::val_type_from_type)
            .into_iter()
            .collect();

        let mut params: Vec<ValType> = vec![];
        let mut locals_map: FxHashMap<String, (u32, ValType)> = FxHashMap::default();
        let mut local_idx: u32 = 0;

        if let Some(arguments) = &function_definition.arguments {
            for arg_type in arguments {
                if let ArgumentType::Argument(arg) = arg_type {
                    cov_mark::hit!(wasm_codegen_emit_function_params);
                    let vt = Self::val_type_from_type(&arg.ty)
                        .expect("Function parameter type must not be unit");
                    params.push(vt);
                    let prev = locals_map.insert(arg.name(), (local_idx, vt));
                    debug_assert!(
                        prev.is_none(),
                        "parameter `{}` collides with an existing entry in locals_map; \
                         the type-checker should have rejected duplicate parameter names",
                        arg.name(),
                    );
                    local_idx += 1;
                }
            }
        }

        // Parameters occupy local indices 0..param_count. Regular locals follow.
        // WASM requires declaring only the additional (non-param) locals in Function::new().
        let param_count = local_idx;

        let has_return_value = !results.is_empty();

        #[allow(clippy::cast_possible_truncation)]
        let type_idx = self.types.len() as u32;
        self.types.push((params, results));
        self.functions.push(type_idx);

        let is_main = fn_name == "main";
        let should_export = function_definition.visibility == Visibility::Public && !is_main;
        if should_export {
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }
        if is_main && function_definition.visibility == Visibility::Public {
            self.has_main = true;
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }

        Self::pre_scan_locals(
            &function_definition.body,
            ctx,
            &mut locals_map,
            &mut local_idx,
        );

        let local_declarations: Vec<(u32, ValType)> = {
            let mut sorted_locals: Vec<(u32, ValType)> = locals_map
                .values()
                .copied()
                .filter(|(idx, _)| *idx >= param_count)
                .collect();
            sorted_locals.sort_by_key(|(idx, _)| *idx);
            sorted_locals.into_iter().map(|(_, vt)| (1, vt)).collect()
        };

        let mut func = Function::new(local_declarations);

        self.lower_statement(
            std::iter::once(Statement::Block(function_definition.body.clone())).peekable(),
            &mut vec![function_definition.body.clone()],
            ctx,
            &mut func,
            &locals_map,
        );

        // For functions with a non-void return type, emit `unreachable` before the function
        // body's `end` instruction. This satisfies WASM validators when all control-flow paths
        // inside the function exit via explicit `return` instructions (e.g. if/else where both
        // arms return). The `unreachable` instruction is dead code in that case and never
        // executes at runtime. For void functions the instruction is omitted since they do not
        // require any value on the stack at `end`.
        if has_return_value {
            func.instruction(&Instruction::Unreachable);
        }

        func.instruction(&Instruction::End);

        self.func_names.push((self.func_idx, fn_name.clone()));
        let local_name_entries: Vec<(u32, String)> = {
            let mut entries: Vec<(u32, String)> = locals_map
                .iter()
                .map(|(name, (idx, _))| (*idx, name.clone()))
                .collect();
            entries.sort_by_key(|(idx, _)| *idx);
            entries
        };
        if !local_name_entries.is_empty() {
            self.local_names.push((self.func_idx, local_name_entries));
        }

        self.bodies.push(func);
        self.func_idx += 1;
    }

    /// Pre-scans the function body to discover all local variable declarations.
    ///
    /// WASM requires all locals to be declared at the start of a function body.
    /// This method traverses the AST to find all `ConstantDefinition` and
    /// `VariableDefinition` statements and registers them as locals before
    /// instruction emission begins.
    fn pre_scan_locals(
        block: &BlockType,
        ctx: &TypedContext,
        locals_map: &mut FxHashMap<String, (u32, ValType)>,
        local_idx: &mut u32,
    ) {
        for stmt in block.statements() {
            match &stmt {
                Statement::ConstantDefinition(constant_definition) => {
                    let val_type = match ctx
                        .get_node_typeinfo(constant_definition.id)
                        .expect("Constant definition must have a type info")
                        .kind
                    {
                        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => ValType::I64,
                        _ => ValType::I32,
                    };
                    let prev =
                        locals_map.insert(constant_definition.name(), (*local_idx, val_type));
                    debug_assert!(
                        prev.is_none(),
                        "local `{}` collides with an existing entry in locals_map; \
                         the type-checker should have rejected shadowing",
                        constant_definition.name(),
                    );
                    *local_idx += 1;
                }
                Statement::VariableDefinition(variable_definition) => {
                    let val_type = match ctx
                        .get_node_typeinfo(variable_definition.id)
                        .expect("Variable definition must have type info")
                        .kind
                    {
                        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => ValType::I64,
                        _ => ValType::I32,
                    };
                    let prev =
                        locals_map.insert(variable_definition.name(), (*local_idx, val_type));
                    debug_assert!(
                        prev.is_none(),
                        "local `{}` collides with an existing entry in locals_map; \
                         the type-checker should have rejected shadowing",
                        variable_definition.name(),
                    );
                    *local_idx += 1;
                }
                Statement::Block(inner_block) => {
                    Self::pre_scan_locals(inner_block, ctx, locals_map, local_idx);
                }
                Statement::If(if_statement) => {
                    Self::pre_scan_locals(&if_statement.if_arm, ctx, locals_map, local_idx);
                    if let Some(else_arm) = &if_statement.else_arm {
                        Self::pre_scan_locals(else_arm, ctx, locals_map, local_idx);
                    }
                }
                _ => {}
            }
        }
    }

    /// Recursively lowers AST statements to WASM instructions.
    ///
    /// This method handles all statement types including control flow, blocks, and
    /// non-deterministic constructs. It maintains a stack of parent blocks to track
    /// nesting context.
    ///
    /// # Statement Types
    ///
    /// - **Block types** (regular, forall, exists, assume, unique) - Recursively lower
    ///   nested statements with appropriate custom instruction encoding
    /// - **Expression statements** - Evaluate expressions
    /// - **Assignment statements** - Store expression result to a mutable local variable
    /// - **Return statements** - Generate WASM return instructions
    /// - **Constant definitions** - Initialize locals with compile-time literal values
    /// - **Variable definitions** - Initialize locals with any value-producing expression
    /// - **If statements** - Conditional branching with optional else arm
    ///
    /// # Non-Deterministic Blocks
    ///
    /// For non-deterministic block types (forall, exists, assume, unique), this method:
    /// 1. Emits the custom 0xfc opcode with block type (0x40 for void)
    /// 2. Recursively lowers nested statements
    /// 3. Emits the end instruction (0x0b)
    ///
    /// # Parameters
    ///
    /// - `statements_iterator` - Iterator over statements to lower
    /// - `parent_blocks_stack` - Stack tracking enclosing block contexts
    /// - `ctx` - Typed context for type information lookup
    /// - `func` - WASM function body being built
    /// - `locals_map` - Map from variable names to (`local_index`, `ValType`)
    #[allow(clippy::too_many_lines)]
    fn lower_statement<I: Iterator<Item = Statement>>(
        &self,
        mut statements_iterator: Peekable<I>,
        parent_blocks_stack: &mut Vec<BlockType>,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
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
                            func,
                            locals_map,
                        );
                    }
                    parent_blocks_stack.pop();
                }
                BlockType::Forall(forall_block) => {
                    cov_mark::hit!(wasm_codegen_emit_forall_block);
                    self.emit_nondet_block_start(func, FORALL_OPCODE);
                    parent_blocks_stack.push(BlockType::Forall(forall_block.clone()));
                    for stmt in forall_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                            func,
                            locals_map,
                        );
                    }
                    self.emit_nondet_block_end(func);
                    parent_blocks_stack.pop();
                }
                BlockType::Assume(assume_block) => {
                    cov_mark::hit!(wasm_codegen_emit_assume_block);
                    self.emit_nondet_block_start(func, ASSUME_OPCODE);
                    parent_blocks_stack.push(BlockType::Assume(assume_block.clone()));
                    for stmt in assume_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                            func,
                            locals_map,
                        );
                    }
                    self.emit_nondet_block_end(func);
                    parent_blocks_stack.pop();
                }
                BlockType::Exists(exists_block) => {
                    cov_mark::hit!(wasm_codegen_emit_exists_block);
                    self.emit_nondet_block_start(func, EXISTS_OPCODE);
                    parent_blocks_stack.push(BlockType::Exists(exists_block.clone()));
                    for stmt in exists_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                            func,
                            locals_map,
                        );
                    }
                    self.emit_nondet_block_end(func);
                    parent_blocks_stack.pop();
                }
                BlockType::Unique(unique_block) => {
                    cov_mark::hit!(wasm_codegen_emit_unique_block);
                    self.emit_nondet_block_start(func, UNIQUE_OPCODE);
                    parent_blocks_stack.push(BlockType::Unique(unique_block.clone()));
                    for stmt in unique_block.statements.clone() {
                        self.lower_statement(
                            std::iter::once(stmt).peekable(),
                            parent_blocks_stack,
                            ctx,
                            func,
                            locals_map,
                        );
                    }
                    self.emit_nondet_block_end(func);
                    parent_blocks_stack.pop();
                }
            },
            Statement::Expression(expression) => {
                self.lower_expression(&expression, ctx, func, locals_map);
                let expr_produces_value = ctx
                    .get_node_typeinfo(expression.id())
                    .is_some_and(|ti| !matches!(ti.kind, TypeInfoKind::Unit));
                if expr_produces_value {
                    // Do not drop if this is the trailing result of a non-void non-det block —
                    // the value serves as the block's result consumed by the enclosing context.
                    let is_block_result = statements_iterator.peek().is_none()
                        && parent_blocks_stack
                            .last()
                            .is_some_and(|b| b.is_non_det() && !b.is_void());
                    if !is_block_result {
                        func.instruction(&Instruction::Drop);
                    }
                }
            }
            Statement::Assign(assign_statement) => {
                self.lower_assign_statement(&assign_statement, ctx, func, locals_map);
            }
            Statement::Return(return_statement) => {
                self.lower_expression(&return_statement.expression.borrow(), ctx, func, locals_map);
                func.instruction(&Instruction::Return);
            }
            Statement::Loop(_loop_statement) => todo!(),
            Statement::Break(_break_statement) => todo!(),
            Statement::If(if_statement) => {
                self.lower_if_statement(&if_statement, ctx, func, locals_map, parent_blocks_stack);
            }
            Statement::VariableDefinition(variable_definition_statement) => {
                cov_mark::hit!(wasm_codegen_emit_variable_definition);
                let (local_idx, _) = locals_map
                    .get(&variable_definition_statement.name())
                    .expect("Variable local not found in pre-scan");
                match &variable_definition_statement.value {
                    None => todo!("Uninitialized variable definitions are not supported"),
                    Some(expr_ref) => {
                        let expr = expr_ref.borrow();
                        let local_idx = *local_idx;
                        self.lower_expression(&expr, ctx, func, locals_map);
                        func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }
            }
            Statement::TypeDefinition(_type_definition_statement) => todo!(),
            Statement::Assert(_assert_statement) => todo!(),
            Statement::ConstantDefinition(constant_definition) => {
                cov_mark::hit!(wasm_codegen_emit_constant_definition);
                self.lower_literal(&constant_definition.value, ctx, func);
                let (local_idx, _) = locals_map
                    .get(&constant_definition.name())
                    .expect("Local not found in pre-scan");
                func.instruction(&Instruction::LocalSet(*local_idx));
            }
        }
    }

    /// Lowers an AST expression to WASM instructions on the operand stack.
    ///
    /// This method recursively evaluates expressions and emits WASM instructions that
    /// compute the expression's value at runtime. The result is left on the WASM
    /// operand stack.
    ///
    /// # Supported Expressions
    ///
    /// - **`Literals`** - Compile-time constants (numbers, booleans)
    /// - **`Identifiers`** - Load values from local variables
    /// - **`Uzumaki`** - Non-deterministic value generation via custom opcodes
    /// - **`Binary`** - Arithmetic, bitwise, comparison, and logical operators;
    ///   sign-sensitive variants selected from the left operand type
    /// - **`PrefixUnary`** - Negation (`-`), logical not (`!`), bitwise not (`~`)
    /// - **`Parenthesized`** - Transparent wrapper; delegates to the inner expression
    /// - **`FunctionCall`** - Plain identifier-based calls (method/higher-order: `todo!()`)
    ///
    /// # Parameters
    ///
    /// - `expression` - AST expression node to lower
    /// - `ctx` - Typed context for type lookups
    /// - `func` - WASM function body being built
    /// - `locals_map` - Map from variable names to (`local_index`, `ValType`)
    fn lower_expression(
        &self,
        expression: &Expression,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) {
        match expression {
            Expression::ArrayIndexAccess(_array_index_access_expression) => todo!(),
            Expression::Binary(binary_expression) => {
                self.lower_binary_expression(binary_expression, ctx, func, locals_map);
            }
            Expression::MemberAccess(_member_access_expression) => todo!(),
            Expression::TypeMemberAccess(_type_member_access_expression) => todo!(),
            Expression::FunctionCall(function_call_expression) => {
                match self.lower_function_call(function_call_expression, ctx, func, locals_map) {
                    Ok(()) => {}
                    Err(CodegenError::UnsupportedCalleeKind) => {
                        todo!(
                            "Non-identifier function calls (method calls, higher-order) \
                             are not yet implemented"
                        )
                    }
                    Err(CodegenError::UnknownFunction(name)) => {
                        panic!(
                            "Function '{name}' not found in name-to-index map; \
                                the type-checker should have caught undefined functions"
                        )
                    }
                }
            }
            Expression::Struct(_struct_expression) => todo!(),
            Expression::PrefixUnary(prefix_unary_expression) => {
                self.lower_prefix_unary_expression(prefix_unary_expression, ctx, func, locals_map);
            }
            Expression::Parenthesized(parenthesized_expression) => {
                cov_mark::hit!(wasm_codegen_emit_parenthesized_expression);
                self.lower_expression(
                    &parenthesized_expression.expression.borrow(),
                    ctx,
                    func,
                    locals_map,
                );
            }
            Expression::Literal(literal) => self.lower_literal(literal, ctx, func),
            Expression::Identifier(identifier) => {
                let (local_idx, _) = locals_map
                    .get(&identifier.name)
                    .expect("Variable not found");
                func.instruction(&Instruction::LocalGet(*local_idx));
            }
            Expression::Type(_) => todo!(),
            Expression::Uzumaki(uzumaki_expression) => {
                if ctx.is_node_i32(uzumaki_expression.id) {
                    cov_mark::hit!(wasm_codegen_emit_uzumaki_i32);
                    self.emit_uzumaki(func, UZUMAKI_I32_OPCODE);
                    return;
                }
                if ctx.is_node_i64(uzumaki_expression.id) {
                    cov_mark::hit!(wasm_codegen_emit_uzumaki_i64);
                    self.emit_uzumaki(func, UZUMAKI_I64_OPCODE);
                    return;
                }
                panic!("Unsupported Uzumaki expression type: {uzumaki_expression:?}");
            }
        }
    }

    /// Lowers a plain identifier-based function call to a WASM `call` instruction.
    ///
    /// Pushes each argument onto the WASM operand stack in positional order, then emits
    /// `call <func_idx>`. Argument labels (if present) are ignored because WASM is purely
    /// positional and the type-checker has already validated label correctness and argument
    /// count.
    ///
    /// # Supported Call Kinds
    ///
    /// Only `Expression::Identifier`-based callees are supported. Method calls
    /// (`MemberAccess`), associated function calls (`TypeMemberAccess`), and
    /// higher-order calls are out of scope and return
    /// [`CodegenError::UnsupportedCalleeKind`].
    ///
    /// # Recursion
    ///
    /// Direct or indirect recursion is explicitly forbidden in Inference (Power of 10,
    /// Rule 1). The type-checker is responsible for detecting and rejecting recursive
    /// call graphs. At codegen time, recursive calls are left as `todo!` until the
    /// analysis pass is in place.
    ///
    /// # Parameters
    ///
    /// - `fce` - Function call expression node
    /// - `ctx` - Typed context for type lookups
    /// - `func` - WASM function body being built
    /// - `locals_map` - Map from variable names to (`local_index`, `ValType`)
    ///
    /// # Errors
    ///
    /// Returns [`CodegenError`] if the callee is not a plain identifier or the
    /// function name is not in the pre-built index map.
    fn lower_function_call(
        &self,
        fce: &inference_ast::nodes::FunctionCallExpression,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) -> Result<(), CodegenError> {
        let Expression::Identifier(_) = &fce.function else {
            return Err(CodegenError::UnsupportedCalleeKind);
        };

        cov_mark::hit!(wasm_codegen_emit_function_call);

        if let Some(arguments) = &fce.arguments {
            for (_label, expr_ref) in arguments {
                self.lower_expression(&expr_ref.borrow(), ctx, func, locals_map);
            }
        }

        let func_name = fce.name();
        let func_idx = self
            .func_name_to_idx
            .get(&func_name)
            .copied()
            .ok_or(CodegenError::UnknownFunction(func_name))?;

        func.instruction(&Instruction::Call(func_idx));
        Ok(())
    }

    /// Lowers an assignment statement to WASM instructions.
    ///
    /// # WASM encoding
    ///
    /// For `x = expr;` where `x` is a local variable:
    /// ```text
    /// lower_expression(right)    // push value onto WASM operand stack
    /// LocalSet(target_idx)       // pop value and store to local
    /// ```
    ///
    /// This is identical to variable definition initialization -- the difference is that
    /// the local index is resolved from the LHS identifier rather than from a
    /// `VariableDefinitionStatement.name()`.
    ///
    /// # Supported Targets
    ///
    /// Only `Expression::Identifier` targets are currently supported. Member access and
    /// array index targets require memory operations and are deferred to compound type support.
    ///
    /// # Parameters
    ///
    /// - `assign_stmt` - The assignment statement AST node to lower
    /// - `ctx` - Typed context for type information lookup
    /// - `func` - WASM function body being built
    /// - `locals_map` - Map from variable names to (`local_index`, `ValType`)
    fn lower_assign_statement(
        &self,
        assign_stmt: &AssignStatement,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) {
        let left = assign_stmt.left.borrow();
        match &*left {
            Expression::Identifier(identifier) => {
                cov_mark::hit!(wasm_codegen_emit_assign_identifier);
                let (local_idx, _) = locals_map
                    .get(&identifier.name)
                    .expect("Assignment target variable not found");
                let local_idx = *local_idx;
                self.lower_expression(&assign_stmt.right.borrow(), ctx, func, locals_map);
                func.instruction(&Instruction::LocalSet(local_idx));
            }
            _ => todo!("Assignment to non-identifier targets (member access, array index) not yet supported"),
        }
    }

    /// Lowers an `if`/`else` statement to WASM structured control flow.
    ///
    /// # WASM encoding
    ///
    /// For `if cond { ... }` (no else arm):
    /// ```text
    /// lower_expression(condition)   // leaves i32 (0 or 1) on stack
    /// If(BlockType::Empty)          // 0x04 0x40
    ///   lower statements in if_arm
    /// End                           // 0x0b
    /// ```
    ///
    /// For `if cond { ... } else { ... }`:
    /// ```text
    /// lower_expression(condition)   // leaves i32 (0 or 1) on stack
    /// If(BlockType::Empty)          // 0x04 0x40
    ///   lower statements in if_arm
    /// Else                          // 0x05
    ///   lower statements in else_arm
    /// End                           // 0x0b
    /// ```
    ///
    /// `BlockType::Empty` is correct because Inference `if`/`else` is a statement, not an
    /// expression — it does not produce a value on the WASM operand stack.
    ///
    /// # Parameters
    ///
    /// - `if_stmt` - The if statement AST node to lower
    /// - `ctx` - Typed context for type information lookup
    /// - `func` - WASM function body being built
    /// - `locals_map` - Map from variable names to (`local_index`, `ValType`)
    /// - `parent_blocks_stack` - Stack tracking enclosing block contexts (passed through to nested
    ///   statement lowering)
    fn lower_if_statement(
        &self,
        if_stmt: &inference_ast::nodes::IfStatement,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
        parent_blocks_stack: &mut Vec<BlockType>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_if_statement);

        self.lower_expression(&if_stmt.condition.borrow(), ctx, func, locals_map);
        func.instruction(&Instruction::If(WasmBlockType::Empty));

        for stmt in if_stmt.if_arm.statements() {
            self.lower_statement(
                std::iter::once(stmt).peekable(),
                parent_blocks_stack,
                ctx,
                func,
                locals_map,
            );
        }

        if let Some(else_arm) = &if_stmt.else_arm {
            cov_mark::hit!(wasm_codegen_emit_if_with_else);
            func.instruction(&Instruction::Else);
            for stmt in else_arm.statements() {
                self.lower_statement(
                    std::iter::once(stmt).peekable(),
                    parent_blocks_stack,
                    ctx,
                    func,
                    locals_map,
                );
            }
        }

        func.instruction(&Instruction::End);
    }

    /// Returns `true` if `kind` is an unsigned integer type.
    ///
    /// Used during binary expression lowering to select the sign-sensitive WASM
    /// instruction variants (`DivU`, `RemU`, `LtU`, `LeU`, `GtU`, `GeU`, `ShrU`).
    fn is_unsigned_type(kind: &TypeInfoKind) -> bool {
        matches!(
            kind,
            TypeInfoKind::Number(
                NumberType::U8 | NumberType::U16 | NumberType::U32 | NumberType::U64
            )
        )
    }

    /// Returns `true` if `kind` maps to a 64-bit WASM value type.
    fn is_i64_type(kind: &TypeInfoKind) -> bool {
        matches!(
            kind,
            TypeInfoKind::Number(NumberType::I64 | NumberType::U64)
        )
    }

    /// Lowers a binary expression to WASM stack instructions.
    ///
    /// Strategy (stack machine):
    /// 1. Lower left operand → value on WASM stack
    /// 2. Lower right operand → value on WASM stack
    /// 3. Determine dispatch from the left operand's `TypeInfoKind`
    /// 4. Emit the appropriate WASM binary instruction
    ///
    /// Dispatch is always driven by the **left** operand type (not the result type) because
    /// comparison operators produce `Bool` (always i32) and cannot be used for dispatch.
    /// The type-checker guarantees that left and right operand types match for all binary ops.
    ///
    /// Signed vs unsigned variants are selected based on whether the left operand type is an
    /// unsigned integer (`u8`, `u16`, `u32`, `u64`). `Eq`/`Ne` have no sign-distinct WASM
    /// variant — they compare bit patterns identically for all integer representations.
    ///
    /// Logical `&&`/`||` are lowered as bitwise `i32.and`/`i32.or` because the type-checker
    /// constrains both operands to `bool` (i32 0 or 1), making bitwise and short-circuit
    /// evaluation produce identical results.
    ///
    /// # WASM Trap Conditions
    ///
    /// `Div` and `Mod` can cause WASM traps (immediate runtime termination):
    /// - Division or remainder by zero traps for all integer div/rem instructions.
    /// - `i32.div_s(i32::MIN, -1)` and `i64.div_s(i64::MIN, -1)` trap due to signed overflow
    ///   (the positive result does not fit in the signed range).
    /// - `i32.rem_s` / `i64.rem_s` with `(MIN, -1)` do **not** trap (the remainder is 0).
    #[allow(clippy::too_many_lines)]
    fn lower_binary_expression(
        &self,
        be: &BinaryExpression,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_binary_expression);

        self.lower_expression(&be.left.borrow(), ctx, func, locals_map);
        self.lower_expression(&be.right.borrow(), ctx, func, locals_map);

        let left_type_info = ctx
            .get_node_typeinfo(be.left.borrow().id())
            .expect("Binary expression left operand must have type info");
        let is_i64 = Self::is_i64_type(&left_type_info.kind);
        let is_unsigned = Self::is_unsigned_type(&left_type_info.kind);

        let instruction = match be.operator {
            OperatorKind::Add => {
                if is_i64 {
                    Instruction::I64Add
                } else {
                    Instruction::I32Add
                }
            }
            OperatorKind::Sub => {
                if is_i64 {
                    Instruction::I64Sub
                } else {
                    Instruction::I32Sub
                }
            }
            OperatorKind::Mul => {
                if is_i64 {
                    Instruction::I64Mul
                } else {
                    Instruction::I32Mul
                }
            }
            OperatorKind::Div => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64DivU,
                (true, false) => Instruction::I64DivS,
                (false, true) => Instruction::I32DivU,
                (false, false) => Instruction::I32DivS,
            },
            OperatorKind::Mod => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64RemU,
                (true, false) => Instruction::I64RemS,
                (false, true) => Instruction::I32RemU,
                (false, false) => Instruction::I32RemS,
            },
            OperatorKind::And => Instruction::I32And,
            OperatorKind::Or => Instruction::I32Or,
            OperatorKind::Eq => {
                if is_i64 {
                    Instruction::I64Eq
                } else {
                    Instruction::I32Eq
                }
            }
            OperatorKind::Ne => {
                if is_i64 {
                    Instruction::I64Ne
                } else {
                    Instruction::I32Ne
                }
            }
            OperatorKind::Lt => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64LtU,
                (true, false) => Instruction::I64LtS,
                (false, true) => Instruction::I32LtU,
                (false, false) => Instruction::I32LtS,
            },
            OperatorKind::Le => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64LeU,
                (true, false) => Instruction::I64LeS,
                (false, true) => Instruction::I32LeU,
                (false, false) => Instruction::I32LeS,
            },
            OperatorKind::Gt => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64GtU,
                (true, false) => Instruction::I64GtS,
                (false, true) => Instruction::I32GtU,
                (false, false) => Instruction::I32GtS,
            },
            OperatorKind::Ge => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64GeU,
                (true, false) => Instruction::I64GeS,
                (false, true) => Instruction::I32GeU,
                (false, false) => Instruction::I32GeS,
            },
            OperatorKind::BitAnd => {
                if is_i64 {
                    Instruction::I64And
                } else {
                    Instruction::I32And
                }
            }
            OperatorKind::BitOr => {
                if is_i64 {
                    Instruction::I64Or
                } else {
                    Instruction::I32Or
                }
            }
            OperatorKind::BitXor => {
                if is_i64 {
                    Instruction::I64Xor
                } else {
                    Instruction::I32Xor
                }
            }
            OperatorKind::Shl => {
                if is_i64 {
                    Instruction::I64Shl
                } else {
                    Instruction::I32Shl
                }
            }
            OperatorKind::Shr => match (is_i64, is_unsigned) {
                (true, true) => Instruction::I64ShrU,
                (true, false) => Instruction::I64ShrS,
                (false, true) => Instruction::I32ShrU,
                (false, false) => Instruction::I32ShrS,
            },
            OperatorKind::Pow => {
                todo!(
                    "Power operator (`**`) deferred — no native WASM instruction; \
                     see .claude/plans/codegen/new-pow-operator/master_plan.md"
                )
            }
        };

        func.instruction(&instruction);
    }

    /// Lowers a prefix unary expression to WASM stack instructions.
    ///
    /// # Lowering patterns
    ///
    /// - `Neg` (`-x`): `[0_const, lower(x), Sub]` — WASM has no integer negation opcode;
    ///   the standard idiom is `0 - x`.
    /// - `Not` (`!x`): `[lower(x), I32Eqz]` — inverts boolean (0→1, 1→0) using WASM test op.
    /// - `BitNot` (`~x`): `[lower(x), -1_const, Xor]` — `x ^ -1` inverts all bits;
    ///   works identically for i32 and i64.
    fn lower_prefix_unary_expression(
        &self,
        pue: &PrefixUnaryExpression,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_prefix_unary_expression);

        let type_info = ctx
            .get_node_typeinfo(pue.id)
            .expect("Prefix unary expression must have type info");
        let is_i64 = Self::is_i64_type(&type_info.kind);

        match pue.operator {
            UnaryOperatorKind::Neg => {
                cov_mark::hit!(wasm_codegen_emit_unary_neg);
                if is_i64 {
                    func.instruction(&Instruction::I64Const(0));
                } else {
                    func.instruction(&Instruction::I32Const(0));
                }
                self.lower_expression(&pue.expression.borrow(), ctx, func, locals_map);
                if is_i64 {
                    func.instruction(&Instruction::I64Sub);
                } else {
                    func.instruction(&Instruction::I32Sub);
                }
            }
            UnaryOperatorKind::Not => {
                cov_mark::hit!(wasm_codegen_emit_unary_not);
                self.lower_expression(&pue.expression.borrow(), ctx, func, locals_map);
                func.instruction(&Instruction::I32Eqz);
            }
            UnaryOperatorKind::BitNot => {
                cov_mark::hit!(wasm_codegen_emit_unary_bitnot);
                self.lower_expression(&pue.expression.borrow(), ctx, func, locals_map);
                if is_i64 {
                    func.instruction(&Instruction::I64Const(-1));
                    func.instruction(&Instruction::I64Xor);
                } else {
                    func.instruction(&Instruction::I32Const(-1));
                    func.instruction(&Instruction::I32Xor);
                }
            }
        }
    }

    /// Converts an AST literal to WASM constant instructions.
    ///
    /// Literals are compile-time constants that get emitted as WASM const instructions
    /// that push the value onto the operand stack.
    ///
    /// # Literal Types
    ///
    /// - **Bool** - Emitted as `i32.const` (0 for false, 1 for true) per WASM convention
    /// - **Number** - Emitted as the appropriate const instruction based on inferred type
    ///
    /// # Parameters
    ///
    /// - `literal` - AST literal node to convert
    /// - `ctx` - Typed context for type lookups
    /// - `func` - WASM function body being built
    #[allow(clippy::unused_self)]
    fn lower_literal(&self, literal: &Literal, ctx: &TypedContext, func: &mut Function) {
        match literal {
            Literal::Array(_array_literal) => todo!(),
            Literal::Bool(bool_literal) => {
                func.instruction(&Instruction::I32Const(i32::from(bool_literal.value)));
            }
            Literal::String(_string_literal) => todo!(),
            Literal::Number(number_literal) => {
                let type_info = ctx
                    .get_node_typeinfo(number_literal.id)
                    .expect("Number literal must have type info");
                match type_info.kind {
                    TypeInfoKind::Number(NumberType::I8 | NumberType::I16 | NumberType::I32) => {
                        let val = number_literal
                            .value
                            .parse::<i32>()
                            .expect("Failed to parse signed 32-bit integer literal");
                        func.instruction(&Instruction::I32Const(val));
                    }
                    TypeInfoKind::Number(NumberType::U8) => {
                        let val = i32::from(
                            number_literal
                                .value
                                .parse::<u8>()
                                .expect("Failed to parse unsigned 8-bit integer literal"),
                        );
                        func.instruction(&Instruction::I32Const(val));
                    }
                    TypeInfoKind::Number(NumberType::U16) => {
                        let val = i32::from(
                            number_literal
                                .value
                                .parse::<u16>()
                                .expect("Failed to parse unsigned 16-bit integer literal"),
                        );
                        func.instruction(&Instruction::I32Const(val));
                    }
                    TypeInfoKind::Number(NumberType::U32) => {
                        let val = number_literal
                            .value
                            .parse::<u32>()
                            .expect("Failed to parse unsigned 32-bit integer literal")
                            .cast_signed();
                        func.instruction(&Instruction::I32Const(val));
                    }
                    TypeInfoKind::Number(NumberType::I64) => {
                        let val = number_literal
                            .value
                            .parse::<i64>()
                            .expect("Failed to parse signed 64-bit integer literal");
                        func.instruction(&Instruction::I64Const(val));
                    }
                    TypeInfoKind::Number(NumberType::U64) => {
                        let val = number_literal
                            .value
                            .parse::<u64>()
                            .expect("Failed to parse unsigned 64-bit integer literal")
                            .cast_signed();
                        func.instruction(&Instruction::I64Const(val));
                    }
                    _ => panic!("Unsupported number literal type: {:?}", type_info.kind),
                }
            }
            Literal::Unit(_unit_literal) => todo!(),
        }
    }

    /// Emits the start of a non-deterministic block.
    ///
    /// Writes the custom 0xfc prefix followed by the specific opcode and void block
    /// type (0x40). The block body follows, terminated by `emit_nondet_block_end`.
    #[allow(clippy::unused_self)]
    fn emit_nondet_block_start(&self, func: &mut Function, opcode: u8) {
        func.raw([OPCODE_PREFIX, opcode, BLOCK_TYPE_VOID]);
    }

    /// Emits the end of a non-deterministic block.
    ///
    /// Writes the standard WASM `end` instruction (0x0b) to close the block.
    #[allow(clippy::unused_self)]
    fn emit_nondet_block_end(&self, func: &mut Function) {
        func.raw([END_OPCODE]);
    }

    /// Emits a uzumaki (non-deterministic value) instruction.
    ///
    /// Writes the custom 0xfc prefix followed by the uzumaki opcode.
    /// This is a standalone instruction (not a block) that produces a
    /// non-deterministic value of the corresponding type on the stack.
    #[allow(clippy::unused_self)]
    fn emit_uzumaki(&self, func: &mut Function, opcode: u8) {
        func.raw([OPCODE_PREFIX, opcode]);
    }

    /// Returns whether a public `main()` function was compiled.
    pub(crate) fn has_main(&self) -> bool {
        self.has_main
    }

    /// Assembles the complete WASM binary module from all accumulated sections.
    ///
    /// Builds the following WASM sections in order:
    /// 1. **Type section** - All function signatures
    /// 2. **Function section** - Type index for each function
    /// 3. **Export section** - Exported functions
    /// 4. **Code section** - Function bodies
    /// 5. **Name section** (custom) - Debug names for module, functions, and locals
    ///
    /// # Returns
    ///
    /// Complete WASM binary as `Vec<u8>`.
    pub(crate) fn finish(&self) -> Vec<u8> {
        let mut module = Module::new();

        let mut type_section = TypeSection::new();
        for (params, results) in &self.types {
            type_section
                .ty()
                .function(params.iter().copied(), results.iter().copied());
        }
        module.section(&type_section);

        let mut function_section = FunctionSection::new();
        for &type_idx in &self.functions {
            function_section.function(type_idx);
        }
        module.section(&function_section);

        if !self.exports.is_empty() {
            let mut export_section = ExportSection::new();
            for (name, kind, idx) in &self.exports {
                export_section.export(name, *kind, *idx);
            }
            module.section(&export_section);
        }

        let mut code_section = CodeSection::new();
        for body in &self.bodies {
            code_section.function(body);
        }
        module.section(&code_section);

        let mut name_section = NameSection::new();
        name_section.module(&self.module_name);

        if !self.func_names.is_empty() {
            let mut func_name_map = NameMap::new();
            for (idx, name) in &self.func_names {
                func_name_map.append(*idx, name);
            }
            name_section.functions(&func_name_map);
        }

        if !self.local_names.is_empty() {
            let mut indirect_map = IndirectNameMap::new();
            for (func_idx, locals) in &self.local_names {
                let mut local_map = NameMap::new();
                for (local_idx, name) in locals {
                    local_map.append(*local_idx, name);
                }
                indirect_map.append(*func_idx, &local_map);
            }
            name_section.locals(&indirect_map);
        }

        module.section(&name_section);

        module.finish()
    }
}
