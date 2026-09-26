//! A055: a function's parameters must fit the words its target's runtime can
//! declare for one function.
//!
//! One target has such a limit today. `SpaceWasm`'s decoder counts a function's
//! parameters in four-byte words — two for a 64-bit value, one for anything
//! else — and keeps the count in a single byte (`Func::parameter_size: u8` in
//! `spacewasm` 0.7.1), so it refuses to load a module whose function declares
//! more than 255. The post-link conformance check in
//! `inference-target-conformance` has always refused such a module, naming the
//! WebAssembly function and the number. This rule asks the same question of the
//! source, so the refusal points at the declaration and says which parameters
//! the words went to.
//!
//! ## What is counted
//!
//! The signature code generation emits, word for word:
//!
//! - each declared parameter at the words its type lowers to: two for `i64`
//!   and `u64`, one for `bool` and every narrower integer, and one for a struct,
//!   an array or an enum, the first two being passed as an address and the
//!   third as its tag. A parameter written `_: T` counts like a named one: it
//!   binds nothing, but it still occupies a slot and every call still passes
//!   an argument for it. A bare positional `T`, which A050 rejects, counts as
//!   the `_: T` that repairs it;
//! - a `self` receiver, which is the address of its struct: one word;
//! - the hidden pointer a struct or array *result* is written through, which
//!   the caller passes ahead of every declared parameter: one word.
//!
//! A parameter whose type has no lowering adds nothing, and each such type is
//! refused somewhere else: `()` by A049, `string` by A048, a type parameter or
//! a type application by A051, a name nothing declares by the type checker,
//! and a function type or a `spec`'s name — which pass analysis — by code
//! generation, which has no lowering for either. No repair of one makes the
//! function cheaper than the words already counted, so a function reported
//! here stays over the limit once they are fixed. The count is a lower bound
//! all the same: a repair that gives such a parameter a type with a lowering
//! adds that type's words, which can take a function over the limit, and this
//! rule reports it once the repair is made.
//!
//! The one thing code generation appends to a signature that is not counted is
//! a specification function's hidden choice parameters. They exist only in a
//! proof-mode build, and this target builds in no mode but compile.
//!
//! The counting mirrors code generation's own signature lowering rather than
//! calling it: that lowering lives in `inference-wasm-codegen`, a later phase
//! this crate cannot depend on. Each seam is mirrored on the input code
//! generation reads it from — a parameter on its type node, the hidden pointer
//! on the return type's type-checker kind — and the two are held together by a
//! differential test in `inference-tests`, which compiles the corpus for the
//! `SpaceWasm` target and compares [`param_words`] with the parameter words the
//! conformance checker reads off every function of the emitted module.
//!
//! ## Which functions
//!
//! Every function the artifact defines: each top-level function of every file
//! in the program, and each method of each top-level struct. A `spec` is
//! skipped, because a compile-mode build strips it and this target builds in no
//! other mode.
//!
//! An `external fn` is skipped, because it is an import rather than a
//! definition, and one over the limit is refused regardless. Bound to a linked
//! module, its body is merged into the artifact and the post-link check
//! measures it there, which is also the only check that sees a function a
//! linked module defines without this program declaring it. Bound to the host,
//! it has at least 128 parameters, and an embedder can register a host function
//! of nine at most, which the host-import check refuses.
//!
//! ## Where it is reported
//!
//! On the declaration, spanning its parameter list from the first parameter to
//! the last, which is where either remedy is written: gathering related
//! parameters into one struct, or splitting the function.

use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, TypeId};
use inference_ast::nodes::{ArgData, ArgKind, Def, Location, SimpleTypeKind, TypeNode};
use inference_compiler_interface::TargetName;
use inference_fn_key::FnKey;
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};
use inference_type_checker::typed_context::TypedContext;
use rustc_hash::FxHashMap;

use crate::errors::{AnalysisDiagnostic, LabeledDiagnostic, ParamWords, ParamWordsGroup};
use crate::walker::render_type;

/// The most parameter words `SpaceWasm` accepts in one function.
///
/// A transcription of `inference_target_conformance::spacewasm::MAX_PARAM_WORDS`,
/// which carries the upstream source it was read from. This crate cannot depend
/// on that one, so `inference-tests` holds the two equal.
pub const SPACEWASM_MAX_PARAM_WORDS: u32 = 255;

crate::rule! {
    /// A function's parameters must fit the words its target's runtime can
    /// declare for one function.
    #[id = "A055"]
    #[name = "Parameter words over the target's per-function limit"]
    #[severity = error]
    pub struct ParamWordsExceeded;
    fn check(ctx: &TypedContext, options: AnalysisOptions) -> Vec<LabeledDiagnostic> {
        let Some(limit) = param_word_limit(options.target) else {
            return Vec::new();
        };
        let arena = ctx.arena();
        let mut findings = Vec::new();
        for function in defined_functions(ctx) {
            let words = signature_words(ctx, &function);
            if words.total() <= limit {
                continue;
            }
            let location = parameter_list_span(function.args)
                .unwrap_or(arena[function.def_id].location);
            findings.push(LabeledDiagnostic::new(
                function.module_path.to_vec(),
                AnalysisDiagnostic::ParamWordsExceeded {
                    function: function.source_name(arena),
                    words,
                    location,
                },
            ));
        }
        findings
    }
}

/// The most parameter words `target`'s runtime accepts in one function, or
/// `None` when it sets no limit of its own.
///
/// A match rather than a comparison with [`TargetName::SpaceWasm`], so a target
/// added to the vocabulary has to decide here instead of inheriting silence.
fn param_word_limit(target: TargetName) -> Option<u32> {
    match target {
        TargetName::SpaceWasm => Some(SPACEWASM_MAX_PARAM_WORDS),
        TargetName::Wasm32 | TargetName::Stellar => None,
    }
}

/// Every function a compile-mode artifact of the program defines, with the
/// parameter words its lowered signature declares.
///
/// Keyed by the [`FnKey`] code generation assigns the same function, so a caller
/// can pair each entry with the module's own function — which is what the
/// differential test in `inference-tests` does, through
/// [`FnKey::name_section_symbol`]. It is the count this rule measures, and the
/// only reason it is public is that test.
#[must_use = "returns the parameter words of every defined function"]
pub fn param_words(ctx: &TypedContext) -> FxHashMap<FnKey, u32> {
    defined_functions(ctx)
        .iter()
        .map(|function| (function.key(ctx.arena()), signature_words(ctx, function).total()))
        .collect()
}

/// One function the artifact defines, with the scope that names it and the
/// signature it declares.
struct DefinedFunction<'a> {
    /// The defining file's module path, empty for the entry file.
    module_path: &'a [String],
    /// The struct a method is declared on, `None` for a top-level function.
    struct_name: Option<&'a str>,
    def_id: DefId,
    args: &'a [ArgData],
    returns: Option<TypeId>,
}

impl DefinedFunction<'_> {
    /// The key code generation files this function under.
    fn key(&self, arena: &AstArena) -> FnKey {
        let name = arena.def_name(self.def_id);
        match self.struct_name {
            Some(struct_name) => FnKey::method_in(self.module_path.to_vec(), struct_name, name),
            None => FnKey::free_in(self.module_path.to_vec(), name),
        }
    }

    /// The function as a call spells it: `Struct::method` for a method, so the
    /// message names a declaration the reader can find even when two structs
    /// declare the same method name.
    fn source_name(&self, arena: &AstArena) -> String {
        let name = arena.def_name(self.def_id);
        match self.struct_name {
            Some(struct_name) => format!("{struct_name}::{name}"),
            None => name.to_string(),
        }
    }
}

/// The functions a compile-mode build of the program emits a body for, file by
/// file in the type context's order.
///
/// The same set code generation collects: top-level functions and the methods of
/// top-level structs. A `spec` is stripped in compile mode, and an `external fn`
/// is an import.
fn defined_functions(ctx: &TypedContext) -> Vec<DefinedFunction<'_>> {
    let arena = ctx.arena();
    let mut functions = Vec::new();
    for source_file in ctx.source_files() {
        let module_path = source_file.module_path.as_slice();
        for &def_id in &source_file.defs {
            match &arena[def_id].kind {
                Def::Function { args, returns, .. } => functions.push(DefinedFunction {
                    module_path,
                    struct_name: None,
                    def_id,
                    args,
                    returns: *returns,
                }),
                Def::Struct { name, methods, .. } => {
                    for &method_id in methods {
                        if let Def::Function { args, returns, .. } = &arena[method_id].kind {
                            functions.push(DefinedFunction {
                                module_path,
                                struct_name: Some(arena[*name].name.as_str()),
                                def_id: method_id,
                                args,
                                returns: *returns,
                            });
                        }
                    }
                }
                Def::Spec { .. }
                | Def::ExternFunction { .. }
                | Def::Enum { .. }
                | Def::Constant { .. } => {}
            }
        }
    }
    functions
}

/// The parameter words `function`'s lowered signature declares, itemized.
fn signature_words(ctx: &TypedContext, function: &DefinedFunction<'_>) -> ParamWords {
    let arena = ctx.arena();
    let mut receiver = false;
    let mut params: Vec<ParamWordsGroup> = Vec::new();
    for arg in function.args {
        let ty = match &arg.kind {
            ArgKind::SelfRef { .. } => {
                receiver = true;
                continue;
            }
            ArgKind::Named { ty, .. } | ArgKind::Ignored { ty } | ArgKind::TypeOnly(ty) => *ty,
        };
        let Some(words_each) = type_words(ctx, ty, function.module_path) else {
            continue;
        };
        let rendered = render_type(&TypeInfo::from_type_id(arena, ty).kind);
        match params.iter_mut().find(|group| group.ty == rendered) {
            Some(group) => group.count += 1,
            None => params.push(ParamWordsGroup {
                ty: rendered,
                count: 1,
                words_each,
            }),
        }
    }
    let result_pointer = function
        .returns
        .filter(|&ty| returns_through_pointer(ctx, ty, function.module_path))
        .map(|ty| render_type(&TypeInfo::from_type_id(arena, ty).kind));
    ParamWords {
        receiver,
        params,
        result_pointer,
    }
}

/// The words a parameter of type `ty` occupies in a lowered signature, or `None`
/// when code generation gives the type no lowering.
///
/// Mirrors `Compiler::val_type_from_type_id` in `inference-wasm-codegen`, arm for
/// arm and on the same queries: an `i64` value type is two words and an `i32`
/// one, and every type that lowers lowers to one of those two.
fn type_words(ctx: &TypedContext, ty: TypeId, module_path: &[String]) -> Option<u32> {
    let arena = ctx.arena();
    match &arena[ty].kind {
        TypeNode::Simple(SimpleTypeKind::Unit)
        | TypeNode::Generic { .. }
        | TypeNode::Function { .. }
        | TypeNode::QualifiedName { .. } => None,
        TypeNode::Simple(
            SimpleTypeKind::Bool
            | SimpleTypeKind::I8
            | SimpleTypeKind::U8
            | SimpleTypeKind::I16
            | SimpleTypeKind::U16
            | SimpleTypeKind::I32
            | SimpleTypeKind::U32,
        ) => Some(1),
        TypeNode::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => Some(2),
        // An array is the address of its first element, provided the element it
        // ultimately holds has a lowering of its own.
        TypeNode::Array { .. } => {
            type_words(ctx, innermost_array_element(arena, ty), module_path).map(|_| 1)
        }
        TypeNode::Qualified { .. } => {
            let path = arena[ty].kind.qualified_segments(arena).unwrap_or_default();
            ctx.qualified_type_is_nominal(&path, module_path).then_some(1)
        }
        TypeNode::Custom(ident) => {
            let name = &arena[*ident].name;
            let nominal = !matches!(name.as_str(), "string" | "String")
                && (ctx.lookup_struct_in(name, module_path).is_some()
                    || ctx.lookup_enum_in(name, module_path).is_some());
            nominal.then_some(1)
        }
    }
}

/// The element type an array type ultimately holds, or `ty` itself when it names
/// no array.
fn innermost_array_element(arena: &AstArena, ty: TypeId) -> TypeId {
    let mut current = ty;
    while let TypeNode::Array { element, .. } = &arena[current].kind {
        current = *element;
    }
    current
}

/// Whether a function returning `ty` receives the result through a hidden
/// pointer rather than as a value.
///
/// Mirrors `Compiler::register_sret_if_compound` in `inference-wasm-codegen`: an
/// array, and a bare or `::`-qualified name that resolves to a struct. An enum is
/// returned as its tag.
fn returns_through_pointer(ctx: &TypedContext, ty: TypeId, module_path: &[String]) -> bool {
    match &TypeInfo::from_type_id(ctx.arena(), ty).kind {
        TypeInfoKind::Array(..) => true,
        TypeInfoKind::Custom(name) => ctx.lookup_struct_in(name, module_path).is_some(),
        TypeInfoKind::Qualified(path) | TypeInfoKind::QualifiedName(path) => {
            let segments: Vec<String> = path.split("::").map(ToString::to_string).collect();
            ctx.lookup_struct_by_qualified_path(&segments, module_path).is_some()
        }
        _ => false,
    }
}

/// The span of a parameter list, from the start of its first parameter to the
/// end of its last, or `None` for an empty one.
fn parameter_list_span(args: &[ArgData]) -> Option<Location> {
    let (first, last) = (args.first()?.location, args.last()?.location);
    Some(Location::new(
        first.offset_start,
        last.offset_end,
        first.start_line,
        first.start_column,
        last.end_line,
        last.end_column,
    ))
}
