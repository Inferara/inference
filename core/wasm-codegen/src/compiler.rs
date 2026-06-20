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
    ArgData, ArgKind, BlockKind, Def, Expr, OperatorKind, SimpleTypeKind, Stmt, TypeNode,
    UnaryOperatorKind,
    Visibility,
};
use inference_type_checker::{
    type_info::{NumberType, TypeInfo, TypeInfoKind},
    typed_context::TypedContext,
};
use wasm_encoder::{
    BlockType as WasmBlockType, CodeSection, ConstExpr, EntityType, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection, IndirectNameMap,
    Instruction, MemorySection, MemoryType, Module, NameMap, NameSection, TypeSection, ValType,
};

use crate::memory::{
    self, ArraySlot, CompoundFieldLayout, FrameLayout, MEMORY_INDEX, STACK_POINTER_INIT,
    STACK_SIZE, StructSlot, align_to, align_to_frame, compute_struct_field_layout,
    emit_array_param_copy, emit_ptr_offset_addr, emit_sret_copy, emit_stack_epilogue,
    emit_stack_prologue, emit_struct_param_copy, natural_alignment_for_type, type_byte_size,
};

/// Origin of a function definition being lowered.
///
/// Distinguishes top-level definitions from those nested inside a `spec`
/// block. Used to gate WASM `export` emission so that `pub fn` inside a spec
/// does not become an exported entry point (no spec-inner export site is
/// reachable from outside the module). `SpecInner` carries the owning **bare**
/// spec name (not file-folded); call lowering combines it with the spec's
/// defining file (`current_module_path`) to build the injective spec [`FnKey`]
/// for intra-spec resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FunctionOrigin {
    TopLevel,
    SpecInner(String),
}

/// The triple yielded by [`Compiler::finish_and_take`]: the assembled WASM
/// binary, the per-spec function indices, and the per-function shadow-stack
/// frame sizes (canonical [`FnKey`] → bytes).
type FinishedModule = (Vec<u8>, FxHashMap<String, Vec<u32>>, FxHashMap<FnKey, u32>);

/// Structured key identifying every WASM function, shared with the analysis
/// passes so codegen and the call-graph agree on identity by construction.
///
/// Re-exported from [`inference_fn_key`] for the in-crate references that use
/// the bare `FnKey` name; see that crate for the variant and `Display`
/// documentation.
pub(crate) use inference_fn_key::FnKey;

/// RAII guard that saves and restores `Compiler::current_spec`.
///
/// `enter` swaps in a new value and stashes the prior one; `Drop` writes
/// the prior value back. This is the canonical save/restore RAII pattern
/// (same shape as `MutexGuard`), so nested guards compose correctly: an
/// inner guard's drop restores the outer's spec rather than clearing it.
///
/// `Deref`/`DerefMut` forward to the wrapped compiler so the guard is used
/// in place of `&mut self` for the duration of the visit. This avoids the
/// borrow conflict that arises from a slot-shaped guard
/// (`&mut self.current_spec`) coexisting with `self.<method>()` calls.
struct SpecScopeGuard<'a> {
    compiler: &'a mut Compiler,
    previous: Option<String>,
}

impl<'a> SpecScopeGuard<'a> {
    fn enter(compiler: &'a mut Compiler, spec: Option<String>) -> Self {
        let previous = std::mem::replace(&mut compiler.current_spec, spec);
        Self { compiler, previous }
    }
}

impl std::ops::Deref for SpecScopeGuard<'_> {
    type Target = Compiler;
    fn deref(&self) -> &Compiler {
        self.compiler
    }
}

impl std::ops::DerefMut for SpecScopeGuard<'_> {
    fn deref_mut(&mut self) -> &mut Compiler {
        self.compiler
    }
}

impl Drop for SpecScopeGuard<'_> {
    fn drop(&mut self) {
        self.compiler.current_spec = self.previous.take();
    }
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

/// Maximum number of scalar elements that uzumaki unrolling will emit
/// instructions for. Each element produces ~5 WASM instructions (load
/// frame pointer, compute offset, add, uzumaki, store), so 65 536
/// elements = 327 680 instructions -- a reasonable upper bound before
/// instruction explosion becomes a concern.
const MAX_UZUMAKI_UNROLL_ELEMENTS: u32 = 65_536;

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

/// A single WASM function import emitted for an `external fn`.
///
/// `module` / `field` are the two-level WASM import name. `module` is the
/// logical, platform-independent module reference (`ExternOrigin::logical_module`,
/// `::`-joined), and `field` is the export field the linker satisfies. `type_idx`
/// indexes the shared [`Compiler::types`] table; identical signatures dedup onto
/// the same entry. Imports occupy WASM function indices `0..N`, ahead of every
/// locally defined function (see [`Compiler::register_imports`]).
#[derive(Debug, Clone)]
struct ImportEntry {
    module: String,
    field: String,
    type_idx: u32,
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
    /// Name of the struct type being returned. Carried purely for diagnostic
    /// messages (e.g., field-not-found panics) so we don't misattribute the
    /// failure to the enclosing function name.
    struct_name: String,
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
///
/// For `Function`, the callee is a bare free-function name; the compiler
/// applies spec-aware preference (try `SpecFree` then `Free`) at the lookup
/// site. For the method variants, the [`FnKey`] is already resolved by the
/// method-name lookup helpers, which encode the spec-vs-top-level decision.
enum ResolvedCallee {
    /// Plain same-file (or spec-local) function call via `Expr::Identifier`.
    /// Resolved with spec-aware preference against the current file's module
    /// path at the lookup site.
    Function(String),
    /// A call whose target the type checker resolved to a specific function in
    /// a (possibly different) file — an item-imported bare call
    /// (`use lib::arith::{add}; add()`) or a qualified path
    /// (`math::arith::add(...)`). The [`FnKey`] is already file-qualified by the
    /// callee's defining file, so it resolves directly with no spec preference
    /// (the callee is a top-level function elsewhere).
    QualifiedFunction(FnKey),
    /// Associated function call via `Expr::TypeMemberAccess` (e.g., `Point::new()`).
    AssociatedFunction { key: FnKey },
    /// Instance method call via `Expr::MemberAccess` (e.g., `p.translate()`).
    InstanceMethod {
        key: FnKey,
        receiver_expr_id: ExprId,
        method_name_id: IdentId,
    },
}

impl ResolvedCallee {
    /// Display form of the resolved callee for use in diagnostic messages
    /// and panic descriptions. Returns the bare name for free functions and
    /// the mangled display of the [`FnKey`] for methods.
    fn display_name(&self) -> String {
        match self {
            Self::Function(name) => name.clone(),
            Self::QualifiedFunction(key)
            | Self::AssociatedFunction { key, .. }
            | Self::InstanceMethod { key, .. } => key.to_string(),
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
#[allow(clippy::struct_excessive_bools)]
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
    /// Maps function keys to their WASM function section indices.
    func_name_to_idx: FxHashMap<FnKey, u32>,
    /// Function imports emitted for `external fn` declarations, in
    /// registration order. Each occupies WASM function index `i` for its
    /// position `i` in this vector (imports come before all local functions).
    imports: Vec<ImportEntry>,
    /// Maps an `external fn` name to its WASM import function index (`0..N`).
    /// Calls to an extern lower to `call <this index>` rather than a local
    /// function index. Populated alongside [`Self::imports`] during Stage 1.
    extern_name_to_idx: FxHashMap<String, u32>,
    /// Sticky flag: set to `true` when any function requires linear memory.
    has_memory: bool,
    /// Maps function keys to their array return type metadata.
    func_array_returns: FxHashMap<FnKey, ArrayReturnInfo>,
    /// Maps function keys to their struct return type metadata.
    func_struct_returns: FxHashMap<FnKey, StructReturnInfo>,
    /// Name of the function currently being compiled (display form, used for
    /// diagnostics). The lookup-shaped companion is [`Self::current_fn_key`].
    current_fn_name: String,
    /// Structured key for the function currently being compiled. Set to
    /// `Some(_)` while entering [`Self::visit_function_definition`] and used
    /// as the lookup key for sret return-emission so we don't have to
    /// recompute the variant from `current_spec` + `current_fn_name` at every
    /// call site.
    current_fn_key: Option<FnKey>,
    /// Name of the spec that owns the function currently being compiled, if
    /// any. Set by `visit_function_definition` for spec-inner functions and
    /// methods so that intra-spec call resolution can prefer the mangled
    /// `"<spec>.<callee>"` key before falling back to the bare name.
    current_spec: Option<String>,
    /// Source-root-relative module path of the file whose function is currently
    /// being compiled (empty for the entry file). Set per function alongside
    /// `current_fn_name`/`current_fn_key`. Lowering reads it to resolve a bare
    /// struct/enum name in this file to its file-qualified canonical layout key
    /// so two files defining a same-named type get distinct layouts.
    current_module_path: Vec<String>,
    // Per-function state (set in visit_function_definition, used by lowering methods)
    func: Option<Function>,
    locals_map: FxHashMap<String, (u32, ValType)>,
    frame_layout: Option<FrameLayout>,
    loop_ctx: LoopContext,
    parent_blocks_stack: Vec<BlockKind>,
    /// When true, zero-valued stores into frame memory can be elided because
    /// the function prologue's `memory.fill 0` guarantees all slots start at
    /// zero. Set only during variable initialization (`Stmt::VarDef`), never
    /// during assignment where slots may hold non-zero data.
    init_zero_elision: bool,
    /// WASM function indices for functions that originated in `spec` blocks,
    /// keyed by spec name. Populated during Stage 1 registration in proof mode;
    /// consumed (moved out) by [`Self::finish_and_take`] so the Rocq translator
    /// can emit per-spec `Definition <mod>__<SpecName>_specs : list N`
    /// definitions.
    spec_func_indices_by_spec: FxHashMap<String, Vec<u32>>,
    /// Real shadow-stack frame size in bytes for each function, keyed by its
    /// canonical [`FnKey`] display string. Recorded in
    /// [`Self::visit_function_definition`] right after the frame layout is
    /// computed; frameless functions record 0. Moved out by
    /// [`Self::finish_and_take`] so the analysis↔codegen frame-size soundness
    /// invariant (A036's estimate ≥ this) can be checked cross-crate.
    ///
    /// Keyed by the structured [`FnKey`] (not its lossy `Display` string) so the
    /// cross-crate parity test cannot collapse two distinct functions whose keys
    /// render to the same string into one slot.
    frame_sizes: FxHashMap<FnKey, u32>,
    /// When true, dynamic (runtime-index) array accesses are preceded by a
    /// bounds-check guard (`index >= length → unreachable`). Derived in
    /// Set by [`crate::codegen`] for every Compile-mode build (the deployed
    /// artifact is always checked); left `false` in Proof mode and at
    /// `Compiler::new` call sites so those paths stay unguarded.
    emit_bounds_checks: bool,
    /// WASM local index of the scratch i32 used to single-evaluate a dynamic
    /// array index for the bounds-check guard (AD-3). Reserved per-function in
    /// [`Self::visit_function_definition`] only when `emit_bounds_checks` is set
    /// and the body actually contains a dynamic array index (the only case that
    /// emits a guard); `None` otherwise — including for constant-index-only and
    /// frameless functions. Reset per function alongside the rest of the
    /// per-function state.
    bounds_check_scratch_local: Option<u32>,
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
            imports: Vec::new(),
            extern_name_to_idx: FxHashMap::default(),
            has_memory: false,
            func_array_returns: FxHashMap::default(),
            func_struct_returns: FxHashMap::default(),
            current_fn_name: String::new(),
            current_fn_key: None,
            current_spec: None,
            current_module_path: Vec::new(),
            func: None,
            locals_map: FxHashMap::default(),
            frame_layout: None,
            loop_ctx: LoopContext::default(),
            parent_blocks_stack: Vec::new(),
            init_zero_elision: false,
            spec_func_indices_by_spec: FxHashMap::default(),
            frame_sizes: FxHashMap::default(),
            emit_bounds_checks: false,
            bounds_check_scratch_local: None,
        }
    }

    /// Enables or disables runtime array bounds-check emission.
    ///
    /// [`crate::codegen`] enables it for every Compile-mode build (so the
    /// deployed artifact always traps on a dynamic out-of-range access) and
    /// leaves it `false` in Proof mode. Test call sites of [`Self::new`] keep
    /// the default `false`, so their emitted bytes stay unguarded.
    pub(crate) fn set_emit_bounds_checks(&mut self, enabled: bool) {
        self.emit_bounds_checks = enabled;
    }

    /// Records a single WASM function index as belonging to `spec_name`.
    pub(crate) fn record_spec_index(&mut self, spec_name: &str, idx: u32) {
        self.spec_func_indices_by_spec
            .entry(spec_name.to_string())
            .or_default()
            .push(idx);
    }

    /// Ensures `spec_name` has an entry in `spec_func_indices_by_spec`,
    /// inserting an empty index list if absent. Called for every visited
    /// `spec` block in proof mode so user-authored `spec MySpec { }` (with
    /// zero inner emittable defs) still produces a per-spec `.v` definition
    /// and theorem downstream.
    pub(crate) fn ensure_spec_registered(&mut self, spec_name: &str) {
        self.spec_func_indices_by_spec
            .entry(spec_name.to_string())
            .or_default();
    }

    /// Borrows the recorded `(spec_name -> [func_idx])` map so a caller can
    /// validate it before [`Self::finish_and_take`] consumes the compiler and
    /// emits the `inference.spec_funcs` section.
    pub(crate) fn spec_func_indices(&self) -> &FxHashMap<String, Vec<u32>> {
        &self.spec_func_indices_by_spec
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
        key: FnKey,
        return_ty_id: TypeId,
        arena: &AstArena,
        ctx: &TypedContext,
        module_path: &[String],
    ) -> Result<(), CodegenError> {
        let return_type_info = TypeInfo::from_type_id(arena, return_ty_id);
        match &return_type_info.kind {
            TypeInfoKind::Array(elem_type, length) => {
                let elem_sz = type_byte_size(&elem_type.kind, ctx, module_path)?;
                self.func_array_returns.insert(
                    key,
                    ArrayReturnInfo {
                        elem_kind: elem_type.kind.clone(),
                        elem_size: elem_sz,
                        length: *length,
                    },
                );
            }
            TypeInfoKind::Custom(custom_name) => {
                if let Some(struct_info) = ctx.lookup_struct_in(custom_name, module_path) {
                    let (total_size, field_slots) =
                        compute_struct_field_layout(&struct_info, ctx, module_path)?;
                    self.func_struct_returns.insert(
                        key,
                        StructReturnInfo {
                            total_size,
                            field_slots,
                            struct_name: custom_name.clone(),
                        },
                    );
                }
            }
            // A `::`-qualified return type names a cross-file struct by its path
            // rather than a bare name. Resolving it here recovers the same
            // `StructInfo` a bare return would, so a function returning a qualified
            // struct uses the sret convention instead of falling through to the
            // non-sret path (which would panic on the returned struct literal).
            TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
                let segments: Vec<String> = path.split("::").map(ToString::to_string).collect();
                if let Some(struct_info) =
                    ctx.lookup_struct_by_qualified_path(&segments, module_path)
                {
                    let (total_size, field_slots) =
                        compute_struct_field_layout(&struct_info, ctx, module_path)?;
                    self.func_struct_returns.insert(
                        key,
                        StructReturnInfo {
                            total_size,
                            field_slots,
                            struct_name: struct_info.name,
                        },
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Registers a function import for every `external fn` reachable from this
    /// source file, assigning each WASM function index `0..N` ahead of all local
    /// functions.
    ///
    /// For each extern this:
    /// 1. lowers its declared signature to a WASM `(params, results)` type and
    ///    dedups it into [`Self::types`] (identical signatures share one entry);
    /// 2. records an [`ImportEntry`] carrying the logical module / export field
    ///    from the Phase 1 provenance ([`TypedContext::extern_origin`]);
    /// 3. maps the extern's name to its import function index so call lowering can
    ///    emit `call <import_idx>`.
    ///
    /// Externs without provenance (a bare `external fn` with no binding `use`) are
    /// skipped: they cannot be emitted as a well-formed two-level import, and
    /// analysis rule A024 already gates *calling* an unlinked extern. Returns the
    /// number of imports registered, which is the base index for local functions.
    ///
    /// Must run before [`Self::build_func_name_to_idx`] so local indices follow
    /// the imports.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn register_imports(
        &mut self,
        arena: &AstArena,
        extern_def_ids: &[DefId],
        ctx: &TypedContext,
    ) -> Result<u32, CodegenError> {
        for &def_id in extern_def_ids {
            let Def::ExternFunction {
                name,
                args,
                returns,
                ..
            } = &arena[def_id].kind
            else {
                continue;
            };
            let extern_name = arena[*name].name.clone();
            // Resolve provenance by the declaring `DefId`, not by name. Two
            // same-named externs can coexist (a bound top-level `f` and an
            // unbound spec-inner `f`); a name-keyed lookup cannot tell them apart
            // and would bind the unbound declaration to the bound one's origin,
            // registering a spurious/duplicate import. The decl-keyed query
            // returns `None` for the unbound one, so it is correctly skipped.
            let Some(origin) = ctx.extern_origin_by_decl(def_id) else {
                continue;
            };

            let params = Self::import_param_types(arena, args, ctx)?;
            let results = match returns {
                Some(ty_id) => Self::val_type_from_type_id(arena, *ty_id, ctx, &[])?
                    .into_iter()
                    .collect::<Vec<_>>(),
                None => Vec::new(),
            };
            let type_idx = self.intern_type(params, results);

            let import_idx = self.imports.len() as u32;
            self.imports.push(ImportEntry {
                module: origin.logical_module,
                field: origin.export_field,
                type_idx,
            });
            self.extern_name_to_idx.insert(extern_name, import_idx);
        }

        Ok(self.imports.len() as u32)
    }

    /// Lowers an extern's declared parameter types to WASM value types. An
    /// ignored parameter (`external fn f(_: i32)`) still occupies an ABI slot:
    /// the call site pushes the argument and the real `.wasm` export declares
    /// that parameter, so it is lowered as a real param just like a named or
    /// type-only one. This keeps the import signature in lock-step with the
    /// validator's `lower_extern_signature`. A `unit` parameter cannot reach
    /// this point: the validator rejects it (`LowerSignatureError::UnitParameter`)
    /// earlier in the pipeline.
    fn import_param_types(
        arena: &AstArena,
        args: &[ArgData],
        ctx: &TypedContext,
    ) -> Result<Vec<ValType>, CodegenError> {
        let mut params = Vec::with_capacity(args.len());
        for arg in args {
            let ty = match &arg.kind {
                ArgKind::Named { ty, .. }
                | ArgKind::TypeOnly(ty)
                | ArgKind::Ignored { ty } => *ty,
                // The type-checker now rejects `self` on an extern, so this is
                // unreachable from valid source; drop it to match codegen, which
                // emits no receiver param for an import.
                ArgKind::SelfRef { .. } => continue,
            };
            if let Some(val) = Self::val_type_from_type_id(arena, ty, ctx, &[])? {
                params.push(val);
            }
        }
        Ok(params)
    }

    /// Returns the index of `(params, results)` in [`Self::types`], appending a
    /// new entry only when no identical signature is already present. Used so an
    /// import and a local function (or two imports) with the same signature share
    /// one type entry.
    fn intern_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        if let Some(idx) = self
            .types
            .iter()
            .position(|(p, r)| p == &params && r == &results)
        {
            #[allow(clippy::cast_possible_truncation)]
            return idx as u32;
        }
        #[allow(clippy::cast_possible_truncation)]
        let idx = self.types.len() as u32;
        self.types.push((params, results));
        idx
    }

    /// Sets the WASM function index at which body compilation begins. Imports
    /// occupy `0..base`, so the first locally defined function body is index
    /// `base`. Called once after [`Self::register_imports`], before any body is
    /// compiled.
    pub(crate) fn set_local_func_base(&mut self, base: u32) {
        self.func_idx = base;
    }

    /// Builds the function name-to-WASM-index map from the source file's function definitions.
    ///
    /// `base_idx` is the WASM function index assigned to `func_def_ids[0]`. Top-level
    /// functions pass the import count `N` (so locals follow the imports); spec-originated
    /// functions are routed through
    /// [`Self::build_func_name_to_idx_with_spec_names`] instead.
    ///
    /// Must be called before `visit_function_definition` so that forward references
    /// resolve correctly during call lowering.
    pub(crate) fn build_func_name_to_idx(
        &mut self,
        arena: &AstArena,
        funcs: &[crate::EmittableFn],
        ctx: &TypedContext,
        base_idx: u32,
    ) -> Result<(), CodegenError> {
        #[allow(clippy::cast_possible_truncation)]
        for (idx, entry) in funcs.iter().enumerate() {
            let fn_name = arena.def_name(entry.def_id);
            let key = FnKey::free_in(entry.module_path.clone(), fn_name);
            assert!(
                !self.func_name_to_idx.contains_key(&key),
                "Top-level function '{key}' collides with an existing \
                 top-level function. The type-checker should have rejected \
                 the duplicate at the source level."
            );
            self.func_name_to_idx
                .insert(key.clone(), idx as u32 + base_idx);

            if let Def::Function { returns, .. } = &arena[entry.def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(
                    key,
                    *return_ty_id,
                    arena,
                    ctx,
                    &entry.module_path,
                )?;
            }
        }
        Ok(())
    }

    /// Variant of [`Self::build_func_name_to_idx`] for spec-inner functions that
    /// carries the per-function spec name. Each `(spec_name, def_id)` entry is
    /// registered under the mangled internal key `"{spec_name}.{fn_name}"`.
    ///
    /// Returns the WASM function index assigned to each entry, parallel to the
    /// input slice. The caller threads these indices into [`Self::record_spec_index`]
    /// so the recorded per-spec list stays in lockstep with registration.
    pub(crate) fn build_func_name_to_idx_with_spec_names(
        &mut self,
        arena: &AstArena,
        spec_funcs: &[crate::EmittableSpecFn],
        ctx: &TypedContext,
        base_idx: u32,
    ) -> Result<Vec<u32>, CodegenError> {
        let mut assigned = Vec::with_capacity(spec_funcs.len());
        #[allow(clippy::cast_possible_truncation)]
        for (idx, entry) in spec_funcs.iter().enumerate() {
            let fn_name = arena.def_name(entry.def_id);
            let key = FnKey::spec_free_folded(&entry.module_path, &entry.spec_name, fn_name);
            assert!(
                !self.func_name_to_idx.contains_key(&key),
                "Spec-inner function '{key}' collides with an \
                 existing spec-inner function. The type-checker should have \
                 rejected the duplicate at the source level."
            );
            let assigned_idx = idx as u32 + base_idx;
            self.func_name_to_idx.insert(key.clone(), assigned_idx);
            assigned.push(assigned_idx);

            if let Def::Function { returns, .. } = &arena[entry.def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(
                    key,
                    *return_ty_id,
                    arena,
                    ctx,
                    &entry.module_path,
                )?;
            }
        }
        Ok(assigned)
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
        methods: &[crate::EmittableMethod],
        ctx: &TypedContext,
        base_idx: u32,
    ) -> Result<(), CodegenError> {
        #[allow(clippy::cast_possible_truncation)]
        for (i, entry) in methods.iter().enumerate() {
            let method_name = arena.def_name(entry.def_id).to_string();
            let key = FnKey::method_in(
                entry.module_path.clone(),
                &entry.struct_name,
                &method_name,
            );

            assert!(
                !self.func_name_to_idx.contains_key(&key),
                "Method '{key}' collides with an \
                 existing top-level method on the same struct."
            );
            self.func_name_to_idx
                .insert(key.clone(), base_idx + i as u32);

            if let Def::Function { returns, .. } = &arena[entry.def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(
                    key,
                    *return_ty_id,
                    arena,
                    ctx,
                    &entry.module_path,
                )?;
            }
        }
        Ok(())
    }

    /// Variant of [`Self::build_method_name_to_idx`] for spec-inner struct
    /// methods. Each `(spec_name, struct_name, def_id)` entry registers under
    /// the mangled internal key `"{spec_name}.{StructName}.{method_name}"`.
    ///
    /// Returns the WASM function index assigned to each entry, parallel to the
    /// input slice (mirrors [`Self::build_func_name_to_idx_with_spec_names`]).
    pub(crate) fn build_method_name_to_idx_with_spec_names(
        &mut self,
        arena: &AstArena,
        spec_methods: &[crate::EmittableSpecMethod],
        ctx: &TypedContext,
        base_idx: u32,
    ) -> Result<Vec<u32>, CodegenError> {
        let mut assigned = Vec::with_capacity(spec_methods.len());
        #[allow(clippy::cast_possible_truncation)]
        for (i, entry) in spec_methods.iter().enumerate() {
            let method_name = arena.def_name(entry.def_id).to_string();
            let key = FnKey::spec_method_folded(
                &entry.module_path,
                &entry.spec_name,
                &entry.struct_name,
                &method_name,
            );

            assert!(
                !self.func_name_to_idx.contains_key(&key),
                "Spec-inner method '{key}' \
                 collides with an existing spec-inner method on the same struct."
            );
            let assigned_idx = base_idx + i as u32;
            self.func_name_to_idx.insert(key.clone(), assigned_idx);
            assigned.push(assigned_idx);

            if let Def::Function { returns, .. } = &arena[entry.def_id].kind
                && let Some(return_ty_id) = returns
            {
                self.register_sret_if_compound(
                    key,
                    *return_ty_id,
                    arena,
                    ctx,
                    &entry.module_path,
                )?;
            }
        }
        Ok(assigned)
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
        module_path: &[String],
    ) -> Result<Option<ValType>, CodegenError> {
        match &arena[ty_id].kind {
            TypeNode::Simple(SimpleTypeKind::Unit) => Ok(None),
            TypeNode::Simple(
                SimpleTypeKind::Bool
                | SimpleTypeKind::I8
                | SimpleTypeKind::U8
                | SimpleTypeKind::I16
                | SimpleTypeKind::U16
                | SimpleTypeKind::I32
                | SimpleTypeKind::U32,
            )
            | TypeNode::Array { .. } => Ok(Some(ValType::I32)),
            TypeNode::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => Ok(Some(ValType::I64)),
            TypeNode::Generic { .. } => todo!(),
            TypeNode::Function { .. } => todo!(),
            // `TypeQualifiedName` is the dead AST variant; the parser produces
            // `Qualified` for every `::`-qualified type. It stays an error rather
            // than a panic for defense-in-depth.
            TypeNode::QualifiedName { .. } => Err(CodegenError::UnsupportedType {
                rendered: arena[ty_id].kind.qualified_path(arena).unwrap_or_default(),
            }),
            TypeNode::Qualified { .. } => {
                // A `::`-qualified type that resolves to a struct or enum is an I32
                // pointer, like a bare struct/enum reference. The type-checker has
                // already validated and bound the path; re-resolving here mirrors
                // the `Custom` arm's defense-in-depth so a malformed path errors at
                // the codegen boundary instead of emitting plausible WASM.
                let path = arena[ty_id].kind.qualified_segments(arena).unwrap_or_default();
                if ctx.qualified_type_is_nominal(&path, module_path) {
                    Ok(Some(ValType::I32))
                } else {
                    Err(CodegenError::UnsupportedType {
                        rendered: path.join("::"),
                    })
                }
            }
            TypeNode::Custom(ident_id) => {
                let name = &arena[*ident_id].name;
                if ctx.lookup_struct_in(name, module_path).is_some()
                    || ctx.lookup_enum_in(name, module_path).is_some()
                {
                    Ok(Some(ValType::I32))
                } else {
                    // The type-checker rejects an unknown type before codegen, so
                    // this is unreachable from a well-formed pipeline. Returning an
                    // error rather than `todo!()` keeps a malformed type from
                    // panicking the compiler (H6 defense-in-depth).
                    Err(CodegenError::UnsupportedType {
                        rendered: name.clone(),
                    })
                }
            }
        }
    }

    /// Translates an AST function definition to a WASM function body.
    ///
    /// Wraps [`Self::visit_function_definition_body`] in a [`SpecScopeGuard`]
    /// so that `current_spec` resets to `None` on every exit path, including
    /// the `?` early return from `compute_frame_layout`. The guard wraps
    /// `&mut self` and forwards via `Deref`/`DerefMut`, which avoids the
    /// partial-borrow conflict that a `&mut self.current_spec` slot guard
    /// would have with `self.<method>(...)` calls inside the body.
    pub(crate) fn visit_function_definition(
        &mut self,
        def_id: DefId,
        arena: &AstArena,
        ctx: &TypedContext,
        method_struct_name: Option<&str>,
        module_path: &[String],
        origin: &FunctionOrigin,
    ) -> Result<(), CodegenError> {
        let spec = match origin {
            FunctionOrigin::SpecInner(name) => Some(name.clone()),
            FunctionOrigin::TopLevel => None,
        };
        let mut guard = SpecScopeGuard::enter(self, spec);
        guard.visit_function_definition_body(def_id, arena, ctx, method_struct_name, module_path, origin)
    }

    #[allow(clippy::too_many_lines)]
    fn visit_function_definition_body(
        &mut self,
        def_id: DefId,
        arena: &AstArena,
        ctx: &TypedContext,
        method_struct_name: Option<&str>,
        module_path: &[String],
        origin: &FunctionOrigin,
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
        // Record which file this function belongs to so struct/enum metadata
        // lookups during lowering resolve bare type names relative to this file
        // (two files may each define a same-named struct).
        self.current_module_path = module_path.to_vec();
        // Compute the structured key for this function so sret lookups
        // and call-site resolution stay in lockstep with the registration
        // variant chosen by `build_*_name_to_idx{,_with_spec_names}`. Every key —
        // including spec items — is qualified by its defining file: `current_spec`
        // is the bare spec name, combined with the spec's defining file
        // (`current_module_path`, set above) into the injective spec key.
        let current_spec = self.current_spec.clone();
        let current_fn_key = match (current_spec.as_deref(), method_struct_name) {
            (Some(spec), Some(struct_name)) => {
                FnKey::spec_method_folded(module_path, spec, struct_name, &raw_name)
            }
            (Some(spec), None) => FnKey::spec_free_folded(module_path, spec, &raw_name),
            (None, Some(struct_name)) => {
                FnKey::method_in(module_path.to_vec(), struct_name, &raw_name)
            }
            (None, None) => FnKey::free_in(module_path.to_vec(), &raw_name),
        };
        // For diagnostics and debug names we keep the mangled-string form
        // (`Struct.method` / bare name); spec-inner-ness is implicit via
        // `current_spec` for any consumer that needs it. The `.` here is the
        // method-name separator, the same one `FnKey::Display` uses.
        let fn_name = if let Some(struct_name) = method_struct_name {
            format!("{struct_name}.{raw_name}")
        } else {
            raw_name
        };
        self.current_fn_name.clone_from(&fn_name);
        self.current_fn_key = Some(current_fn_key.clone());

        let is_array_return = self.func_array_returns.contains_key(&current_fn_key);
        let is_struct_return = self.func_struct_returns.contains_key(&current_fn_key);
        let is_sret = is_array_return || is_struct_return;

        let results: Vec<ValType> = if is_sret {
            vec![]
        } else {
            match returns {
                Some(ty_id) => Self::val_type_from_type_id(arena, ty_id, ctx, module_path)?
                    .into_iter()
                    .collect(),
                None => vec![],
            }
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
                    let vt = Self::val_type_from_type_id(arena, *ty, ctx, module_path)?
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
        let is_top_level = matches!(*origin, FunctionOrigin::TopLevel);
        // The program's WASM ABI is the entry file's public surface: only an
        // entry-file (empty module path) top-level `pub fn` is exported. A `pub
        // fn` in an imported file is intra-project visibility, not an export, so
        // imported internals don't leak into the export section. Methods and
        // spec-inner functions are never exported. A future `export` keyword
        // will control exports explicitly.
        let is_entry_file = module_path.is_empty();
        let is_exportable_position =
            vis == Visibility::Public && !is_method && is_top_level && is_entry_file;
        let should_export = is_exportable_position && !is_main;
        if should_export {
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }
        if is_main && is_exportable_position {
            self.has_main = true;
            self.exports
                .push((fn_name.clone(), ExportKind::Func, self.func_idx));
        }

        Self::pre_scan_locals(arena, body_id, ctx, &mut self.locals_map, &mut local_idx);

        self.frame_layout = Self::compute_frame_layout(
            arena,
            body_id,
            ctx,
            local_idx,
            &args,
            method_struct_name,
            module_path,
        )?;

        // Record the real frame size (0 for frameless functions) keyed by the
        // structured `FnKey` itself, not its lossy `Display` rendering: two
        // distinct keys can render to the same string, so keying on the string
        // would let one function's frame overwrite another's. This map is the
        // interchange format the cross-crate A036 frame-size parity test reads.
        // The key was set above and is always present here.
        let frame_size = self.frame_layout.as_ref().map_or(0, |l| l.total_size);
        self.frame_sizes.insert(
            self.current_fn_key
                .as_ref()
                .expect("current_fn_key is set at the top of visit_function_definition")
                .clone(),
            frame_size,
        );

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

        let has_frame = self.frame_layout.is_some();
        if has_frame {
            local_declarations.push((1, ValType::I32));
        }

        // Reserve an i32 scratch local for the bounds-check guard so a dynamic
        // array index can be single-evaluated via `local.tee` (AD-3). It is
        // reserved iff the function actually contains a dynamic index (the only
        // case that emits a guard), independent of whether a frame exists: an
        // immutable-`self` method like `self.arr[idx]` needs no frame slot yet
        // still emits the guard. Tying the reservation to guard emission keeps
        // constant-index-only functions byte-identical to an unchecked build.
        // The scratch sits at the next free local after the named locals and the
        // optional frame-pointer temp, so its index and its push order agree.
        if self.emit_bounds_checks && Self::body_has_dynamic_array_index(arena, body_id) {
            local_declarations.push((1, ValType::I32));
            self.bounds_check_scratch_local = Some(local_idx + u32::from(has_frame));
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
                            // A struct parameter (bare `Custom` name or a
                            // `::`-qualified path) has a frame slot allocated in
                            // `compute_frame_layout`; copy the caller's data into
                            // it so the callee mutates its own copy (value
                            // semantics). The slot is keyed by the parameter name,
                            // so both forms share the same copy once the layout
                            // pass gave the qualified form a slot.
                            TypeInfoKind::Custom(_)
                            | TypeInfoKind::Qualified(_)
                            | TypeInfoKind::QualifiedName(_) => {
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

        // Lower the function body. A function-body non-deterministic modifier
        // (`fn f() forall { … }`) is recorded as the body block's `block_kind`
        // rather than as a nested block, so — unlike an inline `forall { … }`
        // statement, which flows through `lower_block` — it would otherwise be
        // flattened away here. Emit the matching nondet wrapper so the
        // quantifier survives into the WASM (and therefore the proof-mode Rocq
        // output): a bare top-level `BI_uzumaki_num` has no opsem reduction and
        // cannot be proven `cannot_trap`, whereas a `BI_forall`-wrapped body is
        // discharged by the verifier's `C_forall` + `instance_elem` machinery.
        let block = &arena[body_id];
        let body_nondet_op = match block.block_kind {
            BlockKind::Forall => Some(FORALL_OPCODE),
            BlockKind::Exists => Some(EXISTS_OPCODE),
            BlockKind::Assume => Some(ASSUME_OPCODE),
            BlockKind::Unique => Some(UNIQUE_OPCODE),
            BlockKind::Regular => None,
        };
        let body_stmts: Vec<StmtId> = block.stmts.clone();
        if let Some(op) = body_nondet_op {
            self.emit_nondet_block_start(op);
            self.loop_ctx.wasm_block_depth += 1;
        }
        for stmt_id in body_stmts {
            self.lower_statement(arena, stmt_id, ctx);
        }
        if body_nondet_op.is_some() {
            self.loop_ctx.wasm_block_depth -= 1;
            self.emit_nondet_block_end();
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
        self.bounds_check_scratch_local = None;
        self.loop_ctx = LoopContext::default();
        self.parent_blocks_stack.clear();
        // `current_spec` is reset by `SpecScopeGuard` in the caller.
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

    /// Returns `true` if the function body contains at least one *dynamic* array
    /// index — an `Expr::ArrayIndexAccess` whose index is not a numeric literal.
    ///
    /// A non-`NumberLiteral` index is exactly the case that takes the dynamic
    /// branch of [`Self::emit_index_offset`] and therefore emits a bounds-check
    /// guard. The bounds-check scratch local is reserved iff this returns `true`
    /// (and `emit_bounds_checks` is set), so functions that only index by
    /// constants reserve no scratch and stay byte-identical to an unchecked
    /// build, while a dynamic index — even through an immutable-`self` method
    /// like `self.arr[idx]` that needs no frame slot — still gets its scratch.
    ///
    /// The walk mirrors [`Self::pre_scan_locals`]' block descent (regular,
    /// `if`/`else`, `loop`, and non-det blocks all flow through `Stmt::Block`)
    /// and additionally descends into every sub-expression so nested forms such
    /// as `m[i][j]`, `arr[idx].x`, and indices inside calls are not missed.
    fn body_has_dynamic_array_index(arena: &AstArena, block_id: BlockId) -> bool {
        arena[block_id].stmts.iter().any(|&stmt_id| {
            match &arena[stmt_id].kind {
                Stmt::Expr(e) | Stmt::Return { expr: e } | Stmt::Assert { expr: e } => {
                    Self::expr_has_dynamic_array_index(arena, *e)
                }
                Stmt::Assign { left, right } => {
                    Self::expr_has_dynamic_array_index(arena, *left)
                        || Self::expr_has_dynamic_array_index(arena, *right)
                }
                Stmt::VarDef { value, .. } => value
                    .as_ref()
                    .is_some_and(|&v| Self::expr_has_dynamic_array_index(arena, v)),
                Stmt::Block(inner) => Self::body_has_dynamic_array_index(arena, *inner),
                Stmt::If {
                    condition,
                    then_block,
                    else_block,
                } => {
                    Self::expr_has_dynamic_array_index(arena, *condition)
                        || Self::body_has_dynamic_array_index(arena, *then_block)
                        || else_block
                            .as_ref()
                            .is_some_and(|&e| Self::body_has_dynamic_array_index(arena, e))
                }
                Stmt::Loop { condition, body } => {
                    condition
                        .as_ref()
                        .is_some_and(|&c| Self::expr_has_dynamic_array_index(arena, c))
                        || Self::body_has_dynamic_array_index(arena, *body)
                }
                Stmt::Break | Stmt::TypeDef { .. } | Stmt::ConstDef(_) => false,
            }
        })
    }

    /// Recursively reports whether `expr_id` (or any sub-expression) is an
    /// `ArrayIndexAccess` with a non-literal index. Supporting helper for
    /// [`Self::body_has_dynamic_array_index`].
    fn expr_has_dynamic_array_index(arena: &AstArena, expr_id: ExprId) -> bool {
        match &arena[expr_id].kind {
            Expr::ArrayIndexAccess { array, index } => {
                !matches!(arena[*index].kind, Expr::NumberLiteral { .. })
                    || Self::expr_has_dynamic_array_index(arena, *array)
                    || Self::expr_has_dynamic_array_index(arena, *index)
            }
            Expr::Binary { left, right, .. } => {
                Self::expr_has_dynamic_array_index(arena, *left)
                    || Self::expr_has_dynamic_array_index(arena, *right)
            }
            Expr::PrefixUnary { expr, .. }
            | Expr::Parenthesized { expr }
            | Expr::MemberAccess { expr, .. }
            | Expr::TypeMemberAccess { expr, .. } => {
                Self::expr_has_dynamic_array_index(arena, *expr)
            }
            Expr::FunctionCall { function, args, .. } => {
                Self::expr_has_dynamic_array_index(arena, *function)
                    || args
                        .iter()
                        .any(|(_, arg)| Self::expr_has_dynamic_array_index(arena, *arg))
            }
            Expr::StructLiteral { fields, .. } => fields
                .iter()
                .any(|(_, value)| Self::expr_has_dynamic_array_index(arena, *value)),
            Expr::ArrayLiteral { elements } => elements
                .iter()
                .any(|&e| Self::expr_has_dynamic_array_index(arena, e)),
            Expr::Identifier(_)
            | Expr::NumberLiteral { .. }
            | Expr::BoolLiteral { .. }
            | Expr::StringLiteral { .. }
            | Expr::UnitLiteral
            | Expr::Uzumaki
            | Expr::Type(_) => false,
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
        module_path: &[String],
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
                            let elem_sz = type_byte_size(&elem_type.kind, ctx, module_path)?;
                            let byte_count = elem_sz.checked_mul(*length).expect(
                                "Array byte count overflow: element size * length exceeds u32::MAX",
                            );
                            let align =
                                natural_alignment_for_type(&elem_type.kind, ctx, module_path)?;
                            let aligned_offset = align_to(current_offset, align);
                            let element_layout = compute_element_layout_if_struct(
                                &elem_type.kind,
                                ctx,
                                module_path,
                            )?;
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
                        // A struct parameter is passed by value, so it needs its
                        // own frame slot to copy the caller's data into on entry.
                        // `Custom` carries a bare name and `Qualified`/`QualifiedName`
                        // a `::`-joined path; both name a struct that must be
                        // resolved relative to the defining file (a same-named
                        // struct in another file has a different layout), so the
                        // shared resolver is used rather than a bare-name lookup.
                        TypeInfoKind::Custom(_)
                        | TypeInfoKind::Qualified(_)
                        | TypeInfoKind::QualifiedName(_) => {
                            if let Some((struct_info, _defining_path)) =
                                memory::resolve_struct_with_defining_path(
                                    &type_info.kind,
                                    ctx,
                                    module_path,
                                )
                            {
                                let (total_size, field_slots) =
                                    compute_struct_field_layout(&struct_info, ctx, module_path)?;
                                if total_size > 0 {
                                    let max_field_align =
                                        memory::max_struct_alignment(&field_slots);
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
                    if let Some(struct_info) = ctx.lookup_struct_in(struct_name, module_path) {
                        let (total_size, field_slots) =
                            compute_struct_field_layout(&struct_info, ctx, module_path)?;
                        if total_size > 0 {
                            let max_field_align = memory::max_struct_alignment(&field_slots);
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
            module_path,
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

    /// Allocates a single frame slot for a named binding (either a `let` or
    /// `const`) whose declared type is compound (array or struct).
    ///
    /// Scalar bindings (including enum tags) produce no frame slot and are
    /// intentionally no-ops here — they are tracked by `pre_scan_locals` as
    /// WASM locals instead. Zero-sized structs also produce no slot.
    #[allow(clippy::too_many_arguments)]
    fn collect_compound_slot_for_type(
        arena: &AstArena,
        name_id: IdentId,
        type_kind: &TypeInfoKind,
        ctx: &TypedContext,
        array_offsets: &mut FxHashMap<String, ArraySlot>,
        struct_offsets: &mut FxHashMap<String, StructSlot>,
        current_offset: &mut u32,
        module_path: &[String],
    ) -> Result<(), CodegenError> {
        match type_kind {
            TypeInfoKind::Array(elem_type, length) => {
                let elem_sz = type_byte_size(&elem_type.kind, ctx, module_path)?;
                let byte_count = elem_sz.checked_mul(*length).expect(
                    "Array byte count overflow: element size * length exceeds u32::MAX",
                );
                let align = natural_alignment_for_type(&elem_type.kind, ctx, module_path)?;
                let aligned_offset = align_to(*current_offset, align);
                let element_layout =
                    compute_element_layout_if_struct(&elem_type.kind, ctx, module_path)?;
                let slot = ArraySlot {
                    offset: aligned_offset,
                    elem_size: elem_sz,
                    length: *length,
                    element_layout,
                };
                let binding_name = arena[name_id].name.clone();
                array_offsets.insert(binding_name, slot);
                *current_offset = aligned_offset.checked_add(byte_count).expect(
                    "Frame offset overflow: total array allocation exceeds u32::MAX",
                );
            }
            TypeInfoKind::Struct(struct_name, _) | TypeInfoKind::Custom(struct_name) => {
                // A `Struct` carries the defining-file canonical key; prefer it.
                // The bare name alone is not enough when the binding's type was
                // reached by a `::`-qualifier (`let p: lib::geom::Point`): the leaf
                // `Point` is not bound by name in the accessing file, so resolving
                // the bare name against `module_path` would miss the layout. The
                // canonical key identifies the struct by its defining file (#63).
                let struct_info = match type_kind {
                    TypeInfoKind::Struct(_, key) => ctx
                        .lookup_struct(key)
                        .or_else(|| ctx.lookup_struct_in(struct_name, module_path)),
                    _ => ctx.lookup_struct_in(struct_name, module_path),
                };
                debug_assert!(
                    struct_info.is_some()
                        || matches!(type_kind, TypeInfoKind::Custom(_))
                            && ctx.lookup_enum_in(struct_name, module_path).is_some(),
                    "collect_compound_slot_for_type: unresolved Struct/Custom type '{struct_name}' — \
                     type checker should reject unresolved names before codegen",
                );
                if let Some(struct_info) = struct_info {
                    let (total_size, field_slots) =
                        compute_struct_field_layout(&struct_info, ctx, module_path)?;
                    if total_size > 0 {
                        let max_field_align = memory::max_struct_alignment(&field_slots);
                        let aligned_offset = align_to(*current_offset, max_field_align);
                        let slot = StructSlot {
                            offset: aligned_offset,
                            total_size,
                            fields: field_slots,
                        };
                        let binding_name = arena[name_id].name.clone();
                        struct_offsets.insert(binding_name, slot);
                        *current_offset = aligned_offset.checked_add(total_size).expect(
                            "Frame offset overflow: struct allocation exceeds u32::MAX",
                        );
                    }
                }
            }
            // Scalars (incl. enum tags) and zero-sized structs: no frame slot needed.
            _ => {}
        }
        Ok(())
    }

    /// Recursively walks a block collecting array and struct variable declarations.
    ///
    /// Enum types are intentionally excluded — they are pure i32 scalars with no
    /// linear memory footprint, so they do not need frame slots.
    fn collect_compound_slots(
        arena: &AstArena,
        block_id: BlockId,
        ctx: &TypedContext,
        array_offsets: &mut FxHashMap<String, ArraySlot>,
        struct_offsets: &mut FxHashMap<String, StructSlot>,
        current_offset: &mut u32,
        module_path: &[String],
    ) -> Result<(), CodegenError> {
        let block = &arena[block_id];
        for &stmt_id in &block.stmts {
            match &arena[stmt_id].kind {
                Stmt::VarDef { name, .. } => {
                    let type_info = ctx
                        .get_node_typeinfo(NodeId::Stmt(stmt_id))
                        .expect("Variable definition must have type info");
                    Self::collect_compound_slot_for_type(
                        arena,
                        *name,
                        &type_info.kind,
                        ctx,
                        array_offsets,
                        struct_offsets,
                        current_offset,
                        module_path,
                    )?;
                }
                Stmt::ConstDef(const_def_id) => {
                    if let Def::Constant { name, .. } = &arena[*const_def_id].kind {
                        let type_info = ctx
                            .get_node_typeinfo(NodeId::Stmt(stmt_id))
                            .expect("Constant definition must have type info");
                        Self::collect_compound_slot_for_type(
                            arena,
                            *name,
                            &type_info.kind,
                            ctx,
                            array_offsets,
                            struct_offsets,
                            current_offset,
                            module_path,
                        )?;
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
                        module_path,
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
                        module_path,
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
                            module_path,
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
                        module_path,
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
                    let name = resolved.display_name();
                    assert!(
                        !self.callee_is_sret(&resolved),
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
                match value {
                    None => todo!("Uninitialized variable definitions are not supported"),
                    Some(val_expr_id) => {
                        self.lower_named_binding_init(
                            arena,
                            &var_name,
                            val_expr_id,
                            stmt_id,
                            ctx,
                        );
                    }
                }
            }
            Stmt::TypeDef { .. } => todo!(),
            Stmt::Assert { expr } => {
                self.lower_assert_statement(arena, expr, ctx);
            }
            Stmt::ConstDef(const_def_id) => {
                cov_mark::hit!(wasm_codegen_emit_constant_definition);
                if let Def::Constant { name, value, .. } = &arena[const_def_id].kind {
                    let const_name = arena[*name].name.clone();
                    self.lower_named_binding_init(arena, &const_name, *value, stmt_id, ctx);
                }
            }
        }
    }

    /// Lowers the initializer of a `let`/`const` binding, dispatching among the
    /// sret, array-copy, struct-copy, and scalar/literal paths. AD-1 / AD-5
    /// commit to byte-identical WASM emission between function-scoped `const`
    /// and the equivalent immutable `let`; this helper is the single dispatch
    /// site that both arms route through.
    fn lower_named_binding_init(
        &mut self,
        arena: &AstArena,
        name: &str,
        value_expr_id: ExprId,
        stmt_id: StmtId,
        ctx: &TypedContext,
    ) {
        let (local_idx, _) = self
            .locals_map
            .get(name)
            .expect("Binding local not found in pre-scan");
        let local_idx = *local_idx;

        cov_mark::hit!(wasm_codegen_const_typeinfo_lookup);
        let type_info = ctx.get_node_typeinfo(NodeId::Stmt(stmt_id));
        let is_array_type = matches!(
            type_info.as_ref().map(|ti| &ti.kind),
            Some(TypeInfoKind::Array(_, _))
        );
        let is_struct_type = matches!(
            type_info.as_ref().map(|ti| &ti.kind),
            Some(TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_))
        ) && self
            .frame_layout
            .as_ref()
            .is_some_and(|layout| layout.struct_offsets.contains_key(name));
        let is_compound_type = is_array_type || is_struct_type;

        let is_sret_call =
            is_compound_type && self.is_sret_function_call(arena, value_expr_id, ctx);

        let is_array_copy = is_array_type
            && matches!(
                arena[value_expr_id].kind,
                Expr::Identifier(_) | Expr::ArrayIndexAccess { .. } | Expr::MemberAccess { .. }
            );

        let is_struct_copy = is_struct_type
            && matches!(
                arena[value_expr_id].kind,
                Expr::Identifier(_) | Expr::MemberAccess { .. } | Expr::ArrayIndexAccess { .. }
            );

        if is_sret_call {
            self.lower_sret_var_init(arena, value_expr_id, local_idx, name, ctx);
        } else if is_array_copy {
            cov_mark::hit!(wasm_codegen_emit_array_copy);
            self.lower_array_copy_var_init(arena, value_expr_id, local_idx, name, ctx);
        } else if is_struct_copy {
            cov_mark::hit!(wasm_codegen_emit_struct_copy);
            self.lower_struct_copy_var_init(arena, value_expr_id, local_idx, name, ctx);
        } else {
            // init_zero_elision must not leak past lower_expression; reset before LocalSet runs.
            self.init_zero_elision = true;
            self.lower_expression(arena, value_expr_id, ctx, Some(name));
            self.init_zero_elision = false;
            self.func().instruction(&Instruction::LocalSet(local_idx));
        }
    }

    /// Checks whether an expression is a function call to an sret function (array or struct return).
    fn is_sret_function_call(&self, arena: &AstArena, expr_id: ExprId, ctx: &TypedContext) -> bool {
        if let Expr::FunctionCall { function, .. } = &arena[expr_id].kind
            && let Some(resolved) = self.resolve_function_callee(arena, *function, ctx)
        {
            return self.callee_is_sret(&resolved);
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
                .resolve_callee(&resolved)
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
                let variant_name = &arena[variant_name_id].name;
                // The type checker keyed this node's enum type by the enum's
                // defining file. For a namespace-qualified variant
                // (`geo::Color::Blue`) the type expression is itself a `::` path the
                // bare-name extractor cannot read, so resolve the enum by its
                // canonical key first; fall back to the bare type name for a local
                // `Enum::Variant`, keeping single-file output identical.
                let enum_info = match ctx.get_node_typeinfo(NodeId::Expr(expr_id)).map(|t| t.kind) {
                    Some(TypeInfoKind::Enum(_, key)) => ctx.lookup_enum(&key),
                    _ => None,
                }
                .or_else(|| {
                    let type_name = Self::extract_type_name_from_type_expr(arena, type_expr)?;
                    ctx.lookup_enum_in(&type_name, &self.current_module_path)
                });

                if let Some(enum_info) = enum_info {
                    let tag = enum_info
                        .variant_index(variant_name)
                        .expect("TypeMemberAccess: unknown enum variant");
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let tag_i32 = tag as i32;
                    self.func().instruction(&Instruction::I32Const(tag_i32));
                } else {
                    let type_name = Self::extract_type_name_from_type_expr(arena, type_expr)
                        .unwrap_or_else(|| "<qualified>".to_string());
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
                    Some(ResolvedCallee::AssociatedFunction { key, .. }) => {
                        self.lower_associated_function_call(arena, &key, &args, ctx, None);
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
                    Some(ResolvedCallee::QualifiedFunction(ref key)) => {
                        match self.lower_qualified_function_call(arena, key, &args, ctx) {
                            Ok(()) => {}
                            Err(CodegenError::UnknownFunction(key)) => {
                                panic!(
                                    "Function '{key}' not found in name-to-index map; \
                                     the type-checker should have caught this — a qualified \
                                     path to a proof-only spec function is rejected there"
                                )
                            }
                            Err(e) => panic!("qualified function call lowering failed: {e}"),
                        }
                    }
                    None => {
                        // The callee did not resolve to any known call form. The
                        // only way to reach here is a call the type-checker should
                        // have rejected — notably a qualified path to a proof-only
                        // `spec`-inner function or `spec`-inner-struct associated
                        // function, which has no executable index. Higher-order
                        // calls are not a language feature, so this is never valid
                        // input; fail loudly rather than emit a malformed module.
                        panic!(
                            "function call callee did not resolve to a lowerable form; \
                             the type-checker should have rejected this call (a qualified \
                             path to a proof-only spec function is rejected there)"
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
                self.lower_array_literal(arena, expr_id, &elements, var_name, ctx);
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
                    | TypeInfoKind::Enum(_, _) => {
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
                    TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
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
        // The type checker resolved every cross-file call — including paths that
        // cross `pub use` re-exports — to the callee's defining file. When the
        // callee lives in a *different* file than the one being compiled, trust
        // that recorded target: it names the file the registration pass keyed
        // the function under, which the call site's own bare-name resolution
        // cannot reach (an item import binds a name whose definition is
        // elsewhere; a qualified path crosses re-exports). A same-file call is
        // left to the normal resolution below so its lowering — and byte
        // output — is identical to a single-file program. Trusting the recorded
        // target for a `TypeMemberAccess` chain also distinguishes a qualified
        // function path (`math::arith::add`) from a struct associated function
        // (`Point::new`), which share that expression shape.
        // A namespace-qualified associated call (`geo::Point::new(...)`) was
        // resolved by the type checker to a struct method whose mangled name is
        // keyed by the struct's defining file. Its `TypeMemberAccess` expression
        // (`geo::Point::new`) is not a plain type the bare-name resolution can
        // read, so the recorded target — carrying the struct name and defining
        // file — is the source of truth.
        // A cross-file associated call — a namespace-qualified `geo::Point::new`,
        // an item-imported `A::make()` whose `A` is defined elsewhere, or an
        // entry-file assoc reached via `root::Type::m()`. The recorded target
        // names the struct's defining file, which the call site's bare-name
        // resolution cannot reach. A *same-file* associated call (`module_path ==
        // current`) is left to the `TypeMemberAccess` arm below so its spec-aware
        // lookup still finds a spec-inner struct's associated function (registered
        // as a `SpecMethod`, not a top-level `Method`); routing it by a plain
        // `method_in` key here would miss that registration.
        if let Some(target) = ctx.call_target(function_expr_id)
            && let Some(struct_name) = &target.receiver_struct
            && target.module_path != self.current_module_path
            && matches!(&arena[function_expr_id].kind, Expr::TypeMemberAccess { .. })
        {
            let key = FnKey::method_in(
                target.module_path.clone(),
                struct_name.clone(),
                target.name.clone(),
            );
            return Some(ResolvedCallee::AssociatedFunction { key });
        }
        // A cross-file *free* function reached by item import or bare path. An
        // instance-method call (`recv.method()`) also records a cross-file target
        // now that dispatch is canonical-key-driven, but it carries a receiver
        // struct and must lower as a method (via the `MemberAccess` arm below),
        // not a free function — so this free-function branch excludes it.
        if let Some(target) = ctx.call_target(function_expr_id)
            && target.receiver_struct.is_none()
            && target.module_path != self.current_module_path
        {
            let key = FnKey::free_in(target.module_path.clone(), target.name.clone());
            return Some(ResolvedCallee::QualifiedFunction(key));
        }
        // A same-file free function reached through a `::` namespace path —
        // `root::helper()` for an entry item imported via `use root;` (#63). The
        // recorded target is a free function (no receiver struct) whose defining
        // file is the one being compiled, so the cross-file branch above did not
        // fire. Its `TypeMemberAccess` expression (`root::helper`) is not a struct
        // associated function, so route it by key rather than letting the
        // `TypeMemberAccess` arm below mis-resolve it as a method and return
        // `None`. The key equals the one a bare `helper()` call resolves to, so
        // the lowering is identical.
        if let Some(target) = ctx.call_target(function_expr_id)
            && target.receiver_struct.is_none()
            && matches!(&arena[function_expr_id].kind, Expr::TypeMemberAccess { .. })
        {
            let key = FnKey::free_in(target.module_path.clone(), target.name.clone());
            return Some(ResolvedCallee::QualifiedFunction(key));
        }
        match &arena[function_expr_id].kind {
            Expr::Identifier(ident_id) => {
                Some(ResolvedCallee::Function(arena[*ident_id].name.clone()))
            }
            Expr::TypeMemberAccess {
                expr: type_expr,
                name: method_name,
            } => {
                let key = self.resolve_associated_fn_key(arena, *type_expr, *method_name, ctx)?;
                Some(ResolvedCallee::AssociatedFunction { key })
            }
            Expr::MemberAccess {
                expr: receiver,
                name: method_name,
            } => {
                let key =
                    self.resolve_method_fn_key(arena, *receiver, *method_name, ctx)?;
                Some(ResolvedCallee::InstanceMethod {
                    key,
                    receiver_expr_id: *receiver,
                    method_name_id: *method_name,
                })
            }
            _ => None,
        }
    }

    /// Lowers a plain function call to a WASM `call` instruction.
    ///
    /// When called from within a spec-inner function the lookup tries the
    /// mangled `"<spec>.<callee>"` key first so sibling calls resolve to the
    /// spec's own definition before falling back to a top-level fn of the
    /// same bare name.
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

        // An `external fn` call targets its import index (0..N) rather than a
        // local function index. Imports never participate in spec-mangled
        // lookup, so this probe precedes the free-callee resolution.
        if let Some(&import_idx) = self.extern_name_to_idx.get(callee_name) {
            cov_mark::hit!(wasm_codegen_emit_extern_call);
            self.func().instruction(&Instruction::Call(import_idx));
            return Ok(());
        }

        let func_idx = self
            .resolve_free_callee_idx(callee_name)
            .ok_or_else(|| CodegenError::UnknownFunction(callee_name.to_owned()))?;

        self.func().instruction(&Instruction::Call(func_idx));
        Ok(())
    }

    /// Lowers a call whose target was resolved by the type checker to a specific
    /// (possibly cross-file) function, identified by its already file-qualified
    /// [`FnKey`]. Used for item-imported bare calls and qualified paths.
    ///
    /// A missing index is an internal invariant violation: the type checker
    /// rejects every call it cannot lower (a qualified path to a proof-only spec
    /// function among them), so by the time codegen runs the key must be
    /// registered. This returns [`CodegenError::UnknownFunction`] rather than
    /// panicking on a miss, keeping codegen from crashing if a future change ever
    /// reopens a path the type checker forgot to gate.
    fn lower_qualified_function_call(
        &mut self,
        arena: &AstArena,
        key: &FnKey,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
    ) -> Result<(), CodegenError> {
        cov_mark::hit!(wasm_codegen_emit_qualified_function_call);

        for (_label, arg_expr_id) in call_args {
            self.lower_expression(arena, *arg_expr_id, ctx, None);
        }

        let func_idx = self
            .resolve_idx_by_key(key)
            .ok_or_else(|| CodegenError::UnknownFunction(key.to_string()))?;

        self.func().instruction(&Instruction::Call(func_idx));
        Ok(())
    }

    /// Resolves a free-function bare name to its WASM function index,
    /// preferring the spec-mangled key when inside a spec scope, then the
    /// current file's qualified key. This is the fallback for a same-file call
    /// the type checker did not record a target for; cross-file targets resolve
    /// through [`ResolvedCallee::QualifiedFunction`].
    fn resolve_free_callee_idx(&self, callee_name: &str) -> Option<u32> {
        if let Some(spec) = self.current_spec.as_deref() {
            let key = FnKey::spec_free_folded(&self.current_module_path, spec, callee_name);
            if let Some(idx) = self.func_name_to_idx.get(&key).copied() {
                return Some(idx);
            }
        }
        let key = FnKey::free_in(self.current_module_path.clone(), callee_name);
        self.func_name_to_idx.get(&key).copied()
    }

    /// Resolves a callee `FnKey` directly to its WASM function index. Used
    /// when the key has already been determined (method calls).
    fn resolve_idx_by_key(&self, key: &FnKey) -> Option<u32> {
        self.func_name_to_idx.get(key).copied()
    }

    /// Returns `true` if a free-function bare name resolves to a function
    /// with sret (array or struct) return, accounting for the current spec
    /// scope.
    fn is_sret_free(&self, callee_name: &str) -> bool {
        if let Some(spec) = self.current_spec.as_deref() {
            let key = FnKey::spec_free_folded(&self.current_module_path, spec, callee_name);
            if self.func_array_returns.contains_key(&key)
                || self.func_struct_returns.contains_key(&key)
            {
                return true;
            }
        }
        let key = FnKey::free_in(self.current_module_path.clone(), callee_name);
        self.func_array_returns.contains_key(&key)
            || self.func_struct_returns.contains_key(&key)
    }

    /// Returns `true` if a [`FnKey`] resolves to a function with sret
    /// (array or struct) return.
    fn is_sret_by_key(&self, key: &FnKey) -> bool {
        self.func_array_returns.contains_key(key)
            || self.func_struct_returns.contains_key(key)
    }

    /// Resolves a callee for either case in [`ResolvedCallee`]: free
    /// functions take the spec-aware free-name path, methods look up by
    /// the already-determined [`FnKey`].
    fn resolve_callee(&self, resolved: &ResolvedCallee) -> Option<u32> {
        match resolved {
            ResolvedCallee::Function(name) => self.resolve_free_callee_idx(name),
            ResolvedCallee::QualifiedFunction(key)
            | ResolvedCallee::AssociatedFunction { key, .. }
            | ResolvedCallee::InstanceMethod { key, .. } => self.resolve_idx_by_key(key),
        }
    }

    /// Returns `true` if a [`ResolvedCallee`] resolves to an sret function.
    fn callee_is_sret(&self, resolved: &ResolvedCallee) -> bool {
        match resolved {
            ResolvedCallee::Function(name) => self.is_sret_free(name),
            ResolvedCallee::QualifiedFunction(key)
            | ResolvedCallee::AssociatedFunction { key, .. }
            | ResolvedCallee::InstanceMethod { key, .. } => self.is_sret_by_key(key),
        }
    }

    /// Resolves the registered [`FnKey`] for an instance method call.
    ///
    /// Given the receiver expression and method name, determines the receiver's struct type
    /// from the type context and probes [`Self::func_name_to_idx`] via
    /// [`Self::lookup_method_fn_key`]. When the call site lives inside a spec
    /// scope, the `FnKey::SpecMethod` candidate is tried first; otherwise the
    /// top-level `FnKey::Method` candidate is used. Returns `None` if the
    /// receiver has no type info or the method is not registered.
    fn resolve_method_fn_key(
        &self,
        arena: &AstArena,
        receiver_expr_id: ExprId,
        method_name_id: IdentId,
        ctx: &TypedContext,
    ) -> Option<FnKey> {
        let method_name = &arena[method_name_id].name;
        let receiver_type = ctx.get_node_typeinfo(NodeId::Expr(receiver_expr_id))?;
        // The receiver carries its struct's canonical identity (`Struct(bare, key)`).
        // Resolving the method by the key — rather than the bare name from the call
        // site's scope — keeps dispatch on the value's actual struct when a
        // same-named struct exists in another file. A keyless `Custom` receiver
        // (spec-inner or forward reference) has no canonical key, so it falls back to
        // the call-site bare-name path.
        match &receiver_type.kind {
            TypeInfoKind::Struct(struct_name, canonical_key) => {
                self.lookup_method_fn_key_by_key(struct_name, canonical_key, method_name, ctx)
            }
            TypeInfoKind::Custom(struct_name) => {
                self.lookup_method_fn_key(struct_name, method_name, ctx)
            }
            _ => None,
        }
    }

    /// Returns the defining-file module path of the struct named `struct_name`
    /// as seen from the file currently being compiled. A method's mangled name
    /// is qualified by the struct's defining file; if the struct can't be
    /// resolved (it should always resolve post-type-check) the current file is
    /// assumed so single-file behavior is unaffected.
    fn struct_defining_module_path(&self, struct_name: &str, ctx: &TypedContext) -> Vec<String> {
        ctx.struct_module_path(struct_name, &self.current_module_path)
            .unwrap_or_else(|| self.current_module_path.clone())
    }

    /// Spec-aware method lookup. Constructs the candidate [`FnKey`] directly
    /// (the value `FnKey` already encodes the (spec, struct, method) triple
    /// that used to live in a separate `method_mangled_names` lookup map) and
    /// probes [`Self::func_name_to_idx`] for it. When a spec is active, the
    /// `SpecMethod` candidate is tried first so an intra-spec method call
    /// resolves to the spec-inner registration; otherwise — or on miss — the
    /// top-level `Method` candidate is tried. The top-level `Method` key is
    /// qualified by the struct's defining file so a method on a struct in an
    /// imported file resolves to that file's registration.
    fn lookup_method_fn_key(
        &self,
        struct_name: &str,
        method_name: &str,
        ctx: &TypedContext,
    ) -> Option<FnKey> {
        if let Some(spec) = self.current_spec.as_deref() {
            let candidate =
                FnKey::spec_method_folded(&self.current_module_path, spec, struct_name, method_name);
            if self.func_name_to_idx.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        let module_path = self.struct_defining_module_path(struct_name, ctx);
        let candidate = FnKey::method_in(module_path, struct_name, method_name);
        self.func_name_to_idx
            .contains_key(&candidate)
            .then_some(candidate)
    }

    /// Spec-aware method lookup keyed by the receiver's **canonical struct key**.
    ///
    /// Unlike [`Self::lookup_method_fn_key`], the file-qualified `Method`
    /// candidate is tried *before* the active spec's `SpecMethod` candidate. The
    /// receiver's canonical key (`lib::ext::Helper`) names the exact struct the
    /// value has, so the method registered under that struct's own defining file
    /// is the authoritative target — even inside a spec that happens to define its
    /// own same-named struct. Probing the spec first would resolve a cross-file
    /// `lib::ext::Helper.tag` to the spec's own `Helper.tag` (a wrong callee, and
    /// an out-of-bounds field load when the layouts differ), since the spec probe
    /// keys only by the bare name.
    ///
    /// The spec probe is reached only as a fallback, when no file-qualified
    /// `Method` candidate exists for the receiver's struct: that is exactly the
    /// case of a spec-inner struct (whose methods register as `SpecMethod`, never
    /// as a top-level `Method`), so the spec's own inner-struct method still
    /// dispatches to itself with no over-correction.
    ///
    /// A key that does not resolve to a defining file (defensive: it always should
    /// post-type-check) falls back to the bare-name path so behavior is never
    /// worse than before.
    fn lookup_method_fn_key_by_key(
        &self,
        struct_name: &str,
        canonical_key: &str,
        method_name: &str,
        ctx: &TypedContext,
    ) -> Option<FnKey> {
        let Some(module_path) = ctx.module_path_of_struct_key(canonical_key) else {
            // A struct receiver always carries a key that resolves post-type-check;
            // a miss means an upstream invariant broke. The bare-name path is the
            // safe degrade, but surface the violation in test/debug builds.
            debug_assert!(
                false,
                "struct receiver `{struct_name}` has canonical key `{canonical_key}` \
                 with no resolvable defining struct after type-checking"
            );
            return self.lookup_method_fn_key(struct_name, method_name, ctx);
        };
        let method_candidate = FnKey::method_in(module_path, struct_name, method_name);
        if self.func_name_to_idx.contains_key(&method_candidate) {
            return Some(method_candidate);
        }
        // No top-level method for this struct identity: the receiver is the active
        // spec's own inner struct (its methods register only as `SpecMethod`).
        if let Some(spec) = self.current_spec.as_deref() {
            let candidate =
                FnKey::spec_method_folded(&self.current_module_path, spec, struct_name, method_name);
            if self.func_name_to_idx.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
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

        let fn_key = self
            .resolve_method_fn_key(arena, receiver_expr_id, method_name_id, ctx)
            .unwrap_or_else(|| {
                let method_name = &arena[method_name_id].name;
                panic!(
                    "Instance method call: could not resolve mangled name for method \
                     '{method_name}' (receiver has no type info or non-struct type)"
                )
            });

        let is_sret = self.is_sret_by_key(&fn_key);

        let func_idx = self.resolve_idx_by_key(&fn_key).unwrap_or_else(|| {
            panic!("Method '{fn_key}' not found in func_name_to_idx")
        });

        if is_sret {
            cov_mark::hit!(wasm_codegen_emit_instance_method_sret);
            let sret_idx = sret_local.unwrap_or_else(|| {
                panic!(
                    "Instance method call to compound-returning method '{fn_key}' \
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

    /// Resolves the registered [`FnKey`] for an associated function call (`Type::method()`).
    ///
    /// Extracts the type name from the expression, then performs the same
    /// spec-aware [`FnKey`] candidate probe as [`Self::lookup_method_fn_key`].
    /// Returns `None` if the type name cannot be extracted or the method is
    /// not registered in either scope.
    fn resolve_associated_fn_key(
        &self,
        arena: &AstArena,
        type_expr_id: ExprId,
        method_name_id: IdentId,
        ctx: &TypedContext,
    ) -> Option<FnKey> {
        let type_name = Self::extract_type_name_from_type_expr(arena, type_expr_id)?;
        let method_name = &arena[method_name_id].name;
        self.lookup_method_fn_key(&type_name, method_name, ctx)
    }

    /// Lowers an associated function call (`Type::method(args)`) to WASM instructions.
    ///
    /// Associated functions have no `self` parameter. The callee is resolved via
    /// the type name and method name, probed against [`Self::func_name_to_idx`],
    /// and called with only the user-provided arguments.
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
        fn_key: &FnKey,
        call_args: &[(Option<IdentId>, ExprId)],
        ctx: &TypedContext,
        sret_local: Option<u32>,
    ) {
        cov_mark::hit!(wasm_codegen_emit_associated_function_call);

        let is_sret = self.is_sret_by_key(fn_key);

        let func_idx = self.resolve_idx_by_key(fn_key).unwrap_or_else(|| {
            panic!("Method '{fn_key}' not found in func_name_to_idx")
        });

        if is_sret {
            cov_mark::hit!(wasm_codegen_emit_associated_function_sret);
            let sret_idx = sret_local.unwrap_or_else(|| {
                panic!(
                    "Associated function call to compound-returning method '{fn_key}' \
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
        // sret metadata uses the structured `current_fn_key` so spec-inner
        // and top-level functions / methods with identical bare names look
        // up to the right metadata without any per-call string rebuilding.
        // `current_fn_key` is set at the top of `visit_function_definition`
        // and this method is only reachable from inside that body — so
        // `Option::expect` here documents the invariant without runtime cost.
        let self_key = self
            .current_fn_key
            .clone()
            .expect("lower_sret_return called outside `visit_function_definition`");
        if let Some(return_info) = self.func_array_returns.get(&self_key).cloned() {
            self.lower_array_sret_return(arena, return_expr_id, sret_idx, ctx, &return_info)
        } else if let Some(return_info) = self.func_struct_returns.get(&self_key).cloned() {
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
            TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_)
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
                    let field_slots = compute_element_layout_if_struct(
                        &return_info.elem_kind,
                        ctx,
                        &self.current_module_path,
                    )
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
                    &return_info.struct_name,
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

        let receiver_expr = match &resolved {
            ResolvedCallee::InstanceMethod {
                receiver_expr_id, ..
            } => Some(*receiver_expr_id),
            _ => None,
        };

        if self.callee_is_sret(&resolved) {
            self.func().instruction(&Instruction::LocalGet(sret_idx));

            if let Some(receiver) = receiver_expr {
                self.lower_expression(arena, receiver, ctx, None);
            }

            for (_label, arg_expr_id) in &args {
                self.lower_expression(arena, *arg_expr_id, ctx, None);
            }
            let func_idx = self
                .resolve_callee(&resolved)
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
            TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_) | TypeInfoKind::Array(_, _)
        );

        let array_len = Self::array_length(array_expr_id, ctx);

        if is_compound_element {
            let elem_sz = type_byte_size(&elem_type_info.kind, ctx, &self.current_module_path)
                .expect("array index write: type_byte_size failed for compound element");

            // dest: array_base + index * struct_size
            self.lower_expression(arena, array_expr_id, ctx, None);
            self.emit_index_offset(arena, index_expr_id, elem_sz, array_len, ctx);
            // src: RHS expression (struct pointer)
            self.lower_expression(arena, right_expr_id, ctx, None);
            self.emit_memory_copy(elem_sz);
        } else {
            let elem_sz = memory::element_size(&elem_type_info.kind);
            let store_instr = memory::store_instruction(&elem_type_info.kind);

            self.lower_expression(arena, array_expr_id, ctx, None);
            self.emit_index_offset(arena, index_expr_id, elem_sz, array_len, ctx);
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

    /// Lowers `assert(<cond>)` to a trap-on-false WASM sequence.
    ///
    /// Emits `<cond>; i32.eqz; if (empty); unreachable; end`. The asserted-true
    /// branch falls through with an empty `then`; the asserted-false branch
    /// executes `unreachable`, which every WASM host treats as a trap and which
    /// the Rocq translator already maps to `BI_unreachable`.
    fn lower_assert_statement(
        &mut self,
        arena: &AstArena,
        condition: ExprId,
        ctx: &TypedContext,
    ) {
        cov_mark::hit!(wasm_codegen_emit_assert_statement);

        self.lower_expression(arena, condition, ctx, None);
        self.func().instruction(&Instruction::I32Eqz);
        self.func()
            .instruction(&Instruction::If(WasmBlockType::Empty));
        self.loop_ctx.wasm_block_depth += 1;
        self.func().instruction(&Instruction::Unreachable);
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
            TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_) | TypeInfoKind::Array(_, _)
        );

        let elem_sz = if is_compound_element {
            type_byte_size(&elem_type_info.kind, ctx, &self.current_module_path)
                .expect("array index access: type_byte_size failed for compound element")
        } else {
            memory::element_size(&elem_type_info.kind)
        };

        let array_len = Self::array_length(array_expr_id, ctx);

        self.lower_expression(arena, array_expr_id, ctx, None);
        self.emit_index_offset(arena, index_expr_id, elem_sz, array_len, ctx);

        if !is_compound_element {
            let load_instr = memory::load_instruction(&elem_type_info.kind);
            self.func().instruction(&load_instr);
        }
    }

    /// Returns the length of the array that `array_expr_id` evaluates to, when
    /// the type checker stamped an `Array(_, length)` type on that sub-expression.
    ///
    /// The element-type info on the `ArrayIndexAccess` node discards the length;
    /// the array sub-expression retains it. Returns `None` for any other type
    /// (e.g. an unresolved expression), in which case the bounds-check guard is
    /// skipped rather than panicking.
    fn array_length(array_expr_id: ExprId, ctx: &TypedContext) -> Option<u32> {
        match ctx.get_node_typeinfo(NodeId::Expr(array_expr_id)) {
            Some(TypeInfo {
                kind: TypeInfoKind::Array(_, length),
                ..
            }) => Some(length),
            _ => None,
        }
    }

    /// Emits the byte-offset computation for an array index expression.
    ///
    /// On entry the array base address is already on the WASM stack. For a
    /// constant index the offset folds to an `i32.const` add (no runtime guard —
    /// constant indices are validated statically by analysis rule A037, AD-5).
    /// For a dynamic index the runtime index is lowered, then — when
    /// `emit_bounds_checks` is set and `array_len` is known — a bounds-check
    /// guard traps before the offset multiply (AD-3, AD-4).
    fn emit_index_offset(
        &mut self,
        arena: &AstArena,
        index_expr_id: ExprId,
        elem_sz: u32,
        array_len: Option<u32>,
        ctx: &TypedContext,
    ) {
        if let Some(byte_offset) = try_const_index_byte_offset(arena, index_expr_id, elem_sz) {
            if byte_offset != 0 {
                self.func().instruction(&Instruction::I32Const(byte_offset));
                self.func().instruction(&Instruction::I32Add);
            }
        } else {
            self.lower_expression(arena, index_expr_id, ctx, None);
            self.emit_bounds_check_guard(array_len);
            #[allow(clippy::cast_possible_wrap)]
            self.func()
                .instruction(&Instruction::I32Const(elem_sz as i32));
            self.func().instruction(&Instruction::I32Mul);
            self.func().instruction(&Instruction::I32Add);
        }
    }

    /// Emits the runtime bounds-check guard for a dynamic array index.
    ///
    /// Precondition: the stack top holds the just-lowered index, sitting above
    /// the array base address (`[base, index]`). The guard single-evaluates the
    /// index into a scratch local, then traps when `index >= length`:
    ///
    /// ```wat
    /// local.tee $scratch   ;; [base, index]; $scratch = index
    /// local.get $scratch   ;; [base, index, index]
    /// i32.const N          ;; [base, index, index, N]
    /// i32.ge_u             ;; [base, index, cond]   (unsigned: also traps negatives, which arrive as huge u32)
    /// if (empty)           ;; [base, index]
    ///   unreachable
    /// end                  ;; [base, index]
    /// ```
    ///
    /// The empty-result `if` leaves `base` and `index` untouched on the stack,
    /// so the caller's offset multiply proceeds unchanged. No guard is emitted
    /// when `emit_bounds_checks` is unset or `array_len` is unknown (the offset
    /// computation stays valid either way).
    fn emit_bounds_check_guard(&mut self, array_len: Option<u32>) {
        let Some(length) = array_len.filter(|_| self.emit_bounds_checks) else {
            return;
        };
        cov_mark::hit!(wasm_codegen_emit_bounds_check);

        let scratch = self.bounds_check_scratch_local.expect(
            "bounds-check scratch local must be reserved: a dynamic array index implies a frame \
             layout, which reserves the scratch under emit_bounds_checks",
        );

        #[allow(clippy::cast_possible_wrap)]
        let length = length as i32;

        self.func().instruction(&Instruction::LocalTee(scratch));
        self.func().instruction(&Instruction::LocalGet(scratch));
        self.func().instruction(&Instruction::I32Const(length));
        self.func().instruction(&Instruction::I32GeU);
        self.func()
            .instruction(&Instruction::If(WasmBlockType::Empty));
        self.loop_ctx.wasm_block_depth += 1;
        self.func().instruction(&Instruction::Unreachable);
        self.loop_ctx.wasm_block_depth -= 1;
        self.func().instruction(&Instruction::End);
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
                .lookup_struct_in(struct_name, &self.current_module_path)
                .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));
            if !struct_info.fields.is_empty() {
                let (_, computed_fields) =
                    compute_struct_field_layout(&struct_info, ctx, &self.current_module_path)?;
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
                ..
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
    /// Zero-valued elements are skipped when `init_zero_elision` is set, which is
    /// only true during variable initialization (not assignment). This is safe
    /// because the function prologue's `memory.fill 0` guarantees the frame is
    /// zeroed at initialization time, but assignment may target slots with
    /// non-zero data from prior operations. Sret returns use
    /// `lower_array_sret_return` directly, which always emits stores.
    fn lower_array_literal(
        &mut self,
        arena: &AstArena,
        expr_id: ExprId,
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
                self.init_zero_elision,
            );
        } else {
            let elem_kind = match ctx
                .get_node_typeinfo(NodeId::Expr(expr_id))
                .map(|info| info.kind)
            {
                Some(TypeInfoKind::Array(elem, _)) => elem.kind,
                other => panic!(
                    "array literal '{parent_var_name}' has non-array type info: {other:?}"
                ),
            };
            self.store_array_literal_elements(
                arena,
                elements,
                &elem_kind,
                slot_offset,
                frame_ptr_local,
                ctx,
                self.init_zero_elision,
            );
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

    /// Recursively stores the leaves of a (possibly multi-dimensional) scalar
    /// array literal into the frame slot at `dest_base_offset`.
    ///
    /// Mirrors [`Self::emit_array_uzumaki_recursive`], but stores literal values
    /// rather than non-deterministic opcodes. For an `Array(inner, _)` element
    /// kind, each sub-array literal recurses at offset `dest_base_offset + i *
    /// stride` (where `stride` is the inner sub-array's total byte size); a
    /// non-literal array element (identifier or call) is copied with
    /// `memory.copy`. For a scalar leaf, the value is lowered and stored.
    ///
    /// The leaf store emits the **unconditional** `local.get; i32.const off;
    /// i32.add` address sequence (not [`memory::emit_ptr_offset_addr`], which
    /// elides the `i32.const 0; i32.add` at offset 0) and [`memory::store_instruction`]
    /// so that single-dimensional scalar arrays produce byte-identical output to
    /// the pre-recursion path. Scalar leaves are zero-elided through the same
    /// `skip_zero_stores` thread used by the struct-element path.
    #[allow(clippy::too_many_arguments)]
    fn store_array_literal_elements(
        &mut self,
        arena: &AstArena,
        elements: &[ExprId],
        elem_kind: &TypeInfoKind,
        dest_base_offset: u32,
        frame_ptr_local: u32,
        ctx: &TypedContext,
        skip_zero_stores: bool,
    ) {
        let stride = type_byte_size(elem_kind, ctx, &self.current_module_path)
            .expect("element byte size must be computable for array literal leaves");

        // For a struct leaf (reached only via the `Array` arm's recursion on a
        // nested array-of-structs literal), the field layout is constant for this
        // recursion level, so compute it once. Mirrors `compute_element_layout_if_struct`.
        //
        // The leaf is resolved by its canonical key, not by bare name: a
        // `::`-qualified element type (`[[lib::geom::Pt; 2]; 1]`) carries a
        // `Struct(name, key)` whose leaf name is not bound in the accessing file,
        // so a bare-name lookup against `current_module_path` would miss it (or, with
        // a same-named local struct in scope, find the wrong layout and store the
        // literal's fields at the wrong offsets — a silent miscompile). Laying out
        // by the element's defining file keeps stores and member reads in agreement.
        let struct_leaf_layout = match elem_kind {
            TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
                memory::resolve_struct_with_defining_path(elem_kind, ctx, &self.current_module_path)
                    .map(|(struct_info, defining_path)| {
                        let (total_size, field_slots) =
                            memory::compute_struct_field_layout(&struct_info, ctx, &defining_path)
                                .expect("struct field layout must be computable for array literal leaves");
                        (name.clone(), total_size, field_slots)
                    })
            }
            _ => None,
        };

        for (i, &element_id) in elements.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let off = dest_base_offset
                .checked_add(
                    (i as u32)
                        .checked_mul(stride)
                        .expect("array literal element offset overflow"),
                )
                .expect("array literal base + element offset overflow");

            // Struct leaf: reached only via the `Array` arm's recursion on a nested
            // array-of-structs literal. Mirrors `lower_array_literal_struct_elements`,
            // the path single-dim AoS uses, so each element's fields land at
            // `off + field_offset`. An enum `Custom` leaf has `struct_leaf_layout == None`
            // and falls through to the scalar arm below (enums are scalar-sized).
            if let Some((ref struct_name, struct_total_size, ref field_slots)) = struct_leaf_layout {
                if let Expr::StructLiteral { fields, .. } = &arena[element_id].kind {
                    let fields: Vec<_> = fields.iter().map(|(id, expr)| (*id, *expr)).collect();
                    self.lower_struct_literal_fields(
                        arena,
                        &fields,
                        field_slots,
                        frame_ptr_local,
                        off,
                        ctx,
                        struct_name,
                        0,
                        skip_zero_stores,
                    );
                } else {
                    memory::emit_ptr_offset_addr(self.func(), frame_ptr_local, off);
                    self.lower_expression(arena, element_id, ctx, None);
                    self.emit_memory_copy(struct_total_size);
                }
                continue;
            }

            match elem_kind {
                TypeInfoKind::Array(inner, _) => {
                    if let Expr::ArrayLiteral {
                        elements: inner_elements,
                    } = &arena[element_id].kind
                    {
                        let inner_elements = inner_elements.clone();
                        self.store_array_literal_elements(
                            arena,
                            &inner_elements,
                            &inner.kind,
                            off,
                            frame_ptr_local,
                            ctx,
                            skip_zero_stores,
                        );
                    } else {
                        memory::emit_ptr_offset_addr(self.func(), frame_ptr_local, off);
                        self.lower_expression(arena, element_id, ctx, None);
                        self.emit_memory_copy(stride);
                    }
                }
                scalar_kind => {
                    if skip_zero_stores && Self::is_syntactic_zero(arena, element_id) {
                        continue;
                    }
                    self.func()
                        .instruction(&Instruction::LocalGet(frame_ptr_local));
                    #[allow(clippy::cast_possible_wrap)]
                    self.func().instruction(&Instruction::I32Const(off as i32));
                    self.func().instruction(&Instruction::I32Add);
                    self.lower_expression(arena, element_id, ctx, None);
                    self.func()
                        .instruction(&memory::store_instruction(scalar_kind));
                }
            }
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
        assert!(
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
            self.init_zero_elision,
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
        assert!(
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
                            TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
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
                    ..
                } => {
                    self.lower_array_field(
                        arena,
                        field_value_expr_id,
                        elem_kind,
                        *elem_size,
                        *length,
                        base_ptr_local,
                        offset,
                        ctx,
                        skip_zero_stores,
                    );
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

    /// Lowers an array-typed struct field initializer into the field's slot.
    ///
    /// An array literal is stored element by element, dispatching on the element
    /// kind: 1D arrays of (flat) structs go through
    /// [`Self::lower_array_literal_struct_elements`] (the machinery shared with
    /// top-level array-of-struct locals); multi-dimensional arrays go through
    /// [`Self::store_array_literal_elements`], which recurses over nested scalar
    /// arrays and nested arrays-of-structs; and 1D arrays of scalars/enums use
    /// direct element-wise stores. Any non-literal initializer (identifier,
    /// function call) is copied whole with `memory.copy`.
    #[allow(clippy::too_many_arguments)]
    fn lower_array_field(
        &mut self,
        arena: &AstArena,
        field_value_expr_id: ExprId,
        elem_kind: &TypeInfoKind,
        elem_size: u32,
        length: u32,
        base_ptr_local: u32,
        offset: u32,
        ctx: &TypedContext,
        skip_zero_stores: bool,
    ) {
        if let Expr::ArrayLiteral { elements } = &arena[field_value_expr_id].kind {
            let elements: Vec<_> = elements.clone();
            if let Some(elem_field_slots) =
                compute_element_layout_if_struct(elem_kind, ctx, &self.current_module_path)
                    .expect("array element struct layout already validated during frame layout")
            {
                // 1D array of (flat) structs.
                self.lower_array_literal_struct_elements(
                    arena,
                    &elements,
                    &elem_field_slots,
                    base_ptr_local,
                    offset,
                    elem_size,
                    ctx,
                    skip_zero_stores,
                );
            } else if matches!(elem_kind, TypeInfoKind::Array(_, _)) {
                // Multi-dimensional array field: delegate to the recursive
                // leaf-store machinery shared with top-level array locals, which
                // handles nested scalar arrays and nested arrays-of-structs.
                self.store_array_literal_elements(
                    arena,
                    &elements,
                    elem_kind,
                    offset,
                    base_ptr_local,
                    ctx,
                    skip_zero_stores,
                );
            } else {
                // 1D array of scalars/enums: each element stored directly. The
                // address sequence elides `i32.const 0; i32.add` at offset 0,
                // which differs from the unconditional sequence emitted by
                // `store_array_literal_elements`, so scalar fields are not routed
                // through it.
                let store_instr = memory::store_instruction(elem_kind);
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
            }
        } else {
            let array_byte_size = elem_size
                .checked_mul(length)
                .expect("Array byte size overflow: elem_size * length exceeds u32::MAX");
            emit_ptr_offset_addr(self.func(), base_ptr_local, offset);
            self.lower_expression(arena, field_value_expr_id, ctx, None);
            self.emit_memory_copy(array_byte_size);
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

        let (TypeInfoKind::Struct(struct_name, _) | TypeInfoKind::Custom(struct_name)) =
            &struct_type.kind
        else {
            panic!(
                "MemberAccess: struct expression has non-struct type: {:?}",
                struct_type.kind
            )
        };

        // The receiver's type carries the file-qualified canonical key of its
        // struct. A chained access (`o.mid.a`) reaches a struct whose bare name
        // may name a *different* struct in the file being emitted, so resolving
        // the bare name against `current_module_path` would pick the wrong
        // layout. Prefer the canonical key, which identifies the struct by its
        // defining file (#63).
        let struct_info = match &struct_type.kind {
            TypeInfoKind::Struct(_, key) => ctx
                .lookup_struct(key)
                .or_else(|| ctx.lookup_struct_in(struct_name, &self.current_module_path)),
            _ => ctx.lookup_struct_in(struct_name, &self.current_module_path),
        }
        .unwrap_or_else(|| panic!("Struct '{struct_name}' not found in type context"));

        let (_, field_slots) =
            compute_struct_field_layout(&struct_info, ctx, &self.current_module_path)
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

    /// Assembles the complete WASM binary from accumulated sections AND
    /// returns the recorded spec function indices and per-function frame sizes
    /// alongside it.
    ///
    /// Consumes `self` so the recorded maps are moved out exactly once; there is
    /// no separate drain step and no flag to track. The custom
    /// `inference.spec_funcs` section is emitted from `self.spec_func_indices_by_spec`
    /// before the move. The `frame_sizes` map (canonical [`FnKey`] →
    /// real shadow-stack frame bytes) is surfaced for the cross-crate A036
    /// frame-size soundness check.
    pub(crate) fn finish_and_take(self) -> FinishedModule {
        let mut module = Module::new();

        let mut type_section = TypeSection::new();
        for (params, results) in &self.types {
            type_section
                .ty()
                .function(params.iter().copied(), results.iter().copied());
        }
        module.section(&type_section);

        // Import section sits between Type and Function (WASM section order).
        // Imported functions occupy the lowest function indices, so emitting it
        // here is what makes the local `func_idx` base reservation correct.
        if !self.imports.is_empty() {
            cov_mark::hit!(wasm_codegen_emit_import_section);
            let mut import_section = ImportSection::new();
            for import in &self.imports {
                import_section.import(
                    &import.module,
                    &import.field,
                    EntityType::Function(import.type_idx),
                );
            }
            module.section(&import_section);
        }

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

        if !self.spec_func_indices_by_spec.is_empty() {
            let spec_section =
                crate::spec_section::SpecFuncSection::new(&self.spec_func_indices_by_spec);
            module.section(&spec_section);
        }

        (
            module.finish(),
            self.spec_func_indices_by_spec,
            self.frame_sizes,
        )
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
    module_path: &[String],
) -> Result<Option<Vec<memory::StructFieldSlot>>, CodegenError> {
    match kind {
        TypeInfoKind::Struct(..) | TypeInfoKind::Custom(_) => {
            // Resolve the element struct by its canonical key, not by bare name.
            // A `::`-qualified element type (`[lib::geom::Point; 2]`) carries a
            // `Struct(_, key)` whose leaf name is not bound in the accessing file,
            // so a bare-name lookup against `module_path` would miss it and the
            // struct-element path would be skipped. Mirrors the scalar-struct field
            // resolution so an array-of-struct local lays out by the element's
            // defining file regardless of how its type was named (#63).
            let Some((struct_info, defining_path)) =
                memory::resolve_struct_with_defining_path(kind, ctx, module_path)
            else {
                return Ok(None);
            };
            let (_, field_slots) =
                compute_struct_field_layout(&struct_info, ctx, &defining_path)?;
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
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
        assert!(!wasm.is_empty());
        assert!(!has_memory_section(&wasm));
    }

    #[test]
    fn finish_with_memory_includes_memory_section() {
        cov_mark::check!(wasm_codegen_emit_memory_section);
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
        assert!(has_memory_section(&wasm));
    }

    #[test]
    fn finish_with_memory_validates_via_wasmparser() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
        inf_wasmparser::validate(&wasm)
            .unwrap_or_else(|e| panic!("Generated WASM with memory is invalid: {e}"));
    }

    #[test]
    fn finish_with_memory_exports_memory_and_stack_pointer() {
        let mut compiler = Compiler::new("test");
        compiler.enable_memory();
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
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
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
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
        let (wasm, _spec_map, _frame_sizes) = compiler.finish_and_take();
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

    #[test]
    fn emit_bounds_checks_defaults_off_and_toggles() {
        let mut compiler = Compiler::new("test");
        assert!(
            !compiler.emit_bounds_checks,
            "bounds checks must default off so Proof mode / Compiler::new output stays unguarded"
        );
        compiler.set_emit_bounds_checks(true);
        assert!(compiler.emit_bounds_checks);
        compiler.set_emit_bounds_checks(false);
        assert!(!compiler.emit_bounds_checks);
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

    /// On normal exit, the guard restores the previous `current_spec`.
    #[test]
    fn spec_scope_guard_restores_on_normal_exit() {
        let mut compiler = Compiler::new("test");
        compiler.current_spec = Some("Outer".to_string());
        {
            let _guard = SpecScopeGuard::enter(&mut compiler, Some("Active".to_string()));
        }
        assert_eq!(
            compiler.current_spec.as_deref(),
            Some("Outer"),
            "guard must restore previous current_spec on drop; got {:?}",
            compiler.current_spec
        );
    }

    /// On panic-induced unwind, the guard's `Drop` still fires and restores
    /// `current_spec`. This is the load-bearing case: it proves the field
    /// can't leak past the function boundary even on early `?` propagation
    /// (the drop semantics are identical).
    #[test]
    fn spec_scope_guard_restores_on_unwind() {
        let mut compiler = Compiler::new("test");
        compiler.current_spec = Some("Outer".to_string());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = SpecScopeGuard::enter(&mut compiler, Some("Active".to_string()));
            panic!("simulated failure inside guarded scope");
        }));
        assert!(result.is_err(), "panic must propagate out of catch_unwind");
        assert_eq!(
            compiler.current_spec.as_deref(),
            Some("Outer"),
            "guard must restore previous current_spec on unwind; got {:?}",
            compiler.current_spec
        );
    }

    /// The guard sets `current_spec` to the spec passed at entry, so the
    /// body of a guarded scope observes the new value before drop. If the
    /// pre-entry value was `None`, drop restores `None`.
    #[test]
    fn spec_scope_guard_sets_field_on_entry() {
        let mut compiler = Compiler::new("test");
        let guard = SpecScopeGuard::enter(&mut compiler, Some("MySpec".to_string()));
        assert_eq!(guard.current_spec.as_deref(), Some("MySpec"));
        drop(guard);
        assert!(compiler.current_spec.is_none());
    }

    /// Nested guards must compose: the inner guard's drop restores the
    /// outer guard's spec, not `None`. Without save/restore semantics this
    /// test fails (the inner drop would clear the outer's value).
    #[test]
    fn spec_scope_guard_nested_restores_outer() {
        let mut compiler = Compiler::new("test");
        {
            let mut outer = SpecScopeGuard::enter(&mut compiler, Some("Outer".to_string()));
            assert_eq!(outer.current_spec.as_deref(), Some("Outer"));
            {
                let inner = SpecScopeGuard::enter(&mut outer, Some("Inner".to_string()));
                assert_eq!(inner.current_spec.as_deref(), Some("Inner"));
            }
            assert_eq!(
                outer.current_spec.as_deref(),
                Some("Outer"),
                "inner guard drop must restore outer's spec"
            );
        }
        assert!(compiler.current_spec.is_none());
    }
}
