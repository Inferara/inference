//! Scope-aware resolution of `external fn` declarations.
//!
//! Every phase that lowers, checks or reasons about a call to an `external fn`
//! must first answer the same question: *which* declaration does this bare name
//! mean at this point in the program? [`ExternIndex`] answers it once, for the
//! whole program, so those phases cannot drift apart.

use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;
use rustc_hash::FxHashMap;

/// The `external fn` declarations of a program, keyed by the scope that
/// introduces them.
///
/// Resolution must be by *declaration*, not by name. Two `external fn`s may
/// share a name — a bound top-level `sum` and an unbound spec-inner `sum` — and
/// only the declaration a given call site actually names decides whether that
/// call has a linked body and which module it comes from. A `use … from` clause
/// binds top-level declarations only, so a spec-inner one is never bound; a
/// name-keyed lookup would hand it the top-level declaration's origin and emit
/// an obligation naming a merged body the call does not reach.
///
/// Every consumer resolves through this one index, so their answers agree by
/// construction rather than by separate walks happening to coincide: `A024`
/// decides whether a call reaches a *bound* extern, and the specification
/// translator decides which declaration an obligation names. Through the full
/// pipeline `A024` rejects an unbound-extern call before translation runs,
/// which makes the agreement defense in depth rather than the sole guard; a
/// pipeline that skips analysis, as the proof-mode test gates do, has nothing
/// else.
///
/// Two scopes exhaust the language: a file's top level and a `spec` block
/// inside it. Specs do not nest, so an inner scope is keyed by its spec name
/// and a lookup is a two-step walk rather than a stack.
#[derive(Default)]
pub struct ExternIndex {
    decls: FxHashMap<ExternScope, FxHashMap<String, DefId>>,
}

/// A file, or a `spec` block within it — the two places an `external fn` may be
/// declared.
#[derive(PartialEq, Eq, Hash)]
struct ExternScope {
    module_path: Vec<String>,
    spec: Option<String>,
}

impl ExternIndex {
    /// Collects every `external fn` declaration in the program, in the scope
    /// that introduces it. A scope that declares none records no entry.
    #[must_use = "the index is the return value"]
    pub fn build(arena: &AstArena) -> Self {
        let mut decls: FxHashMap<ExternScope, FxHashMap<String, DefId>> = FxHashMap::default();
        for file in arena.source_files() {
            let spec_scopes = file
                .defs
                .iter()
                .filter_map(|&def_id| match &arena[def_id].kind {
                    Def::Spec { name, defs, .. } => {
                        Some((Some(arena[*name].name.clone()), defs.as_slice()))
                    }
                    _ => None,
                });
            for (spec, defs) in std::iter::once((None, file.defs.as_slice())).chain(spec_scopes) {
                let externs = extern_decls(arena, defs);
                if !externs.is_empty() {
                    decls.insert(
                        ExternScope {
                            module_path: file.module_path.clone(),
                            spec,
                        },
                        externs,
                    );
                }
            }
        }
        Self { decls }
    }

    /// The `external fn` a bare `name` resolves to in the file at
    /// `module_path`, innermost scope first, or `None` when the name is not an
    /// extern there.
    ///
    /// `spec` names the `spec` block the point of use sits in, and is `None` at
    /// the file's top level: a top-level call cannot see a spec-inner
    /// declaration, so passing `None` consults the top-level scope alone.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup(&self, module_path: &[String], spec: Option<&str>, name: &str) -> Option<DefId> {
        match spec {
            Some(spec) => self
                .in_scope(module_path, Some(spec), name)
                .or_else(|| self.lookup_top_level(module_path, name)),
            None => self.lookup_top_level(module_path, name),
        }
    }

    /// The top-level `external fn` named `name` in the file at `module_path`,
    /// ignoring every `spec` block within that file.
    ///
    /// This is the scope a `use … from` clause reaches: the clause is
    /// file-scoped and binds top-level declarations only, so a same-named
    /// spec-inner declaration must not be found here.
    #[must_use = "this is a pure lookup with no side effects"]
    pub fn lookup_top_level(&self, module_path: &[String], name: &str) -> Option<DefId> {
        self.in_scope(module_path, None, name)
    }

    fn in_scope(&self, module_path: &[String], spec: Option<&str>, name: &str) -> Option<DefId> {
        let scope = ExternScope {
            module_path: module_path.to_vec(),
            spec: spec.map(str::to_string),
        };
        self.decls.get(&scope)?.get(name).copied()
    }
}

/// The `external fn` declarations `defs` introduces directly, keeping the first
/// of a repeated name — a repeat within one scope is a type error reported
/// earlier, so which one wins cannot matter to a valid program.
fn extern_decls(arena: &AstArena, defs: &[DefId]) -> FxHashMap<String, DefId> {
    let mut externs = FxHashMap::default();
    for &def_id in defs {
        if let Def::ExternFunction { name, .. } = &arena[def_id].kind {
            externs.entry(arena[*name].name.clone()).or_insert(def_id);
        }
    }
    externs
}
