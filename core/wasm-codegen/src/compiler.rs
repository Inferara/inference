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
    self, ArraySlot, CompoundFieldLayout, FrameLayout, MEMORY_INDEX, STACK_POINTER_INIT,
    STACK_SIZE, StructSlot, align_to, align_to_frame, compute_struct_field_layout,
    emit_array_param_copy, emit_ptr_offset_addr, emit_sret_copy, emit_stack_epilogue,
    emit_stack_prologue, emit_struct_param_copy, natural_alignment_for_type, type_byte_size,
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

/// Maximum number of scalar elements that uzumaki unrolling will emit
/// instructions for. Each element produces ~5 WASM instructions (load
/// frame pointer, compute offset, add, uzumaki, store), so 65 536
/// elements = 327 680 instructions -- a reasonable upper bound before
/// instruction explosion becomes a concern.
const MAX_UZUMAKI_UNROLL_ELEMENTS: u32 = 65_536;

/// Separator used in mangled method names: `"{StructName}.{method_name}"`.
///
/// Dot is used because it matches Zig's convention and is standard across
/// the WASM ecosystem. Since `.` is a syntax token in Inference (member
/// access), it cannot appear in user-defined identifiers, making collisions
/// impossible without any additional validation.
const METHOD_SEPARATOR: &str = ".";

/// Recurses through `Array(elem, _)` until it finds the leaf (non-array) scalar type.
fn leaf_scalar_type(kind: &TypeInfoKind) -> &TypeInfoKind {
    match kind {
        TypeInfoKind::Array(inner, _) => leaf_scalar_type(&inner.kind),
        other => other,
    }
}

/// Multiplies all dimension lengths together to get the total number of leaf scalars.
///
/// For `[[[i32; 2]; 3]; 4]` this returns `2 * 3 * 4 = 24`.
fn total_leaf_count(kind: &TypeInfoKind, length: u32) -> u32 {
    match kind {
        TypeInfoKind::Array(inner, inner_len) => {
            let sub_count = total_leaf_count(&inner.kind, *inner_len);
            length.checked_mul(sub_count).expect(
                "total_leaf_count overflow: product of all dimension lengths exceeds u32::MAX",
            )
        }
        _ => length,
    }
}

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

/// Metadata about a function that returns a struct type.
///
/// Populated during `build_func_name_to_idx` so that callers and callees
/// know the sret calling convention parameters at code generation time.
#[derive(Debug, Clone)]
struct StructReturnInfo {
    total_size: u32,
    field_slots: Vec<memory::StructFieldSlot>,
}

/// Result of resolving a struct field's offset and type during member access.
///
/// Produced by [`Compiler::resolve_struct_field_offset`] to provide
/// self-documenting field access at call sites.
struct ResolvedField {
    offset: u32,
    type_kind: TypeInfoKind,
    layout: memory::CompoundFieldLayout,
}

/// Resolved callee of a `FunctionCall` expression.
///
/// Produced by [`Compiler::resolve_function_callee`] to consolidate the
/// three-way callee pattern (`Identifier`, `TypeMemberAccess`, `MemberAccess`)
/// that appears across multiple codegen methods.
enum ResolvedCallee {
    /// Plain function call via `Expr::Identifier`.
    Function(String),
    /// Associated function call via `Expr::TypeMemberAccess` (e.g., `Point::new()`).
    AssociatedFunction {
        mangled_name: String,
        type_expr_id: ExprId,
        method_name_id: IdentId,
    },
    /// Instance method call via `Expr::MemberAccess` (e.g., `p.translate()`).
    InstanceMethod {
        mangled_name: String,
        receiver_expr_id: ExprId,
        method_name_id: IdentId,
    },
}

impl ResolvedCallee {
    /// Returns the resolved WASM function name regardless of variant.
    fn callee_name(&self) -> &str {
        match self {
            Self::Function(name) => name,
            Self::AssociatedFunction { mangled_name, .. }
            | Self::InstanceMethod { mangled_name, .. } => mangled_name,
        }
    }
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
    /// Maps function names to their struct return type metadata.
    func_struct_returns: FxHashMap<String, StructReturnInfo>,
    /// Maps `(type_name, method_name)` to the mangled WASM function name.
    method_mangled_names: FxHashMap<(String, String), String>,
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
            func_struct_returns: FxHashMap::default(),
            method_mangled_names: FxHashMap::default(),
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

    /// Returns the WASM function index that the first method will occupy,
    /// given the number of top-level functions that precede it.
    pub(crate) fn func_idx_after_toplevel(&self, toplevel_count: u32) -> u32 {
        self.func_idx + toplevel_count
    }

    /// Returns the number of registered function/method name-to-index mappings.
    ///
    /// Used by traversal code to verify Stage 1 registration produced the expected
    /// number of entries before any body compilation begins.
    #[cfg(debug_assertions)]
    pub(crate) fn registered_function_count(&self) -> usize {
        self.func_name_to_idx.len()
    }

    /// Registers sret metadata for a function that returns a compound type (array or struct).
    ///
    /// If the return type is an array, inserts into `func_array_returns`.
    /// If the return type is a custom (struct), computes the field layout and inserts
    /// into `func_struct_returns`. Otherwise does nothing.
    fn register_sret_if_compound(
        &mut self,
        name: String,
        return_ty_id: TypeId,
        arena: &AstArena,
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        let return_type_info = TypeInfo::from_type_id(arena, return_ty_id);
        match &return_type_info.kind {
            TypeInfoKind::Array(elem_type, length) => {
                let elem_sz = type_byte_size(&elem_type.kind, ctx)?;
                self.func_array_returns.insert(
                    name,
                    ArrayReturnInfo {
                        elem_kind: elem_type.kind.clone(),
                        elem_size: elem_sz,
                        length: *length,
                    },
                );
            }
            TypeInfoKind::Custom(custom_name) => {
                if let Some(struct_info) = ctx.lookup_struct(custom_name) {
                    let (total_size, field_slots) = compute_struct_field_layout(&struct_info, ctx)?;
                    self.func_struct_returns.insert(
                        name,
                        StructReturnInfo {
                            total_size,
                            field_slots,
                        },
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Builds the function name-to-WASM-index map from the source file's function definitions.
    ///
    /// Must be called before `visit_function_definition` so that forward references
    /// resolve correctly during call lowering.
    pub(crate) fn build_func_name_to_idx(
        &mut self,
        arena: &AstArena,
        func_def_ids: &[DefId],
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        #[allow(clippy::cast_possible_truncation)]
        for (idx, &def_id) in func_def_ids.iter().enumerate() {
            let fn_name = arena.def_name(def_id).to_string();
            self.func_name_to_idx
                .insert(fn_name.clone(), idx as u32 + self.func_idx);

            if let Def::Function { returns, .. } = &arena[def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(fn_name, *return_ty_id, arena, ctx)?;
            }
        }
        Ok(())
    }

    /// Builds the method name-to-WASM-index map from struct definitions.
    ///
    /// For each struct that has methods, this function:
    /// 1. Computes a mangled name `"{struct_name}.{method_name}"` for each method
    /// 2. Inserts the mangled name into `func_name_to_idx` with the next available index
    /// 3. Records the `(struct_name, method_name) -> mangled_name` mapping
    /// 4. Detects sret return types and populates `func_array_returns`/`func_struct_returns`
    ///
    /// Must be called after `build_func_name_to_idx` so that method indices follow
    /// top-level function indices. Must be called before any body compilation so that
    /// forward references (methods calling functions and vice versa) resolve correctly.
    pub(crate) fn build_method_name_to_idx(
        &mut self,
        arena: &AstArena,
        method_defs: &[(String, DefId)],
        ctx: &TypedContext,
        base_idx: u32,
    ) -> Result<(), CodegenError> {
        #[allow(clippy::cast_possible_truncation)]
        for (i, (struct_name, def_id)) in method_defs.iter().enumerate() {
            let method_name = arena.def_name(*def_id).to_string();
            let mangled_name = format!("{struct_name}{METHOD_SEPARATOR}{method_name}");

            assert!(
                !self.func_name_to_idx.contains_key(&mangled_name),
                "Mangled method name '{mangled_name}' collides with an existing function; \
                 top-level functions must not use the `TypeName.method_name` naming pattern"
            );
            self.func_name_to_idx
                .insert(mangled_name.clone(), base_idx + i as u32);
            self.method_mangled_names
                .insert((struct_name.clone(), method_name), mangled_name.clone());

            if let Def::Function { returns, .. } = &arena[*def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(mangled_name, *return_ty_id, arena, ctx)?;
            }
        }
        Ok(())
    }

    /// Maps an Inference type to the corresponding WASM `ValType`.
    ///
    /// Returns `None` for unit types because unit functions produce no WASM value.
    /// Struct types (identified via `TypeNode::Custom` resolved against `TypedContext`)
    /// are represented as `ValType::I32` pointers into linear memory.
    fn val_type_from_type_id(
        arena: &AstArena,
        ty_id: TypeId,
        ctx: &TypedContext,
    ) -> Option<ValType> {
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
            TypeNode::Custom(ident_id) => {
                let name = &arena[*ident_id].name;
                if ctx.lookup_struct(name).is_some() || ctx.lookup_enum(name).is_some() {
                    Some(ValType::I32)
                } else {
                    todo!("Unsupported custom type in WASM codegen: {name}")
                }
            }
        }
    }

    /// Translates an AST function definition to a WASM function body.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn visit_function_definition(
        &mut self,
        def_id: DefId,
        arena: &AstArena,
        ctx: &TypedContext,
        method_struct_name: Option<&str>,
    ) -> Result<(), CodegenError> {
        let (fn_name_id, vis, args, returns, body_id) = match &arena[def_id].kind {
            Def::Function {
                name,
                vis,
                args,
                returns,
                body,
                ..
            } => (*name, vis.clone(), args.clone(), *returns, *body),
            _ => return Ok(()),
        };

        let raw_name = arena[fn_name_id].name.clone();
        // For methods, use the mangled name for sret lookups, debug names, and current_fn_name.
        // For top-level functions, fn_name == raw_name.
        let fn_name = if let Some(struct_name) = method_struct_name {
            format!("{struct_name}{METHOD_SEPARATOR}{raw_name}")
        } else {
            raw_name
        };
        self.current_fn_name.clone_from(&fn_name);

        let is_array_return = self.func_array_returns.contains_key(&fn_name);
        let is_struct_return = self.func_struct_returns.contains_key(&fn_name);
        let is_sret = is_array_return || is_struct_return;

        let results: Vec<ValType> = if is_sret {
            vec![]
        } else {
            returns
                .and_then(|ty_id| Self::val_type_from_type_id(arena, ty_id, ctx))
                .into_iter()
                .collect()
        };

        let mut params: Vec<ValType> = vec![];
        self.locals_map.clear();
        self.loop_ctx = LoopContext::default();
        self.parent_blocks_stack.clear();
        let mut local_idx: u32 = 0;

        if is_sret {
            params.push(ValType::I32);
            self.locals_map
                .insert("sret".to_string(), (0, ValType::I32));
            local_idx = 1;
        }

        for arg in &args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    cov_mark::hit!(wasm_codegen_emit_function_params);
                    let vt = Self::val_type_from_type_id(arena, *ty, ctx)
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
                    cov_mark::hit!(wasm_codegen_emit_self_param);
                    params.push(ValType::I32);
                    let prev = self
                        .locals_map
                        .insert("self".to_string(), (local_idx, ValType::I32));
                    assert!(
                        prev.is_none(),
                        "parameter `self` collides with an existing entry in locals_map; \
                         the type-checker should have rejected duplicate parameter names",
                    );
                    local_idx += 1;
                    // self is a struct pointer; method body will use memory loads/stores
                    self.has_memory = true;
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
        let has_return_value = is_sret || !results.is_empty();

        #[allow(clippy::cast_possible_truncation)]
        let type_idx = self.types.len() as u32;
        self.types.push((params, results));
        self.functions.push(type_idx);

        if is_sret {
            self.has_memory = true;
        }

        let is_method = method_struct_name.is_some();
        let is_main = fn_name == "main";
        // Methods are not exported as WASM exports. A future `export` keyword will
        // control which functions are exported. For now, only top-level `pub` functions
        // (except `main`, which gets special handling) become WASM exports.
        let should_export = vis == Visibility::Public && !is_main && !is_method;
        if should_export {
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }
        if is_main && vis == Visibility::Public && !is_method {
            self.has_main = true;
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }

        Self::pre_scan_locals(arena, body_id, ctx, &mut self.locals_map, &mut local_idx);

        self.frame_layout =
            Self::compute_frame_layout(arena, body_id, ctx, local_idx, &args, method_struct_name)?;

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

            // Copy-on-entry: for each compound-typed parameter (array, struct, or mut self),
            // copy the caller's data into the callee's frame to enforce value semantics.
            for arg in &args {
                match &arg.kind {
                    ArgKind::Named { name, .. } => {
                        let arg_name = arena[*name].name.clone();
                        let arg_type_info = {
                            let ty_id = match &arg.kind {
                                ArgKind::Named { ty, .. } => *ty,
                                _ => unreachable!(),
                            };
                            TypeInfo::from_type_id(arena, ty_id)
                        };
                        let param_local = self
                            .locals_map
                            .get(&arg_name)
                            .expect("Compound parameter must be in locals_map")
                            .0;
                        match &arg_type_info.kind {
                            TypeInfoKind::Array(elem_type, _length) => {
                                let slot = layout
                                    .array_offsets
                                    .get(&arg_name)
                                    .expect("Array parameter must have a frame slot");
                                emit_array_param_copy(
                                    func,
                                    layout,
                                    slot,
                                    param_local,
                                    &elem_type.kind,
                                );
                            }
                            // Custom: unresolved AST type (params/returns); Struct: resolved type (body variables via TypedContext)
                            TypeInfoKind::Custom(_) => {
                                if let Some(slot) = layout.struct_offsets.get(&arg_name) {
                                    emit_struct_param_copy(func, layout, slot, param_local);
                                }
                            }
                            _ => {}
                        }
                    }
                    ArgKind::SelfRef { is_mut: true } => {
                        cov_mark::hit!(wasm_codegen_emit_self_copy_on_entry);
                        if let Some(slot) = layout.struct_offsets.get("self") {
                            let self_local = self
                                .locals_map
                                .get("self")
                                .expect("`self` must be in locals_map for mut self method")
                                .0;
                            emit_struct_param_copy(func, layout, slot, self_local);
                        }
                    }
                    _ => {}
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
            // All non-void paths exit via explicit `return` which emits its own epilogue.
            // The trailing epilogue would be dead code. Keep only `unreachable` so that WASM
            // validators accept the implicit `end` (unreachable is polymorphic on the stack).
            // Precondition: analysis rule A007 guarantees all non-void functions return on
            // every path. Without that guarantee, this site could be the only stack-pointer
            // restoration on a missing-return path.
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
        Ok(())
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
                            // Explicit: enums are i32 tags; keep visible if the catch-all changes.
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
                        // Explicit: enums are i32 tags; keep visible if the catch-all changes.
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
    ///
    /// The `method_struct_name` parameter should be `Some("TypeName")` when compiling
    /// a method body, so that `ArgKind::SelfRef { is_mut: true }` can look up the
    /// struct layout and allocate a frame slot for the mutable `self` copy.
    #[allow(clippy::too_many_lines)]
    fn compute_frame_layout(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        frame_ptr_local_idx: u32,
        args: &[inference_ast::nodes::ArgData],
        method_struct_name: Option<&str>,
    ) -> Result<Option<FrameLayout>, CodegenError> {
        let mut array_offsets = FxHashMap::default();
        let mut struct_offsets = FxHashMap::default();
        let mut current_offset: u32 = 0;

        for arg in args {
            match &arg.kind {
                ArgKind::Named { name, ty, .. } => {
                    let type_info = TypeInfo::from_type_id(arena, *ty);
                    match &type_info.kind {
                        TypeInfoKind::Array(elem_type, length) => {
                            let elem_sz = type_byte_size(&elem_type.kind, ctx)?;
                            let byte_count = elem_sz.checked_mul(*length).expect(
                                "Array byte count overflow: element size * length exceeds u32::MAX",
                            );
                            let align = natural_alignment_for_type(&elem_type.kind, ctx)?;
                            let aligned_offset = align_to(current_offset, align);
                            let element_layout =
                                compute_element_layout_if_struct(&elem_type.kind, ctx)?;
                            let slot = ArraySlot {
                                offset: aligned_offset,
                                elem_size: elem_sz,
                                length: *length,
                                element_layout,
                            };
                            let arg_name = arena[*name].name.clone();
                            array_offsets.insert(arg_name, slot);
                            current_offset = aligned_offset.checked_add(byte_count).expect(
                                "Frame offset overflow: total array allocation exceeds u32::MAX",
                            );
                        }
                        // Custom: unresolved AST type (params/returns); Struct: resolved type (body variables via TypedContext)
                        TypeInfoKind::Custom(custom_name) => {
                            if let Some(struct_info) = ctx.lookup_struct(custom_name) {
                                let (total_size, field_slots) =
                                    compute_struct_field_layout(&struct_info, ctx)?;
                                if total_size > 0 {
                                    let max_field_align =
                                        memory::max_struct_alignment(&field_slots, ctx)?;
                                    let aligned_offset = align_to(current_offset, max_field_align);
                                    let slot = StructSlot {
                                        offset: aligned_offset,
                                        total_size,
                                        fields: field_slots,
                                    };
                                    let arg_name = arena[*name].name.clone();
                                    struct_offsets.insert(arg_name, slot);
                                    current_offset = aligned_offset.checked_add(total_size).expect(
                                        "Frame offset overflow: struct allocation exceeds u32::MAX",
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
                ArgKind::SelfRef { is_mut } if *is_mut => {
                    let struct_name = method_struct_name.expect(
                        "ArgKind::SelfRef encountered but no method_struct_name provided; \
                         this indicates a bug in traverse_t_ast_with_compiler",
                    );
                    if let Some(struct_info) = ctx.lookup_struct(struct_name) {
                        let (total_size, field_slots) =
                            compute_struct_field_layout(&struct_info, ctx)?;
                        if total_size > 0 {
                            let max_field_align = memory::max_struct_alignment(&field_slots, ctx)?;
                            let aligned_offset = align_to(current_offset, max_field_align);
                            let slot = StructSlot {
                                offset: aligned_offset,
                                total_size,
                                fields: field_slots,
                            };
                            struct_offsets.insert("self".to_string(), slot);
                            current_offset = aligned_offset.checked_add(total_size).expect(
                                "Frame offset overflow: struct allocation exceeds u32::MAX",
                            );
                        }
                    }
                }
                // Immutable self or non-self args: no frame slot needed
                _ => {}
            }
        }

        Self::collect_compound_slots(
            arena,
            block_id,
            ctx,
            &mut array_offsets,
            &mut struct_offsets,
            &mut current_offset,
        )?;

        if current_offset == 0 {
            return Ok(None);
        }

        let total_size = align_to_frame(current_offset);
        assert!(
            total_size <= STACK_SIZE,
            "Frame size ({total_size} bytes) exceeds available stack memory ({STACK_SIZE} bytes)"
        );

        Ok(Some(FrameLayout {
            total_size,
            array_offsets,
            struct_offsets,
            frame_ptr_local: frame_ptr_local_idx,
        }))
    }

    /// Recursively walks a block collecting array and struct variable declarations.
    ///
    /// Enum types are intentionally excluded — they are pure i32 scalars with no
    /// linear memory footprint, so they do not need frame slots.
    #[allow(clippy::too_many_lines)]
    fn collect_compound_slots(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        array_offsets: &mut FxHashMap<String, ArraySlot>,
        struct_offsets: &mut FxHashMap<String, StructSlot>,
        current_offset: &mut u32,
    ) -> Result<(), CodegenError> {
        let block = &arena[block_id];
        for &stmt_id in &block.stmts {
            match &arena[stmt_id].kind {
                Stmt::VarDef { name, .. } => {
                    let type_info = ctx
                        .get_node_typeinfo(NodeId::Stmt(stmt_id))
                        .expect("Variable definition must have type info");
                    match &type_info.kind {
                        TypeInfoKind::Array(elem_type, length) => {
                            let elem_sz = type_byte_size(&elem_type.kind, ctx)?;
                            let byte_count = elem_sz.checked_mul(*length).expect(
                                "Array byte count overflow: element size * length exceeds u32::MAX",
                            );
                            let align = natural_alignment_for_type(&elem_type.kind, ctx)?;
                            let aligned_offset = align_to(*current_offset, align);
                            let element_layout =
                                compute_element_layout_if_struct(&elem_type.kind, ctx)?;
                            let slot = ArraySlot {
                                offset: aligned_offset,
                                elem_size: elem_sz,
                                length: *length,
                                element_layout,
                            };
                            let var_name = arena[*name].name.clone();
                            array_offsets.insert(var_name, slot);
                            *current_offset = aligned_offset.checked_add(byte_count).expect(
                                "Frame offset overflow: total array allocation exceeds u32::MAX",
                            );
                        }
                        TypeInfoKind::Struct(struct_name) | TypeInfoKind::Custom(struct_name) => {
                            if let Some(struct_info) = ctx.lookup_struct(struct_name) {
                                let (total_size, field_slots) =
                                    compute_struct_field_layout(&struct_info, ctx)?;
                                if total_size > 0 {
                                    let max_field_align =
                                        memory::max_struct_alignment(&field_slots, ctx)?;
                                    let aligned_offset = align_to(*current_offset, max_field_align);
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
                    )?;
                }
                Stmt::If {
                    then_block,
                    else_block,
                    ..
                } => {
                    let saved_offset = *current_offset;
                    Self::collect_compound_slots(
                        arena,
                        *then_block,
                        ctx,
                        array_offsets,
                        struct_offsets,
                        current_offset,
                    )?;
                    let then_end = *current_offset;
                    if let Some(else_id) = else_block {
                        *current_offset = saved_offset;
                        Self::collect_compound_slots(
                            arena,
                            *else_id,
                            ctx,
                            array_offsets,
                            struct_offsets,
                            current_offset,
                        )?;
                        *current_offset = (*current_offset).max(then_end);
                    }
                }
                Stmt::Loop { body, .. } => {
                    Self::collect_compound_slots(
                        arena,
                        *body,
                        ctx,
                        array_offsets,
                        struct_offsets,
                        current_offset,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
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
                // The type checker rejects standalone calls to compound-returning
                // functions/methods, so these paths should be unreachable.
                if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind
                    && let Some(resolved) = self.resolve_function_callee(arena, *function, ctx)
                {
                    let name = resolved.callee_name();
                    assert!(
                        !self.func_array_returns.contains_key(name)
                            && !self.func_struct_returns.contains_key(name),
                        "standalone call to compound-returning function/method '{name}' \
                         should have been rejected by the type checker",
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
                        let is_struct_type = matches!(
                            var_type_info.as_ref().map(|ti| &ti.kind),
                            Some(TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_))
                        ) && self
                            .frame_layout
                            .as_ref()
                            .is_some_and(|layout| layout.struct_offsets.contains_key(&var_name));
                        let is_compound_type = is_array_type || is_struct_type;

                        // Detect sret call (array-returning or struct-returning function/method)
                        let is_sret_call =
                            is_compound_type && self.is_sret_function_call(arena, val_expr_id, ctx);

                        // Detect array-to-array copy
                        let is_array_copy = is_array_type
                            && matches!(
                                arena[val_expr_id].kind,
                                Expr::Identifier(_)
                                    | Expr::ArrayIndexAccess { .. }
                                    | Expr::MemberAccess { .. }
                            );

                        // Detect struct-to-struct copy (from identifier, member access, or array index)
                        let is_struct_copy = is_struct_type
                            && matches!(
                                arena[val_expr_id].kind,
                                Expr::Identifier(_)
                                    | Expr::MemberAccess { .. }
                                    | Expr::ArrayIndexAccess { .. }
                            );

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
                        } else if is_struct_copy {
                            cov_mark::hit!(wasm_codegen_emit_struct_copy);
                            self.lower_struct_copy_var_init(
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

    /// Checks whether an expression is a function call to an sret function (array or struct return).
    fn is_sret_function_call(&self, arena: &AstArena, expr_id: ExprId, ctx: &TypedContext) -> bool {
        if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind
            && let Some(resolved) = self.resolve_function_callee(arena, *function, ctx)
        {
            let name = resolved.callee_name();
            return self.func_array_returns.contains_key(name)
                || self.func_struct_returns.contains_key(name);
        }
        false
    }

    /// Lowers sret function call initialization for a variable definition.
    ///
    /// Works for both array-returning and struct-returning function calls.
    /// Looks up the destination offset from either `array_offsets` or `struct_offsets`
    /// in the frame layout.
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
            .expect("Compound variable requires frame layout");
        let dest_offset = if let Some(array_slot) = layout.array_offsets.get(var_name) {
            array_slot.offset
        } else if let Some(struct_slot) = layout.struct_offsets.get(var_name) {
            struct_slot.offset
        } else {
            panic!(
                "Destination variable '{var_name}' not found in array_offsets or struct_offsets"
            );
        };
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

            let resolved = self
                .resolve_function_callee(arena, function, ctx)
                .expect("sret callee must be an identifier, TypeMemberAccess, or MemberAccess");

            let callee_name = resolved.callee_name().to_owned();
            let receiver_expr = match &resolved {
                ResolvedCallee::InstanceMethod {
                    receiver_expr_id, ..
                } => Some(*receiver_expr_id),
                _ => None,
            };

            if let Some(receiver) = receiver_expr {
                self.lower_expression(arena, receiver, ctx, None);
            }

            // Push regular arguments
            for (_label, arg_expr_id) in &args {
                self.lower_expression(arena, *arg_expr_id, ctx, None);
            }
            let func_idx = self
                .func_name_to_idx
                .get(&callee_name)
                .copied()
                .expect("sret callee must be in func_name_to_idx");
            self.func().instruction(&Instruction::Call(func_idx));
        } else {
            unreachable!("lower_sret_var_init called with non-FunctionCall expression");
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
        let byte_size = dest_slot
            .elem_size
            .checked_mul(dest_slot.length)
            .expect("Array byte size overflow: elem_size * length exceeds u32::MAX");
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
        self.emit_memory_copy(byte_size);

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

    /// Lowers struct copy initialization for a variable definition.
    ///
    /// Emits `memory.copy` from the source struct pointer to the destination
    /// frame slot, then sets the local to point to the destination. This
    /// preserves value semantics: modifying the copy does not affect the original.
    fn lower_struct_copy_var_init(
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
            .expect("Struct variable requires frame layout");
        let dest_slot = layout
            .struct_offsets
            .get(var_name)
            .expect("Destination struct not in frame layout");
        let byte_size = dest_slot.total_size;
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
        self.emit_memory_copy(byte_size);

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
            Expr::TypeMemberAccess {
                expr: type_expr,
                name: variant_name_id,
            } => {
                let type_name = Self::extract_type_name_from_type_expr(arena, type_expr)
                    .expect("TypeMemberAccess: could not extract type name");
                let variant_name = &arena[variant_name_id].name;

                if let Some(enum_info) = ctx.lookup_enum(&type_name) {
                    let tag = enum_info
                        .variant_index(variant_name)
                        .expect("TypeMemberAccess: unknown enum variant");
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let tag_i32 = tag as i32;
                    self.func().instruction(&Instruction::I32Const(tag_i32));
                } else {
                    todo!(
                        "TypeMemberAccess for non-enum type `{type_name}::{variant_name}` \
                         is not yet supported in wasm codegen"
                    );
                }
            }
            Expr::FunctionCall { function, args, .. } => {
                let args: Vec<_> = args.iter().map(|(l, e)| (*l, *e)).collect();
                match self.resolve_function_callee(arena, function, ctx) {
                    Some(ResolvedCallee::InstanceMethod {
                        receiver_expr_id,
                        method_name_id,
                        ..
                    }) => {
                        self.lower_instance_method_call(
                            arena,
                            receiver_expr_id,
                            method_name_id,
                            &args,
                            ctx,
                            None,
                        );
                    }
                    Some(ResolvedCallee::AssociatedFunction {
                        type_expr_id,
                        method_name_id,
                        ..
                    }) => {
                        self.lower_associated_function_call(
                            arena,
                            type_expr_id,
                            method_name_id,
                            &args,
                            ctx,
                            None,
                        );
                    }
                    Some(ResolvedCallee::Function(ref name)) => {
                        match self.lower_function_call(arena, name, &args, ctx) {
                            Ok(()) => {}
                            Err(CodegenError::UnknownFunction(name)) => {
                                panic!(
                                    "Function '{name}' not found in name-to-index map; \
                                     the type-checker should have caught undefined functions"
                                )
                            }
                            Err(e) => panic!("function call lowering failed: {e}"),
                        }
                    }
                    None => {
                        todo!(
                            "Non-identifier function calls (higher-order) \
                             are not yet implemented"
                        )
                    }
                }
            }
            Expr::StructLiteral { name: _, fields } => {
                cov_mark::hit!(wasm_codegen_emit_struct_literal);
                let var_name = enclosing_var_name.unwrap_or_else(|| {
                    unreachable!(
                        "struct literal in unsupported position should have been caught by type checker"
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
                    unreachable!(
                        "array literal in unsupported position should have been caught by type checker"
                    )
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
                let type_info = ctx
                    .get_node_typeinfo(node_id)
                    .expect("Uzumaki expression must have type info");
                match &type_info.kind {
                    TypeInfoKind::Bool
                    | TypeInfoKind::Number(
                        NumberType::I8
                        | NumberType::U8
                        | NumberType::I16
                        | NumberType::U16
                        | NumberType::I32
                        | NumberType::U32,
                    )
                    | TypeInfoKind::Enum(_) => {
                        cov_mark::hit!(wasm_codegen_emit_uzumaki_i32);
                        self.emit_uzumaki(UZUMAKI_I32_OPCODE);
                    }
                    TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => {
                        cov_mark::hit!(wasm_codegen_emit_uzumaki_i64);
                        self.emit_uzumaki(UZUMAKI_I64_OPCODE);
                    }
                    TypeInfoKind::Array(elem_type, length) => {
                        cov_mark::hit!(wasm_codegen_emit_array_uzumaki);
                        let length = *length;
                        let elem_type = elem_type.clone();
                        let var_name = enclosing_var_name.unwrap_or_else(|| {
                            panic!(
                                "Array uzumaki (expr_id={expr_id:?}) has no enclosing variable name"
                            )
                        });
                        if let Err(e) =
                            self.lower_array_uzumaki(arena, &elem_type, length, var_name)
                        {
                            panic!("array uzumaki lowering failed: {e}");
                        }
                    }
                    TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) => {
                        cov_mark::hit!(wasm_codegen_emit_struct_uzumaki);
                        let name = name.clone();
                        let var_name = enclosing_var_name.unwrap_or_else(|| {
                            panic!(
                                "Struct uzumaki (expr_id={expr_id:?}) has no enclosing variable name"
                            )
                        });
                        if let Err(e) = self.lower_struct_uzumaki(ctx, &name, var_name) {
                            panic!("struct uzumaki lowering failed: {e}");
                        }
                    }
                    _ => panic!("Unsupported Uzumaki expression type: {type_info:?}"),
                }
            }
        }
    }

    /// Resolves the callee of a `FunctionCall` expression into a [`ResolvedCallee`].
    ///
    /// Given the `function` expression of a `FunctionCall` AST node, determines
    /// which of the three call patterns it represents and resolves the corresponding
    /// WASM function name. Returns `None` if the expression does not match any
    /// known callee pattern (e.g., higher-order calls).
    fn resolve_function_callee(
        &self,
        arena: &AstArena,
        function_expr_id: ExprId,
        ctx: &TypedContext,
    ) -> Option<ResolvedCallee> {
        match &arena[function_expr_id].kind {
            Expr::Identifier(ident_id) => {
                Some(ResolvedCallee::Function(arena[*ident_id].name.clone()))
            }
            Expr::TypeMemberAccess {
                expr: type_expr,
                name: method_name,
            } => {
                let mangled =
                    self.resolve_associated_mangled_name(arena, *type_expr, *method_name)?;
                Some(ResolvedCallee::AssociatedFunction {
                    mangled_name: mangled,
                    type_expr_id: *type_expr,
                    method_name_id: *method_name,
                })
            }
            Expr::MemberAccess {
                expr: receiver,
                name: method_name,
            } => {
                let mangled =
                    self.resolve_method_mangled_name(arena, *receiver, *method_name, ctx)?;
                Some(ResolvedCallee::InstanceMethod {
                    mangled_name: mangled,
                    receiver_expr_id: *receiver,
                    method_name_id: *method_name,
                })
            }
            _ => None,
        }
    }

    /// Lowers a plain function call to a WASM `call` instruction.
    fn lower_function_call(
        &mut self,
        arena: &AstArena,
        callee_name: &str,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        cov_mark::hit!(wasm_codegen_emit_function_call);

        for (_label, arg_expr_id) in call_args {
            self.lower_expression(arena, *arg_expr_id, ctx, None);
        }

        let func_idx = self
            .func_name_to_idx
            .get(callee_name)
            .copied()
            .ok_or_else(|| CodegenError::UnknownFunction(callee_name.to_owned()))?;

        self.func().instruction(&Instruction::Call(func_idx));
        Ok(())
    }

    /// Resolves the mangled WASM function name for an instance method call.
    ///
    /// Given the receiver expression and method name, determines the receiver's struct type
    /// from the type context and looks up the corresponding mangled name in `method_mangled_names`.
    /// Returns `None` if the receiver has no type info or the method is not registered.
    fn resolve_method_mangled_name(
        &self,
        arena: &AstArena,
        receiver_expr_id: ExprId,
        method_name_id: IdentId,
        ctx: &TypedContext,
    ) -> Option<String> {
        let method_name = &arena[method_name_id].name;
        let receiver_type = ctx.get_node_typeinfo(NodeId::Expr(receiver_expr_id))?;
        let (TypeInfoKind::Struct(struct_name) | TypeInfoKind::Custom(struct_name)) =
            &receiver_type.kind
        else {
            return None;
        };
        self.method_mangled_names
            .get(&(struct_name.clone(), method_name.clone()))
            .cloned()
    }

    /// Lowers an instance method call (`receiver.method(args)`) to WASM instructions.
    ///
    /// Resolves the receiver's struct type, looks up the mangled method name
    /// (`TypeName.method_name`), pushes the receiver as the implicit `self`
    /// argument, then pushes user arguments, and emits `call`.
    ///
    /// When the method returns a compound type (struct or array), the sret calling
    /// convention is used. If `sret_local` is `Some(local_idx)`, the local at that
    /// index holds the sret destination pointer and is pushed as the first WASM
    /// argument before the receiver. If `sret_local` is `None` and the method
    /// returns a compound type, this is a codegen limitation (expression-position
    /// compound-returning method calls require temporary frame allocation).
    fn lower_instance_method_call(
        &mut self,
        arena: &AstArena,
        receiver_expr_id: ExprId,
        method_name_id: IdentId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
        sret_local: Option<u32>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_instance_method_call);

        let mangled_name = self
            .resolve_method_mangled_name(arena, receiver_expr_id, method_name_id, ctx)
            .unwrap_or_else(|| {
                let method_name = &arena[method_name_id].name;
                panic!(
                    "Instance method call: could not resolve mangled name for method \
                     '{method_name}' (receiver has no type info or non-struct type)"
                )
            });

        let is_sret = self.func_array_returns.contains_key(&mangled_name)
            || self.func_struct_returns.contains_key(&mangled_name);

        let func_idx = self
            .func_name_to_idx
            .get(&mangled_name)
            .copied()
            .unwrap_or_else(|| {
                panic!("Mangled method name '{mangled_name}' not found in func_name_to_idx")
            });

        if is_sret {
            cov_mark::hit!(wasm_codegen_emit_instance_method_sret);
            let sret_idx = sret_local.unwrap_or_else(|| {
                panic!(
                    "Instance method call to compound-returning method '{mangled_name}' \
                     in expression position without sret destination. \
                     Compound-returning calls are only supported in variable initialization \
                     and return positions."
                )
            });
            self.func().instruction(&Instruction::LocalGet(sret_idx));
        }

        // Push receiver as the implicit `self` argument
        self.lower_expression(arena, receiver_expr_id, ctx, None);

        // Push user arguments
        for (_label, arg_expr_id) in call_args {
            self.lower_expression(arena, *arg_expr_id, ctx, None);
        }

        self.func().instruction(&Instruction::Call(func_idx));
    }

    /// Extracts the type name from the `expr` part of a `TypeMemberAccess` expression.
    ///
    /// Handles `Expr::Type(TypeId)` with `TypeNode::Custom(ident_id)` and
    /// `Expr::Identifier(ident_id)` patterns, matching the type-checker's resolution logic.
    fn extract_type_name_from_type_expr(arena: &AstArena, type_expr_id: ExprId) -> Option<String> {
        match &arena[type_expr_id].kind {
            Expr::Type(ty_id) => match &arena[*ty_id].kind {
                TypeNode::Custom(ident_id) => Some(arena[*ident_id].name.clone()),
                _ => None,
            },
            Expr::Identifier(ident_id) => Some(arena[*ident_id].name.clone()),
            _ => None,
        }
    }

    /// Resolves the mangled WASM function name for an associated function call (`Type::method()`).
    ///
    /// Extracts the type name from the expression, then looks up the mangled name
    /// in `method_mangled_names`. Returns `None` if the type name cannot be extracted
    /// or the method is not registered.
    fn resolve_associated_mangled_name(
        &self,
        arena: &AstArena,
        type_expr_id: ExprId,
        method_name_id: IdentId,
    ) -> Option<String> {
        let type_name = Self::extract_type_name_from_type_expr(arena, type_expr_id)?;
        let method_name = &arena[method_name_id].name;
        self.method_mangled_names
            .get(&(type_name, method_name.clone()))
            .cloned()
    }

    /// Lowers an associated function call (`Type::method(args)`) to WASM instructions.
    ///
    /// Associated functions have no `self` parameter. The callee is resolved via
    /// the type name and method name, looked up in `method_mangled_names`, and
    /// called with only the user-provided arguments.
    ///
    /// When the method returns a compound type (struct or array), the sret calling
    /// convention is used. If `sret_local` is `Some(local_idx)`, the local at that
    /// index holds the sret destination pointer and is pushed as the first WASM
    /// argument. If `sret_local` is `None` and the method returns a compound type,
    /// this is a codegen limitation (expression-position compound-returning method
    /// calls require temporary frame allocation).
    fn lower_associated_function_call(
        &mut self,
        arena: &AstArena,
        type_expr_id: ExprId,
        method_name_id: IdentId,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
        sret_local: Option<u32>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_associated_function_call);

        let mangled_name = self
            .resolve_associated_mangled_name(arena, type_expr_id, method_name_id)
            .unwrap_or_else(|| {
                let method_name = &arena[method_name_id].name;
                panic!(
                    "Associated function call: could not resolve mangled name for \
                     method '{method_name}' (type expression has no resolvable type name)"
                )
            });

        let is_sret = self.func_array_returns.contains_key(&mangled_name)
            || self.func_struct_returns.contains_key(&mangled_name);

        let func_idx = self
            .func_name_to_idx
            .get(&mangled_name)
            .copied()
            .unwrap_or_else(|| {
                panic!("Mangled method name '{mangled_name}' not found in func_name_to_idx")
            });

        if is_sret {
            cov_mark::hit!(wasm_codegen_emit_associated_function_sret);
            let sret_idx = sret_local.unwrap_or_else(|| {
                panic!(
                    "Associated function call to compound-returning method '{mangled_name}' \
                     in expression position without sret destination. \
                     Compound-returning calls are only supported in variable initialization \
                     and return positions."
                )
            });
            self.func().instruction(&Instruction::LocalGet(sret_idx));
        }

        for (_label, arg_expr_id) in call_args {
            self.lower_expression(arena, *arg_expr_id, ctx, None);
        }

        self.func().instruction(&Instruction::Call(func_idx));
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
                let is_struct_literal = matches!(&arena[right].kind, Expr::StructLiteral { .. });
                let is_struct_type = self
                    .frame_layout
                    .as_ref()
                    .is_some_and(|layout| layout.struct_offsets.contains_key(name));
                let is_array_literal = matches!(&arena[right].kind, Expr::ArrayLiteral { .. });
                let is_array_type = self
                    .frame_layout
                    .as_ref()
                    .is_some_and(|layout| layout.array_offsets.contains_key(name));
                if is_struct_literal {
                    self.lower_expression(arena, right, ctx, Some(name));
                    self.func().instruction(&Instruction::Drop);
                } else if is_struct_type {
                    let layout = self.frame_layout.as_ref().unwrap();
                    let dest_slot = &layout.struct_offsets[name];
                    let byte_size = dest_slot.total_size;
                    // dest = local (already points to frame slot)
                    self.func().instruction(&Instruction::LocalGet(local_idx));
                    // src = RHS expression (struct pointer)
                    self.lower_expression(arena, right, ctx, None);
                    self.emit_memory_copy(byte_size);
                } else if is_array_literal {
                    self.lower_expression(arena, right, ctx, Some(name));
                    self.func().instruction(&Instruction::Drop);
                } else if is_array_type {
                    let layout = self.frame_layout.as_ref().unwrap();
                    let dest_slot = &layout.array_offsets[name];
                    let byte_size = dest_slot
                        .elem_size
                        .checked_mul(dest_slot.length)
                        .expect("Array byte size overflow: elem_size * length exceeds u32::MAX");
                    // dest = local (already points to frame slot)
                    self.func().instruction(&Instruction::LocalGet(local_idx));
                    // src = RHS expression (array pointer)
                    self.lower_expression(arena, right, ctx, None);
                    self.emit_memory_copy(byte_size);
                } else {
                    self.lower_expression(arena, right, ctx, None);
                    self.func().instruction(&Instruction::LocalSet(local_idx));
                }
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

    /// Lowers the return expression in an sret function (array or struct return).
    fn lower_sret_return(
        &mut self,
        arena: &AstArena,
        return_expr_id: ExprId,
        sret_idx: u32,
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        if let Some(return_info) = self.func_array_returns.get(&self.current_fn_name).cloned() {
            self.lower_array_sret_return(arena, return_expr_id, sret_idx, ctx, &return_info)
        } else if let Some(return_info) =
            self.func_struct_returns.get(&self.current_fn_name).cloned()
        {
            self.lower_struct_sret_return(arena, return_expr_id, sret_idx, ctx, &return_info)
        } else {
            panic!(
                "sret function '{}' has neither ArrayReturnInfo nor StructReturnInfo",
                self.current_fn_name
            );
        }
    }

    /// Lowers a return expression in an array-returning sret function.
    ///
    /// For scalar-element arrays, emits per-element stores to the sret pointer.
    /// For struct-element arrays, emits per-element `lower_struct_literal_fields`
    /// or `memory.copy` depending on the expression form.
    fn lower_array_sret_return(
        &mut self,
        arena: &AstArena,
        return_expr_id: ExprId,
        sret_idx: u32,
        ctx: &TypedContext,
        return_info: &ArrayReturnInfo,
    ) -> Result<(), CodegenError> {
        let elem_size = return_info.elem_size;
        let byte_size = return_info
            .elem_size
            .checked_mul(return_info.length)
            .expect("sret return: array byte size overflow");
        let is_struct_element = matches!(
            &return_info.elem_kind,
            TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_)
        );

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
                if is_struct_element {
                    let field_slots = compute_element_layout_if_struct(&return_info.elem_kind, ctx)
                        .expect("sret return: struct layout computation failed")
                        .expect("Struct element must have field layout");
                    self.lower_array_literal_struct_elements(
                        arena,
                        &elements,
                        &field_slots,
                        sret_idx,
                        0,
                        elem_size,
                        ctx,
                        false,
                    );
                } else {
                    let store_instr = memory::store_instruction(&return_info.elem_kind);
                    for (i, element_id) in elements.iter().enumerate() {
                        #[allow(clippy::cast_possible_truncation)]
                        let byte_offset = (i as u32) * elem_size;
                        emit_ptr_offset_addr(self.func(), sret_idx, byte_offset);
                        self.lower_expression(arena, *element_id, ctx, None);
                        self.func().instruction(&store_instr);
                    }
                }
            }
            Expr::FunctionCall { function, args, .. } => {
                self.lower_sret_return_call_forwarding(arena, *function, args, sret_idx, ctx)?;
            }
            Expr::MemberAccess { .. } | Expr::ArrayIndexAccess { .. } => {
                self.func().instruction(&Instruction::LocalGet(sret_idx));
                self.lower_expression(arena, return_expr_id, ctx, None);
                self.emit_memory_copy(byte_size);
            }
            _ => {
                return Err(CodegenError::UnsupportedSretReturnExpression);
            }
        }

        Ok(())
    }

    /// Lowers a return expression in a struct-returning sret function.
    fn lower_struct_sret_return(
        &mut self,
        arena: &AstArena,
        return_expr_id: ExprId,
        sret_idx: u32,
        ctx: &TypedContext,
        return_info: &StructReturnInfo,
    ) -> Result<(), CodegenError> {
        match &arena[return_expr_id].kind {
            Expr::Identifier(ident_id) => {
                let name = &arena[*ident_id].name;
                let (source_local, _) = self
                    .locals_map
                    .get(name)
                    .expect("Return identifier not found in locals_map");
                let source_local = *source_local;
                emit_sret_copy(self.func(), sret_idx, source_local, return_info.total_size);
            }
            Expr::StructLiteral { fields, .. } => {
                let fields: Vec<_> = fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                let field_slots = return_info.field_slots.clone();
                self.lower_struct_literal_fields(
                    arena,
                    &fields,
                    &field_slots,
                    sret_idx,
                    0,
                    ctx,
                    &self.current_fn_name.clone(),
                    0,
                    false,
                );
            }
            Expr::FunctionCall { function, args, .. } => {
                self.lower_sret_return_call_forwarding(arena, *function, args, sret_idx, ctx)?;
            }
            Expr::MemberAccess { .. } | Expr::ArrayIndexAccess { .. } => {
                self.func().instruction(&Instruction::LocalGet(sret_idx));
                self.lower_expression(arena, return_expr_id, ctx, None);
                self.emit_memory_copy(return_info.total_size);
            }
            _ => {
                return Err(CodegenError::UnsupportedSretReturnExpression);
            }
        }

        Ok(())
    }

    /// Forwards the sret pointer to a callee that also uses sret convention.
    fn lower_sret_return_call_forwarding(
        &mut self,
        arena: &AstArena,
        function: ExprId,
        args: &[(Option<IdentId>, ExprId)],
        sret_idx: u32,
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        let args: Vec<_> = args.iter().map(|(l, e)| (*l, *e)).collect();

        let resolved = self
            .resolve_function_callee(arena, function, ctx)
            .ok_or(CodegenError::UnsupportedSretReturnExpression)?;

        let callee_name = resolved.callee_name().to_owned();
        let receiver_expr = match &resolved {
            ResolvedCallee::InstanceMethod {
                receiver_expr_id, ..
            } => Some(*receiver_expr_id),
            _ => None,
        };

        if self.func_array_returns.contains_key(&callee_name)
            || self.func_struct_returns.contains_key(&callee_name)
        {
            self.func().instruction(&Instruction::LocalGet(sret_idx));

            if let Some(receiver) = receiver_expr {
                self.lower_expression(arena, receiver, ctx, None);
            }

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
        Ok(())
    }

    /// Lowers an array index write (`arr[i] = value`).
    ///
    /// For scalar elements, emits a store at `base + index * elem_size`.
    /// For struct elements, emits `memory.copy` from the source struct pointer
    /// to `base + index * struct_size`.
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

        let is_compound_element = matches!(
            &elem_type_info.kind,
            TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_) | TypeInfoKind::Array(_, _)
        );

        if is_compound_element {
            let elem_sz = type_byte_size(&elem_type_info.kind, ctx)
                .expect("array index write: type_byte_size failed for compound element");

            // dest: array_base + index * struct_size
            self.lower_expression(arena, array_expr_id, ctx, None);
            self.emit_index_offset(arena, index_expr_id, elem_sz, ctx);
            // src: RHS expression (struct pointer)
            self.lower_expression(arena, right_expr_id, ctx, None);
            self.emit_memory_copy(elem_sz);
        } else {
            let elem_sz = memory::element_size(&elem_type_info.kind);
            let store_instr = memory::store_instruction(&elem_type_info.kind);

            self.lower_expression(arena, array_expr_id, ctx, None);
            self.emit_index_offset(arena, index_expr_id, elem_sz, ctx);
            self.lower_expression(arena, right_expr_id, ctx, None);

            self.func().instruction(&store_instr);
        }
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

    /// Lowers an array index access expression (`arr[i]`) to WASM instructions.
    ///
    /// For scalar elements, emits a load from `base + index * elem_size`.
    /// For struct elements, pushes a pointer (`base + index * struct_size`) without
    /// loading, enabling chained member access like `arr[0].x`.
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

        let is_compound_element = matches!(
            &elem_type_info.kind,
            TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_) | TypeInfoKind::Array(_, _)
        );

        let elem_sz = if is_compound_element {
            type_byte_size(&elem_type_info.kind, ctx)
                .expect("array index access: type_byte_size failed for compound element")
        } else {
            memory::element_size(&elem_type_info.kind)
        };

        self.lower_expression(arena, array_expr_id, ctx, None);
        self.emit_index_offset(arena, index_expr_id, elem_sz, ctx);

        if !is_compound_element {
            let load_instr = memory::load_instruction(&elem_type_info.kind);
            self.func().instruction(&load_instr);
        }
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
    ///
    /// Handles scalar arrays of any dimensionality by recursing through nested
    /// `Array(elem, len)` layers until it reaches the leaf scalar type, then
    /// emitting one uzumaki + store per leaf position. Analysis rule A028
    /// guarantees that struct-element arrays never reach this path.
    fn lower_array_uzumaki(
        &mut self,
        _arena: &AstArena,
        elem_type: &TypeInfo,
        length: u32,
        enclosing_var_name: &str,
    ) -> Result<(), CodegenError> {
        let total = total_leaf_count(&elem_type.kind, length);
        if total > MAX_UZUMAKI_UNROLL_ELEMENTS {
            return Err(CodegenError::ArrayTooLargeForUzumaki {
                total_elements: total,
                max: MAX_UZUMAKI_UNROLL_ELEMENTS,
            });
        }

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

        let slot_offset = slot.offset;
        let frame_ptr_local = layout.frame_ptr_local;

        let leaf_kind = leaf_scalar_type(&elem_type.kind);
        let leaf_size = memory::element_size(leaf_kind);
        let uzumaki_opcode = if Self::is_i64_type(leaf_kind) {
            UZUMAKI_I64_OPCODE
        } else {
            UZUMAKI_I32_OPCODE
        };
        let store_instr = memory::store_instruction(leaf_kind);

        self.emit_array_uzumaki_recursive(
            &elem_type.kind,
            length,
            frame_ptr_local,
            slot_offset,
            leaf_size,
            uzumaki_opcode,
            &store_instr,
        );

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
        Ok(())
    }

    /// Recursively emits uzumaki + store instructions for each leaf scalar
    /// position in a potentially multi-dimensional array.
    ///
    /// For `Array(inner_elem, inner_len)` kinds, iterates over `0..count`
    /// sub-arrays and recurses into each. For scalar kinds, iterates over
    /// `0..count` elements, emitting a store at each offset.
    #[allow(clippy::too_many_arguments)]
    fn emit_array_uzumaki_recursive(
        &mut self,
        kind: &TypeInfoKind,
        count: u32,
        frame_ptr_local: u32,
        base_offset: u32,
        leaf_size: u32,
        uzumaki_opcode: u8,
        store_instr: &Instruction<'_>,
    ) {
        match kind {
            TypeInfoKind::Array(inner_elem, inner_len) => {
                let inner_len = *inner_len;
                let inner_total_leaves = total_leaf_count(&inner_elem.kind, inner_len);
                let sub_array_byte_size = inner_total_leaves
                    .checked_mul(leaf_size)
                    .expect("sub-array byte size overflow in recursive uzumaki");
                for i in 0..count {
                    let sub_offset = base_offset
                        .checked_add(
                            i.checked_mul(sub_array_byte_size)
                                .expect("sub-array offset overflow in recursive uzumaki"),
                        )
                        .expect("base + sub-array offset overflow in recursive uzumaki");
                    self.emit_array_uzumaki_recursive(
                        &inner_elem.kind,
                        inner_len,
                        frame_ptr_local,
                        sub_offset,
                        leaf_size,
                        uzumaki_opcode,
                        store_instr,
                    );
                }
            }
            _ => {
                for i in 0..count {
                    #[allow(clippy::cast_possible_wrap)]
                    let byte_offset = base_offset
                        .checked_add(
                            i.checked_mul(leaf_size)
                                .expect("leaf offset overflow in recursive uzumaki"),
                        )
                        .expect("base + leaf offset overflow in recursive uzumaki")
                        as i32;
                    self.func()
                        .instruction(&Instruction::LocalGet(frame_ptr_local));
                    self.func().instruction(&Instruction::I32Const(byte_offset));
                    self.func().instruction(&Instruction::I32Add);
                    self.emit_uzumaki(uzumaki_opcode);
                    self.func().instruction(store_instr);
                }
            }
        }
    }

    /// Lowers a struct-typed uzumaki expression to field-wise non-deterministic stores.
    ///
    /// For each field in the struct layout, emits the appropriate uzumaki opcode
    /// followed by a store at the field's memory offset. The result is that every
    /// field of the struct variable is filled with a non-deterministic value.
    fn lower_struct_uzumaki(
        &mut self,
        ctx: &TypedContext,
        struct_name: &str,
        enclosing_var_name: &str,
    ) -> Result<(), CodegenError> {
        let layout = self
            .frame_layout
            .as_ref()
            .expect("Struct uzumaki requires a frame layout");

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

        if field_slots.is_empty() {
            let struct_info = ctx
                .lookup_struct(struct_name)
                .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));
            if !struct_info.fields.is_empty() {
                let (_, computed_fields) = compute_struct_field_layout(&struct_info, ctx)?;
                for field in &computed_fields {
                    self.emit_struct_field_uzumaki(frame_ptr_local, slot_offset, field)?;
                }
            }
        } else {
            for field in &field_slots {
                self.emit_struct_field_uzumaki(frame_ptr_local, slot_offset, field)?;
            }
        }

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
        Ok(())
    }

    /// Emits uzumaki opcode(s) + store(s) for a single struct field at its memory offset.
    ///
    /// Handles both scalar fields (single uzumaki + store) and array-typed fields
    /// (one uzumaki + store per element). The element count for arrays is checked
    /// against [`MAX_UZUMAKI_UNROLL_ELEMENTS`] to prevent instruction explosion.
    /// Nested struct fields remain rejected (analysis rule A027).
    fn emit_struct_field_uzumaki(
        &mut self,
        frame_ptr_local: u32,
        struct_base_offset: u32,
        field: &memory::StructFieldSlot,
    ) -> Result<(), CodegenError> {
        if let TypeInfoKind::Array(ref _elem, length) = field.type_kind
            && length > MAX_UZUMAKI_UNROLL_ELEMENTS
        {
            return Err(CodegenError::ArrayTooLargeForUzumaki {
                total_elements: length,
                max: MAX_UZUMAKI_UNROLL_ELEMENTS,
            });
        }

        match field.layout {
            CompoundFieldLayout::Scalar => {
                let uzumaki_opcode = if Self::is_i64_type(&field.type_kind) {
                    UZUMAKI_I64_OPCODE
                } else {
                    UZUMAKI_I32_OPCODE
                };
                let store_instr = memory::store_instruction(&field.type_kind);

                #[allow(clippy::cast_possible_wrap)]
                let byte_offset = struct_base_offset
                    .checked_add(field.offset)
                    .expect("byte offset overflow in struct field uzumaki")
                    as i32;

                self.func()
                    .instruction(&Instruction::LocalGet(frame_ptr_local));
                self.func().instruction(&Instruction::I32Const(byte_offset));
                self.func().instruction(&Instruction::I32Add);
                self.emit_uzumaki(uzumaki_opcode);
                self.func().instruction(&store_instr);
            }
            CompoundFieldLayout::NestedArray {
                ref elem_kind,
                elem_size,
                length,
            } => {
                let uzumaki_opcode = if Self::is_i64_type(elem_kind) {
                    UZUMAKI_I64_OPCODE
                } else {
                    UZUMAKI_I32_OPCODE
                };
                let store_instr = memory::store_instruction(elem_kind);
                for i in 0..length {
                    #[allow(clippy::cast_possible_wrap)]
                    let byte_offset = struct_base_offset
                        .checked_add(field.offset)
                        .and_then(|v| i.checked_mul(elem_size).and_then(|ie| v.checked_add(ie)))
                        .expect("byte offset overflow in struct field array uzumaki")
                        as i32;
                    self.func()
                        .instruction(&Instruction::LocalGet(frame_ptr_local));
                    self.func().instruction(&Instruction::I32Const(byte_offset));
                    self.func().instruction(&Instruction::I32Add);
                    self.emit_uzumaki(uzumaki_opcode);
                    self.func().instruction(&store_instr);
                }
            }
            CompoundFieldLayout::NestedStruct { .. } => {
                unreachable!(
                    "emit_struct_field_uzumaki called for nested struct field '{}'; \
                     analysis rule A027 should have rejected uzumaki on structs with nested struct fields",
                    field.name
                );
            }
        }
        Ok(())
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

    /// Returns `true` if the expression is a syntactic zero value that matches
    /// what `memory.fill 0` writes. Used to skip redundant stores into frame slots
    /// that were already zero-initialized by the function prologue.
    ///
    /// Recognized patterns:
    /// - `NumberLiteral { value: "0" }` or `NumberLiteral { value: "-0" }`
    /// - `BoolLiteral { value: false }` (stored as 0)
    /// - `Parenthesized { expr }` wrapping a zero literal
    /// - `PrefixUnary { op: Neg, expr }` wrapping a zero literal
    ///
    /// This is a conservative, local check with no side effects in any matched
    /// pattern. Only false negatives are possible (e.g., `0x0`, `0_0`), which
    /// result in a redundant store -- never a missing one.
    fn is_syntactic_zero(arena: &AstArena, expr_id: ExprId) -> bool {
        match &arena[expr_id].kind {
            Expr::NumberLiteral { value } => value == "0" || value == "-0",
            Expr::BoolLiteral { value } => !value,
            Expr::Parenthesized { expr }
            | Expr::PrefixUnary {
                op: UnaryOperatorKind::Neg,
                expr,
            } => Self::is_syntactic_zero(arena, *expr),
            _ => false,
        }
    }

    /// Lowers an array literal expression.
    ///
    /// For scalar-element arrays, emits per-element stores. For struct-element
    /// arrays, uses `lower_struct_literal_fields` at each element's base offset
    /// to recursively emit field stores. Non-literal struct elements (identifiers,
    /// function calls) are handled via `memory.copy`.
    ///
    /// Zero-valued elements are skipped unconditionally because this function is
    /// only called from frame-local initialization (via `lower_expression`), never
    /// from sret return paths. Sret returns use `lower_array_sret_return` directly,
    /// which emits stores unconditionally because the sret destination is caller
    /// memory (not zero-filled by this function's prologue). The function prologue's
    /// `memory.fill 0` guarantees the frame is already zeroed.
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
        let element_layout = slot.element_layout.clone();
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

        if let Some(ref field_slots) = element_layout {
            self.lower_array_literal_struct_elements(
                arena,
                elements,
                field_slots,
                frame_ptr_local,
                slot_offset,
                slot_elem_size,
                ctx,
                true,
            );
        } else {
            let store_instr = memory::store_instruction_from_slot(slot);
            for (i, &element_id) in elements.iter().enumerate() {
                if Self::is_syntactic_zero(arena, element_id) {
                    continue;
                }
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

    /// Lowers struct-element array literal elements using recursive field stores.
    ///
    /// For each element at index `i`, computes base offset `slot_offset + i * elem_size`.
    /// If the element is a `StructLiteral`, uses `lower_struct_literal_fields` to emit
    /// per-field stores. Otherwise (identifier, function call), evaluates the expression
    /// to get a source pointer and emits `memory.copy` for the full struct size.
    #[allow(clippy::too_many_arguments)]
    fn lower_array_literal_struct_elements(
        &mut self,
        arena: &AstArena,
        elements: &[ExprId],
        field_slots: &[memory::StructFieldSlot],
        frame_ptr_local: u32,
        slot_offset: u32,
        elem_size: u32,
        ctx: &TypedContext,
        skip_zero_stores: bool,
    ) {
        let field_slots_clone = field_slots.to_vec();
        debug_assert!(
            !skip_zero_stores
                || frame_ptr_local == self.frame_layout.as_ref().unwrap().frame_ptr_local,
            "zero-store elision requires frame pointer base, got local {frame_ptr_local}"
        );
        for (i, &element_id) in elements.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let base_offset = slot_offset
                .checked_add((i as u32) * elem_size)
                .expect("byte offset overflow in array literal struct elements");

            if let Expr::StructLiteral { fields, .. } = &arena[element_id].kind {
                let fields: Vec<_> = fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                self.lower_struct_literal_fields(
                    arena,
                    &fields,
                    &field_slots_clone,
                    frame_ptr_local,
                    base_offset,
                    ctx,
                    "<array element>",
                    0,
                    skip_zero_stores,
                );
            } else {
                memory::emit_ptr_offset_addr(self.func(), frame_ptr_local, base_offset);
                self.lower_expression(arena, element_id, ctx, None);
                self.emit_memory_copy(elem_size);
            }
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
            unreachable!("struct literal requires frame layout");
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

        self.lower_struct_literal_fields(
            arena,
            fields,
            &field_slots,
            frame_ptr_local,
            slot_offset,
            ctx,
            enclosing_var_name,
            0,
            true,
        );

        self.func()
            .instruction(&Instruction::LocalGet(frame_ptr_local));
        if slot_offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(slot_offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Emits stores for struct literal fields at a given base offset from a base pointer.
    ///
    /// Handles both sret return destinations and frame-pointer-based local variable
    /// initialization. The `base_ptr_local` is the WASM local holding the destination
    /// pointer (either sret param or frame pointer), and `base_offset` is the byte
    /// offset within that region.
    ///
    /// For scalar fields, emits `base_ptr + base_offset + field.offset` then a store.
    /// For nested struct literal fields, recurses with `base_offset + field.offset`.
    /// For compound fields with non-literal values (identifiers, function calls), emits
    /// `memory.copy` from the source pointer to `base_ptr + base_offset + field.offset`.
    ///
    /// When `skip_zero_stores` is `true`, scalar fields and nested array elements that
    /// are syntactic zero values are skipped because the function prologue's
    /// `memory.fill 0` already initialized the frame to zero. This flag must be `false`
    /// for sret return paths where the destination is caller memory, not the callee's
    /// zero-filled frame.
    ///
    /// NOTE: This function is recursive for nested compound types, but analysis
    /// rule A026 permanently limits nesting to one level. If A026 were ever
    /// relaxed, uzumaki emission would also need extension for deeper nesting.
    #[allow(clippy::too_many_arguments)]
    fn lower_struct_literal_fields(
        &mut self,
        arena: &AstArena,
        fields: &[(IdentId, ExprId)],
        field_slots: &[memory::StructFieldSlot],
        base_ptr_local: u32,
        base_offset: u32,
        ctx: &TypedContext,
        struct_name: &str,
        depth: u32,
        skip_zero_stores: bool,
    ) {
        debug_assert!(
            depth < 3,
            "lower_struct_literal_fields recursion depth {depth} exceeds limit; \
             A026 bounds nesting to one level (max expected depth is 2)"
        );
        debug_assert!(
            !skip_zero_stores
                || base_ptr_local == self.frame_layout.as_ref().unwrap().frame_ptr_local,
            "zero-store elision requires frame pointer base, got local {base_ptr_local}"
        );

        for &(field_name_id, field_value_expr_id) in fields {
            let field_name = &arena[field_name_id].name;
            let field_slot = field_slots
                .iter()
                .find(|fs| fs.name == *field_name)
                .unwrap_or_else(|| {
                    panic!(
                        "Struct field '{field_name}' not found in layout for struct '{struct_name}'"
                    )
                });

            let offset = base_offset + field_slot.offset;

            match &field_slot.layout {
                memory::CompoundFieldLayout::NestedStruct {
                    fields: nested_slots,
                    total_size,
                } => {
                    if let Expr::StructLiteral {
                        fields: inner_fields,
                        ..
                    } = &arena[field_value_expr_id].kind
                    {
                        let nested_name = match &field_slot.type_kind {
                            TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) => {
                                name.as_str()
                            }
                            _ => "<nested struct>",
                        };
                        let inner_fields: Vec<_> =
                            inner_fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                        let nested_slots = nested_slots.clone();
                        self.lower_struct_literal_fields(
                            arena,
                            &inner_fields,
                            &nested_slots,
                            base_ptr_local,
                            offset,
                            ctx,
                            nested_name,
                            depth + 1,
                            skip_zero_stores,
                        );
                    } else {
                        emit_ptr_offset_addr(self.func(), base_ptr_local, offset);
                        self.lower_expression(arena, field_value_expr_id, ctx, None);
                        self.emit_memory_copy(*total_size);
                    }
                }
                memory::CompoundFieldLayout::NestedArray {
                    elem_kind,
                    elem_size,
                    length,
                } => {
                    if let Expr::ArrayLiteral { elements } = &arena[field_value_expr_id].kind {
                        assert!(
                            !matches!(elem_kind, TypeInfoKind::Struct(_) | TypeInfoKind::Custom(_)),
                            "Array literal element-wise store requires scalar element type, \
                             got {elem_kind:?}"
                        );
                        let store_instr = memory::store_instruction(elem_kind);
                        let elements: Vec<_> = elements.clone();
                        for (i, &element_id) in elements.iter().enumerate() {
                            if skip_zero_stores && Self::is_syntactic_zero(arena, element_id) {
                                continue;
                            }
                            #[allow(clippy::cast_possible_truncation)]
                            let elem_byte_offset = offset + (i as u32) * elem_size;
                            emit_ptr_offset_addr(self.func(), base_ptr_local, elem_byte_offset);
                            self.lower_expression(arena, element_id, ctx, None);
                            self.func().instruction(&store_instr);
                        }
                    } else {
                        let array_byte_size = elem_size.checked_mul(*length).expect(
                            "Array byte size overflow: elem_size * length exceeds u32::MAX",
                        );
                        emit_ptr_offset_addr(self.func(), base_ptr_local, offset);
                        self.lower_expression(arena, field_value_expr_id, ctx, None);
                        self.emit_memory_copy(array_byte_size);
                    }
                }
                memory::CompoundFieldLayout::Scalar => {
                    if !(skip_zero_stores && Self::is_syntactic_zero(arena, field_value_expr_id)) {
                        let store_instr = memory::store_instruction(&field_slot.type_kind);
                        emit_ptr_offset_addr(self.func(), base_ptr_local, offset);
                        self.lower_expression(arena, field_value_expr_id, ctx, None);
                        self.func().instruction(&store_instr);
                    }
                }
            }
        }
    }

    /// Lowers a member access expression (e.g., `p.x` or `outer.inner`) to WASM instructions.
    ///
    /// For scalar fields, emits a load from struct pointer + field offset:
    /// ```text
    /// <lower expr>           ;; struct pointer
    /// i32.const <field_offset>
    /// i32.add
    /// <load instruction>     ;; load field value
    /// ```
    ///
    /// For compound fields (nested struct or array), pushes a pointer without loading:
    /// ```text
    /// <lower expr>           ;; struct pointer
    /// i32.const <field_offset>
    /// i32.add                ;; result is pointer to nested compound
    /// ```
    /// This enables chaining: `outer.inner.x` = pointer + pointer + load.
    ///
    /// NOTE: This function supports recursive access chains for nested compound
    /// types (e.g., `outer.inner.x`), but analysis rule A026 permanently limits
    /// nesting to one level. If A026 were ever relaxed, uzumaki emission would
    /// also need extension for deeper nesting.
    fn lower_member_access(
        &mut self,
        arena: &AstArena,
        _member_access_expr_id: ExprId,
        struct_expr_id: ExprId,
        field_name_id: IdentId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_member_access_read);

        let field = self.resolve_struct_field_offset(arena, struct_expr_id, field_name_id, ctx);

        self.lower_expression(arena, struct_expr_id, ctx, None);

        if field.offset > 0 {
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(field.offset as i32));
            self.func().instruction(&Instruction::I32Add);
        }

        if !field.layout.is_compound() {
            let load_instr = memory::load_instruction(&field.type_kind);
            self.func().instruction(&load_instr);
        }
    }

    /// Lowers a member access write (e.g., `p.x = 42`) to WASM instructions.
    ///
    /// For scalar fields, emits a store at struct pointer + field offset:
    /// ```text
    /// <lower struct expr>      ;; struct base pointer
    /// i32.const <field_offset>
    /// i32.add
    /// <lower RHS>              ;; value to store
    /// <store instruction>
    /// ```
    ///
    /// For compound fields (nested structs or arrays), emits a `memory.copy`
    /// from the RHS pointer to the destination field address.
    fn lower_member_access_write(
        &mut self,
        arena: &AstArena,
        struct_expr_id: ExprId,
        field_name_id: IdentId,
        right_expr_id: ExprId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_member_access_write);

        let field = self.resolve_struct_field_offset(arena, struct_expr_id, field_name_id, ctx);

        if field.layout.is_compound() {
            let compound_size = field.layout.byte_size();

            // dest: struct_ptr + field_offset
            self.lower_expression(arena, struct_expr_id, ctx, None);
            if field.offset > 0 {
                #[allow(clippy::cast_possible_wrap)]
                self.func()
                    .instruction(&Instruction::I32Const(field.offset as i32));
                self.func().instruction(&Instruction::I32Add);
            }
            // src: RHS expression (pointer to compound)
            self.lower_expression(arena, right_expr_id, ctx, None);
            self.emit_memory_copy(compound_size);
        } else {
            let store_instr = memory::store_instruction(&field.type_kind);

            self.lower_expression(arena, struct_expr_id, ctx, None);

            if field.offset > 0 {
                #[allow(clippy::cast_possible_wrap)]
                self.func()
                    .instruction(&Instruction::I32Const(field.offset as i32));
                self.func().instruction(&Instruction::I32Add);
            }

            self.lower_expression(arena, right_expr_id, ctx, None);

            self.func().instruction(&store_instr);
        }
    }

    /// Resolves a struct field's byte offset, type kind, and compound layout for member access.
    ///
    /// Tries the precomputed layout in `frame_layout.struct_offsets` first (O(1) lookup
    /// when the struct expression is a simple variable). Falls back to recomputing via
    /// `compute_struct_field_layout` for parameters or complex expressions.
    ///
    /// The returned [`ResolvedField`] allows callers to decide whether to emit a
    /// load instruction (scalar) or push a pointer (compound field).
    fn resolve_struct_field_offset(
        &self,
        arena: &AstArena,
        struct_expr_id: ExprId,
        field_name_id: IdentId,
        ctx: &TypedContext,
    ) -> ResolvedField {
        let field_name = &arena[field_name_id].name;

        if let Some(ref layout) = self.frame_layout
            && let Expr::Identifier(ident_id) = &arena[struct_expr_id].kind
        {
            let var_name = &arena[*ident_id].name;
            if let Some(struct_slot) = layout.struct_offsets.get(var_name) {
                let field_slot = struct_slot
                    .fields
                    .iter()
                    .find(|fs| fs.name == *field_name)
                    .unwrap_or_else(|| {
                        panic!("Field '{field_name}' not found in cached layout for '{var_name}'")
                    });
                return ResolvedField {
                    offset: field_slot.offset,
                    type_kind: field_slot.type_kind.clone(),
                    layout: field_slot.layout.clone(),
                };
            }
        }

        let struct_type = ctx
            .get_node_typeinfo(NodeId::Expr(struct_expr_id))
            .expect("MemberAccess: struct expression must have type info");

        let (TypeInfoKind::Struct(struct_name) | TypeInfoKind::Custom(struct_name)) =
            &struct_type.kind
        else {
            panic!(
                "MemberAccess: struct expression has non-struct type: {:?}",
                struct_type.kind
            )
        };

        let struct_info = ctx
            .lookup_struct(struct_name)
            .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));

        let (_, field_slots) = compute_struct_field_layout(&struct_info, ctx)
            .expect("resolve field offset: struct layout computation failed");
        let field_slot = field_slots
            .iter()
            .find(|fs| fs.name == *field_name)
            .unwrap_or_else(|| {
                panic!("Field '{field_name}' not found in struct '{struct_name}' layout")
            });

        ResolvedField {
            offset: field_slot.offset,
            type_kind: field_slot.type_kind.clone(),
            layout: field_slot.layout.clone(),
        }
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

    fn emit_memory_copy(&mut self, byte_size: u32) {
        #[allow(clippy::cast_possible_wrap)]
        self.func()
            .instruction(&Instruction::I32Const(byte_size as i32));
        self.func().instruction(&Instruction::MemoryCopy {
            src_mem: MEMORY_INDEX,
            dst_mem: MEMORY_INDEX,
        });
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

/// Computes and returns the struct field layout if the given type is a struct.
///
/// Returns `Some(field_slots)` when `kind` is `Struct(name)` or `Custom(name)` and
/// the struct is found in the type context. Returns `None` for non-struct types.
/// Used when building `ArraySlot` to cache the element layout for struct-element arrays.
fn compute_element_layout_if_struct(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
) -> Result<Option<Vec<memory::StructFieldSlot>>, CodegenError> {
    match kind {
        TypeInfoKind::Struct(name) | TypeInfoKind::Custom(name) => {
            let Some(struct_info) = ctx.lookup_struct(name) else {
                return Ok(None);
            };
            let (_, field_slots) = compute_struct_field_layout(&struct_info, ctx)?;
            Ok(Some(field_slots))
        }
        _ => Ok(None),
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
        let byte_offset = index_val.checked_mul(elem_sz as i32)?;
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
