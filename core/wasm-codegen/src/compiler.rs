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
//! 1. **Function lowering** - Convert AST function definitions to WASM function bodies
//! 2. **Statement lowering** - Translate control flow and non-deterministic blocks
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

use rustc_hash::FxHashMap;
use std::iter::Peekable;
use std::rc::Rc;

use inference_ast::nodes::{
    ArgumentType, BlockType, Expression, FunctionDefinition, Literal, SimpleTypeKind, Statement,
    Type, Visibility,
};
use inference_type_checker::{
    type_info::{NumberType, TypeInfoKind},
    typed_context::TypedContext,
};
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, IndirectNameMap,
    Instruction, Module, NameMap, NameSection, TypeSection, ValType,
};

/// Error returned when a function call expression cannot be lowered by the codegen pass.
///
/// This is an internal error type used by [`Compiler::lower_function_call`]. Callers
/// convert it to a `todo!` or `panic!` depending on whether the case is planned or
/// indicates a type-checker inconsistency.
#[derive(Debug)]
enum FunctionCallError {
    /// The callee expression is not a plain identifier (e.g., method call, higher-order call).
    /// This case is out of scope for the current implementation.
    UnsupportedCalleeKind,
    /// The function name was not found in the pre-built index map.
    /// This should never happen if the type-checker ran successfully.
    UnknownFunction(String),
}

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
                    locals_map.insert(arg.name(), (local_idx, vt));
                    local_idx += 1;
                }
            }
        }

        // Parameters occupy local indices 0..param_count. Regular locals follow.
        // WASM requires declaring only the additional (non-param) locals in Function::new().
        let param_count = local_idx;

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

        // Only declare locals that come after parameters (index >= param_count).
        // Parameters are implicitly declared via the function type signature.
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
                    locals_map.insert(constant_definition.name(), (*local_idx, val_type));
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
                    locals_map.insert(variable_definition.name(), (*local_idx, val_type));
                    *local_idx += 1;
                }
                Statement::Block(inner_block) => {
                    Self::pre_scan_locals(inner_block, ctx, locals_map, local_idx);
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
    /// - **Return statements** - Generate WASM return instructions
    /// - **Constant definitions** - Initialize locals with compile-time literal values
    /// - **Variable definitions** - Initialize locals with literals, identifiers, or uzumaki
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
                let is_trailing_in_nondet_void_block = statements_iterator.peek().is_none()
                    && parent_blocks_stack.last().unwrap().is_non_det()
                    && parent_blocks_stack.last().unwrap().is_void();
                if is_trailing_in_nondet_void_block {
                    let expr_produces_value = ctx
                        .get_node_typeinfo(expression.id())
                        .is_some_and(|ti| !matches!(ti.kind, TypeInfoKind::Unit));
                    if expr_produces_value {
                        func.instruction(&Instruction::Drop);
                    }
                }
            }
            Statement::Assign(_assign_statement) => todo!(),
            Statement::Return(return_statement) => {
                self.lower_expression(&return_statement.expression.borrow(), ctx, func, locals_map);
                func.instruction(&Instruction::Return);
            }
            Statement::Loop(_loop_statement) => todo!(),
            Statement::Break(_break_statement) => todo!(),
            Statement::If(_if_statement) => todo!(),
            Statement::VariableDefinition(variable_definition_statement) => {
                cov_mark::hit!(wasm_codegen_emit_variable_definition);
                let (local_idx, _) = locals_map
                    .get(&variable_definition_statement.name())
                    .expect("Variable local not found in pre-scan");
                match &variable_definition_statement.value {
                    None => todo!("Uninitialized variable definitions are not supported"),
                    Some(expr_ref) => {
                        let expr = expr_ref.borrow();
                        match &*expr {
                            Expression::Literal(lit) => {
                                self.lower_literal(lit, ctx, func);
                                func.instruction(&Instruction::LocalSet(*local_idx));
                            }
                            Expression::Identifier(ident) => {
                                let (src_idx, _) = locals_map
                                    .get(&ident.name)
                                    .expect("Source variable not found in locals map");
                                func.instruction(&Instruction::LocalGet(*src_idx));
                                func.instruction(&Instruction::LocalSet(*local_idx));
                            }
                            Expression::Uzumaki(uzumaki_expression) => {
                                if ctx.is_node_i64(uzumaki_expression.id) {
                                    cov_mark::hit!(wasm_codegen_variable_definition_uzumaki_i64);
                                    self.emit_uzumaki(func, UZUMAKI_I64_OPCODE);
                                } else {
                                    cov_mark::hit!(wasm_codegen_variable_definition_uzumaki_i32);
                                    self.emit_uzumaki(func, UZUMAKI_I32_OPCODE);
                                }
                                func.instruction(&Instruction::LocalSet(*local_idx));
                            }
                            Expression::FunctionCall(_) => {
                                let local_idx = *local_idx;
                                self.lower_expression(&expr, ctx, func, locals_map);
                                func.instruction(&Instruction::LocalSet(local_idx));
                            }
                            _ => todo!("Unsupported variable initializer expression"),
                        }
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
    /// - **Literals** - Compile-time constants (numbers, booleans)
    /// - **Identifiers** - Load values from local variables
    /// - **Uzumaki** - Non-deterministic value generation via custom opcodes
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
            Expression::Binary(_binary_expression) => todo!(),
            Expression::MemberAccess(_member_access_expression) => todo!(),
            Expression::TypeMemberAccess(_type_member_access_expression) => todo!(),
            Expression::FunctionCall(function_call_expression) => {
                match self.lower_function_call(function_call_expression, ctx, func, locals_map) {
                    Ok(()) => {}
                    Err(FunctionCallError::UnsupportedCalleeKind) => {
                        todo!(
                            "Non-identifier function calls (method calls, higher-order) \
                             are not yet implemented"
                        )
                    }
                    Err(FunctionCallError::UnknownFunction(name)) => {
                        panic!("Function '{name}' not found in name-to-index map; \
                                the type-checker should have caught undefined functions")
                    }
                }
            }
            Expression::Struct(_struct_expression) => todo!(),
            Expression::PrefixUnary(_prefix_unary_expression) => todo!(),
            Expression::Parenthesized(_parenthesized_expression) => todo!(),
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
    /// [`FunctionCallError::UnsupportedCalleeKind`].
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
    /// Returns [`FunctionCallError`] if the callee is not a plain identifier or the
    /// function name is not in the pre-built index map.
    fn lower_function_call(
        &self,
        fce: &inference_ast::nodes::FunctionCallExpression,
        ctx: &TypedContext,
        func: &mut Function,
        locals_map: &FxHashMap<String, (u32, ValType)>,
    ) -> Result<(), FunctionCallError> {
        let Expression::Identifier(_) = &fce.function else {
            return Err(FunctionCallError::UnsupportedCalleeKind);
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
            .ok_or(FunctionCallError::UnknownFunction(func_name))?;

        func.instruction(&Instruction::Call(func_idx));
        Ok(())
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
