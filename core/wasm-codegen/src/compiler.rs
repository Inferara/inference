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
//!    variable and constant definitions, assignment statements, return, and expression statements
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

use inference_ast::arena::AstArena;
use inference_ast::ids::{BlockId, DefId, ExprId, IdentId, NodeId, StmtId, TypeId};
use inference_ast::nodes::{
    ArgKind, BlockKind, Def, Expr, OperatorKind, SimpleTypeKind, Stmt, TypeNode, UnaryOperatorKind,
    Visibility,
};
use inference_type_checker::{
    type_info::{NumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};
use wasm_encoder::{
    BlockType as WasmBlockType, CodeSection, ConstExpr, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, IndirectNameMap, Instruction, MemorySection,
    MemoryType, Module, NameMap, NameSection, TypeSection, ValType,
};

use crate::memory::{
    self, ArraySlot, FrameLayout, STACK_POINTER_INIT, STACK_SIZE, StructSlot, align_to,
    align_to_frame, compute_struct_field_layout, element_size, emit_array_param_copy,
    emit_sret_copy, emit_sret_element_addr, emit_stack_epilogue, emit_stack_prologue,
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

#[derive(Default)]
struct LoopContext {
    wasm_block_depth: u32,
    loop_exit_depths: Vec<u32>,
}

/// Metadata about a function that returns an array type.
///
/// Populated during `build_func_name_to_idx` so that callers and callees
/// know the sret calling convention parameters at code generation time.
#[derive(Debug, Clone)]
struct ArrayReturnInfo {
    elem_kind: TypeInfoKind,
    elem_size: u32,
    length: u32,
}

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
    func_name_to_idx: FxHashMap<String, u32>,
    /// Sticky flag: set to `true` when any function requires linear memory.
    has_memory: bool,
    /// Maps function names to their array return type metadata.
    func_array_returns: FxHashMap<String, ArrayReturnInfo>,
    /// Name of the function currently being compiled.
    current_fn_name: String,
    // Per-function state (set in visit_function_definition, used by lowering methods)
    func: Option<Function>,
    locals_map: FxHashMap<String, (u32, ValType)>,
    frame_layout: Option<FrameLayout>,
    loop_ctx: LoopContext,
    parent_blocks_stack: Vec<BlockKind>,
}

impl Compiler {
    /// Creates a new compiler instance for building a WASM module.
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
            has_memory: false,
            func_array_returns: FxHashMap::default(),
            current_fn_name: String::new(),
            func: None,
            locals_map: FxHashMap::default(),
            frame_layout: None,
            loop_ctx: LoopContext::default(),
            parent_blocks_stack: Vec::new(),
        }
    }

    fn func(&mut self) -> &mut Function {
        self.func
            .as_mut()
            .expect("func() called outside function compilation")
    }

    /// Builds the function name-to-WASM-index map from the source file's function definitions.
    ///
    /// Must be called before `visit_function_definition` so that forward references
    /// resolve correctly during call lowering.
    pub(crate) fn build_func_name_to_idx(&mut self, arena: &AstArena, func_def_ids: &[DefId]) {
        #[allow(clippy::cast_possible_truncation)]
        for (idx, &def_id) in func_def_ids.iter().enumerate() {
            let fn_name = arena.def_name(def_id).to_string();
            self.func_name_to_idx
                .insert(fn_name.clone(), idx as u32 + self.func_idx);

            if let Def::Function { returns, .. } = &arena[def_id].kind
                && let Some(return_ty_id) = returns
            {
                let return_type_info = TypeInfo::from_type_id(arena, *return_ty_id);
                if let TypeInfoKind::Array(ref elem_type, length) = return_type_info.kind {
                    let elem_sz = element_size(&elem_type.kind);
                    self.func_array_returns.insert(
                        fn_name,
                        ArrayReturnInfo {
                            elem_kind: elem_type.kind.clone(),
                            elem_size: elem_sz,
                            length,
                        },
                    );
                }
            }
        }
    }

    /// Maps an Inference type to the corresponding WASM `ValType`.
    ///
    /// Returns `None` for unit types because unit functions produce no WASM value.
    fn val_type_from_type_id(arena: &AstArena, ty_id: TypeId) -> Option<ValType> {
        match &arena[ty_id].kind {
            TypeNode::Simple(SimpleTypeKind::Unit) => None,
            TypeNode::Simple(
                SimpleTypeKind::Bool
                | SimpleTypeKind::I8
                | SimpleTypeKind::U8
                | SimpleTypeKind::I16
                | SimpleTypeKind::U16
                | SimpleTypeKind::I32
                | SimpleTypeKind::U32,
            )
            | TypeNode::Array { .. } => Some(ValType::I32),
            TypeNode::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => Some(ValType::I64),
            TypeNode::Generic { .. } => todo!(),
            TypeNode::Function { .. } => todo!(),
            TypeNode::QualifiedName { .. } => todo!(),
            TypeNode::Qualified { .. } => todo!(),
            TypeNode::Custom(_) => todo!(),
        }
    }

    /// Translates an AST function definition to a WASM function body.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn visit_function_definition(
        &mut self,
        def_id: DefId,
        arena: &AstArena,
        ctx: &TypedContext,
    ) {
        let (fn_name_id, vis, args, returns, body_id) = match &arena[def_id].kind {
            Def::Function {
                name,
                vis,
                args,
                returns,
                body,
                ..
            } => (*name, vis.clone(), args.clone(), *returns, *body),
            _ => return,
        };

        let fn_name = arena[fn_name_id].name.clone();
        self.current_fn_name.clone_from(&fn_name);

        let is_array_return = self.func_array_returns.contains_key(&fn_name);

        let results: Vec<ValType> = if is_array_return {
            vec![]
        } else {
            returns
                .and_then(|ty_id| Self::val_type_from_type_id(arena, ty_id))
                .into_iter()
                .collect()
        };

        let mut params: Vec<ValType> = vec![];
        self.locals_map.clear();
        self.loop_ctx = LoopContext::default();
        self.parent_blocks_stack.clear();
        let mut local_idx: u32 = 0;

        if is_array_return {
            params.push(ValType::I32);
            self.locals_map
                .insert("sret".to_string(), (0, ValType::I32));
            local_idx = 1;
        }

        for arg in &args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    cov_mark::hit!(wasm_codegen_emit_function_params);
                    let vt = Self::val_type_from_type_id(arena, *ty)
                        .expect("Function parameter type must not be unit");
                    params.push(vt);
                    let arg_name = arena[*name].name.clone();
                    let prev = self.locals_map.insert(arg_name.clone(), (local_idx, vt));
                    assert!(
                        prev.is_none(),
                        "parameter `{arg_name}` collides with an existing entry in locals_map; \
                         the type-checker should have rejected duplicate parameter names",
                    );
                    local_idx += 1;
                }
                ArgKind::SelfRef { .. } => {
                    todo!("Self-reference parameters are not yet supported in WASM codegen")
                }
                ArgKind::Ignored { .. } => {
                    todo!("Ignore arguments are not yet supported in WASM codegen")
                }
                ArgKind::TypeOnly(_) => {
                    todo!("Type arguments are not yet supported in WASM codegen")
                }
            }
        }

        let param_count = local_idx;
        let has_return_value = is_array_return || !results.is_empty();

        #[allow(clippy::cast_possible_truncation)]
        let type_idx = self.types.len() as u32;
        self.types.push((params, results));
        self.functions.push(type_idx);

        if is_array_return {
            self.has_memory = true;
        }

        let is_main = fn_name == "main";
        let should_export = vis == Visibility::Public && !is_main;
        if should_export {
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }
        if is_main && vis == Visibility::Public {
            self.has_main = true;
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }

        Self::pre_scan_locals(arena, body_id, ctx, &mut self.locals_map, &mut local_idx);

        self.frame_layout = Self::compute_frame_layout(arena, body_id, ctx, local_idx, &args);

        if self.frame_layout.is_some() {
            self.has_memory = true;
        }

        let mut local_declarations: Vec<(u32, ValType)> = {
            let mut sorted_locals: Vec<(u32, ValType)> = self
                .locals_map
                .values()
                .copied()
                .filter(|(idx, _)| *idx >= param_count)
                .collect();
            sorted_locals.sort_by_key(|(idx, _)| *idx);
            sorted_locals.into_iter().map(|(_, vt)| (1, vt)).collect()
        };

        if self.frame_layout.is_some() {
            local_declarations.push((1, ValType::I32));
        }

        self.func = Some(Function::new(local_declarations));

        if let (Some(layout), Some(func)) = (&self.frame_layout, &mut self.func) {
            emit_stack_prologue(func, layout);

            // Copy-on-entry: for each array-typed parameter, copy the caller's data
            // into the callee's frame to enforce value semantics.
            for arg in &args {
                if let ArgKind::Named { name, .. } = &arg.kind {
                    let arg_name = arena[*name].name.clone();
                    let arg_type_info = {
                        let ty_id = match &arg.kind {
                            ArgKind::Named { ty, .. } => *ty,
                            _ => unreachable!(),
                        };
                        TypeInfo::from_type_id(arena, ty_id)
                    };
                    if let TypeInfoKind::Array(elem_type, _length) = &arg_type_info.kind {
                        let param_local = self
                            .locals_map
                            .get(&arg_name)
                            .expect("Array parameter must be in locals_map")
                            .0;
                        let slot = layout
                            .array_offsets
                            .get(&arg_name)
                            .expect("Array parameter must have a frame slot");
                        emit_array_param_copy(func, layout, slot, param_local, &elem_type.kind);
                    }
                }
            }
        }

        // Lower the function body
        let block = &arena[body_id];
        let body_stmts: Vec<StmtId> = block.stmts.clone();
        for stmt_id in body_stmts {
            self.lower_statement(arena, stmt_id, ctx);
        }

        if has_return_value {
            if let (Some(layout), Some(func)) = (&self.frame_layout, &mut self.func) {
                emit_stack_epilogue(func, layout);
            }
            self.func().instruction(&Instruction::Unreachable);
        } else if let (Some(layout), Some(func)) = (&self.frame_layout, &mut self.func) {
            emit_stack_epilogue(func, layout);
        }

        self.func().instruction(&Instruction::End);

        self.func_names.push((self.func_idx, fn_name.clone()));
        let mut local_name_entries: Vec<(u32, String)> = self
            .locals_map
            .iter()
            .map(|(name, (idx, _))| (*idx, name.clone()))
            .collect();
        if let Some(ref layout) = self.frame_layout {
            local_name_entries.push((layout.frame_ptr_local, "__frame_ptr".to_string()));
        }
        local_name_entries.sort_by_key(|(idx, _)| *idx);
        if !local_name_entries.is_empty() {
            self.local_names.push((self.func_idx, local_name_entries));
        }

        let completed_func = self
            .func
            .take()
            .expect("func must be Some after compilation");
        self.bodies.push(completed_func);
        self.frame_layout = None;
        self.locals_map.clear();
        self.loop_ctx = LoopContext::default();
        self.parent_blocks_stack.clear();
        self.func_idx += 1;
    }

    /// Pre-scans the function body to discover all local variable declarations.
    fn pre_scan_locals(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        locals_map: &mut FxHashMap<String, (u32, ValType)>,
        local_idx: &mut u32,
    ) {
        let block = &arena[block_id];
        for &stmt_id in &block.stmts {
            match &arena[stmt_id].kind {
                Stmt::ConstDef(const_def_id) => {
                    if let Def::Constant { name, .. } = &arena[*const_def_id].kind {
                        let const_name = arena[*name].name.clone();
                        let val_type = match ctx
                            .get_node_typeinfo(NodeId::Def(*const_def_id))
                            .expect("Constant definition must have a type info")
                            .kind
                        {
                            TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => ValType::I64,
                            _ => ValType::I32,
                        };
                        let prev = locals_map.insert(const_name.clone(), (*local_idx, val_type));
                        assert!(
                            prev.is_none(),
                            "local `{const_name}` collides with an existing entry in locals_map; \
                             the type-checker should have rejected shadowing",
                        );
                        *local_idx += 1;
                    }
                }
                Stmt::VarDef { name, .. } => {
                    let var_name = arena[*name].name.clone();
                    let val_type = match ctx
                        .get_node_typeinfo(NodeId::Stmt(stmt_id))
                        .expect("Variable definition must have type info")
                        .kind
                    {
                        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => ValType::I64,
                        _ => ValType::I32,
                    };
                    let prev = locals_map.insert(var_name.clone(), (*local_idx, val_type));
                    assert!(
                        prev.is_none(),
                        "local `{var_name}` collides with an existing entry in locals_map; \
                         the type-checker should have rejected shadowing",
                    );
                    *local_idx += 1;
                }
                Stmt::Block(inner_block_id) => {
                    Self::pre_scan_locals(arena, *inner_block_id, ctx, locals_map, local_idx);
                }
                Stmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    Self::pre_scan_locals(arena, *then_block, ctx, locals_map, local_idx);
                    if let Some(else_id) = else_block {
                        Self::pre_scan_locals(arena, *else_id, ctx, locals_map, local_idx);
                    }
                }
                Stmt::Loop { body, .. } => {
                    Self::pre_scan_locals(arena, *body, ctx, locals_map, local_idx);
                }
                _ => {}
            }
        }
    }

    /// Computes the stack frame layout for a function.
    fn compute_frame_layout(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        frame_ptr_local_idx: u32,
        args: &[inference_ast::nodes::ArgData],
    ) -> Option<FrameLayout> {
        let mut array_offsets = FxHashMap::default();
        let mut struct_offsets = FxHashMap::default();
        let mut current_offset: u32 = 0;

        for arg in args {
            if let ArgKind::Named { name, ty, .. } = &arg.kind {
                let type_info = TypeInfo::from_type_id(arena, *ty);
                match &type_info.kind {
                    TypeInfoKind::Array(elem_type, length) => {
                        let elem_sz = element_size(&elem_type.kind);
                        let byte_count = elem_sz.checked_mul(*length).expect(
                            "Array byte count overflow: element size * length exceeds u32::MAX",
                        );
                        let aligned_offset = align_to(current_offset, elem_sz);
                        let slot = ArraySlot {
                            offset: aligned_offset,
                            elem_size: elem_sz,
                            length: *length,
                        };
                        let arg_name = arena[*name].name.clone();
                        array_offsets.insert(arg_name, slot);
                        current_offset = aligned_offset.checked_add(byte_count).expect(
                            "Frame offset overflow: total array allocation exceeds u32::MAX",
                        );
                    }
                    TypeInfoKind::Custom(custom_name) => {
                        if let Some(struct_info) = ctx.lookup_struct(custom_name) {
                            let (total_size, field_slots) =
                                compute_struct_field_layout(&struct_info);
                            if total_size > 0 {
                                let max_field_align = field_slots
                                    .iter()
                                    .map(|f| element_size(&f.type_kind))
                                    .max()
                                    .unwrap_or(1);
                                let aligned_offset = align_to(current_offset, max_field_align);
                                let slot = StructSlot {
                                    offset: aligned_offset,
                                    total_size,
                                    fields: field_slots,
                                };
                                let arg_name = arena[*name].name.clone();
                                struct_offsets.insert(arg_name, slot);
                                current_offset =
                                    aligned_offset.checked_add(total_size).expect(
                                        "Frame offset overflow: struct allocation exceeds u32::MAX",
                                    );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Self::collect_compound_slots(
            arena,
            block_id,
            ctx,
            &mut array_offsets,
            &mut struct_offsets,
            &mut current_offset,
        );

        if current_offset == 0 {
            return None;
        }

        let total_size = align_to_frame(current_offset);
        assert!(
            total_size <= STACK_SIZE,
            "Frame size ({total_size} bytes) exceeds available stack memory ({STACK_SIZE} bytes)"
        );

        Some(FrameLayout {
            total_size,
            array_offsets,
            struct_offsets,
            frame_ptr_local: frame_ptr_local_idx,
        })
    }

    /// Recursively walks a block collecting array and struct variable declarations.
    fn collect_compound_slots(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        array_offsets: &mut FxHashMap<String, ArraySlot>,
        struct_offsets: &mut FxHashMap<String, StructSlot>,
        current_offset: &mut u32,
    ) {
        let block = &arena[block_id];
        for &stmt_id in &block.stmts {
            match &arena[stmt_id].kind {
                Stmt::VarDef { name, .. } => {
                    let type_info = ctx
                        .get_node_typeinfo(NodeId::Stmt(stmt_id))
                        .expect("Variable definition must have type info");
                    match &type_info.kind {
                        TypeInfoKind::Array(elem_type, length) => {
                            let elem_sz = element_size(&elem_type.kind);
                            let byte_count = elem_sz.checked_mul(*length).expect(
                                "Array byte count overflow: element size * length exceeds u32::MAX",
                            );
                            let aligned_offset = align_to(*current_offset, elem_sz);
                            let slot = ArraySlot {
                                offset: aligned_offset,
                                elem_size: elem_sz,
                                length: *length,
                            };
                            let var_name = arena[*name].name.clone();
                            array_offsets.insert(var_name, slot);
                            *current_offset = aligned_offset.checked_add(byte_count).expect(
                                "Frame offset overflow: total array allocation exceeds u32::MAX",
                            );
                        }
                        TypeInfoKind::Struct(struct_name) => {
                            if let Some(struct_info) = ctx.lookup_struct(struct_name) {
                                let (total_size, field_slots) =
                                    compute_struct_field_layout(&struct_info);
                                if total_size > 0 {
                                    let max_field_align = field_slots
                                        .iter()
                                        .map(|f| element_size(&f.type_kind))
                                        .max()
                                        .unwrap_or(1);
                                    let aligned_offset =
                                        align_to(*current_offset, max_field_align);
                                    let slot = StructSlot {
                                        offset: aligned_offset,
                                        total_size,
                                        fields: field_slots,
                                    };
                                    let var_name = arena[*name].name.clone();
                                    struct_offsets.insert(var_name, slot);
                                    *current_offset =
                                        aligned_offset.checked_add(total_size).expect(
                                            "Frame offset overflow: struct allocation exceeds u32::MAX",
                                        );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Stmt::Block(inner_block_id) => {
                    Self::collect_compound_slots(
                        arena,
                        *inner_block_id,
                        ctx,
                        array_offsets,
                        struct_offsets,
                        current_offset,
                    );
                }
                Stmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    Self::collect_compound_slots(
                        arena,
                        *then_block,
                        ctx,
                        array_offsets,
                        struct_offsets,
                        current_offset,
                    );
                    if let Some(else_id) = else_block {
                        Self::collect_compound_slots(
                            arena,
                            *else_id,
                            ctx,
                            array_offsets,
                            struct_offsets,
                            current_offset,
                        );
                    }
                }
                Stmt::Loop { body, .. } => {
                    Self::collect_compound_slots(
                        arena, *body, ctx, array_offsets, struct_offsets, current_offset,
                    );
                }
                _ => {}
            }
        }
    }

    /// Lowers an AST statement to WASM instructions.
    #[allow(clippy::too_many_lines)]
    fn lower_statement(&mut self, arena: &AstArena, stmt_id: StmtId, ctx: &TypedContext) {
        let stmt_kind = arena[stmt_id].kind.clone();
        match stmt_kind {
            Stmt::Block(block_id) => {
                self.lower_block(arena, block_id, ctx);
            }
            Stmt::Expr(expr_id) => {
                // The type checker rejects standalone calls to array-returning
                // functions, so this path should be unreachable.
                if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind
                    && let Expr::Identifier(callee_name_id) = &arena[*function].kind
                {
                    let callee_name = &arena[*callee_name_id].name;
                    assert!(
                        !self.func_array_returns.contains_key(callee_name),
                        "standalone call to array-returning function should have been rejected by the type checker",
                    );
                }
                self.lower_expression(arena, expr_id, ctx, None);
                let expr_produces_value = ctx
                    .get_node_typeinfo(NodeId::Expr(expr_id))
                    .is_some_and(|ti| !matches!(ti.kind, TypeInfoKind::Unit));
                if expr_produces_value {
                    self.func().instruction(&Instruction::Drop);
                }
            }
            Stmt::Assign { left, right } => {
                self.lower_assign_statement(arena, left, right, ctx);
            }
            Stmt::Return { expr } => {
                let sret_local = self.locals_map.get("sret").map(|(idx, _)| *idx);
                if let Some(sret_idx) = sret_local {
                    if let Err(e) = self.lower_sret_return(arena, expr, sret_idx, ctx) {
                        panic!("sret return lowering failed: {e}");
                    }
                } else {
                    self.lower_expression(arena, expr, ctx, None);
                }
                if let (Some(layout), Some(func)) = (&self.frame_layout, &mut self.func) {
                    emit_stack_epilogue(func, layout);
                }
                self.func().instruction(&Instruction::Return);
            }
            Stmt::Loop { condition, body } => {
                self.lower_loop_statement(arena, condition, body, ctx);
            }
            Stmt::Break => {
                cov_mark::hit!(wasm_codegen_emit_break);
                let exit_depth = self
                    .loop_ctx
                    .loop_exit_depths
                    .last()
                    .copied()
                    .expect("break outside loop -- should be caught by analysis pass");
                let br_depth = self.loop_ctx.wasm_block_depth - exit_depth - 1;
                self.func().instruction(&Instruction::Br(br_depth));
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                self.lower_if_statement(arena, condition, then_block, else_block, ctx);
            }
            Stmt::VarDef { name, value, .. } => {
                cov_mark::hit!(wasm_codegen_emit_variable_definition);
                let var_name = arena[name].name.clone();
                let (local_idx, _) = self
                    .locals_map
                    .get(&var_name)
                    .expect("Variable local not found in pre-scan");
                match value {
                    None => todo!("Uninitialized variable definitions are not supported"),
                    Some(val_expr_id) => {
                        let local_idx = *local_idx;

                        let var_type_info = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id));
                        let is_array_type = matches!(
                            var_type_info.as_ref().map(|ti| &ti.kind),
                            Some(TypeInfoKind::Array(_, _))
                        );

                        // Detect sret call
                        let is_sret_call =
                            is_array_type && self.is_sret_function_call(arena, val_expr_id);

                        // Detect array-to-array copy
                        let is_array_copy =
                            is_array_type && matches!(arena[val_expr_id].kind, Expr::Identifier(_));

                        if is_sret_call {
                            self.lower_sret_var_init(arena, val_expr_id, local_idx, &var_name, ctx);
                        } else if is_array_copy {
                            cov_mark::hit!(wasm_codegen_emit_array_copy);
                            self.lower_array_copy_var_init(
                                arena,
                                val_expr_id,
                                local_idx,
                                &var_name,
                                ctx,
                            );
                        } else {
                            self.lower_expression(arena, val_expr_id, ctx, Some(&var_name));
                            self.func().instruction(&Instruction::LocalSet(local_idx));
                        }
                    }
                }
            }
            Stmt::TypeDef { .. } => todo!(),
            Stmt::Assert { .. } => todo!(),
            Stmt::ConstDef(const_def_id) => {
                cov_mark::hit!(wasm_codegen_emit_constant_definition);
                if let Def::Constant { name, value, .. } = &arena[const_def_id].kind {
                    let const_name = arena[*name].name.clone();
                    let value = *value;
                    self.lower_expression(arena, value, ctx, None);
                    let (local_idx, _) = self
                        .locals_map
                        .get(&const_name)
                        .expect("Local not found in pre-scan");
                    let local_idx = *local_idx;
                    self.func().instruction(&Instruction::LocalSet(local_idx));
                }
            }
        }
    }

    /// Checks whether an expression is a function call to an sret function.
    fn is_sret_function_call(&self, arena: &AstArena, expr_id: ExprId) -> bool {
        if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind
            && let Expr::Identifier(callee_name_id) = &arena[*function].kind
        {
            let callee_name = &arena[*callee_name_id].name;
            return self.func_array_returns.contains_key(callee_name);
        }
        false
    }

    /// Lowers sret function call initialization for a variable definition.
    fn lower_sret_var_init(
        &mut self,
        arena: &AstArena,
        val_expr_id: ExprId,
        local_idx: u32,
        var_name: &str,
        ctx: &TypedContext,
    ) {
        let layout = self
            .frame_layout
            .as_ref()
            .expect("Array variable requires frame layout");
        let dest_slot = layout
            .array_offsets
            .get(var_name)
            .expect("Destination array not in frame layout");
        let dest_offset = dest_slot.offset;
        let frame_ptr_local = layout.frame_ptr_local;

        if let Expr::FunctionCall { function, args, .. } = &arena[val_expr_id].kind {
            let function = *function;
            let args: Vec<_> = args.iter().map(|(l, e)| (*l, *e)).collect();
            // Push sret pointer: frame_ptr + dest_slot.offset
            self.func()
                .instruction(&Instruction::LocalGet(frame_ptr_local));
            if dest_offset > 0 {
                #[allow(clippy::cast_possible_wrap)]
                self.func()
                    .instruction(&Instruction::I32Const(dest_offset as i32));
                self.func().instruction(&Instruction::I32Add);
            }
            // Push regular arguments
            for (_label, arg_expr_id) in &args {
                self.lower_expression(arena, *arg_expr_id, ctx, None);
            }
            let callee_name = self
                .resolve_callee_name(arena, function)
                .expect("sret callee must be an identifier");
            let func_idx = self
                .func_name_to_idx
                .get(&callee_name)
                .copied()
                .expect("sret callee must be in func_name_to_idx");
            self.func().instruction(&Instruction::Call(func_idx));
        }

        // Set local to point to destination slot
        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if dest_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(dest_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
        self.func().instruction(&Instruction::LocalSet(local_idx));
    }

    /// Lowers array copy initialization for a variable definition.
    fn lower_array_copy_var_init(
        &mut self,
        arena: &AstArena,
        val_expr_id: ExprId,
        local_idx: u32,
        var_name: &str,
        ctx: &TypedContext,
    ) {
        let layout = self
            .frame_layout
            .as_ref()
            .expect("Array variable requires frame layout");
        let dest_slot = layout
            .array_offsets
            .get(var_name)
            .expect("Destination array not in frame layout");
        let byte_size = dest_slot.elem_size * dest_slot.length;
        let dest_offset = dest_slot.offset;
        let frame_ptr_local = layout.frame_ptr_local;

        // dest = frame_ptr + dest_slot.offset
        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if dest_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(dest_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
        // src = lower_expression(identifier) -> source pointer
        self.lower_expression(arena, val_expr_id, ctx, None);
        // byte count
        #[allow(clippy::cast_possible_wrap)]
        self.func()
            .instruction(&Instruction::I32Const(byte_size as i32));
        self.func().instruction(&Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        });

        // Set local to point to destination slot
        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if dest_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(dest_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
        self.func().instruction(&Instruction::LocalSet(local_idx));
    }

    /// Lowers a block (regular or non-det) to WASM instructions.
    fn lower_block(&mut self, arena: &AstArena, block_id: BlockId, ctx: &TypedContext) {
        let block = &arena[block_id];
        let opcode = match block.block_kind {
            BlockKind::Forall => Some(FORALL_OPCODE),
            BlockKind::Exists => Some(EXISTS_OPCODE),
            BlockKind::Assume => Some(ASSUME_OPCODE),
            BlockKind::Unique => Some(UNIQUE_OPCODE),
            BlockKind::Regular => None,
        };

        if let Some(op) = opcode {
            match block.block_kind {
                BlockKind::Forall => cov_mark::hit!(wasm_codegen_emit_forall_block),
                BlockKind::Exists => cov_mark::hit!(wasm_codegen_emit_exists_block),
                BlockKind::Assume => cov_mark::hit!(wasm_codegen_emit_assume_block),
                BlockKind::Unique => cov_mark::hit!(wasm_codegen_emit_unique_block),
                BlockKind::Regular => unreachable!(),
            }
            self.emit_nondet_block_start(op);
            self.loop_ctx.wasm_block_depth += 1;
        }

        let stmts = block.stmts.clone();
        for stmt_id in stmts {
            self.lower_statement(arena, stmt_id, ctx);
        }

        if opcode.is_some() {
            self.loop_ctx.wasm_block_depth -= 1;
            self.emit_nondet_block_end();
        }
    }

    /// Lowers an AST expression to WASM instructions on the operand stack.
    #[allow(clippy::too_many_lines)]
    fn lower_expression(
        &mut self,
        arena: &AstArena,
        expr_id: ExprId,
        ctx: &TypedContext,
        enclosing_var_name: Option<&str>,
    ) {
        let expr_kind = arena[expr_id].kind.clone();
        match expr_kind {
            Expr::ArrayIndexAccess { array, index } => {
                self.lower_array_index_access(arena, expr_id, array, index, ctx);
            }
            Expr::Binary { left, right, op } => {
                self.lower_binary_expression(arena, expr_id, left, right, op, ctx);
            }
            Expr::MemberAccess { expr, name } => {
                self.lower_member_access(arena, expr_id, expr, name, ctx);
            }
            Expr::TypeMemberAccess { .. } => todo!(),
            Expr::FunctionCall { function, args, .. } => {
                let args: Vec<_> = args.iter().map(|(l, e)| (*l, *e)).collect();
                match self.lower_function_call(arena, function, &args, ctx) {
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
                    Err(e) => panic!("function call lowering failed: {e}"),
                }
            }
            Expr::StructLiteral { name: _, fields } => {
                cov_mark::hit!(wasm_codegen_emit_struct_literal);
                let var_name = enclosing_var_name.unwrap_or_else(|| {
                    panic!(
                        "Struct literal (expr_id={expr_id:?}) has no enclosing variable name"
                    )
                });
                let fields: Vec<_> = fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                self.lower_struct_literal(arena, &fields, var_name, ctx);
            }
            Expr::PrefixUnary { expr, op } => {
                self.lower_prefix_unary_expression(arena, expr_id, expr, op, ctx);
            }
            Expr::Parenthesized { expr } => {
                cov_mark::hit!(wasm_codegen_emit_parenthesized_expression);
                self.lower_expression(arena, expr, ctx, enclosing_var_name);
            }
            Expr::ArrayLiteral { ref elements } => {
                cov_mark::hit!(wasm_codegen_emit_array_literal);
                let var_name = enclosing_var_name.unwrap_or_else(|| {
                    panic!("Array literal (expr_id={expr_id:?}) has no enclosing variable name")
                });
                let elements = elements.clone();
                self.lower_array_literal(arena, &elements, var_name, ctx);
            }
            Expr::BoolLiteral { value } => {
                self.func()
                    .instruction(&Instruction::I32Const(i32::from(value)));
            }
            Expr::StringLiteral { .. } => todo!(),
            Expr::NumberLiteral { ref value } => {
                let value = value.clone();
                self.lower_number_literal(expr_id, &value, ctx);
            }
            Expr::UnitLiteral => todo!(),
            Expr::Identifier(ident_id) => {
                let name = &arena[ident_id].name;
                let (local_idx, _) = self.locals_map.get(name).expect("Variable not found");
                let local_idx = *local_idx;
                self.func().instruction(&Instruction::LocalGet(local_idx));
            }
            Expr::Type(_) => todo!(),
            Expr::Uzumaki => {
                let node_id = NodeId::Expr(expr_id);
                if let Some(type_info) = ctx.get_node_typeinfo(node_id)
                    && let TypeInfoKind::Array(ref elem_type, length) = type_info.kind
                {
                    cov_mark::hit!(wasm_codegen_emit_array_uzumaki);
                    let var_name = enclosing_var_name.unwrap_or_else(|| {
                        panic!("Array uzumaki (expr_id={expr_id:?}) has no enclosing variable name")
                    });
                    self.lower_array_uzumaki(arena, elem_type, length, var_name);
                    return;
                }
                if ctx.is_node_i32(node_id) {
                    cov_mark::hit!(wasm_codegen_emit_uzumaki_i32);
                    self.emit_uzumaki(UZUMAKI_I32_OPCODE);
                    return;
                }
                if ctx.is_node_i64(node_id) {
                    cov_mark::hit!(wasm_codegen_emit_uzumaki_i64);
                    self.emit_uzumaki(UZUMAKI_I64_OPCODE);
                    return;
                }
                panic!("Unsupported Uzumaki expression type");
            }
        }
    }

    /// Resolves the callee name from a function expression.
    #[allow(clippy::unused_self)]
    fn resolve_callee_name(&self, arena: &AstArena, function_expr_id: ExprId) -> Option<String> {
        if let Expr::Identifier(ident_id) = &arena[function_expr_id].kind {
            Some(arena[*ident_id].name.clone())
        } else {
            None
        }
    }

    /// Lowers a plain identifier-based function call to a WASM `call` instruction.
    fn lower_function_call(
        &mut self,
        arena: &AstArena,
        function_expr_id: ExprId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        let func_name = self
            .resolve_callee_name(arena, function_expr_id)
            .ok_or(CodegenError::UnsupportedCalleeKind)?;

        cov_mark::hit!(wasm_codegen_emit_function_call);

        let args_copy: Vec<_> = call_args.iter().map(|(l, e)| (*l, *e)).collect();
        for (_label, arg_expr_id) in &args_copy {
            self.lower_expression(arena, *arg_expr_id, ctx, None);
        }

        let func_idx = self
            .func_name_to_idx
            .get(&func_name)
            .copied()
            .ok_or(CodegenError::UnknownFunction(func_name))?;

        self.func().instruction(&Instruction::Call(func_idx));
        Ok(())
    }

    /// Lowers an assignment statement.
    fn lower_assign_statement(
        &mut self,
        arena: &AstArena,
        left: ExprId,
        right: ExprId,
        ctx: &TypedContext,
    ) {
        match &arena[left].kind {
            Expr::Identifier(ident_id) => {
                cov_mark::hit!(wasm_codegen_emit_assign_identifier);
                let name = &arena[*ident_id].name;
                let (local_idx, _) = self
                    .locals_map
                    .get(name)
                    .expect("Assignment target variable not found");
                let local_idx = *local_idx;
                self.lower_expression(arena, right, ctx, None);
                self.func().instruction(&Instruction::LocalSet(local_idx));
            }
            Expr::ArrayIndexAccess { array, index } => {
                let array = *array;
                let index = *index;
                self.lower_array_index_write(arena, left, array, index, right, ctx);
            }
            Expr::MemberAccess { expr, name } => {
                let expr = *expr;
                let name = *name;
                self.lower_member_access_write(arena, expr, name, right, ctx);
            }
            _ => todo!("Assignment to non-identifier targets not yet supported"),
        }
    }

    /// Lowers the return expression in an sret function.
    fn lower_sret_return(
        &mut self,
        arena: &AstArena,
        return_expr_id: ExprId,
        sret_idx: u32,
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        let return_info = self
            .func_array_returns
            .get(&self.current_fn_name)
            .expect("sret function must have ArrayReturnInfo");
        let elem_size = return_info.elem_size;
        let byte_size = return_info.elem_size * return_info.length;
        let store_instr = memory::store_instruction(&return_info.elem_kind);

        match &arena[return_expr_id].kind {
            Expr::Identifier(ident_id) => {
                let name = &arena[*ident_id].name;
                let (source_local, _) = self
                    .locals_map
                    .get(name)
                    .expect("Return identifier not found in locals_map");
                let source_local = *source_local;
                emit_sret_copy(self.func(), sret_idx, source_local, byte_size);
            }
            Expr::ArrayLiteral { elements } => {
                let elements = elements.clone();
                for (i, element_id) in elements.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    let byte_offset = (i as u32) * elem_size;
                    emit_sret_element_addr(self.func(), sret_idx, byte_offset);
                    self.lower_expression(arena, *element_id, ctx, None);
                    self.func().instruction(&store_instr);
                }
            }
            Expr::FunctionCall { function, args, .. } => {
                let function = *function;
                let args: Vec<_> = args.iter().map(|(l, e)| (*l, *e)).collect();
                let callee_name = self
                    .resolve_callee_name(arena, function)
                    .ok_or(CodegenError::UnsupportedSretReturnExpression)?;
                if self.func_array_returns.contains_key(&callee_name) {
                    // Zero-copy sret forwarding
                    self.func().instruction(&Instruction::LocalGet(sret_idx));
                    for (_label, arg_expr_id) in &args {
                        self.lower_expression(arena, *arg_expr_id, ctx, None);
                    }
                    let func_idx = self
                        .func_name_to_idx
                        .get(&callee_name)
                        .copied()
                        .expect("Forwarded sret callee must be in func_name_to_idx");
                    self.func().instruction(&Instruction::Call(func_idx));
                } else {
                    return Err(CodegenError::UnsupportedSretReturnExpression);
                }
            }
            _ => {
                return Err(CodegenError::UnsupportedSretReturnExpression);
            }
        }

        Ok(())
    }

    /// Lowers an array index write (`arr[i] = value`).
    fn lower_array_index_write(
        &mut self,
        arena: &AstArena,
        aiae_expr_id: ExprId,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        right_expr_id: ExprId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_array_index_write);

        let elem_type_info = ctx
            .get_node_typeinfo(NodeId::Expr(aiae_expr_id))
            .expect("ArrayIndexAccess must have type info (element type)");
        let elem_sz = memory::element_size(&elem_type_info.kind);
        let store_instr = memory::store_instruction(&elem_type_info.kind);

        self.lower_expression(arena, array_expr_id, ctx, None);
        self.emit_index_offset(arena, index_expr_id, elem_sz, ctx);
        self.lower_expression(arena, right_expr_id, ctx, None);

        self.func().instruction(&store_instr);
    }

    /// Lowers an `if`/`else` statement to WASM structured control flow.
    fn lower_if_statement(
        &mut self,
        arena: &AstArena,
        condition: ExprId,
        then_block: BlockId,
        else_block: Option<BlockId>,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_if_statement);

        self.lower_expression(arena, condition, ctx, None);
        self.func()
            .instruction(&Instruction::If(WasmBlockType::Empty));
        self.loop_ctx.wasm_block_depth += 1;

        let then_stmts = arena[then_block].stmts.clone();
        for stmt_id in then_stmts {
            self.lower_statement(arena, stmt_id, ctx);
        }

        if let Some(else_id) = else_block {
            cov_mark::hit!(wasm_codegen_emit_if_with_else);
            self.func().instruction(&Instruction::Else);
            let else_stmts = arena[else_id].stmts.clone();
            for stmt_id in else_stmts {
                self.lower_statement(arena, stmt_id, ctx);
            }
        }

        self.loop_ctx.wasm_block_depth -= 1;
        self.func().instruction(&Instruction::End);
    }

    /// Lowers a loop statement to WASM block+loop structured control flow.
    fn lower_loop_statement(
        &mut self,
        arena: &AstArena,
        condition: Option<ExprId>,
        body: BlockId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_loop_statement);

        self.loop_ctx
            .loop_exit_depths
            .push(self.loop_ctx.wasm_block_depth);

        self.func()
            .instruction(&Instruction::Block(WasmBlockType::Empty));
        self.func()
            .instruction(&Instruction::Loop(WasmBlockType::Empty));
        self.loop_ctx.wasm_block_depth += 2;

        if let Some(cond_expr_id) = condition {
            cov_mark::hit!(wasm_codegen_emit_loop_conditional);
            self.lower_expression(arena, cond_expr_id, ctx, None);
            self.func().instruction(&Instruction::I32Eqz);
            self.func().instruction(&Instruction::BrIf(1));
        } else {
            cov_mark::hit!(wasm_codegen_emit_loop_infinite);
        }

        let body_stmts = arena[body].stmts.clone();
        for stmt_id in body_stmts {
            self.lower_statement(arena, stmt_id, ctx);
        }

        self.func().instruction(&Instruction::Br(0));

        self.loop_ctx.wasm_block_depth -= 2;
        self.func().instruction(&Instruction::End);
        self.func().instruction(&Instruction::End);

        self.loop_ctx.loop_exit_depths.pop();
    }

    fn is_unsigned_type(kind: &TypeInfoKind) -> bool {
        matches!(
            kind,
            TypeInfoKind::Number(
                NumberType::U8 | NumberType::U16 | NumberType::U32 | NumberType::U64
            )
        )
    }

    fn is_i64_type(kind: &TypeInfoKind) -> bool {
        matches!(
            kind,
            TypeInfoKind::Number(NumberType::I64 | NumberType::U64)
        )
    }

    /// Lowers an array index access expression (`arr[i]`) to WASM load instructions.
    fn lower_array_index_access(
        &mut self,
        arena: &AstArena,
        aiae_expr_id: ExprId,
        array_expr_id: ExprId,
        index_expr_id: ExprId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_array_index_read);

        let elem_type_info = ctx
            .get_node_typeinfo(NodeId::Expr(aiae_expr_id))
            .expect("ArrayIndexAccess must have type info (element type)");
        let elem_sz = memory::element_size(&elem_type_info.kind);
        let load_instr = memory::load_instruction(&elem_type_info.kind);

        self.lower_expression(arena, array_expr_id, ctx, None);
        self.emit_index_offset(arena, index_expr_id, elem_sz, ctx);

        self.func().instruction(&load_instr);
    }

    /// Emits the byte-offset computation for an array index expression.
    fn emit_index_offset(
        &mut self,
        arena: &AstArena,
        index_expr_id: ExprId,
        elem_sz: u32,
        ctx: &TypedContext,
    ) {
        if let Some(byte_offset) = try_const_index_byte_offset(arena, index_expr_id, elem_sz) {
            if byte_offset != 0 {
                self.func().instruction(&Instruction::I32Const(byte_offset));
                self.func().instruction(&Instruction::I32Add);
            }
        } else {
            self.lower_expression(arena, index_expr_id, ctx, None);
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(elem_sz as i32));
            self.func().instruction(&Instruction::I32Mul);
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Lowers an array-typed uzumaki expression to element-wise non-deterministic stores.
    fn lower_array_uzumaki(
        &mut self,
        _arena: &AstArena,
        elem_type: &TypeInfo,
        length: u32,
        enclosing_var_name: &str,
    ) {
        let parent_var_name = enclosing_var_name;

        let layout = self
            .frame_layout
            .as_ref()
            .expect("Array uzumaki requires a frame layout (function must have arrays)");

        let slot = layout
            .array_offsets
            .get(parent_var_name)
            .unwrap_or_else(|| {
                panic!("Array variable '{parent_var_name}' not found in frame layout offsets")
            });

        let uzumaki_opcode = if Self::is_i64_type(&elem_type.kind) {
            UZUMAKI_I64_OPCODE
        } else {
            UZUMAKI_I32_OPCODE
        };

        let store_instr = memory::store_instruction_from_slot(slot);
        let slot_offset = slot.offset;
        let slot_elem_size = slot.elem_size;
        let frame_ptr_local = layout.frame_ptr_local;

        for i in 0..length {
            #[allow(clippy::cast_possible_wrap)]
            let byte_offset = (slot_offset + i * slot_elem_size) as i32;
            self.func()
                .instruction(&Instruction::LocalGet(frame_ptr_local));
            self.func().instruction(&Instruction::I32Const(byte_offset));
            self.func().instruction(&Instruction::I32Add);
            self.emit_uzumaki(uzumaki_opcode);
            self.func().instruction(&store_instr);
        }

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Lowers a binary expression to WASM stack instructions.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::needless_pass_by_value)]
    fn lower_binary_expression(
        &mut self,
        arena: &AstArena,
        _expr_id: ExprId,
        left: ExprId,
        right: ExprId,
        op: OperatorKind,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_binary_expression);

        self.lower_expression(arena, left, ctx, None);
        self.lower_expression(arena, right, ctx, None);

        let left_type_info = ctx
            .get_node_typeinfo(NodeId::Expr(left))
            .expect("Binary expression left operand must have type info");
        let is_i64 = Self::is_i64_type(&left_type_info.kind);
        let is_unsigned = Self::is_unsigned_type(&left_type_info.kind);

        let instruction = match op {
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
                    "Power operator (`**`) deferred -- no native WASM instruction; \
                     see .claude/plans/codegen/new-pow-operator/master_plan.md"
                )
            }
        };

        self.func().instruction(&instruction);

        if !matches!(
            op,
            OperatorKind::Eq
                | OperatorKind::Ne
                | OperatorKind::Lt
                | OperatorKind::Le
                | OperatorKind::Gt
                | OperatorKind::Ge
                | OperatorKind::Mod
                | OperatorKind::And
                | OperatorKind::Or
                | OperatorKind::Shr
        ) {
            memory::emit_sub_i32_narrowing(self.func(), &left_type_info.kind);
        }
    }

    /// Lowers a prefix unary expression to WASM stack instructions.
    #[allow(clippy::needless_pass_by_value)]
    fn lower_prefix_unary_expression(
        &mut self,
        arena: &AstArena,
        pue_expr_id: ExprId,
        inner_expr_id: ExprId,
        op: UnaryOperatorKind,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_prefix_unary_expression);

        let type_info = ctx
            .get_node_typeinfo(NodeId::Expr(pue_expr_id))
            .expect("Prefix unary expression must have type info");
        let is_i64 = Self::is_i64_type(&type_info.kind);
        let kind = type_info.kind.clone();

        match op {
            UnaryOperatorKind::Neg => {
                cov_mark::hit!(wasm_codegen_emit_unary_neg);
                if is_i64 {
                    self.func().instruction(&Instruction::I64Const(0));
                } else {
                    self.func().instruction(&Instruction::I32Const(0));
                }
                self.lower_expression(arena, inner_expr_id, ctx, None);
                if is_i64 {
                    self.func().instruction(&Instruction::I64Sub);
                } else {
                    self.func().instruction(&Instruction::I32Sub);
                    memory::emit_sub_i32_narrowing(self.func(), &kind);
                }
            }
            UnaryOperatorKind::Not => {
                cov_mark::hit!(wasm_codegen_emit_unary_not);
                self.lower_expression(arena, inner_expr_id, ctx, None);
                self.func().instruction(&Instruction::I32Eqz);
            }
            UnaryOperatorKind::BitNot => {
                cov_mark::hit!(wasm_codegen_emit_unary_bitnot);
                self.lower_expression(arena, inner_expr_id, ctx, None);
                if is_i64 {
                    self.func().instruction(&Instruction::I64Const(-1));
                    self.func().instruction(&Instruction::I64Xor);
                } else {
                    self.func().instruction(&Instruction::I32Const(-1));
                    self.func().instruction(&Instruction::I32Xor);
                    memory::emit_sub_i32_narrowing(self.func(), &kind);
                }
            }
        }
    }

    /// Lowers a number literal to WASM constant instructions.
    fn lower_number_literal(&mut self, expr_id: ExprId, value: &str, ctx: &TypedContext) {
        let type_info = ctx
            .get_node_typeinfo(NodeId::Expr(expr_id))
            .expect("Number literal must have type info");
        match type_info.kind {
            TypeInfoKind::Number(NumberType::I8 | NumberType::I16 | NumberType::I32) => {
                let val = value
                    .parse::<i32>()
                    .expect("Failed to parse signed 32-bit integer literal");
                self.func().instruction(&Instruction::I32Const(val));
            }
            TypeInfoKind::Number(NumberType::U8) => {
                let val = i32::from(
                    value
                        .parse::<u8>()
                        .expect("Failed to parse unsigned 8-bit integer literal"),
                );
                self.func().instruction(&Instruction::I32Const(val));
            }
            TypeInfoKind::Number(NumberType::U16) => {
                let val = i32::from(
                    value
                        .parse::<u16>()
                        .expect("Failed to parse unsigned 16-bit integer literal"),
                );
                self.func().instruction(&Instruction::I32Const(val));
            }
            TypeInfoKind::Number(NumberType::U32) => {
                let val = value
                    .parse::<u32>()
                    .expect("Failed to parse unsigned 32-bit integer literal")
                    .cast_signed();
                self.func().instruction(&Instruction::I32Const(val));
            }
            TypeInfoKind::Number(NumberType::I64) => {
                let val = value
                    .parse::<i64>()
                    .expect("Failed to parse signed 64-bit integer literal");
                self.func().instruction(&Instruction::I64Const(val));
            }
            TypeInfoKind::Number(NumberType::U64) => {
                let val = value
                    .parse::<u64>()
                    .expect("Failed to parse unsigned 64-bit integer literal")
                    .cast_signed();
                self.func().instruction(&Instruction::I64Const(val));
            }
            _ => panic!("Unsupported number literal type: {:?}", type_info.kind),
        }
    }

    /// Lowers an array literal expression.
    fn lower_array_literal(
        &mut self,
        arena: &AstArena,
        elements: &[ExprId],
        enclosing_var_name: &str,
        ctx: &TypedContext,
    ) {
        let parent_var_name = enclosing_var_name;

        let Some(ref layout) = self.frame_layout else {
            self.func().instruction(&Instruction::I32Const(0));
            return;
        };

        let slot = layout
            .array_offsets
            .get(parent_var_name)
            .unwrap_or_else(|| {
                panic!("Array variable '{parent_var_name}' not found in frame layout offsets")
            });

        let slot_length = slot.length;
        let slot_offset = slot.offset;
        let slot_elem_size = slot.elem_size;
        let store_instr = memory::store_instruction_from_slot(slot);
        let frame_ptr_local = layout.frame_ptr_local;

        if slot_length == 0 {
            self.func()
                .instruction(&Instruction::LocalGet(frame_ptr_local));
            if slot_offset > 0 {
                #[allow(clippy::cast_possible_wrap)]
                self.func()
                    .instruction(&Instruction::I32Const(slot_offset as i32));
                self.func().instruction(&Instruction::I32Add);
            }
            return;
        }

        for (i, &element_id) in elements.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let byte_offset = slot_offset + (i as u32) * slot_elem_size;
            self.func()
                .instruction(&Instruction::LocalGet(frame_ptr_local));
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(byte_offset as i32));
            self.func().instruction(&Instruction::I32Add);
            self.lower_expression(arena, element_id, ctx, None);
            self.func().instruction(&store_instr);
        }

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Lowers a struct literal expression to field-by-field stores into the frame slot.
    ///
    /// For each field in the literal, emits:
    /// ```text
    /// local.get $__frame_ptr
    /// i32.const <struct_offset + field_offset>
    /// i32.add
    /// <lower field value expression>
    /// <store instruction for field type>
    /// ```
    ///
    /// After all fields are stored, pushes the struct pointer onto the stack:
    /// ```text
    /// local.get $__frame_ptr
    /// i32.const <struct_offset>
    /// i32.add
    /// ```
    fn lower_struct_literal(
        &mut self,
        arena: &AstArena,
        fields: &[(IdentId, ExprId)],
        enclosing_var_name: &str,
        ctx: &TypedContext,
    ) {
        let Some(ref layout) = self.frame_layout else {
            self.func().instruction(&Instruction::I32Const(0));
            return;
        };

        let slot = layout
            .struct_offsets
            .get(enclosing_var_name)
            .unwrap_or_else(|| {
                panic!(
                    "Struct variable '{enclosing_var_name}' not found in frame layout struct_offsets"
                )
            });

        let slot_offset = slot.offset;
        let frame_ptr_local = layout.frame_ptr_local;
        let field_slots = slot.fields.clone();

        for &(field_name_id, field_value_expr_id) in fields {
            let field_name = &arena[field_name_id].name;
            let field_slot = field_slots
                .iter()
                .find(|fs| fs.name == *field_name)
                .unwrap_or_else(|| {
                    panic!(
                        "Struct field '{field_name}' not found in layout for variable \
                         '{enclosing_var_name}'"
                    )
                });

            #[allow(clippy::cast_possible_wrap)]
            let byte_offset = (slot_offset + field_slot.offset) as i32;
            let store_instr = memory::store_instruction(&field_slot.type_kind);

            self.func()
                .instruction(&Instruction::LocalGet(frame_ptr_local));
            self.func().instruction(&Instruction::I32Const(byte_offset));
            self.func().instruction(&Instruction::I32Add);
            self.lower_expression(arena, field_value_expr_id, ctx, None);
            self.func().instruction(&store_instr);
        }

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Lowers a member access expression (e.g., `p.x`) to a load from struct pointer + field offset.
    ///
    /// The generated WASM code:
    /// 1. Evaluates the struct expression (pushes the struct base pointer)
    /// 2. Adds the field's byte offset
    /// 3. Emits the appropriate load instruction for the field type
    ///
    /// ```text
    /// <lower expr>           ;; struct pointer
    /// i32.const <field_offset>
    /// i32.add
    /// <load instruction>     ;; load field value
    /// ```
    fn lower_member_access(
        &mut self,
        arena: &AstArena,
        _member_access_expr_id: ExprId,
        struct_expr_id: ExprId,
        field_name_id: IdentId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_member_access_read);

        let struct_type = ctx
            .get_node_typeinfo(NodeId::Expr(struct_expr_id))
            .expect("MemberAccess struct expression must have type info");

        let struct_name = match &struct_type.kind {
            TypeInfoKind::Struct(name) => name.clone(),
            _ => panic!(
                "MemberAccess struct expression has non-struct type: {:?}",
                struct_type.kind
            ),
        };

        let struct_info = ctx
            .lookup_struct(&struct_name)
            .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));

        let field_name = &arena[field_name_id].name;
        let (_, field_slots) = compute_struct_field_layout(&struct_info);
        let field_slot = field_slots
            .iter()
            .find(|fs| fs.name == *field_name)
            .unwrap_or_else(|| {
                panic!("Field '{field_name}' not found in struct '{struct_name}' layout")
            });

        let field_offset = field_slot.offset;
        let load_instr = memory::load_instruction(&field_slot.type_kind);

        self.lower_expression(arena, struct_expr_id, ctx, None);

        if field_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(field_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }

        self.func().instruction(&load_instr);
    }

    /// Lowers a member access write (e.g., `p.x = 42`) to a store at struct pointer + field offset.
    ///
    /// The generated WASM code:
    /// 1. Evaluates the struct expression (pushes the struct base pointer)
    /// 2. Adds the field's byte offset
    /// 3. Evaluates the RHS value expression
    /// 4. Emits the appropriate store instruction for the field type
    ///
    /// ```text
    /// <lower expr>           ;; struct pointer
    /// i32.const <field_offset>
    /// i32.add
    /// <lower RHS>            ;; value to store
    /// <store instruction>    ;; store field value
    /// ```
    fn lower_member_access_write(
        &mut self,
        arena: &AstArena,
        struct_expr_id: ExprId,
        field_name_id: IdentId,
        right_expr_id: ExprId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_member_access_write);

        let struct_type = ctx
            .get_node_typeinfo(NodeId::Expr(struct_expr_id))
            .expect("MemberAccess write: struct expression must have type info");

        let struct_name = match &struct_type.kind {
            TypeInfoKind::Struct(name) => name.clone(),
            _ => panic!(
                "MemberAccess write: struct expression has non-struct type: {:?}",
                struct_type.kind
            ),
        };

        let struct_info = ctx
            .lookup_struct(&struct_name)
            .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));

        let field_name = &arena[field_name_id].name;
        let (_, field_slots) = compute_struct_field_layout(&struct_info);
        let field_slot = field_slots
            .iter()
            .find(|fs| fs.name == *field_name)
            .unwrap_or_else(|| {
                panic!("Field '{field_name}' not found in struct '{struct_name}' layout")
            });

        let field_offset = field_slot.offset;
        let store_instr = memory::store_instruction(&field_slot.type_kind);

        self.lower_expression(arena, struct_expr_id, ctx, None);

        if field_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(field_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }

        self.lower_expression(arena, right_expr_id, ctx, None);

        self.func().instruction(&store_instr);
    }

    fn emit_nondet_block_start(&mut self, opcode: u8) {
        self.func().raw([OPCODE_PREFIX, opcode, BLOCK_TYPE_VOID]);
    }

    fn emit_nondet_block_end(&mut self) {
        self.func().raw([END_OPCODE]);
    }

    fn emit_uzumaki(&mut self, opcode: u8) {
        self.func().raw([OPCODE_PREFIX, opcode]);
    }

    pub(crate) fn has_main(&self) -> bool {
        self.has_main
    }

    #[cfg(test)]
    pub(crate) fn enable_memory(&mut self) {
        self.has_memory = true;
    }

    /// Assembles the complete WASM binary from accumulated sections.
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

        if self.has_memory {
            cov_mark::hit!(wasm_codegen_emit_memory_section);
            let mut memory_section = MemorySection::new();
            memory_section.memory(MemoryType {
                minimum: 1,
                maximum: Some(1),
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memory_section);
        }

        if self.has_memory {
            let mut global_section = GlobalSection::new();
            global_section.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &ConstExpr::i32_const(STACK_POINTER_INIT),
            );
            module.section(&global_section);
        }

        let has_func_exports = !self.exports.is_empty();
        if has_func_exports || self.has_memory {
            let mut export_section = ExportSection::new();
            for (name, kind, idx) in &self.exports {
                export_section.export(name, *kind, *idx);
            }
            if self.has_memory {
                export_section.export("memory", ExportKind::Memory, 0);
                export_section.export("__stack_pointer", ExportKind::Global, 0);
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

/// Returns the pre-computed byte offset when `index_expr` is a constant number literal.
fn try_const_index_byte_offset(
    arena: &AstArena,
    index_expr_id: ExprId,
    elem_sz: u32,
) -> Option<i32> {
    if let Expr::NumberLiteral { ref value } = arena[index_expr_id].kind {
        let index_val = value.parse::<i32>().ok()?;
        #[allow(clippy::cast_possible_wrap)]
        let byte_offset = index_val.wrapping_mul(elem_sz as i32);
        Some(byte_offset)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_without_memory_omits_memory_section() {
        let compiler = Compiler::new("test");
        let wasm = compiler.finish();
        assert!(!wasm.is_empty());
        assert!(!has_memory_section(&wasm));
    }

    #[test]
    fn finish_with_memory_includes_memory_section() {
        cov_mark::check!(wasm_codegen_emit_memory_section);
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let wasm = compiler.finish();
        assert!(has_memory_section(&wasm));
    }

    #[test]
    fn finish_with_memory_validates_via_wasmparser() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let wasm = compiler.finish();
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("Generated WASM with memory is invalid: {e}"));
    }

    #[test]
    fn finish_with_memory_exports_memory_and_stack_pointer() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let wasm = compiler.finish();
        let wat =
            wasmprinter::print_bytes(&wasm).unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("(export \"memory\""),
            "WAT must export memory:\n{wat}"
        );
        assert!(
            wat.contains("(export \"__stack_pointer\""),
            "WAT must export __stack_pointer:\n{wat}"
        );
    }

    #[test]
    fn finish_with_memory_has_correct_stack_pointer_init() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let wasm = compiler.finish();
        let wat =
            wasmprinter::print_bytes(&wasm).unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("i32.const 65536"),
            "Stack pointer must be initialized to 65536 (one page):\n{wat}"
        );
    }

    #[test]
    fn finish_with_memory_has_mutable_global() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let wasm = compiler.finish();
        let wat =
            wasmprinter::print_bytes(&wasm).unwrap_or_else(|e| panic!("Failed to print WAT: {e}"));
        assert!(
            wat.contains("(mut i32)"),
            "Stack pointer global must be mutable i32:\n{wat}"
        );
    }

    #[test]
    fn enable_memory_is_sticky() {
        let mut compiler = Compiler::new("test");
        assert!(!compiler.has_memory);
        compiler.enable_memory();
        assert!(compiler.has_memory);
    }

    fn has_memory_section(wasm: &[u8]) -> bool {
        let mut pos = 8;
        while pos < wasm.len() {
            let section_id = wasm[pos];
            pos += 1;
            let (size, consumed) = read_leb128_u32(&wasm[pos..]);
            pos += consumed;
            if section_id == 5 {
                return true;
            }
            pos += size as usize;
        }
        false
    }

    fn read_leb128_u32(bytes: &[u8]) -> (u32, usize) {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;
        for (i, &byte) in bytes.iter().enumerate() {
            result |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return (result, i + 1);
            }
            shift += 7;
        }
        (result, bytes.len())
    }
}
