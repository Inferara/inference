//! Memory infrastructure for stack-allocated compound types (arrays, structs).
//!
//! This module provides the data structures and helpers for managing linear memory
//! in the WebAssembly codegen pipeline. Arrays are stored in linear memory using a
//! shadow stack with a `__stack_pointer` global that grows downward from the top of
//! the stack region.
//!
//! # Memory Layout
//!
//! How large the memory is and how much of it is stack are not decided here:
//! [`crate::MemoryLayout`] carries both, and this module works in terms of
//! whatever it says. The default layout is one page that is entirely stack.
//!
//! ```text
//! Stack-first layout
//! +--------------------------------------------+  pages * 64KB
//! |     Data region (empty by default, where    |
//! |     data sections and a heap would live)    |
//! +-- __stack_pointer --------------------------+  stack size
//! |                                             |
//! |         Stack (grows downward)              |
//! |                                             |
//! +--------------------------------------------+  0x00000
//! overflow below 0 = WASM OOB trap
//! ```
//!
//! # Responsibilities
//!
//! | Concern                 | Location                            |
//! |-------------------------|-------------------------------------|
//! | Named WASM local registration | `pre_scan_locals()` in compiler.rs |
//! | Scratch WASM local registration | [`RegionEmit`] here, spliced into the declarations by compiler.rs |
//! | Frame layout computation| `compute_frame_layout()` here       |
//! | Load/store helpers      | `store_instruction()` / `load_instruction()` here |
//! | Whole-region fill/copy  | [`emit_stack_prologue()`] / [`emit_memcpy_via_locals()`] here |
//! | Prologue/epilogue       | `emit_stack_prologue()` / `emit_stack_epilogue()` here |
//! | Section assembly        | `finish()` in compiler.rs           |
//!
//! # WebAssembly feature level
//!
//! By default every region fill and region copy is lowered to plain loads and
//! stores, so generated modules stay within the WebAssembly 1.0 instruction set.
//! A build that permits [`EmitFeatures::bulk_memory`] instead gets the single
//! `memory.fill` or `memory.copy` each region operation is a lowering of.
//!
//! The two forms are chosen at one place per operation — the first statement of
//! [`emit_frame_zero_fill`], [`emit_memcpy_via_locals`] and
//! [`emit_memcpy_via_stack`] — and the bulk form allocates no scratch local,
//! which [`RegionEmit`] asserts. Everything upstream of those three functions,
//! including every frame layout and every call site in compiler.rs, is identical
//! either way.

use crate::errors::CodegenError;
use crate::target::EmitFeatures;
use inference_type_checker::StructInfo;
use inference_type_checker::type_info::{NumberType, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::{FxHashMap, FxHashSet};
use wasm_encoder::{BlockType, Function, Instruction, MemArg, ValType};

/// Stack frame alignment in bytes (matches LLVM/Rust WASM convention).
///
/// Defined beside [`crate::MemoryLayout`] in `inference-compiler-interface`,
/// whose validation reads it: the grid this module rounds every frame to is the
/// same grid a rejected stack size is measured against, and one definition is
/// what keeps the two from parting ways.
pub(crate) use inference_compiler_interface::FRAME_ALIGNMENT;

/// WASM global index for `__stack_pointer` (the only global in the module).
const STACK_POINTER_GLOBAL: u32 = 0;

/// WASM memory index (only one linear memory per module).
pub(crate) const MEMORY_INDEX: u32 = 0;

/// Describes a single array's location within a stack frame.
#[derive(Debug, Clone)]
pub(crate) struct ArraySlot {
    /// Byte offset from the frame pointer to the start of this array.
    pub offset: u32,
    /// Size in bytes of each element.
    pub elem_size: u32,
    /// Number of elements in the array.
    pub length: u32,
    /// Cached field layout for struct-element arrays.
    ///
    /// Populated during frame layout computation when the array element type
    /// is a struct. For non-struct element arrays, this is `None`.
    /// Avoids per-access recomputation of the inner struct's field offsets.
    pub element_layout: Option<Vec<StructFieldSlot>>,
}

/// Describes the memory layout of a struct field that may itself be a compound type.
///
/// For primitive (scalar) fields, no sub-layout is needed -- load/store instructions
/// operate directly. For compound fields (nested struct or array), the sub-layout
/// caches the inner structure so that chained member access can resolve offsets
/// without recomputation.
///
/// The nesting depth is bounded to one level by analysis rule A026.
#[derive(Debug, Clone)]
pub(crate) enum CompoundFieldLayout {
    /// A primitive/scalar field -- load/store directly.
    Scalar,
    /// A field whose type is another struct.
    NestedStruct {
        fields: Vec<StructFieldSlot>,
        total_size: u32,
    },
    /// A field whose type is an array.
    ///
    /// All array-typed fields use pointer semantics during member access,
    /// regardless of element type. An `Array(i32, 3)` field is `NestedArray`,
    /// not `Scalar`, because it occupies contiguous memory that must be
    /// addressed by offset rather than loaded as a single WASM value.
    NestedArray {
        elem_kind: TypeInfoKind,
        elem_size: u32,
        /// Natural alignment of one element, computed in the array's defining
        /// file context. Cached so a field's alignment can be recovered from the
        /// slot without re-resolving the element type by name at the access site
        /// (which would be wrong for a compound element — struct or nested array —
        /// whose type is not visible where the enclosing struct is used) (#63).
        elem_align: u32,
        length: u32,
    },
}

impl CompoundFieldLayout {
    /// Returns `true` if this field layout represents a compound type (nested struct or array).
    ///
    /// Compound fields require pointer semantics during member access rather than
    /// scalar load/store instructions.
    pub(crate) fn is_compound(&self) -> bool {
        !matches!(self, Self::Scalar)
    }

    /// Returns the total byte size of this compound field layout.
    ///
    /// # Panics
    /// Panics if called on a `Scalar` variant — scalars should use `element_size()`.
    pub(crate) fn byte_size(&self) -> u32 {
        match self {
            Self::NestedStruct { total_size, .. } => *total_size,
            Self::NestedArray { elem_size, length, .. } => elem_size
                .checked_mul(*length)
                .expect("NestedArray byte size overflow: elem_size * length exceeds u32::MAX"),
            Self::Scalar => panic!("byte_size() called on Scalar — use element_size() instead"),
        }
    }
}

/// Describes a single struct field's location within a struct instance.
#[derive(Debug, Clone)]
pub(crate) struct StructFieldSlot {
    /// The field name as declared in the struct definition.
    pub name: String,
    /// Byte offset from the start of the struct to this field.
    pub offset: u32,
    /// The type kind of this field, used for load/store instruction selection.
    pub type_kind: TypeInfoKind,
    /// The compound layout of this field, caching nested struct/array structure.
    pub layout: CompoundFieldLayout,
}

/// Describes a struct instance's location within a stack frame.
#[derive(Debug, Clone)]
pub(crate) struct StructSlot {
    /// Byte offset from the frame pointer to the start of this struct.
    pub offset: u32,
    /// Total size in bytes of the struct (including internal and trailing padding for alignment).
    pub total_size: u32,
    /// Per-field layout information, in declaration order.
    pub fields: Vec<StructFieldSlot>,
}

/// Resolves a struct *field/element* type to its definition and defining-file
/// path, preferring the canonical key.
///
/// A `Struct` kind carries the defining-file canonical key; a `::`-qualified
/// field type (`p: lib::geom::Point`) resolves to one whose leaf name is not
/// bound by name in the accessing file, so a bare-name lookup against
/// `module_path` would miss it. The key identifies the struct by its defining
/// file, so it is tried first, with the bare-name lookup as the fallback for a
/// `Custom` kind (which carries no key).
///
/// The returned defining-file path is what layout recursion must thread into the
/// struct's nested fields: a same-named struct in another file has a different
/// layout, so resolving its fields relative to the access site would compute the
/// wrong offsets (#63).
pub(crate) fn resolve_struct_with_defining_path(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
) -> Option<(StructInfo, Vec<String>)> {
    let info = match kind {
        TypeInfoKind::Struct(name, key) => ctx
            .lookup_struct(key)
            .or_else(|| ctx.lookup_struct_in(name, module_path)),
        TypeInfoKind::Custom(name) => ctx.lookup_struct_in(name, module_path),
        // A `::`-qualified annotation (`p: lib::geom::Point`) names a struct by
        // its file path rather than a bare name; the leaf is not bound by name in
        // the accessing file, so a bare-name lookup would miss it. Walking the
        // path from the accessing file recovers the same struct a bare or
        // canonical-key form would, keeping a by-value qualified parameter on the
        // same slot+copy path as a bare struct parameter.
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
            ctx.lookup_struct_by_qualified_path(&split_qualified_path(path), module_path)
        }
        _ => None,
    }?;
    let defining_path = ctx.module_path_of_scope(info.definition_scope_id);
    Some((info, defining_path))
}

/// Splits a `::`-joined type path (`lib::geom::Point`) into its segments, the
/// form [`TypedContext::lookup_struct_by_qualified_path`] expects. A
/// [`TypeInfoKind::Qualified`]/[`TypeInfoKind::QualifiedName`] carries its path
/// as a single joined string.
fn split_qualified_path(path: &str) -> Vec<String> {
    path.split("::").map(ToString::to_string).collect()
}

/// Computes the byte layout for a struct's fields.
///
/// Iterates fields in declaration order, aligning each field to its natural
/// alignment and computing running offsets. Returns `(total_size, field_slots)`.
///
/// The total size is the offset just past the last field's data, rounded up to
/// the struct's overall alignment (the maximum alignment of any field). This
/// matches C struct layout rules (`repr(C)`) and ensures arrays of structs
/// would be correctly aligned.
///
/// For compound fields (nested structs or arrays), the field's
/// [`CompoundFieldLayout`] caches the inner layout so that chained member
/// access can resolve offsets without recomputation.
///
/// The layout is a function of `struct_info` alone — its fields resolve relative
/// to the file that *defines* the struct, derived from
/// [`StructInfo::definition_scope_id`](inference_type_checker::StructInfo::definition_scope_id).
/// The `module_path` argument names the accessing file and does not influence the
/// result; two files accessing the same struct compute identical offsets, and a
/// same-named struct in another file never shadows a nested field's type (#63).
///
/// # Errors
///
/// Returns [`CodegenError::CycleInStructLayout`] if a struct transitively
/// contains itself (defense-in-depth; the type checker normally prevents this).
///
/// Returns [`CodegenError::StructNotFoundInTypeContext`] if a nested struct
/// name cannot be found in the type context.
pub(crate) fn compute_struct_field_layout(
    struct_info: &StructInfo,
    ctx: &TypedContext,
    module_path: &[String],
) -> Result<(u32, Vec<StructFieldSlot>), CodegenError> {
    let visited = FxHashSet::default();
    compute_struct_field_layout_with_visited(struct_info, ctx, module_path, &visited)
}

fn compute_struct_field_layout_with_visited(
    struct_info: &StructInfo,
    ctx: &TypedContext,
    _access_module_path: &[String],
    visited: &FxHashSet<String>,
) -> Result<(u32, Vec<StructFieldSlot>), CodegenError> {
    if struct_info.fields.is_empty() {
        return Ok((0, vec![]));
    }
    // A struct's field types resolve relative to the file that *defines* it, not
    // the file accessing it: a same-named struct in another file has a different
    // layout. Re-derive the defining path from the struct itself so the layout is
    // independent of the access site (#63).
    let module_path = ctx.module_path_of_scope(struct_info.definition_scope_id);
    let module_path = module_path.as_slice();
    let mut current_offset: u32 = 0;
    let mut max_align: u32 = 1;
    let mut field_slots = Vec::with_capacity(struct_info.fields.len());

    // Each field gets a fresh clone of the ancestor visited set so that
    // sibling fields of the same struct type (e.g. `a: Point; b: Point`)
    // don't falsely trigger cycle detection against each other.
    for field in &struct_info.fields {
        let mut field_visited = visited.clone();
        let layout = compute_field_layout_with_visited(
            &field.type_info.kind,
            ctx,
            module_path,
            &mut field_visited,
        )?;

        let size = match &layout {
            CompoundFieldLayout::NestedStruct { total_size, .. } => *total_size,
            CompoundFieldLayout::NestedArray {
                elem_size, length, ..
            } => elem_size
                .checked_mul(*length)
                .expect("Array byte count overflow: element size * length exceeds u32::MAX"),
            CompoundFieldLayout::Scalar => type_byte_size(&field.type_info.kind, ctx, module_path)?,
        };

        let align = natural_alignment_for_type(&field.type_info.kind, ctx, module_path)?;
        let aligned_offset = align_to(current_offset, align);

        if align > max_align {
            max_align = align;
        }

        let resolved_kind = match &field.type_info.kind {
            TypeInfoKind::Custom(name) if ctx.lookup_enum_in(name, module_path).is_some() => {
                TypeInfoKind::Enum(name.clone(), name.clone())
            }
            other => other.clone(),
        };
        field_slots.push(StructFieldSlot {
            name: field.name.clone(),
            offset: aligned_offset,
            type_kind: resolved_kind,
            layout,
        });

        current_offset = aligned_offset
            .checked_add(size)
            .expect("Struct field layout overflow: total size exceeds u32::MAX");
    }

    let total_size = align_to(current_offset, max_align);
    Ok((total_size, field_slots))
}

/// Determines the [`CompoundFieldLayout`] for a given field type.
fn compute_field_layout_with_visited(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
    visited: &mut FxHashSet<String>,
) -> Result<CompoundFieldLayout, CodegenError> {
    match kind {
        TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
            if let Some((inner_struct, defining_path)) =
                resolve_struct_with_defining_path(kind, ctx, module_path)
            {
                if !visited.insert(name.clone()) {
                    return Err(CodegenError::CycleInStructLayout { name: name.clone() });
                }
                let (total_size, fields) = compute_struct_field_layout_with_visited(
                    &inner_struct,
                    ctx,
                    &defining_path,
                    visited,
                )?;
                Ok(CompoundFieldLayout::NestedStruct { fields, total_size })
            } else if ctx.lookup_enum_in(name, module_path).is_some() {
                Ok(CompoundFieldLayout::Scalar)
            } else {
                Err(CodegenError::StructNotFoundInTypeContext { name: name.clone() })
            }
        }
        TypeInfoKind::Array(elem_type, length) => Ok(CompoundFieldLayout::NestedArray {
            elem_kind: elem_type.kind.clone(),
            elem_size: type_byte_size_with_visited(&elem_type.kind, ctx, module_path, visited)?,
            elem_align: natural_alignment_for_type(&elem_type.kind, ctx, module_path)?,
            length: *length,
        }),
        // An `Enum` (incl. a resolved qualified enum) is a scalar i32 tag.
        _ => Ok(CompoundFieldLayout::Scalar),
    }
}

/// Per-function stack frame layout for compound type allocations (arrays and structs).
///
/// Stored on `Compiler` as per-function state: set at the start of
/// `visit_function_definition` and cleared after the function body is compiled.
#[derive(Debug)]
pub(crate) struct FrameLayout {
    /// Total frame size in bytes, rounded up to [`FRAME_ALIGNMENT`].
    pub total_size: u32,
    /// Maps source-level array variable names to their frame slots.
    pub array_offsets: FxHashMap<String, ArraySlot>,
    /// Maps source-level struct variable names to their frame slots.
    pub struct_offsets: FxHashMap<String, StructSlot>,
    /// WASM local index of the synthetic `__frame_ptr` local.
    pub frame_ptr_local: u32,
    /// Byte offset of a shared staging region used to build a self-referential
    /// compound reassignment (`p = P { x: p.y, y: p.x }`) before copying it into
    /// the destination slot. `None` unless the function body contains at least
    /// one such assignment, so functions without one stay byte-identical.
    ///
    /// One region is reused across every self-referential assignment in the body:
    /// each build-then-copy is sequential and the region is dead after each copy,
    /// so it is sized to the largest such destination.
    pub scratch_offset: Option<u32>,
}

/// Returns the byte size of a single element for the given type.
///
/// Used by `compute_frame_layout` and store/load instruction selection.
///
/// # Cross-crate invariant
///
/// Every supported type's natural alignment is at most 8 bytes. The analysis
/// crate's A036 (`inference-analysis`, `rules::stack_depth::MAX_SLOT_PADDING`)
/// relies on this to bound per-slot frame padding at 7 bytes; a wider type
/// (e.g. i128/v128) would break that soundness invariant. Adding one here must
/// also update `MAX_SLOT_PADDING` and the guard test
/// `every_supported_type_aligns_within_max_slot_padding` in this module.
#[must_use = "returns element size in bytes"]
pub(crate) fn element_size(kind: &TypeInfoKind) -> u32 {
    match kind {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => 1,
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => 2,
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) | TypeInfoKind::Enum(_, _) => 4,
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => 8,
        // The scalar widths above are exactly what the two layout wrappers,
        // [`type_byte_size`] and [`natural_alignment_for_type`], delegate here
        // for. Those wrappers are exhaustive over `TypeInfoKind`: they size a
        // struct, a `::`-qualified nominal type and an array themselves, and they
        // refuse `string`, `()`, and the generic, function and spec types outright.
        // So every kind that can reach this arm was admitted by a layout pass that
        // should have refused it.
        _ => unreachable!(
            "`{kind:?}` has no element size; the layout wrappers size compounds themselves \
             and refuse every kind that describes no bytes, so nothing that reaches a \
             layout can be one"
        ),
    }
}

/// Computes the byte size for any type, including compound types.
///
/// For primitive types (Bool, Number), delegates to [`element_size()`].
/// For `Struct(name)` and `Custom(name)`, looks up the struct via `ctx` and
/// computes layout recursively. For `Array(elem, len)`, recurses into the
/// element type and multiplies by length.
///
/// The recursion depth is bounded to 2 levels by analysis rule A026
/// (one level of nesting). A visited set guards against cycles as
/// defense-in-depth (the type checker catches cycles before codegen runs).
pub(crate) fn type_byte_size(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
) -> Result<u32, CodegenError> {
    let mut visited = FxHashSet::default();
    type_byte_size_with_visited(kind, ctx, module_path, &mut visited)
}

/// Whether a nominal type carrier names an enum rather than a struct.
///
/// The bare-name and `::`-qualified carriers resolve through different lookups,
/// so a layout that admits one must ask the matching question for the other or a
/// qualified enum would be reported as a missing struct.
fn resolves_to_enum(kind: &TypeInfoKind, ctx: &TypedContext, module_path: &[String]) -> bool {
    match kind {
        TypeInfoKind::Struct(name, _) | TypeInfoKind::Custom(name) => {
            ctx.lookup_enum_in(name, module_path).is_some()
        }
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => ctx
            .lookup_enum_by_qualified_path(&split_qualified_path(path), module_path)
            .is_some(),
        _ => false,
    }
}

/// The size boundary. Deliberately exhaustive over [`TypeInfoKind`]: every kind
/// either has a width this returns, or is refused here with the rule that owns
/// the located diagnostic. That exhaustiveness is what lets [`element_size`],
/// [`store_instruction`] and [`load_instruction`] treat a width-less kind as
/// unreachable, and it is why a new `TypeInfoKind` variant fails to compile here
/// rather than silently acquiring a scalar width.
fn type_byte_size_with_visited(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
    visited: &mut FxHashSet<String>,
) -> Result<u32, CodegenError> {
    match kind {
        // The four nominal carriers share one resolution: a bare name, a
        // canonical-keyed struct and a `::`-qualified path all denote a struct or
        // an enum, and which spelling the annotation used is not a layout fact.
        TypeInfoKind::Struct(name, _)
        | TypeInfoKind::Custom(name)
        | TypeInfoKind::Qualified(name)
        | TypeInfoKind::QualifiedName(name) => {
            if !visited.insert(name.clone()) {
                return Err(CodegenError::CycleInStructLayout { name: name.clone() });
            }
            if let Some((struct_info, defining_path)) =
                resolve_struct_with_defining_path(kind, ctx, module_path)
            {
                let (total_size, _) = compute_struct_field_layout_with_visited(
                    &struct_info,
                    ctx,
                    &defining_path,
                    visited,
                )?;
                Ok(total_size)
            } else if resolves_to_enum(kind, ctx, module_path) {
                Ok(element_size(&TypeInfoKind::Enum(name.clone(), name.clone())))
            } else {
                Err(CodegenError::StructNotFoundInTypeContext { name: name.clone() })
            }
        }
        TypeInfoKind::Array(elem_type, length) => {
            let elem_sz = type_byte_size_with_visited(&elem_type.kind, ctx, module_path, visited)?;
            Ok(elem_sz
                .checked_mul(*length)
                .expect("Array byte count overflow: element size * length exceeds u32::MAX"))
        }
        TypeInfoKind::Bool | TypeInfoKind::Number(_) | TypeInfoKind::Enum(_, _) => {
            Ok(element_size(kind))
        }
        // The kinds with no representation in linear memory are refused here
        // rather than at the leaf, because this is the boundary every layout path
        // crosses: a local's frame slot, a struct field, an array element and an
        // indexed access all ask this question. Refusing at the leaf instead
        // would put the check behind twenty infallible call sites.
        TypeInfoKind::String => Err(CodegenError::UnsupportedConstruct {
            construct: "a `string` value in memory".to_string(),
            rule: "A048",
            location: None,
        }),
        TypeInfoKind::Unit => Err(CodegenError::UnsupportedConstruct {
            construct: "a unit value in memory".to_string(),
            rule: "A049",
            location: None,
        }),
        // A generic type is never instantiated (#320), a function type has no
        // value, and a spec type names a proof-only item. None of them describes
        // bytes, and none is owned by an analysis rule, so the refusal names the
        // type rather than a rule.
        TypeInfoKind::Generic(_) | TypeInfoKind::Function(_) | TypeInfoKind::Spec(_) => {
            Err(CodegenError::UnsupportedType {
                rendered: kind.to_string(),
            })
        }
    }
}

/// Returns the natural alignment in bytes for any type, including compound types.
///
/// For primitive types, alignment equals size. For structs, alignment is the
/// maximum alignment of any field. For arrays, alignment is the element alignment.
///
/// A visited set guards against cycles as defense-in-depth, matching the
/// pattern used in [`type_byte_size`] and [`compute_struct_field_layout`].
///
/// # Cross-crate invariant
///
/// The result is at most 8 bytes for every supported type. A036 in
/// `inference-analysis` (`rules::stack_depth::MAX_SLOT_PADDING`) depends on this
/// bound; a type aligned wider than 8 would make A036 under-approximate codegen
/// frames and become unsound. The guard test
/// `every_supported_type_aligns_within_max_slot_padding` enforces it here.
pub(crate) fn natural_alignment_for_type(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
) -> Result<u32, CodegenError> {
    let mut visited = FxHashSet::default();
    natural_alignment_with_visited(kind, ctx, module_path, &mut visited)
}

/// Returns the maximum natural alignment across all fields of a struct.
///
/// Used when computing padding/alignment for struct frame slots so the
/// struct base address is suitably aligned for every field.
///
/// Alignment is derived from each field slot's already-computed
/// [`CompoundFieldLayout`] rather than re-resolving the field's type by name.
/// The slots are produced by [`compute_struct_field_layout`], which lays each
/// nested type out relative to its *defining* file; recovering alignment from
/// them keeps it independent of the file accessing the struct, so a nested
/// cross-file field whose type is not visible at the access site still aligns
/// correctly (#63).
#[must_use = "returns the struct's overall alignment"]
pub(crate) fn max_struct_alignment(field_slots: &[StructFieldSlot]) -> u32 {
    field_slots.iter().map(field_slot_alignment).max().unwrap_or(1)
}

/// Returns the natural alignment of a single field from its cached layout.
///
/// A scalar field aligns to its element size; a nested struct aligns to the
/// maximum alignment of its own fields; an array aligns to its element's
/// alignment. This mirrors [`natural_alignment_for_type`] but reads the
/// pre-computed slot rather than re-resolving the type by name.
fn field_slot_alignment(slot: &StructFieldSlot) -> u32 {
    match &slot.layout {
        CompoundFieldLayout::Scalar => element_size(&slot.type_kind),
        CompoundFieldLayout::NestedStruct { fields, .. } => max_struct_alignment(fields),
        CompoundFieldLayout::NestedArray { elem_align, .. } => *elem_align,
    }
}

/// The alignment boundary, exhaustive over [`TypeInfoKind`] for the same reason
/// [`type_byte_size_with_visited`] is, and admitting and refusing exactly the same
/// kinds: a layout asks both questions, so the two must agree on which types
/// describe bytes whichever it asks first.
fn natural_alignment_with_visited(
    kind: &TypeInfoKind,
    ctx: &TypedContext,
    module_path: &[String],
    visited: &mut FxHashSet<String>,
) -> Result<u32, CodegenError> {
    match kind {
        TypeInfoKind::Struct(name, _)
        | TypeInfoKind::Custom(name)
        | TypeInfoKind::Qualified(name)
        | TypeInfoKind::QualifiedName(name) => {
            if !visited.insert(name.clone()) {
                return Err(CodegenError::CycleInStructLayout { name: name.clone() });
            }
            if let Some((struct_info, defining_path)) =
                resolve_struct_with_defining_path(kind, ctx, module_path)
            {
                let mut max_align = 1u32;
                for f in &struct_info.fields {
                    let align = natural_alignment_with_visited(
                        &f.type_info.kind,
                        ctx,
                        &defining_path,
                        &mut visited.clone(),
                    )?;
                    if align > max_align {
                        max_align = align;
                    }
                }
                Ok(max_align)
            } else if resolves_to_enum(kind, ctx, module_path) {
                Ok(element_size(&TypeInfoKind::Enum(name.clone(), name.clone())))
            } else {
                Err(CodegenError::StructNotFoundInTypeContext { name: name.clone() })
            }
        }
        TypeInfoKind::Array(elem_type, _) => {
            natural_alignment_with_visited(&elem_type.kind, ctx, module_path, visited)
        }
        TypeInfoKind::Bool | TypeInfoKind::Number(_) | TypeInfoKind::Enum(_, _) => {
            Ok(element_size(kind))
        }
        TypeInfoKind::String => Err(CodegenError::UnsupportedConstruct {
            construct: "a `string` value in memory".to_string(),
            rule: "A048",
            location: None,
        }),
        TypeInfoKind::Unit => Err(CodegenError::UnsupportedConstruct {
            construct: "a unit value in memory".to_string(),
            rule: "A049",
            location: None,
        }),
        TypeInfoKind::Generic(_) | TypeInfoKind::Function(_) | TypeInfoKind::Spec(_) => {
            Err(CodegenError::UnsupportedType {
                rendered: kind.to_string(),
            })
        }
    }
}

/// Rounds `offset` up to the nearest multiple of `alignment`.
///
/// Used to align array offsets within a frame to their element type's
/// natural alignment, matching LLVM/Rust convention.
#[must_use = "returns the aligned offset"]
pub(crate) fn align_to(offset: u32, alignment: u32) -> u32 {
    debug_assert!(
        alignment.is_power_of_two(),
        "alignment must be a power of two"
    );
    (offset + alignment - 1) & !(alignment - 1)
}

/// Rounds `size` up to the nearest multiple of [`FRAME_ALIGNMENT`].
#[must_use = "returns the aligned size"]
pub(crate) fn align_to_frame(size: u32) -> u32 {
    align_to(size, FRAME_ALIGNMENT)
}

/// Selects the appropriate WASM store instruction for an element type.
///
/// The `MemArg` uses offset 0 and the natural alignment for the element size:
/// - 1 byte: align=0 (2^0 = 1)
/// - 2 bytes: align=1 (2^1 = 2)
/// - 4 bytes: align=2 (2^2 = 4)
/// - 8 bytes: align=3 (2^3 = 8)
#[must_use = "returns the WASM store instruction"]
pub(crate) fn store_instruction(elem_type: &TypeInfoKind) -> Instruction<'static> {
    let memarg = MemArg {
        offset: 0,
        align: natural_alignment(elem_type),
        memory_index: MEMORY_INDEX,
    };
    match elem_type {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => {
            Instruction::I32Store8(memarg)
        }
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => Instruction::I32Store16(memarg),
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) | TypeInfoKind::Enum(_, _) => {
            Instruction::I32Store(memarg)
        }
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Store(memarg),
        // A compound element is copied as a byte region rather than stored, so it
        // never reaches here. Every other kind that describes no bytes — `string`,
        // `()`, and the generic, function and spec types — is refused by the two
        // exhaustive layout wrappers, [`type_byte_size`] and
        // [`natural_alignment_for_type`], while the layout that owns this element
        // is computed, which happens before any instruction is emitted.
        _ => unreachable!(
            "`{elem_type:?}` has no store width; the layout wrappers refuse every kind that \
             describes no bytes before emission, so no element that reaches a store can be one"
        ),
    }
}

/// Selects the appropriate WASM load instruction for an element type.
///
/// Uses sign-appropriate extension for sub-i32 types:
/// - Signed types (`i8`, `i16`): `i32.load8_s`, `i32.load16_s` (sign-extending)
/// - Unsigned types (`u8`, `u16`, `bool`): `i32.load8_u`, `i32.load16_u` (zero-extending)
#[must_use = "returns the WASM load instruction"]
pub(crate) fn load_instruction(elem_type: &TypeInfoKind) -> Instruction<'static> {
    let memarg = MemArg {
        offset: 0,
        align: natural_alignment(elem_type),
        memory_index: MEMORY_INDEX,
    };
    match elem_type {
        TypeInfoKind::Bool | TypeInfoKind::Number(NumberType::U8) => Instruction::I32Load8U(memarg),
        TypeInfoKind::Number(NumberType::I8) => Instruction::I32Load8S(memarg),
        TypeInfoKind::Number(NumberType::U16) => Instruction::I32Load16U(memarg),
        TypeInfoKind::Number(NumberType::I16) => Instruction::I32Load16S(memarg),
        TypeInfoKind::Number(NumberType::I32 | NumberType::U32) | TypeInfoKind::Enum(_, _) => {
            Instruction::I32Load(memarg)
        }
        TypeInfoKind::Number(NumberType::I64 | NumberType::U64) => Instruction::I64Load(memarg),
        // A compound element is read through its own pointer rather than loaded,
        // so it never reaches here. Every other kind that describes no bytes —
        // `string`, `()`, and the generic, function and spec types — is refused by
        // the two exhaustive layout wrappers, [`type_byte_size`] and
        // [`natural_alignment_for_type`], while the layout that owns this element
        // is computed, which happens before any instruction is emitted.
        _ => unreachable!(
            "`{elem_type:?}` has no load width; the layout wrappers refuse every kind that \
             describes no bytes before emission, so no element that reaches a load can be one"
        ),
    }
}

/// Emits sub-i32 narrowing instructions to truncate an i32 value to the
/// width of a sub-i32 type after arithmetic operations.
///
/// - Signed (i8, i16): shift-left then arithmetic-shift-right (sign-extend)
/// - Unsigned (u8, u16): AND with bitmask (zero-extend)
/// - i32/u32/i64/u64/bool: no-op, returns false
///
/// Returns `true` if narrowing instructions were emitted.
pub(crate) fn emit_sub_i32_narrowing(func: &mut Function, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Number(NumberType::I8) => {
            // Sign-extend from 8 bits: (x << 24) >>s 24
            func.instruction(&Instruction::I32Const(24));
            func.instruction(&Instruction::I32Shl);
            func.instruction(&Instruction::I32Const(24));
            func.instruction(&Instruction::I32ShrS);
            true
        }
        TypeInfoKind::Number(NumberType::I16) => {
            // Sign-extend from 16 bits: (x << 16) >>s 16
            func.instruction(&Instruction::I32Const(16));
            func.instruction(&Instruction::I32Shl);
            func.instruction(&Instruction::I32Const(16));
            func.instruction(&Instruction::I32ShrS);
            true
        }
        TypeInfoKind::Number(NumberType::U8) => {
            // Zero-extend from 8 bits: x & 0xFF
            func.instruction(&Instruction::I32Const(0xFF));
            func.instruction(&Instruction::I32And);
            true
        }
        TypeInfoKind::Number(NumberType::U16) => {
            // Zero-extend from 16 bits: x & 0xFFFF
            func.instruction(&Instruction::I32Const(0xFFFF));
            func.instruction(&Instruction::I32And);
            true
        }
        _ => false,
    }
}

/// Emits the ABI-entry normalization for one narrow scalar parameter of an
/// exported function: `local.get p; <normalize>; local.set p`.
///
/// A WebAssembly host may pass any i32 bit pattern for a narrow parameter, so
/// exported functions canonicalize before the body runs:
/// - u8/u16: mask to the low bits (zero-extend), i8/i16: sign-extend from the
///   low bits — the C "argument takes the low bits" convention, reusing the
///   [`emit_sub_i32_narrowing`] shapes.
/// - bool: truthiness (`p != 0`, encoded `i32.eqz; i32.eqz`) — any nonzero
///   host value means `true`, matching C hosts and the existing `if`/`!`
///   lowerings, which already treat any nonzero i32 as true. A `& 1` mask was
///   rejected: it would map a host `2` to `false`, contradicting the existing
///   `if b` behavior for the same argument.
///
/// Every shape is a fixed point on canonical in-language values, so in-domain
/// calls (including entry-file sibling calls) are unchanged. i32/u32/i64/u64,
/// enum, and compound parameters are not normalized (returns `false`, emits
/// nothing): full-width ints need no truncation, and an enum tag has no
/// bit-width truncation story (tag-domain validation is emitted by the
/// prologue's enum tag guard in the compiler).
///
/// Returns `true` if normalization instructions were emitted.
pub(crate) fn emit_entry_param_normalization(
    func: &mut Function,
    kind: &TypeInfoKind,
    param_local: u32,
) -> bool {
    match kind {
        TypeInfoKind::Bool => {
            func.instruction(&Instruction::LocalGet(param_local));
            func.instruction(&Instruction::I32Eqz);
            func.instruction(&Instruction::I32Eqz);
            func.instruction(&Instruction::LocalSet(param_local));
            true
        }
        TypeInfoKind::Number(
            NumberType::I8 | NumberType::I16 | NumberType::U8 | NumberType::U16,
        ) => {
            func.instruction(&Instruction::LocalGet(param_local));
            emit_sub_i32_narrowing(func, kind);
            func.instruction(&Instruction::LocalSet(param_local));
            true
        }
        _ => false,
    }
}

/// Emits the shift-count mask for a narrow-typed shift: `i32.const 7; i32.and`
/// (8-bit) or `i32.const 15; i32.and` (16-bit) applied to the count on top of
/// the operand stack.
///
/// The language rule is "a shift count is taken modulo the operand type's bit
/// width". WebAssembly's `ishl`/`ishr` already mask the count modulo 32/64 —
/// exactly the type width for i32/u32/i64/u64, so those types need nothing
/// (returns `false`). Narrow types promote to i32, where wasm's mod-32 mask
/// produces a cliff (`u8 x << 8` is 0 but `x << 32` is `x`); masking the count
/// to the declared width first extends wasm's own semantics to the type.
/// Applies to both `<<` and `>>`: `>>`'s exemption from result re-narrowing is
/// a value-domain property (an in-domain operand stays in-domain under any
/// effective count), which says nothing about which count is effective.
///
/// The mask is unconditional at narrow shift sites — a literal count for a
/// narrow-typed shift cannot type-check today (bare literals are i32 and
/// binary operands do not coerce) and const-declared counts reach codegen as
/// opaque locals, so a provably-in-range constant that could skip the mask
/// does not exist as an expressible shape.
///
/// Returns `true` if a mask was emitted.
pub(crate) fn emit_shift_count_mask(func: &mut Function, kind: &TypeInfoKind) -> bool {
    match kind {
        TypeInfoKind::Number(NumberType::I8 | NumberType::U8) => {
            func.instruction(&Instruction::I32Const(7));
            func.instruction(&Instruction::I32And);
            true
        }
        TypeInfoKind::Number(NumberType::I16 | NumberType::U16) => {
            func.instruction(&Instruction::I32Const(15));
            func.instruction(&Instruction::I32And);
            true
        }
        _ => false,
    }
}

/// Returns the natural alignment exponent for an element type.
///
/// WASM encodes alignment as `log2(byte_alignment)`:
/// - 1-byte types: 0 (2^0 = 1)
/// - 2-byte types: 1 (2^1 = 2)
/// - 4-byte types: 2 (2^2 = 4)
/// - 8-byte types: 3 (2^3 = 8)
fn natural_alignment(elem_type: &TypeInfoKind) -> u32 {
    match element_size(elem_type) {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        _ => unreachable!("element_size returns only 1, 2, 4, or 8"),
    }
}

/// Byte size at or below which a whole-region fill or copy is emitted as
/// straight-line stores rather than an index loop.
///
/// 128 bytes is sixteen eight-byte stores. At that scale straight-line code is
/// comparable in size to the loop that would replace it, runs faster (no branch
/// per chunk), and keeps typical stack frames loop-free — which matters for the
/// Rocq translation, where a loop-free prologue is discharged without induction.
///
/// This is unrelated to [`UNROLL_THRESHOLD`]: that one counts *elements* and
/// selects a typed per-element copy, this one counts *bytes* and selects the
/// shape of an untyped whole-region copy.
const BULK_UNROLL_LIMIT_BYTES: u32 = 128;

/// One endpoint of a byte copy: a base pointer held in a WASM local, plus a
/// constant displacement from it.
///
/// The displacement is folded into each access's `offset` immediate instead of
/// being materialized with `i32.add`, so an unrolled copy emits no address
/// arithmetic at all. Keeping a base and its displacement in one value also stops
/// a call site from pairing one endpoint's local with the other's displacement,
/// which four loose `u32` parameters would invite.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MemAddr {
    /// WASM local holding the base address.
    pub local: u32,
    /// Constant byte displacement from the base address.
    pub offset: u32,
}

/// The per-function state of the region fill and copy lowerings: which
/// WebAssembly features they may use, and the i32 scratch locals the
/// feature-free forms need.
///
/// The two concerns travel together because they are two halves of one decision.
/// Only the lowered forms need scratch locals, so a single value answers both
/// "which shape does this region operation take" and "which locals must the
/// function declare" — and the accessors can assert that the bulk form never
/// allocates, which is what makes the bulk output byte-identical to a build from
/// before the lowering existed. That invariant is machine-checked rather than
/// reviewed.
///
/// A function declares a scratch local only if it actually emits the construct
/// that needs it, so a function that emits no copy and no region loop declares
/// nothing and its bytes are unaffected by the lowering. Precision is by
/// construction: allocation happens at the emission site, not from a predicate
/// that would have to predict the emission conditions.
///
/// Indices are handed out in first-use order from the first free local — after
/// the named locals and after the eagerly reserved frame-pointer, bounds-check
/// and narrow-division temporaries — and the compiler appends one declaration per
/// allocated slot to the end of the function's declaration list.
///
/// This is per-function state: it is rebuilt at the start of every function body,
/// alongside the `Function` its indices refer to.
#[derive(Debug)]
pub(crate) struct RegionEmit {
    features: EmitFeatures,
    next_index: u32,
    dst: Option<u32>,
    src: Option<u32>,
    counter: Option<u32>,
}

impl RegionEmit {
    /// Creates the state for one function body. Indices are handed out starting
    /// at `first_free_index`, which must be one past the last local the enclosing
    /// function already declares.
    pub(crate) fn new(first_free_index: u32, features: EmitFeatures) -> Self {
        Self {
            features,
            next_index: first_free_index,
            dst: None,
            src: None,
            counter: None,
        }
    }

    /// Whether region operations may be emitted as single bulk-memory
    /// instructions.
    pub(crate) fn bulk_memory(&self) -> bool {
        self.features.bulk_memory
    }

    /// Local holding the destination base address of a stack-convention copy.
    pub(crate) fn dst(&mut self) -> u32 {
        self.assert_lowering();
        Self::assign(&mut self.next_index, &mut self.dst)
    }

    /// Local holding the source base address of a stack-convention copy.
    pub(crate) fn src(&mut self) -> u32 {
        self.assert_lowering();
        Self::assign(&mut self.next_index, &mut self.src)
    }

    /// Local holding the induction variable of a region fill or copy loop.
    pub(crate) fn counter(&mut self) -> u32 {
        self.assert_lowering();
        Self::assign(&mut self.next_index, &mut self.counter)
    }

    /// Only the feature-free lowerings need a scratch local. A bulk-memory build
    /// that reached here would declare an extra local and shift every subsequent
    /// index, so the byte-identity of its output is enforced here instead of
    /// being re-checked against goldens.
    fn assert_lowering(&self) {
        debug_assert!(
            !self.features.bulk_memory,
            "a bulk-memory region operation is a single instruction and must allocate no scratch local"
        );
    }

    /// The local declarations this function must append for the scratch slots
    /// allocated so far.
    ///
    /// Every scratch local is an `i32`, so one declaration per allocated slot is
    /// enough and their relative order does not matter; what matters is that they
    /// are appended *after* the function's existing declarations, which is where
    /// their indices were handed out from.
    #[must_use = "returns the scratch local declarations to append"]
    pub(crate) fn declarations(&self) -> Vec<(u32, ValType)> {
        let allocated = [self.dst, self.src, self.counter]
            .into_iter()
            .flatten()
            .count();
        vec![(1, ValType::I32); allocated]
    }

    fn assign(next_index: &mut u32, slot: &mut Option<u32>) -> u32 {
        if let Some(index) = *slot {
            return index;
        }
        let index = *next_index;
        *next_index += 1;
        *slot = Some(index);
        index
    }
}

/// One statically sized unit of an untyped byte copy.
///
/// A region of any byte length is covered by whole 8-byte units followed by at
/// most one unit of each smaller width, so descending through
/// [`Self::DESCENDING`] once always lands exactly on the region's end.
#[derive(Debug, Clone, Copy)]
enum CopyWidth {
    I64,
    I32,
    I16,
    I8,
}

impl CopyWidth {
    /// Widest first — the order a region is consumed in.
    const DESCENDING: [Self; 4] = [Self::I64, Self::I32, Self::I16, Self::I8];

    fn bytes(self) -> u32 {
        match self {
            Self::I64 => 8,
            Self::I32 => 4,
            Self::I16 => 2,
            Self::I8 => 1,
        }
    }

    /// Loads one unit. The narrow loads are zero-extending: a copy reproduces
    /// raw bytes, and the paired narrow store writes back only the low bits, so
    /// sign extension would be discarded work.
    fn load(self, offset: u32) -> Instruction<'static> {
        let memarg = copy_memarg(offset);
        match self {
            Self::I64 => Instruction::I64Load(memarg),
            Self::I32 => Instruction::I32Load(memarg),
            Self::I16 => Instruction::I32Load16U(memarg),
            Self::I8 => Instruction::I32Load8U(memarg),
        }
    }

    fn store(self, offset: u32) -> Instruction<'static> {
        let memarg = copy_memarg(offset);
        match self {
            Self::I64 => Instruction::I64Store(memarg),
            Self::I32 => Instruction::I32Store(memarg),
            Self::I16 => Instruction::I32Store16(memarg),
            Self::I8 => Instruction::I32Store8(memarg),
        }
    }
}

/// Memory immediate for a copy access.
///
/// The alignment hint is 0 (one byte) on every copy access, deliberately
/// departing from the natural-alignment convention that
/// [`store_instruction`]/[`load_instruction`] follow: a copy addresses a whole
/// region whose base may be a struct field or array element aligned to as little
/// as one byte, and an over-stated hint is a lie about the address. Hints carry
/// no semantics in WebAssembly 1.0, so the conservative value costs nothing.
fn copy_memarg(offset: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align: 0,
        memory_index: MEMORY_INDEX,
    }
}

/// Emits the stack prologue for a function with stack-allocated arrays.
///
/// The prologue decrements `__stack_pointer`, saves the frame pointer, and
/// zero-initializes the entire frame — see [`emit_frame_zero_fill`] for the shape
/// the fill takes.
///
/// # Stack overflow protection
///
/// `i32.sub` uses modular arithmetic and never traps. If the subtraction wraps
/// (SP goes "below 0"), the result is a large unsigned value: the frame is at
/// most the configured stack size and SP is at least 0, so a wrapped frame
/// pointer is at least `2^32 - stack_size`. `MemoryLayout::resolve` requires
/// `memory_bytes + stack_size <= 2^32`, which is exactly the statement that
/// `2^32 - stack_size` is at or past the end of memory. WebAssembly computes an
/// effective address as `base + offset` without 32-bit wraparound, so the first
/// zero-fill store — the one at offset 0, emitted first in both the unrolled and
/// the looped form — fails its bounds check and traps before any byte is
/// written.
///
/// That headroom invariant is what a larger memory would otherwise cost. The
/// out-of-bounds region a wrapped pointer must land in shrinks as the declared
/// memory grows, and at 65536 pages it vanishes entirely — the layout is
/// rejected rather than allowed to emit a prologue whose overflow writes into
/// the top of memory instead of trapping.
///
/// Growing the memory also changes the stack's upper neighbour: with more than
/// one page, addresses just above the stack region are valid data memory rather
/// than out of bounds, so an overflow *upward* past `stack_size` would corrupt
/// data instead of trapping. No code this compiler emits moves the stack pointer
/// above its initial value — the prologue only subtracts, and the epilogue adds
/// back exactly the frame size the prologue took — so that direction is
/// unreachable from a compiled program. `__stack_pointer` is an exported mutable
/// global, so a host can still set it anywhere; that was equally true before the
/// layout was configurable.
///
/// That ordering is what makes the lowered fill observationally equal to the
/// `memory.fill` a bulk-memory build emits instead, whose up-front bounds check
/// likewise traps with no partial write. One caveat applies equally to every form
/// and is not a difference between them: the prologue commits the wrapped pointer to
/// `__stack_pointer` before it touches memory, so the global is already updated
/// when the access traps. A trap unwinds the whole instance, making that
/// unobservable unless the host re-enters afterwards — which is outside the
/// semantic contract the compiler targets. Do not try to "fix" it by sinking
/// `global.set` below the fill: that buys nothing here and the store at offset 0
/// must stay the first memory access.
///
/// The trap is defense in depth. Analysis rules A035 (recursion) and A036
/// (stack depth) statically reject any program whose frames could exhaust the
/// stack, and `compute_frame_layout` independently asserts a single frame fits
/// in the configured stack size, so an accepted program never reaches the
/// wrapping case.
///
/// **Optimization opportunity**: When all array elements are explicitly initialized
/// (e.g., `let arr: [i32; 3] = [1, 2, 3]`), the zero-fill is redundant since
/// every byte will be overwritten. This is intentionally not optimized to ensure
/// deterministic behavior for partially-initialized arrays and to simplify the
/// implementation. A future optimization pass could skip the fill when all
/// arrays in the frame are fully initialized — but must preserve the overflow trap
/// by emitting an explicit guard (`if SP < 0 then unreachable`).
///
/// ```text
/// global.get $__stack_pointer
/// i32.const <frame_size>
/// i32.sub
/// local.tee $__frame_ptr
/// global.set $__stack_pointer
/// ;; then the zero fill, see emit_frame_zero_fill
/// ```
pub(crate) fn emit_stack_prologue(
    func: &mut Function,
    layout: &FrameLayout,
    region: &mut RegionEmit,
) {
    assert!(
        layout.total_size > 0,
        "emit_stack_prologue called with zero-size frame; there is nothing to zero-initialize"
    );
    cov_mark::hit!(wasm_codegen_emit_stack_prologue);
    #[allow(clippy::cast_possible_wrap)]
    let frame_size = layout.total_size as i32;
    func.instruction(&Instruction::GlobalGet(STACK_POINTER_GLOBAL));
    func.instruction(&Instruction::I32Const(frame_size));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::LocalTee(layout.frame_ptr_local));
    func.instruction(&Instruction::GlobalSet(STACK_POINTER_GLOBAL));
    emit_frame_zero_fill(func, layout.frame_ptr_local, layout.total_size, region);
}

/// Zero-fills `total_size` bytes starting at the frame pointer.
///
/// With bulk memory permitted this is one `memory.fill` over the whole frame.
/// Otherwise the fill is decomposed: frame sizes are multiples of
/// [`FRAME_ALIGNMENT`], so both lowered forms decompose exactly and neither needs
/// a tail. Small frames become straight-line `i64.store`s; larger ones become a
/// loop that clears 16 bytes per iteration (two stores), which halves the branch
/// overhead of an 8-byte body.
///
/// The frame's lowest address is the first one touched in all three forms — the
/// `memory.fill` bounds-checks the whole region up front, the unrolled form
/// stores at offset 0 first, and the looped form starts its induction variable at
/// 0. [`emit_stack_prologue`] documents why that ordering is load-bearing.
///
/// The loop is emitted atomically — no user expression is lowered inside it — so
/// it cannot contain a `break` or `continue`, and it needs no entry in the
/// compiler's loop-context or block-depth bookkeeping.
fn emit_frame_zero_fill(
    func: &mut Function,
    frame_ptr: u32,
    total_size: u32,
    region: &mut RegionEmit,
) {
    /// Bytes cleared per iteration of the looped form.
    const STRIDE: u32 = 16;

    if region.bulk_memory() {
        cov_mark::hit!(wasm_codegen_frame_fill_bulk);
        func.instruction(&Instruction::LocalGet(frame_ptr));
        func.instruction(&Instruction::I32Const(0));
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(total_size as i32));
        func.instruction(&Instruction::MemoryFill(MEMORY_INDEX));
        return;
    }

    debug_assert_eq!(
        total_size % FRAME_ALIGNMENT,
        0,
        "frame sizes are rounded to FRAME_ALIGNMENT, which the fill decomposition relies on"
    );
    let zero_store = |offset: u32| {
        Instruction::I64Store(MemArg {
            offset: u64::from(offset),
            align: 3,
            memory_index: MEMORY_INDEX,
        })
    };

    if total_size <= BULK_UNROLL_LIMIT_BYTES {
        cov_mark::hit!(wasm_codegen_frame_fill_unrolled);
        for offset in (0..total_size).step_by(CopyWidth::I64.bytes() as usize) {
            func.instruction(&Instruction::LocalGet(frame_ptr));
            func.instruction(&Instruction::I64Const(0));
            func.instruction(&zero_store(offset));
        }
        return;
    }

    cov_mark::hit!(wasm_codegen_frame_fill_loop);
    let index = region.counter();
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(index));
    func.instruction(&Instruction::Loop(BlockType::Empty));
    for offset in (0..STRIDE).step_by(CopyWidth::I64.bytes() as usize) {
        func.instruction(&Instruction::LocalGet(frame_ptr));
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I64Const(0));
        func.instruction(&zero_store(offset));
    }
    func.instruction(&Instruction::LocalGet(index));
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(STRIDE as i32));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalTee(index));
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(total_size as i32));
    func.instruction(&Instruction::I32Ne);
    func.instruction(&Instruction::BrIf(0));
    func.instruction(&Instruction::End);
}

/// Copies `byte_size` bytes from `src` to `dst`, both given as a base local plus
/// a constant displacement.
///
/// With bulk memory permitted this is one `memory.copy` over the whole region.
/// Otherwise the copy runs forward in 8-byte chunks with a statically unrolled
/// 4/2/1 tail: small regions become straight-line loads and stores whose
/// displacements are folded into the access offsets; larger ones become an
/// 8-byte-per-iteration loop followed by the same static tail.
///
/// # Overlap
///
/// `memory.copy` has memmove semantics, so the overlap reasoning below constrains
/// only the lowered forms. A forward byte-order copy is correct for regions that
/// are identical or disjoint, and every site that reaches this helper is one of
/// those two:
///
/// - Array and struct parameter copies write the callee's freshly decremented
///   frame and read the address the caller supplied. A compound parameter is a
///   pointer the caller was free to obtain from its own frame *or* from a region
///   it was itself handed by reference, so the source is not necessarily a copy
///   in the immediate caller. It is nonetheless at or above the caller's stack
///   pointer, because every live frame lies above it, while the destination was
///   just carved out below it — so the two are disjoint whatever the source
///   names.
/// - The sret return copy writes the caller-provided destination and reads the
///   returned value's address in the callee. That address is a callee frame slot,
///   disjoint from the caller's frame for the same reason, or a region reached
///   through a by-reference parameter. In the latter case both endpoints are
///   whole named slots the caller's layout carved separately, because A016 and
///   A017 confine a compound-returning call to a fresh binding's initializer and
///   the type checker forbids that binding reusing a parameter's name. The
///   endpoints coincide only when a method returns the value it was called on.
/// - Body-level compound copies (via [`emit_memcpy_via_stack`]) move between
///   whole named slots, individual array elements at bounds-checked stride
///   multiples, or layout-disjoint struct fields. A right-hand side that reads
///   the destination is routed through the frame's scratch region first, so a
///   self-referential reassignment never reads a slot it is concurrently
///   writing.
///
/// A parameter passed by reference does alias the caller's region, and two of
/// them can name the same region or one a sub-range of the other — `f(x, x)`,
/// `f(s, s.f)`. That never reaches this helper as *partial* overlap: a parameter
/// is passed by reference only because nothing writes it, so it is never a copy
/// destination, and reaching a strict sub-range of a region takes a projection,
/// which appears only as a source here. What remains is identical endpoints, and
/// for those each byte's read and write coincide, which a forward copy handles.
///
/// # Loop emission
///
/// The loop is emitted atomically after all sub-expression lowering, so no user
/// `break` or `continue` can be lowered inside it and it bypasses the compiler's
/// loop-context and block-depth bookkeeping safely.
pub(crate) fn emit_memcpy_via_locals(
    func: &mut Function,
    dst: MemAddr,
    src: MemAddr,
    byte_size: u32,
    region: &mut RegionEmit,
) {
    // Ahead of the zero-size early return: `memory.copy` of zero bytes is well
    // defined and is what a bulk-memory build has always emitted here, whereas
    // the lowered form has nothing to emit.
    if region.bulk_memory() {
        cov_mark::hit!(wasm_codegen_memcpy_bulk);
        emit_ptr_offset_addr(func, dst.local, dst.offset);
        emit_ptr_offset_addr(func, src.local, src.offset);
        emit_memory_copy_raw(func, byte_size);
        return;
    }

    if byte_size == 0 {
        return;
    }

    if byte_size <= BULK_UNROLL_LIMIT_BYTES {
        cov_mark::hit!(wasm_codegen_memcpy_unrolled);
        emit_copy_chunks(func, dst, src, 0, byte_size);
        return;
    }

    cov_mark::hit!(wasm_codegen_memcpy_loop);
    let chunk = CopyWidth::I64.bytes();
    let looped_bytes = byte_size - byte_size % chunk;
    let index = region.counter();
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(index));
    func.instruction(&Instruction::Loop(BlockType::Empty));
    func.instruction(&Instruction::LocalGet(dst.local));
    func.instruction(&Instruction::LocalGet(index));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalGet(src.local));
    func.instruction(&Instruction::LocalGet(index));
    func.instruction(&Instruction::I32Add);
    func.instruction(&CopyWidth::I64.load(src.offset));
    func.instruction(&CopyWidth::I64.store(dst.offset));
    func.instruction(&Instruction::LocalGet(index));
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(chunk as i32));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalTee(index));
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(looped_bytes as i32));
    func.instruction(&Instruction::I32Ne);
    func.instruction(&Instruction::BrIf(0));
    func.instruction(&Instruction::End);

    emit_copy_chunks(func, dst, src, looped_bytes, byte_size - looped_bytes);
}

/// Emits straight-line loads and stores covering `[start, start + len)` of both
/// endpoints, in descending unit width, folding every displacement into the
/// access offset immediates.
fn emit_copy_chunks(func: &mut Function, dst: MemAddr, src: MemAddr, start: u32, len: u32) {
    let end = start + len;
    let mut at = start;
    for width in CopyWidth::DESCENDING {
        while end - at >= width.bytes() {
            func.instruction(&Instruction::LocalGet(dst.local));
            func.instruction(&Instruction::LocalGet(src.local));
            func.instruction(&width.load(src.offset + at));
            func.instruction(&width.store(dst.offset + at));
            at += width.bytes();
        }
    }
    debug_assert_eq!(
        at, end,
        "descending copy widths must land on the region end"
    );
}

/// Copies `byte_size` bytes between two addresses already on the WASM operand
/// stack, pushed destination first and source second.
///
/// That push order is the convention of every body-level compound copy site, and
/// it is exactly the order `memory.copy` consumes its operands in, so a
/// bulk-memory build appends the size and the instruction and is done.
///
/// The lowered form instead moves the two addresses into scratch locals so the
/// copy can address them repeatedly; the copy itself is
/// [`emit_memcpy_via_locals`], whose documentation covers overlap and loop
/// emission.
///
/// A zero-byte region still consumes both addresses in either form, matching the
/// stack effect of a copy of any other size.
pub(crate) fn emit_memcpy_via_stack(func: &mut Function, byte_size: u32, region: &mut RegionEmit) {
    if region.bulk_memory() {
        cov_mark::hit!(wasm_codegen_memcpy_via_stack_bulk);
        emit_memory_copy_raw(func, byte_size);
        return;
    }

    let dst_local = region.dst();
    let src_local = region.src();
    func.instruction(&Instruction::LocalSet(src_local));
    func.instruction(&Instruction::LocalSet(dst_local));
    emit_memcpy_via_locals(
        func,
        MemAddr {
            local: dst_local,
            offset: 0,
        },
        MemAddr {
            local: src_local,
            offset: 0,
        },
        byte_size,
        region,
    );
}

/// Emits the `i32.const <size>` + `memory.copy` pair that completes a bulk copy
/// whose destination and source addresses are already on the operand stack,
/// destination first.
fn emit_memory_copy_raw(func: &mut Function, byte_size: u32) {
    #[allow(clippy::cast_possible_wrap)]
    func.instruction(&Instruction::I32Const(byte_size as i32));
    func.instruction(&Instruction::MemoryCopy {
        src_mem: MEMORY_INDEX,
        dst_mem: MEMORY_INDEX,
    });
}

/// Threshold: arrays with more than this many elements are copied as an untyped
/// byte region instead of element by element.
///
/// This counts elements and gates a *typed* copy that reads and writes with the
/// element's own load/store instruction. It is deliberately not related to
/// [`BULK_UNROLL_LIMIT_BYTES`], which counts bytes and picks the shape of an
/// untyped region copy — the two answer different questions and changing one to
/// track the other would rewrite the emitted bytes of every small-array
/// parameter copy for no semantic gain.
const UNROLL_THRESHOLD: u32 = 16;

/// Emits copy-on-entry code for one array-typed parameter.
///
/// Copies `slot.length` elements from the caller's pointer (`param_local`) into
/// the callee's frame at `slot.offset` from `__frame_ptr`, then updates
/// `param_local` to point to the callee's copy.
///
/// For arrays with N <= 16 elements, the copy is unrolled element by element
/// with the element type's own load/store. For larger arrays (N > 16), and for
/// any array whose elements are compound (load/store instructions cannot move a
/// struct or a nested array), the whole region is copied as untyped bytes by
/// [`emit_memcpy_via_locals`].
///
/// After the copy, the parameter local is overwritten with the callee's frame
/// address so that subsequent reads/writes inside the function operate on the
/// local copy (value semantics).
///
/// ```text
/// ;; unrolled copy (N <= 16):
/// local.get $__frame_ptr
/// i32.const <offset + i * elem_size>
/// i32.add
/// local.get $param_ptr
/// i32.const <i * elem_size>
/// i32.add
/// i32.load / i32.load8_s / ...     ;; load source element
/// i32.store / i32.store8 / ...     ;; store to destination
/// ;; ... repeat for each element
///
/// ;; region copy (N > 16 or compound elements):
/// ;; see emit_memcpy_via_locals
///
/// ;; update param local:
/// local.get $__frame_ptr
/// i32.const <offset>
/// i32.add
/// local.set $param_ptr
/// ```
pub(crate) fn emit_array_param_copy(
    func: &mut Function,
    layout: &FrameLayout,
    slot: &ArraySlot,
    param_local: u32,
    elem_type: &TypeInfoKind,
    region: &mut RegionEmit,
) {
    cov_mark::hit!(wasm_codegen_emit_array_param_copy);

    let byte_size = slot
        .elem_size
        .checked_mul(slot.length)
        .expect("array param copy: byte size overflow");

    let is_compound_element = matches!(
        elem_type,
        TypeInfoKind::Struct(_, _) | TypeInfoKind::Custom(_) | TypeInfoKind::Array(_, _)
    );

    if slot.length > UNROLL_THRESHOLD || is_compound_element {
        emit_memcpy_via_locals(
            func,
            MemAddr {
                local: layout.frame_ptr_local,
                offset: slot.offset,
            },
            MemAddr {
                local: param_local,
                offset: 0,
            },
            byte_size,
            region,
        );
    } else {
        // Unrolled element-by-element copy
        let load_instr = load_instruction(elem_type);
        let store_instr = store_instruction(elem_type);
        for i in 0..slot.length {
            #[allow(clippy::cast_possible_wrap)]
            let byte_offset = (slot.offset + i * slot.elem_size) as i32;
            #[allow(clippy::cast_possible_wrap)]
            let src_offset = (i * slot.elem_size) as i32;

            // destination address
            func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
            func.instruction(&Instruction::I32Const(byte_offset));
            func.instruction(&Instruction::I32Add);

            // load from source (param pointer + element offset)
            func.instruction(&Instruction::LocalGet(param_local));
            if src_offset > 0 {
                func.instruction(&Instruction::I32Const(src_offset));
                func.instruction(&Instruction::I32Add);
            }
            func.instruction(&load_instr);

            // store to destination
            func.instruction(&store_instr);
        }
    }

    // Update the parameter local to point to the callee's copy
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    if slot.offset > 0 {
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(slot.offset as i32));
        func.instruction(&Instruction::I32Add);
    }
    func.instruction(&Instruction::LocalSet(param_local));
}

/// Emits copy-on-entry code for one struct-typed parameter.
///
/// Copies `slot.total_size` bytes from the caller's pointer (`param_local`) into
/// the callee's frame at `slot.offset` from `__frame_ptr`, then updates
/// `param_local` to point to the callee's copy. This enforces value semantics:
/// modifications to the struct parameter inside the callee do not affect the
/// caller's original.
///
/// Unlike array param copy (which may unroll element-by-element for small arrays),
/// struct param copy always moves the slot as an untyped byte region: structs have
/// heterogeneous field types that cannot be unrolled with a single load/store
/// instruction pair.
///
/// ```text
/// ;; region copy, see emit_memcpy_via_locals
///
/// ;; update param local:
/// local.get $__frame_ptr
/// i32.const <offset>         ;; omitted when offset is 0
/// i32.add                    ;; omitted when offset is 0
/// local.set $param_ptr
/// ```
pub(crate) fn emit_struct_param_copy(
    func: &mut Function,
    layout: &FrameLayout,
    slot: &StructSlot,
    param_local: u32,
    region: &mut RegionEmit,
) {
    cov_mark::hit!(wasm_codegen_emit_struct_param_copy);

    emit_memcpy_via_locals(
        func,
        MemAddr {
            local: layout.frame_ptr_local,
            offset: slot.offset,
        },
        MemAddr {
            local: param_local,
            offset: 0,
        },
        slot.total_size,
        region,
    );

    // Update the parameter local to point to the callee's copy
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    if slot.offset > 0 {
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(slot.offset as i32));
        func.instruction(&Instruction::I32Add);
    }
    func.instruction(&Instruction::LocalSet(param_local));
}

/// Copies a compound value from a source pointer to the sret destination.
///
/// Used in `return arr` inside an sret function: copies the returned value to
/// the caller-provided sret pointer. The source is usually a callee frame slot,
/// which sits below the caller's, but a returned parameter that was passed by
/// reference points into the caller's memory instead. Either way the two regions
/// are disjoint or identical — see the overlap section of
/// [`emit_memcpy_via_locals`], which the forward copy there handles.
pub(crate) fn emit_sret_copy(
    func: &mut Function,
    sret_local: u32,
    source_local: u32,
    byte_size: u32,
    region: &mut RegionEmit,
) {
    cov_mark::hit!(wasm_codegen_emit_sret_copy);

    emit_memcpy_via_locals(
        func,
        MemAddr {
            local: sret_local,
            offset: 0,
        },
        MemAddr {
            local: source_local,
            offset: 0,
        },
        byte_size,
        region,
    );
}

/// Emits the address computation `base_ptr + byte_offset` onto the WASM stack.
///
/// Used when writing individual elements to a destination pointer, such as
/// sret return buffers or frame-pointer-based struct/array slots.
///
/// ```text
/// local.get $base_ptr
/// i32.const <byte_offset>   ;; omitted when offset is 0
/// i32.add                   ;; omitted when offset is 0
/// ```
pub(crate) fn emit_ptr_offset_addr(func: &mut Function, base_ptr_local: u32, byte_offset: u32) {
    func.instruction(&Instruction::LocalGet(base_ptr_local));
    if byte_offset > 0 {
        #[allow(clippy::cast_possible_wrap)]
        func.instruction(&Instruction::I32Const(byte_offset as i32));
        func.instruction(&Instruction::I32Add);
    }
}

/// Emits the stack epilogue to restore `__stack_pointer` before exiting.
///
/// ```text
/// local.get $__frame_ptr
/// i32.const <frame_size>
/// i32.add
/// global.set $__stack_pointer
/// ```
pub(crate) fn emit_stack_epilogue(func: &mut Function, layout: &FrameLayout) {
    cov_mark::hit!(wasm_codegen_emit_stack_epilogue);
    #[allow(clippy::cast_possible_wrap)]
    let frame_size = layout.total_size as i32;
    func.instruction(&Instruction::LocalGet(layout.frame_ptr_local));
    func.instruction(&Instruction::I32Const(frame_size));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(STACK_POINTER_GLOBAL));
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::nodes::Visibility;
    use inference_compiler_interface::PAGE_SIZE;
    use inference_type_checker::type_info::TypeInfo;
    use inference_type_checker::{StructFieldInfo, StructInfo};
    use inference_type_checker::typed_context::TypedContext;

    /// Region state for a WebAssembly 1.0 build — the default — handing out
    /// scratch locals from `first_free_index`.
    fn lowered(first_free_index: u32) -> RegionEmit {
        RegionEmit::new(first_free_index, EmitFeatures::default())
    }

    /// Region state for a build permitted to emit bulk-memory instructions.
    fn bulk(first_free_index: u32) -> RegionEmit {
        RegionEmit::new(first_free_index, EmitFeatures { bulk_memory: true })
    }

    /// The `i32.const <size>` + `memory.copy` pair every bulk copy ends with,
    /// spelled out here so the expectations do not restate the emitter.
    fn bulk_copy(f: &mut Function, byte_size: i32) {
        f.instruction(&Instruction::I32Const(byte_size));
        f.instruction(&Instruction::MemoryCopy {
            src_mem: MEMORY_INDEX,
            dst_mem: MEMORY_INDEX,
        });
    }

    #[test]
    fn align_to_frame_zero() {
        assert_eq!(align_to_frame(0), 0);
    }

    #[test]
    fn align_to_frame_exact_multiple() {
        assert_eq!(align_to_frame(16), 16);
        assert_eq!(align_to_frame(32), 32);
    }

    #[test]
    fn align_to_frame_rounds_up() {
        assert_eq!(align_to_frame(1), 16);
        assert_eq!(align_to_frame(12), 16);
        assert_eq!(align_to_frame(17), 32);
        assert_eq!(align_to_frame(33), 48);
    }

    #[test]
    fn stack_pointer_init_equals_stack_size() {
        let layout = crate::MemoryLayout::default();
        assert_eq!(layout.stack_size(), 65536);
        assert_eq!(
            layout.stack_pointer_init(),
            layout.stack_size().cast_signed()
        );
    }

    #[test]
    fn default_stack_fills_exactly_one_page() {
        let layout = crate::MemoryLayout::default();
        assert_eq!(layout.pages(), 1);
        assert_eq!(layout.stack_size(), PAGE_SIZE);
    }

    #[test]
    fn stack_pointer_init_fits_in_i32() {
        assert!(
            i32::try_from(crate::MemoryLayout::default().stack_size()).is_ok(),
            "the stack size must fit in i32 for the stack pointer initializer"
        );
    }

    #[test]
    fn frame_alignment_is_sixteen() {
        assert_eq!(FRAME_ALIGNMENT, 16);
    }

    #[test]
    fn array_slot_fields_accessible() {
        let slot = ArraySlot {
            offset: 0,
            elem_size: 4,
            length: 3,
            element_layout: None,
        };
        assert_eq!(slot.offset, 0);
        assert_eq!(slot.elem_size, 4);
        assert_eq!(slot.length, 3);
    }

    #[test]
    fn frame_layout_fields_accessible() {
        let layout = FrameLayout {
            total_size: 16,
            array_offsets: FxHashMap::default(),
            struct_offsets: FxHashMap::default(),
            frame_ptr_local: 0,
            scratch_offset: None,
        };
        assert_eq!(layout.total_size, 16);
        assert!(layout.array_offsets.is_empty());
        assert_eq!(layout.frame_ptr_local, 0);
    }

    #[test]
    fn element_size_bool() {
        assert_eq!(element_size(&TypeInfoKind::Bool), 1);
    }

    #[test]
    fn element_size_i8() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I8)), 1);
    }

    #[test]
    fn element_size_u8() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::U8)), 1);
    }

    #[test]
    fn element_size_i16() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I16)), 2);
    }

    #[test]
    fn element_size_i32() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I32)), 4);
    }

    #[test]
    fn element_size_i64() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::I64)), 8);
    }

    #[test]
    fn element_size_u64() {
        assert_eq!(element_size(&TypeInfoKind::Number(NumberType::U64)), 8);
    }

    #[test]
    fn natural_alignment_1_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Bool), 0);
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I8)), 0);
    }

    #[test]
    fn natural_alignment_2_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I16)), 1);
    }

    #[test]
    fn natural_alignment_4_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I32)), 2);
    }

    #[test]
    fn natural_alignment_8_byte() {
        assert_eq!(natural_alignment(&TypeInfoKind::Number(NumberType::I64)), 3);
    }

    #[test]
    fn align_to_identity() {
        assert_eq!(align_to(0, 4), 0);
        assert_eq!(align_to(4, 4), 4);
        assert_eq!(align_to(8, 8), 8);
    }

    #[test]
    fn align_to_rounds_up() {
        assert_eq!(align_to(1, 4), 4);
        assert_eq!(align_to(3, 4), 4);
        assert_eq!(align_to(5, 8), 8);
        assert_eq!(align_to(7, 8), 8);
        assert_eq!(align_to(9, 4), 12);
    }

    #[test]
    fn align_to_one_byte() {
        assert_eq!(align_to(0, 1), 0);
        assert_eq!(align_to(1, 1), 1);
        assert_eq!(align_to(7, 1), 7);
        assert_eq!(align_to(100, 1), 100);
    }

    #[test]
    fn emit_sub_i32_narrowing_i8() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I8));
        assert!(emitted, "should emit narrowing for i8");
    }

    #[test]
    fn emit_sub_i32_narrowing_i16() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I16));
        assert!(emitted, "should emit narrowing for i16");
    }

    #[test]
    fn emit_sub_i32_narrowing_u8() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U8));
        assert!(emitted, "should emit narrowing for u8");
    }

    #[test]
    fn emit_sub_i32_narrowing_u16() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U16));
        assert!(emitted, "should emit narrowing for u16");
    }

    #[test]
    fn emit_sub_i32_narrowing_i32_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I32));
        assert!(!emitted, "should not emit narrowing for i32");
    }

    #[test]
    fn emit_sub_i32_narrowing_u32_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U32));
        assert!(!emitted, "should not emit narrowing for u32");
    }

    #[test]
    fn emit_sub_i32_narrowing_i64_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::I64));
        assert!(!emitted, "should not emit narrowing for i64");
    }

    #[test]
    fn emit_sub_i32_narrowing_u64_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Number(NumberType::U64));
        assert!(!emitted, "should not emit narrowing for u64");
    }

    #[test]
    fn emit_sub_i32_narrowing_bool_noop() {
        let mut func = Function::new(vec![]);
        let emitted = emit_sub_i32_narrowing(&mut func, &TypeInfoKind::Bool);
        assert!(!emitted, "should not emit narrowing for bool");
    }

    /// Raw instruction bytes emitted into a fresh (empty-locals) function.
    fn body_of(build: impl FnOnce(&mut Function)) -> Vec<u8> {
        let mut func = Function::new(vec![]);
        build(&mut func);
        func.into_raw_body()
    }

    #[test]
    fn entry_param_normalization_bool_is_double_eqz() {
        let actual = body_of(|f| {
            let emitted = emit_entry_param_normalization(f, &TypeInfoKind::Bool, 3);
            assert!(emitted, "bool parameter must be normalized");
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(3));
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::LocalSet(3));
        });
        assert_eq!(actual, expected, "bool normalization must be `p != 0` around the local");
    }

    #[test]
    fn entry_param_normalization_i8_sign_extends() {
        let actual = body_of(|f| {
            assert!(emit_entry_param_normalization(f, &TypeInfoKind::Number(NumberType::I8), 2));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(2));
            f.instruction(&Instruction::I32Const(24));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(24));
            f.instruction(&Instruction::I32ShrS);
            f.instruction(&Instruction::LocalSet(2));
        });
        assert_eq!(actual, expected, "i8 normalization must sign-extend the low byte");
    }

    #[test]
    fn entry_param_normalization_i16_sign_extends() {
        let actual = body_of(|f| {
            assert!(emit_entry_param_normalization(f, &TypeInfoKind::Number(NumberType::I16), 0));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::I32Const(16));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(16));
            f.instruction(&Instruction::I32ShrS);
            f.instruction(&Instruction::LocalSet(0));
        });
        assert_eq!(actual, expected, "i16 normalization must sign-extend the low 16 bits");
    }

    #[test]
    fn entry_param_normalization_u8_masks_low_byte() {
        let actual = body_of(|f| {
            assert!(emit_entry_param_normalization(f, &TypeInfoKind::Number(NumberType::U8), 1));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::I32Const(0xFF));
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::LocalSet(1));
        });
        assert_eq!(actual, expected, "u8 normalization must mask to the low byte");
    }

    #[test]
    fn entry_param_normalization_u16_masks_low_16() {
        let actual = body_of(|f| {
            assert!(emit_entry_param_normalization(f, &TypeInfoKind::Number(NumberType::U16), 4));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(4));
            f.instruction(&Instruction::I32Const(0xFFFF));
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::LocalSet(4));
        });
        assert_eq!(actual, expected, "u16 normalization must mask to the low 16 bits");
    }

    #[test]
    fn entry_param_normalization_wide_and_enum_emit_nothing() {
        let untouched = body_of(|_| {});
        for kind in [
            TypeInfoKind::Number(NumberType::I32),
            TypeInfoKind::Number(NumberType::U32),
            TypeInfoKind::Number(NumberType::I64),
            TypeInfoKind::Number(NumberType::U64),
            TypeInfoKind::Enum("Color".to_string(), "Color".to_string()),
        ] {
            let body = body_of(|f| {
                assert!(
                    !emit_entry_param_normalization(f, &kind, 0),
                    "{kind:?} parameter must not be normalized"
                );
            });
            assert_eq!(body, untouched, "{kind:?} must emit no normalization instructions");
        }
    }

    #[test]
    fn shift_count_mask_8bit_masks_by_7() {
        let expected = body_of(|f| {
            f.instruction(&Instruction::I32Const(7));
            f.instruction(&Instruction::I32And);
        });
        for kind in [
            TypeInfoKind::Number(NumberType::I8),
            TypeInfoKind::Number(NumberType::U8),
        ] {
            let actual = body_of(|f| {
                assert!(emit_shift_count_mask(f, &kind), "{kind:?} shift count must be masked");
            });
            assert_eq!(actual, expected, "{kind:?} shift count must mask by 7");
        }
    }

    #[test]
    fn shift_count_mask_16bit_masks_by_15() {
        let expected = body_of(|f| {
            f.instruction(&Instruction::I32Const(15));
            f.instruction(&Instruction::I32And);
        });
        for kind in [
            TypeInfoKind::Number(NumberType::I16),
            TypeInfoKind::Number(NumberType::U16),
        ] {
            let actual = body_of(|f| {
                assert!(emit_shift_count_mask(f, &kind), "{kind:?} shift count must be masked");
            });
            assert_eq!(actual, expected, "{kind:?} shift count must mask by 15");
        }
    }

    #[test]
    fn shift_count_mask_wide_and_bool_emit_nothing() {
        let untouched = body_of(|_| {});
        for kind in [
            TypeInfoKind::Number(NumberType::I32),
            TypeInfoKind::Number(NumberType::U32),
            TypeInfoKind::Number(NumberType::I64),
            TypeInfoKind::Number(NumberType::U64),
            TypeInfoKind::Bool,
        ] {
            let body = body_of(|f| {
                assert!(
                    !emit_shift_count_mask(f, &kind),
                    "{kind:?} shift count must not be masked"
                );
            });
            assert_eq!(body, untouched, "{kind:?} must emit no shift-count mask");
        }
    }

    fn make_field(name: &str, kind: TypeInfoKind) -> StructFieldInfo {
        StructFieldInfo {
            name: name.to_string(),
            type_info: TypeInfo {
                kind,
                type_params: vec![],
            },
        }
    }

    fn make_struct_info(name: &str, fields: Vec<StructFieldInfo>) -> StructInfo {
        StructInfo {
            name: name.to_string(),
            fields,
            type_params: vec![],
            visibility: Visibility::Public,
            definition_scope_id: 0,
            definition_location: inference_ast::nodes::Location::default(),
        }
    }

    #[test]
    fn struct_layout_single_i32_field() {
        let layout = make_struct_info(
            "Single",
            vec![make_field("x", TypeInfoKind::Number(NumberType::I32))],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(total_size, 4);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].offset, 0);
    }

    #[test]
    fn struct_layout_two_i32_fields() {
        let layout = make_struct_info(
            "Point",
            vec![
                make_field("x", TypeInfoKind::Number(NumberType::I32)),
                make_field("y", TypeInfoKind::Number(NumberType::I32)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(total_size, 8);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].offset, 0);

        assert_eq!(fields[1].name, "y");
        assert_eq!(fields[1].offset, 4);
    }

    #[test]
    fn struct_layout_mixed_types_with_padding() {
        let layout = make_struct_info(
            "Mixed",
            vec![
                make_field("flag", TypeInfoKind::Bool),
                make_field("val", TypeInfoKind::Number(NumberType::I64)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(fields[0].name, "flag");
        assert_eq!(fields[0].offset, 0);

        assert_eq!(fields[1].name, "val");
        assert_eq!(fields[1].offset, 8);

        assert_eq!(total_size, 16);
    }

    #[test]
    fn struct_layout_all_primitive_types() {
        let layout = make_struct_info(
            "AllTypes",
            vec![
                make_field("a", TypeInfoKind::Bool),
                make_field("b", TypeInfoKind::Number(NumberType::I8)),
                make_field("c", TypeInfoKind::Number(NumberType::I16)),
                make_field("d", TypeInfoKind::Number(NumberType::I32)),
                make_field("e", TypeInfoKind::Number(NumberType::I64)),
                make_field("f", TypeInfoKind::Number(NumberType::U8)),
                make_field("g", TypeInfoKind::Number(NumberType::U16)),
                make_field("h", TypeInfoKind::Number(NumberType::U32)),
                make_field("i", TypeInfoKind::Number(NumberType::U64)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(fields.len(), 9);
        assert_eq!(fields[0].offset, 0); // bool: 1 byte
        assert_eq!(fields[1].offset, 1); // i8: 1 byte, align 1
        assert_eq!(fields[2].offset, 2); // i16: 2 bytes, align 2
        assert_eq!(fields[3].offset, 4); // i32: 4 bytes, align 4
        assert_eq!(fields[4].offset, 8); // i64: 8 bytes, align 8
        assert_eq!(fields[5].offset, 16); // u8: 1 byte, align 1
        assert_eq!(fields[6].offset, 18); // u16: 2 bytes, align 2
        assert_eq!(fields[7].offset, 20); // u32: 4 bytes, align 4
        assert_eq!(fields[8].offset, 24); // u64: 8 bytes, align 8
        assert_eq!(total_size, 32);
    }

    #[test]
    fn struct_layout_trailing_padding() {
        let layout = make_struct_info(
            "Trailing",
            vec![
                make_field("big", TypeInfoKind::Number(NumberType::I64)),
                make_field("small", TypeInfoKind::Bool),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(fields[0].offset, 0);

        assert_eq!(fields[1].offset, 8);

        assert_eq!(total_size, 16, "should pad to max alignment (8)");
    }

    #[test]
    fn struct_layout_single_bool() {
        let layout = make_struct_info("Flag", vec![make_field("b", TypeInfoKind::Bool)]);
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(total_size, 1);
        assert_eq!(fields[0].offset, 0);
    }

    #[test]
    fn struct_layout_i16_then_i32_padding() {
        let layout = make_struct_info(
            "Padded",
            vec![
                make_field("a", TypeInfoKind::Number(NumberType::I16)),
                make_field("b", TypeInfoKind::Number(NumberType::I32)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(fields[0].offset, 0);

        assert_eq!(fields[1].offset, 4);

        assert_eq!(total_size, 8);
    }

    #[test]
    fn struct_layout_preserves_field_type_kind() {
        let layout = make_struct_info(
            "Typed",
            vec![
                make_field("x", TypeInfoKind::Number(NumberType::I32)),
                make_field("y", TypeInfoKind::Number(NumberType::I64)),
            ],
        );
        let (_, fields) = compute_struct_field_layout(&layout, &TypedContext::default(), &[]).unwrap();
        assert_eq!(fields[0].type_kind, TypeInfoKind::Number(NumberType::I32));
        assert_eq!(fields[1].type_kind, TypeInfoKind::Number(NumberType::I64));
    }

    #[test]
    fn type_byte_size_primitive_bool() {
        let ctx = TypedContext::default();
        assert_eq!(type_byte_size(&TypeInfoKind::Bool, &ctx, &[]).unwrap(), 1);
    }

    #[test]
    fn type_byte_size_primitive_i32() {
        let ctx = TypedContext::default();
        assert_eq!(
            type_byte_size(&TypeInfoKind::Number(NumberType::I32), &ctx, &[]).unwrap(),
            4
        );
    }

    #[test]
    fn type_byte_size_primitive_i64() {
        let ctx = TypedContext::default();
        assert_eq!(
            type_byte_size(&TypeInfoKind::Number(NumberType::I64), &ctx, &[]).unwrap(),
            8
        );
    }

    #[test]
    fn type_byte_size_array_of_i32() {
        let ctx = TypedContext::default();
        let kind = TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I32),
                type_params: vec![],
            }),
            3,
        );
        assert_eq!(type_byte_size(&kind, &ctx, &[]).unwrap(), 12);
    }

    #[test]
    fn type_byte_size_array_of_i64() {
        let ctx = TypedContext::default();
        let kind = TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            }),
            3,
        );
        assert_eq!(type_byte_size(&kind, &ctx, &[]).unwrap(), 24);
    }

    #[test]
    fn type_byte_size_nested_array() {
        let ctx = TypedContext::default();
        let inner_array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                3,
            ),
            type_params: vec![],
        };
        let kind = TypeInfoKind::Array(Box::new(inner_array), 2);
        assert_eq!(
            type_byte_size(&kind, &ctx, &[]).unwrap(),
            24,
            "[[i32; 3]; 2] = 4*3*2 = 24"
        );
    }

    #[test]
    fn natural_alignment_for_type_i32() {
        let ctx = TypedContext::default();
        assert_eq!(
            natural_alignment_for_type(&TypeInfoKind::Number(NumberType::I32), &ctx, &[]).unwrap(),
            4
        );
    }

    #[test]
    fn natural_alignment_for_type_i64() {
        let ctx = TypedContext::default();
        assert_eq!(
            natural_alignment_for_type(&TypeInfoKind::Number(NumberType::I64), &ctx, &[]).unwrap(),
            8
        );
    }

    #[test]
    fn natural_alignment_for_type_array_of_i64() {
        let ctx = TypedContext::default();
        let kind = TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            }),
            3,
        );
        assert_eq!(
            natural_alignment_for_type(&kind, &ctx, &[]).unwrap(),
            8,
            "array of i64 alignment = element alignment = 8"
        );
    }

    #[test]
    fn natural_alignment_for_type_nested_array_i32() {
        let ctx = TypedContext::default();
        let inner_array = TypeInfo {
            kind: TypeInfoKind::Array(
                Box::new(TypeInfo {
                    kind: TypeInfoKind::Number(NumberType::I32),
                    type_params: vec![],
                }),
                3,
            ),
            type_params: vec![],
        };
        let kind = TypeInfoKind::Array(Box::new(inner_array), 2);
        assert_eq!(
            natural_alignment_for_type(&kind, &ctx, &[]).unwrap(),
            4,
            "[[i32; 3]; 2] alignment = i32 alignment = 4"
        );
    }

    #[test]
    fn struct_layout_field_has_scalar_layout() {
        let ctx = TypedContext::default();
        let info = make_struct_info(
            "Point",
            vec![
                make_field("x", TypeInfoKind::Number(NumberType::I32)),
                make_field("y", TypeInfoKind::Number(NumberType::I32)),
            ],
        );
        let (_, fields) = compute_struct_field_layout(&info, &ctx, &[]).unwrap();
        assert!(matches!(fields[0].layout, CompoundFieldLayout::Scalar));
        assert!(matches!(fields[1].layout, CompoundFieldLayout::Scalar));
    }

    #[test]
    fn struct_layout_nested_struct_field() {
        let mut ctx = TypedContext::default();
        ctx.register_test_struct(
            "Inner",
            &[
                (
                    "x".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I32),
                        type_params: vec![],
                    },
                ),
                (
                    "y".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I32),
                        type_params: vec![],
                    },
                ),
            ],
        )
        .unwrap();

        let info = make_struct_info(
            "Outer",
            vec![
                make_field("inner", TypeInfoKind::Struct("Inner".to_string(), "Inner".to_string())),
                make_field("val", TypeInfoKind::Number(NumberType::I32)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&info, &ctx, &[]).unwrap();
        assert_eq!(total_size, 12, "Inner(8) + val(4) = 12");
        assert_eq!(fields.len(), 2);

        assert_eq!(fields[0].name, "inner");
        assert_eq!(fields[0].offset, 0);
        assert!(
            matches!(
                &fields[0].layout,
                CompoundFieldLayout::NestedStruct {
                    total_size: 8,
                    fields
                } if fields.len() == 2
            ),
            "inner field should be NestedStruct with 2 fields and total_size 8"
        );

        assert_eq!(fields[1].name, "val");
        assert_eq!(fields[1].offset, 8);
        assert!(matches!(fields[1].layout, CompoundFieldLayout::Scalar));
    }

    #[test]
    fn type_byte_size_struct() {
        let mut ctx = TypedContext::default();
        ctx.register_test_struct(
            "Point",
            &[
                (
                    "x".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I32),
                        type_params: vec![],
                    },
                ),
                (
                    "y".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I32),
                        type_params: vec![],
                    },
                ),
            ],
        )
        .unwrap();

        assert_eq!(
            type_byte_size(&TypeInfoKind::Struct("Point".to_string(), "Point".to_string()), &ctx, &[]).unwrap(),
            8,
            "Point {{ x: i32, y: i32 }} = 8 bytes"
        );
    }

    #[test]
    fn natural_alignment_for_type_struct_mixed() {
        let mut ctx = TypedContext::default();
        ctx.register_test_struct(
            "Mixed",
            &[
                (
                    "a".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Bool,
                        type_params: vec![],
                    },
                ),
                (
                    "b".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I64),
                        type_params: vec![],
                    },
                ),
            ],
        )
        .unwrap();

        assert_eq!(
            natural_alignment_for_type(&TypeInfoKind::Struct("Mixed".to_string(), "Mixed".to_string()), &ctx, &[]).unwrap(),
            8,
            "Mixed {{ a: bool, b: i64 }} alignment = max(1, 8) = 8"
        );
    }

    /// Soundness guard for `inference-analysis`'s A036
    /// (`rules::stack_depth::MAX_SLOT_PADDING = 7`): every supported type must
    /// align within 8 bytes, so codegen never inserts more than 7 padding bytes
    /// per slot. If a wider-aligned type (i128/f128/v128/SIMD) is ever added,
    /// this test fails — and `MAX_SLOT_PADDING` must be revisited before A036
    /// silently under-approximates a real frame.
    ///
    /// The `NumberType` coverage is a non-wildcard `match`: adding a variant
    /// breaks compilation here, forcing the new type through this check.
    #[test]
    fn every_supported_type_aligns_within_max_slot_padding() {
        /// Maximum natural alignment A036 assumes for any single slot.
        const MAX_ALIGN: u32 = 8;

        let mut ctx = TypedContext::default();
        ctx.register_test_enum("Color", &["Red", "Green", "Blue"])
            .unwrap();
        ctx.register_test_struct(
            "Wide",
            &[
                (
                    "a".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Bool,
                        type_params: vec![],
                    },
                ),
                (
                    "b".to_string(),
                    TypeInfo {
                        kind: TypeInfoKind::Number(NumberType::I64),
                        type_params: vec![],
                    },
                ),
            ],
        )
        .unwrap();

        let assert_within = |kind: &TypeInfoKind| {
            let align = natural_alignment_for_type(kind, &ctx, &[])
                .unwrap_or_else(|e| panic!("alignment lookup failed for {kind:?}: {e:?}"));
            assert!(
                align <= MAX_ALIGN,
                "{kind:?} aligns to {align} bytes, exceeding MAX_SLOT_PADDING's {MAX_ALIGN}-byte assumption",
            );
        };

        assert_within(&TypeInfoKind::Bool);

        // Exhaustive over NumberType: a new variant makes this `match`
        // non-exhaustive and forces the type through the alignment guard.
        for nt in NumberType::ALL {
            match nt {
                NumberType::I8
                | NumberType::I16
                | NumberType::I32
                | NumberType::I64
                | NumberType::U8
                | NumberType::U16
                | NumberType::U32
                | NumberType::U64 => {
                    let kind = TypeInfoKind::Number(*nt);
                    assert!(
                        element_size(&kind) <= MAX_ALIGN,
                        "{kind:?} has element size exceeding {MAX_ALIGN} bytes",
                    );
                    assert_within(&kind);
                }
            }
        }

        assert_within(&TypeInfoKind::Enum("Color".to_string(), "Color".to_string()));

        let array_of_i64 = TypeInfoKind::Array(
            Box::new(TypeInfo {
                kind: TypeInfoKind::Number(NumberType::I64),
                type_params: vec![],
            }),
            4,
        );
        assert_within(&array_of_i64);

        assert_within(&TypeInfoKind::Struct("Wide".to_string(), "Wide".to_string()));
    }

    /// A frame layout with the given size, holding no named slots — enough for
    /// the prologue emitter, which only reads `total_size` and `frame_ptr_local`.
    fn frame_of(total_size: u32, frame_ptr_local: u32) -> FrameLayout {
        FrameLayout {
            total_size,
            array_offsets: FxHashMap::default(),
            struct_offsets: FxHashMap::default(),
            frame_ptr_local,
            scratch_offset: None,
        }
    }

    /// One zero-fill store of an unrolled prologue: `local.get $fp;
    /// i64.const 0; i64.store align=3 offset=<offset>`.
    fn fill_store(f: &mut Function, frame_ptr: u32, offset: u64) {
        f.instruction(&Instruction::LocalGet(frame_ptr));
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Store(MemArg {
            offset,
            align: 3,
            memory_index: MEMORY_INDEX,
        }));
    }

    /// One zero-fill store of a looped prologue, addressed through the induction
    /// variable: `local.get $fp; local.get $i; i32.add; i64.const 0;
    /// i64.store align=3 offset=<offset>`.
    fn fill_store_indexed(f: &mut Function, frame_ptr: u32, index: u32, offset: u64) {
        f.instruction(&Instruction::LocalGet(frame_ptr));
        f.instruction(&Instruction::LocalGet(index));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64Store(MemArg {
            offset,
            align: 3,
            memory_index: MEMORY_INDEX,
        }));
    }

    /// The `global.get`/`i32.sub`/`local.tee`/`global.set` prefix every prologue
    /// opens with, before any memory is touched.
    fn prologue_prefix(f: &mut Function, frame_size: i32, frame_ptr: u32) {
        f.instruction(&Instruction::GlobalGet(0));
        f.instruction(&Instruction::I32Const(frame_size));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalTee(frame_ptr));
        f.instruction(&Instruction::GlobalSet(0));
    }

    /// The looped zero fill of a frame of `frame_size` bytes.
    fn fill_loop(f: &mut Function, frame_ptr: u32, index: u32, frame_size: i32) {
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(index));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        fill_store_indexed(f, frame_ptr, index, 0);
        fill_store_indexed(f, frame_ptr, index, 8);
        f.instruction(&Instruction::LocalGet(index));
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalTee(index));
        f.instruction(&Instruction::I32Const(frame_size));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::BrIf(0));
        f.instruction(&Instruction::End);
    }

    /// A copy endpoint: a base local plus a constant displacement.
    fn addr(local: u32, offset: u32) -> MemAddr {
        MemAddr { local, offset }
    }

    /// The load/store pair a copy of `width` bytes uses, spelled out
    /// independently of [`CopyWidth`] so the tests pin the instruction choice
    /// and the one-byte alignment hint rather than restating the emitter.
    fn copy_load_store(
        width: u32,
        load_offset: u64,
        store_offset: u64,
    ) -> (Instruction<'static>, Instruction<'static>) {
        let memarg = |offset| MemArg {
            offset,
            align: 0,
            memory_index: MEMORY_INDEX,
        };
        match width {
            8 => (
                Instruction::I64Load(memarg(load_offset)),
                Instruction::I64Store(memarg(store_offset)),
            ),
            4 => (
                Instruction::I32Load(memarg(load_offset)),
                Instruction::I32Store(memarg(store_offset)),
            ),
            2 => (
                Instruction::I32Load16U(memarg(load_offset)),
                Instruction::I32Store16(memarg(store_offset)),
            ),
            1 => (
                Instruction::I32Load8U(memarg(load_offset)),
                Instruction::I32Store8(memarg(store_offset)),
            ),
            other => panic!("no copy unit of width {other}"),
        }
    }

    /// A straight-line copy expectation: region size and the `(width,
    /// displacement)` units that must cover it.
    type CopyCase = (u32, &'static [(u32, u32)]);

    /// A looped copy expectation: region size, the bytes the loop covers, and
    /// the `(width, displacement)` units of the static tail after it.
    type CopyLoopCase = (u32, i32, &'static [(u32, u32)]);

    /// Straight-line copy units `(width, displacement)`, each addressing both
    /// endpoints through their base locals with the displacement folded into the
    /// access offsets.
    fn copy_units(f: &mut Function, dst: MemAddr, src: MemAddr, units: &[(u32, u32)]) {
        for &(width, at) in units {
            let (load, store) = copy_load_store(
                width,
                u64::from(src.offset + at),
                u64::from(dst.offset + at),
            );
            f.instruction(&Instruction::LocalGet(dst.local));
            f.instruction(&Instruction::LocalGet(src.local));
            f.instruction(&load);
            f.instruction(&store);
        }
    }

    /// The 8-bytes-per-iteration copy loop covering `[0, looped_bytes)`.
    fn copy_loop(f: &mut Function, dst: MemAddr, src: MemAddr, looped_bytes: i32, index: u32) {
        let (load, store) = copy_load_store(8, u64::from(src.offset), u64::from(dst.offset));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(index));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        f.instruction(&Instruction::LocalGet(dst.local));
        f.instruction(&Instruction::LocalGet(index));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(src.local));
        f.instruction(&Instruction::LocalGet(index));
        f.instruction(&Instruction::I32Add);
        f.instruction(&load);
        f.instruction(&store);
        f.instruction(&Instruction::LocalGet(index));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalTee(index));
        f.instruction(&Instruction::I32Const(looped_bytes));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::BrIf(0));
        f.instruction(&Instruction::End);
    }

    #[test]
    fn frame_fill_unrolled_below_limit() {
        const FP: u32 = 2;
        let actual = body_of(|f| emit_stack_prologue(f, &frame_of(16, FP), &mut lowered(3)));
        let expected = body_of(|f| {
            prologue_prefix(f, 16, FP);
            fill_store(f, FP, 0);
            fill_store(f, FP, 8);
        });
        assert_eq!(
            actual, expected,
            "a 16-byte frame is cleared by two i64 stores"
        );
    }

    #[test]
    fn frame_fill_unrolled_at_limit() {
        const FP: u32 = 0;
        #[allow(clippy::cast_possible_wrap)]
        let limit = BULK_UNROLL_LIMIT_BYTES as i32;
        let actual = body_of(|f| {
            emit_stack_prologue(f, &frame_of(BULK_UNROLL_LIMIT_BYTES, FP), &mut lowered(1));
        });
        let expected = body_of(|f| {
            prologue_prefix(f, limit, FP);
            for offset in (0..u64::from(BULK_UNROLL_LIMIT_BYTES)).step_by(8) {
                fill_store(f, FP, offset);
            }
        });
        assert_eq!(
            actual, expected,
            "a frame exactly at the unroll limit stays straight-line"
        );
    }

    #[test]
    fn frame_fill_unrolled_declares_no_scratch_local() {
        let mut region = lowered(4);
        body_of(|f| emit_stack_prologue(f, &frame_of(BULK_UNROLL_LIMIT_BYTES, 3), &mut region));
        assert!(
            region.declarations().is_empty(),
            "an unrolled fill needs no induction variable, so it must declare no local"
        );
    }

    /// The looped fill clears 16 bytes per iteration and compares the advanced
    /// index against the frame size, which is always a multiple of 16 — so the
    /// decomposition is exact and the loop needs no tail.
    #[test]
    fn frame_fill_loop_just_above_limit() {
        const FP: u32 = 1;
        const INDEX: u32 = 5;
        let mut region = lowered(INDEX);
        let actual = body_of(|f| emit_stack_prologue(f, &frame_of(144, FP), &mut region));
        let expected = body_of(|f| {
            prologue_prefix(f, 144, FP);
            fill_loop(f, FP, INDEX, 144);
        });
        assert_eq!(
            actual, expected,
            "a 144-byte frame is cleared by a 16-byte-stride loop"
        );
        assert_eq!(
            region.declarations().len(),
            1,
            "the looped fill declares exactly its induction variable"
        );
    }

    /// The looped form's shape does not grow with the frame: only the bound in
    /// the comparison changes.
    #[test]
    fn frame_fill_loop_large_frame() {
        const FP: u32 = 0;
        const INDEX: u32 = 1;
        let actual = body_of(|f| {
            emit_stack_prologue(f, &frame_of(4096, FP), &mut lowered(INDEX));
        });
        let expected = body_of(|f| {
            prologue_prefix(f, 4096, FP);
            fill_loop(f, FP, INDEX, 4096);
        });
        assert_eq!(
            actual, expected,
            "a large frame uses the same loop with a wider bound"
        );
    }

    /// The frame pointer may have wrapped past the end of memory when the shadow
    /// stack overflows, and WebAssembly computes effective addresses without
    /// 32-bit wraparound. The lowest address in the frame must therefore be the
    /// first one touched, so that such a frame traps before any byte is written —
    /// the property the replaced `memory.fill`'s up-front bounds check provided.
    #[test]
    fn frame_fill_first_memory_access_is_the_store_at_offset_zero() {
        const FP: u32 = 0;
        for frame_size in [16, BULK_UNROLL_LIMIT_BYTES, 144, 4096] {
            #[allow(clippy::cast_possible_wrap)]
            let signed_size = frame_size as i32;
            let body = body_of(|f| {
                emit_stack_prologue(f, &frame_of(frame_size, FP), &mut lowered(1));
            });
            let up_to_first_access = body_of(|f| {
                prologue_prefix(f, signed_size, FP);
                if frame_size > BULK_UNROLL_LIMIT_BYTES {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::LocalSet(1));
                    f.instruction(&Instruction::Loop(BlockType::Empty));
                    fill_store_indexed(f, FP, 1, 0);
                } else {
                    fill_store(f, FP, 0);
                }
            });
            assert!(
                body.starts_with(&up_to_first_access),
                "a {frame_size}-byte frame must reach its offset-0 store before any other access"
            );
        }
    }

    #[test]
    fn memcpy_unrolled_tail_decompositions() {
        let dst = addr(0, 0);
        let src = addr(1, 0);
        // Every region is covered by whole 8-byte units followed by at most one
        // unit of each smaller width, so the tail shapes are fully enumerable.
        // Grouped eight-per-line so the 8-byte run reads as a run.
        #[rustfmt::skip]
        let cases: &[CopyCase] = &[
            (1, &[(1, 0)]),
            (2, &[(2, 0)]),
            (3, &[(2, 0), (1, 2)]),
            (4, &[(4, 0)]),
            (5, &[(4, 0), (1, 4)]),
            (6, &[(4, 0), (2, 4)]),
            (7, &[(4, 0), (2, 4), (1, 6)]),
            (8, &[(8, 0)]),
            (24, &[(8, 0), (8, 8), (8, 16)]),
            (
                127,
                &[
                    (8, 0), (8, 8), (8, 16), (8, 24), (8, 32), (8, 40), (8, 48), (8, 56),
                    (8, 64), (8, 72), (8, 80), (8, 88), (8, 96), (8, 104), (8, 112),
                    (4, 120), (2, 124), (1, 126),
                ],
            ),
        ];
        for &(byte_size, units) in cases {
            let mut region = lowered(2);
            let actual = body_of(|f| emit_memcpy_via_locals(f, dst, src, byte_size, &mut region));
            let expected = body_of(|f| copy_units(f, dst, src, units));
            assert_eq!(actual, expected, "{byte_size}-byte copy decomposition");
            assert!(
                region.declarations().is_empty(),
                "a {byte_size}-byte copy is unrolled and must declare no scratch local"
            );
        }
    }

    #[test]
    fn memcpy_unrolled_at_limit_is_all_eight_byte_units() {
        let dst = addr(3, 0);
        let src = addr(4, 0);
        let units: Vec<(u32, u32)> = (0..BULK_UNROLL_LIMIT_BYTES)
            .step_by(8)
            .map(|at| (8, at))
            .collect();
        let actual = body_of(|f| {
            emit_memcpy_via_locals(f, dst, src, BULK_UNROLL_LIMIT_BYTES, &mut lowered(5));
        });
        let expected = body_of(|f| copy_units(f, dst, src, &units));
        assert_eq!(
            actual, expected,
            "a copy exactly at the unroll limit stays straight-line"
        );
    }

    #[test]
    fn memcpy_folds_displacements_into_offset_immediates() {
        let dst = addr(6, 40);
        let src = addr(7, 8);
        let actual = body_of(|f| emit_memcpy_via_locals(f, dst, src, 12, &mut lowered(8)));
        let expected = body_of(|f| copy_units(f, dst, src, &[(8, 0), (4, 8)]));
        assert_eq!(
            actual, expected,
            "displaced endpoints address through offset immediates, so an unrolled copy \
             emits no i32.add at all"
        );
    }

    #[test]
    fn memcpy_loop_above_limit_with_tails() {
        const INDEX: u32 = 9;
        let dst = addr(0, 0);
        let src = addr(1, 0);
        // `looped_bytes` is the largest multiple of 8 not exceeding the region;
        // whatever remains is emitted as the same static tail an unrolled copy uses.
        let cases: &[CopyLoopCase] = &[
            (129, 128, &[(1, 128)]),
            (131, 128, &[(2, 128), (1, 130)]),
            (135, 128, &[(4, 128), (2, 132), (1, 134)]),
            (136, 136, &[]),
        ];
        for &(byte_size, looped_bytes, tail) in cases {
            let mut region = lowered(INDEX);
            let actual = body_of(|f| emit_memcpy_via_locals(f, dst, src, byte_size, &mut region));
            let expected = body_of(|f| {
                copy_loop(f, dst, src, looped_bytes, INDEX);
                copy_units(f, dst, src, tail);
            });
            assert_eq!(actual, expected, "{byte_size}-byte copy loop and tail");
            assert_eq!(
                region.declarations().len(),
                1,
                "a {byte_size}-byte copy declares exactly its induction variable"
            );
        }
    }

    #[test]
    fn memcpy_loop_keeps_displacements_in_the_access_offsets() {
        const INDEX: u32 = 4;
        let dst = addr(2, 64);
        let src = addr(3, 16);
        let actual = body_of(|f| emit_memcpy_via_locals(f, dst, src, 130, &mut lowered(INDEX)));
        let expected = body_of(|f| {
            copy_loop(f, dst, src, 128, INDEX);
            copy_units(f, dst, src, &[(2, 128)]);
        });
        assert_eq!(
            actual, expected,
            "the loop indexes with $i and keeps the constant displacement in the memarg"
        );
    }

    #[test]
    fn memcpy_of_zero_bytes_emits_nothing() {
        let untouched = body_of(|_| {});
        let mut region = lowered(0);
        let body = body_of(|f| emit_memcpy_via_locals(f, addr(0, 0), addr(1, 0), 0, &mut region));
        assert_eq!(body, untouched, "an empty region copies nothing");
        assert!(region.declarations().is_empty(), "and allocates nothing");
    }

    /// Body-level copy sites push the destination first and the source second,
    /// so the source is on top and pops first.
    #[test]
    fn memcpy_via_stack_pops_source_before_destination() {
        const FIRST_FREE: u32 = 3;
        let mut region = lowered(FIRST_FREE);
        let actual = body_of(|f| emit_memcpy_via_stack(f, 8, &mut region));
        let dst = addr(FIRST_FREE, 0);
        let src = addr(FIRST_FREE + 1, 0);
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalSet(src.local));
            f.instruction(&Instruction::LocalSet(dst.local));
            copy_units(f, dst, src, &[(8, 0)]);
        });
        assert_eq!(actual, expected, "source pops into $s, destination into $d");
    }

    #[test]
    fn memcpy_via_stack_consumes_both_addresses_for_an_empty_region() {
        const FIRST_FREE: u32 = 0;
        let mut region = lowered(FIRST_FREE);
        let actual = body_of(|f| emit_memcpy_via_stack(f, 0, &mut region));
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalSet(FIRST_FREE + 1));
            f.instruction(&Instruction::LocalSet(FIRST_FREE));
        });
        assert_eq!(
            actual, expected,
            "an empty region still consumes the two pushed addresses"
        );
    }

    #[test]
    fn scratch_allocates_in_first_use_order_from_the_first_free_index() {
        let mut region = lowered(7);
        assert_eq!(region.counter(), 7);
        assert_eq!(region.dst(), 8);
        assert_eq!(region.src(), 9);
        assert_eq!(region.counter(), 7, "a second use returns the same local");
        assert_eq!(region.declarations(), vec![(1, ValType::I32); 3]);
    }

    #[test]
    fn scratch_declares_only_what_was_used() {
        let unused = lowered(2);
        assert!(unused.declarations().is_empty());

        let mut only_counter = lowered(2);
        assert_eq!(only_counter.counter(), 2);
        assert_eq!(only_counter.declarations(), vec![(1, ValType::I32)]);
    }

    /// Repeated copies in one function share the scratch locals: each copy is
    /// emitted atomically, so no copy is live across another.
    #[test]
    fn scratch_is_shared_across_copies_in_one_function() {
        let mut region = lowered(4);
        body_of(|f| {
            emit_memcpy_via_stack(f, 16, &mut region);
            emit_memcpy_via_stack(f, 200, &mut region);
        });
        assert_eq!(
            region.declarations().len(),
            3,
            "two copies, one of them looped, need one $d, one $s and one $i in total"
        );
    }

    /// `memory.fill` takes destination, fill byte, length — in that order. The
    /// wrapped stack pointer has to reach the instruction as the destination
    /// operand for the overflow trap `emit_stack_prologue` documents to fire, so
    /// the operand order is pinned, not just the instruction choice.
    #[test]
    fn bulk_frame_fill_is_one_memory_fill() {
        const FP: u32 = 2;

        cov_mark::check!(wasm_codegen_frame_fill_bulk);
        let actual = body_of(|f| emit_stack_prologue(f, &frame_of(4096, FP), &mut bulk(3)));
        let expected = body_of(|f| {
            prologue_prefix(f, 4096, FP);
            f.instruction(&Instruction::LocalGet(FP));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Const(4096));
            f.instruction(&Instruction::MemoryFill(MEMORY_INDEX));
        });
        assert_eq!(actual, expected);
    }

    /// A frame large enough to be looped by the lowering, and one small enough to
    /// be unrolled, are both one instruction in bulk mode — the size thresholds
    /// belong to the lowering alone.
    #[test]
    fn bulk_frame_fill_ignores_the_unroll_threshold() {
        for total_size in [16, BULK_UNROLL_LIMIT_BYTES, 4096] {
            let mut region = bulk(1);
            body_of(|f| emit_stack_prologue(f, &frame_of(total_size, 0), &mut region));
            assert!(
                region.declarations().is_empty(),
                "a {total_size}-byte bulk fill must declare no scratch local"
            );
        }
    }

    #[test]
    fn bulk_memcpy_pushes_both_addresses_then_copies() {
        cov_mark::check!(wasm_codegen_memcpy_bulk);
        let actual = body_of(|f| {
            emit_memcpy_via_locals(f, addr(1, 48), addr(2, 0), 200, &mut bulk(3));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::I32Const(48));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalGet(2));
            f.instruction(&Instruction::I32Const(200));
            f.instruction(&Instruction::MemoryCopy {
                src_mem: MEMORY_INDEX,
                dst_mem: MEMORY_INDEX,
            });
        });
        assert_eq!(
            actual, expected,
            "a zero displacement is folded away, a non-zero one becomes an i32.add"
        );
    }

    /// A zero-byte region is a well-defined `memory.copy` and reaches the
    /// instruction, unlike the lowering, which has nothing to emit. The two
    /// disagree only in whether a no-op is spelled out, and the bulk behavior is
    /// what the pre-lowering compiler emitted.
    #[test]
    fn bulk_memcpy_emits_a_zero_length_copy() {
        let actual = body_of(|f| {
            emit_memcpy_via_locals(f, addr(0, 0), addr(1, 0), 0, &mut bulk(2));
        });
        let expected = body_of(|f| {
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::MemoryCopy {
                src_mem: MEMORY_INDEX,
                dst_mem: MEMORY_INDEX,
            });
        });
        assert_eq!(actual, expected);
    }

    /// The body-level convention pushes destination then source, which is already
    /// `memory.copy`'s operand order: bulk mode appends the length and the
    /// instruction and never spills to a local.
    #[test]
    fn bulk_memcpy_via_stack_consumes_the_pushed_addresses_in_place() {
        cov_mark::check!(wasm_codegen_memcpy_via_stack_bulk);
        let mut region = bulk(7);
        let actual = body_of(|f| emit_memcpy_via_stack(f, 24, &mut region));
        let expected = body_of(|f| {
            f.instruction(&Instruction::I32Const(24));
            f.instruction(&Instruction::MemoryCopy {
                src_mem: MEMORY_INDEX,
                dst_mem: MEMORY_INDEX,
            });
        });
        assert_eq!(actual, expected);
        assert!(
            region.declarations().is_empty(),
            "a bulk copy must not declare the $d/$s spill locals"
        );
    }

    /// Every region operation in bulk mode is exactly its one instruction — no
    /// loop, no tail, no scratch. Region sizes well above every lowering threshold
    /// are used, so a size that would have been looped or unrolled still produces
    /// only the instruction. Pinning the whole expected body rather than searching
    /// it for a `loop` opcode keeps the assertion exact: a raw-byte search can
    /// match an immediate that merely happens to hold the same value.
    #[test]
    fn bulk_region_operations_are_exactly_their_instructions() {
        let mut region = bulk(0);
        let actual = body_of(|f| {
            emit_stack_prologue(f, &frame_of(4096, 0), &mut region);
            emit_memcpy_via_locals(f, addr(0, 0), addr(1, 0), 4096, &mut region);
            emit_memcpy_via_stack(f, 4096, &mut region);
        });
        let expected = body_of(|f| {
            prologue_prefix(f, 4096, 0);
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Const(4096));
            f.instruction(&Instruction::MemoryFill(MEMORY_INDEX));
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::LocalGet(1));
            bulk_copy(f, 4096);
            bulk_copy(f, 4096);
        });
        assert_eq!(actual, expected);
        assert!(
            region.declarations().is_empty(),
            "no bulk region operation may declare a scratch local"
        );
    }

    #[test]
    fn struct_layout_array_field_has_nested_array_layout() {
        let ctx = TypedContext::default();
        let info = make_struct_info(
            "HasArray",
            vec![
                make_field(
                    "arr",
                    TypeInfoKind::Array(
                        Box::new(TypeInfo {
                            kind: TypeInfoKind::Number(NumberType::I32),
                            type_params: vec![],
                        }),
                        3,
                    ),
                ),
                make_field("val", TypeInfoKind::Number(NumberType::I32)),
            ],
        );
        let (total_size, fields) = compute_struct_field_layout(&info, &ctx, &[]).unwrap();
        assert_eq!(fields[0].name, "arr");
        assert_eq!(fields[0].offset, 0);
        assert!(
            matches!(
                fields[0].layout,
                CompoundFieldLayout::NestedArray {
                    elem_size: 4,
                    length: 3,
                    ..
                }
            ),
            "array field should have NestedArray layout"
        );
        assert_eq!(fields[1].name, "val");
        assert_eq!(fields[1].offset, 12);
        assert!(matches!(fields[1].layout, CompoundFieldLayout::Scalar));
        assert_eq!(total_size, 16, "12 bytes array + 4 bytes i32 = 16");
    }
}
