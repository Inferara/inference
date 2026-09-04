//! Canonical WASM function identity, and the name-section namespace it shares
//! with the static-merge linker.
//!
//! A multi-file Inference program flattens every file into one WASM module, so
//! two files may each define a `fn add`, and a struct's associated function may
//! share a bare name with a free function in a sibling file. Identifying a
//! function by a flat `.`-joined string conflates these: the module-join `.` and
//! the struct-method-join `.` are indistinguishable, so `Method` on struct `mid`
//! in file `a` (`a.mid.make`) collides with a free `make` in file `a/mid`
//! (`a.mid.make`). [`FnKey`] is the structured key that partitions the namespace
//! so the collision cannot happen by construction.
//!
//! The code generator (when assigning WASM function indices) and the analysis
//! passes (when building the whole-program call graph for A035/A036) both key
//! functions on this single type, so the two agree on function identity without
//! re-deriving it from parallel string schemes.
//!
//! The static-merge linker and the proof translation are the other two
//! consumers, and they use the crate differently: neither keys anything on
//! [`FnKey`]. They format and read *strings* in the WASM name section, through
//! [`merged_name`], the second scheme written into that one section.
//!
//! The two schemes are disjoint by construction, and keeping them so is this
//! crate's second job. A compiled function's symbol
//! ([`FnKey::name_section_symbol`]) joins Inference identifiers with `.` and is
//! therefore drawn from `[A-Za-z0-9_.]`; every merged name carries
//! [`MERGED_SEPARATOR`]'s `::`, which that alphabet cannot produce. The proof
//! translation resolves an obligation symbol against the post-merge section by
//! string equality, so the disjointness is what stops an obligation about the
//! program's own function from being answered by a linked external's body. Both
//! schemes live here so the one namespace they write into — and the argument
//! that they cannot meet in it — can be read in one place.

#![warn(clippy::pedantic)]

/// Separator used in mangled method names (`"{StructName}.{method_name}"`) and
/// in the defining-file qualifier of a rendered key (`"lib.arith.add"`).
///
/// Dot is used because it matches Zig's convention and is standard across the
/// WASM ecosystem. Since `.` is a syntax token in Inference (member access), it
/// cannot appear in user-defined identifiers, so no single *identifier* can
/// forge a join.
///
/// It is the *only* joiner of the program half of the name section, which is
/// what bounds that half's alphabet to `[A-Za-z0-9_.]` — see
/// [`MERGED_SEPARATOR`] for what that bound buys.
const METHOD_SEPARATOR: &str = ".";

/// Separator that opens the merged-body half of the WASM name section:
/// `"{logical_module}::{…}"`.
///
/// The name section has two producers — code generation, which writes
/// [`FnKey::name_section_symbol`], and the static-merge linker, which writes
/// [`merged_name`] — and the proof translation reads it back, resolving an
/// obligation's function symbol by string equality over the whole post-merge
/// section. The two producers must therefore write disjoint sets of strings.
/// Were they to meet, an obligation minted from a source-level call could
/// resolve to a merged external body and be discharged as *true* while the
/// claim it was written to state about the program is false.
///
/// `::` is what makes them disjoint, and the lexer closes the argument: a
/// compiled function's symbol is a join of Inference identifiers
/// (`[A-Za-z_][A-Za-z0-9_]*`) over `.` alone, so it is drawn from
/// `[A-Za-z0-9_.]` and contains no `:`; every merged name contains `::`.
/// No merged name can equal a compiled function's symbol, whatever a program
/// names its modules, structs and functions — so nothing has to be renamed,
/// reserved, or diagnosed to keep a source module and a linked logical module
/// of the same name apart.
///
/// It is `::` rather than some other `:`-bearing string because a logical
/// module is *already* written that way in source (`use { hash } from
/// crypto::digest;`), so a merged name reads back as the path it came from:
/// `crypto::digest::hash`. A logical module may therefore carry the separator
/// itself; [`merged_name`] stays unambiguous under that, for the reasons given
/// there.
///
/// What the argument bounds is what *this compiler* produces. The linker is a
/// library, and a main module handed to it from elsewhere may carry any name
/// section and any import strings at all, so a name section that did not come
/// from code generation is not bounded by the identifier grammar and cannot be
/// argued about this way. Keeping the schemes disjoint is a construction, not a
/// validation: what a merged module is actually *held* to is the post-merge
/// check that every obligation symbol resolves to exactly one function.
pub const MERGED_SEPARATOR: &str = "::";

/// Marks a merged body that is *internal* to its external module: reachable
/// from a closure root, but named by nothing an Inference declaration can
/// write.
///
/// Where [`MERGED_SEPARATOR`] separates the merged half of the name section
/// from the program half, this separates the merged half from itself. A root is
/// named after the import's export field, which is the `external fn`
/// declaration's own name and so an Inference identifier — it can never begin
/// with `#`. An inner callee instead keeps the debug name its *foreign* module
/// gave it, which is unconstrained and may be exactly that export field. The
/// mark keeps the root — the only merged body an obligation can name — from
/// being shadowed by an external that calls an inner function `double` while
/// also exporting `double`.
///
/// Crate-private, unlike [`MERGED_SEPARATOR`]: nothing outside this crate reads
/// it. The mark is applied by [`merged_name::callee`] and
/// [`merged_name::anonymous`], and recognizing it in a finished name would
/// require knowing where the module prefix ends — which the string alone does
/// not say — so exporting it would offer a constant that cannot be used
/// correctly on its own.
pub(crate) const MERGED_INTERNAL_MARK: &str = "#";

/// Folds a defining file's module path into a spec name, producing the
/// file-qualified spec identifier rendered for display and the proof
/// translation.
///
/// The entry file has an empty module path, leaving the spec name unchanged. A
/// non-entry file's segments are underscore-joined ahead of the spec name
/// (`["lib", "checks"]` + `S` → `lib_checks_S`). Underscore (not `.`) keeps the
/// result a legal Rocq identifier so it travels intact into the proof
/// translation.
///
/// The fold is **lossy** — it is not injective when a segment itself contains a
/// leading/trailing `_` (`["lib", "checks"]` + `S` and `["lib_checks"]` + `S`
/// both yield `lib_checks_S`). It is therefore used **only** for rendering
/// (`FnKey::Display`) and the proof grammar, never for key identity: a spec
/// [`FnKey`] keeps its `module_path` and bare `spec` structurally separate so
/// two such files stay distinct keys. The code generator rejects the rare
/// genuine clash where two distinct files fold to one string (their lossy
/// rendered name would otherwise collide in the WASM spec map and the Rocq
/// grammar).
///
/// This is the single implementation of that fold: code generation's
/// `qualified_spec_name` delegates here and [`FnKey::Display`] renders through
/// it, so every phase produces byte-identical spec identifiers.
#[must_use = "returns the file-qualified spec name"]
pub fn fold_spec_name(module_path: &[String], spec: &str) -> String {
    if module_path.is_empty() {
        spec.to_string()
    } else {
        format!("{}_{spec}", module_path.join("_"))
    }
}

/// Names for the bodies the static-merge linker splices in from an external
/// `.wasm`.
///
/// These share the WASM name section with the compiled functions'
/// [`FnKey::name_section_symbol`]s, so they live beside it: the section is one
/// namespace, and collocating its producers is what keeps the argument that
/// they cannot collide in one place.
///
/// Three separations, each closing a distinct way two bodies could meet:
///
/// - every name carries [`MERGED_SEPARATOR`], which a compiled function's
///   symbol cannot contain, so the merged half and the program half are
///   disjoint;
/// - every name keeps the `logical_module` prefix, because an export field
///   alone is not unique — two externals bound under different logical modules
///   may export, and internally call, the same name;
/// - the inner bodies carry a `#` mark after that prefix, so a foreign
///   module's private callee cannot be named the same as one of that
///   module's own roots.
///
/// A `logical_module` is itself a `::`-joined identifier path (`crypto::digest`
/// from `use { hash } from crypto::digest;`), so the prefix is not a single
/// segment and the module boundary is not found by looking for the first `::`.
/// It does not need to be. An `export_field` is an Inference identifier, so a
/// root decomposes only one way — `crypto::digest::hash` cannot also be module
/// `crypto` with field `digest::hash`. An inner name comes from a foreign
/// module and is unconstrained, so the mark carries that weight instead: it
/// sits immediately after the module prefix, which pins the boundary that the
/// name itself cannot (`a::b::#x` from module `a::b` is not `a::#b::#x` from
/// module `a`).
///
/// [`merged_name::root`] additionally carries a *contract*: a proof-mode
/// obligation that applies a linked `external fn` writes exactly this string,
/// and `wasm-to-v` resolves it verbatim against the linked module's name
/// section. Code generation and the linker therefore both call it rather than
/// formatting the name themselves — a drift between the two would leave the
/// obligation naming a function the module does not carry, or a different one.
///
/// None of these separators reaches the emitted Rocq: `sanitize_rocq_identifier`
/// maps every byte outside `[A-Za-z0-9_]` to `_` and then collapses `__` runs,
/// so `mathlib::double` and `mathlib::#helper` sanitize to the `Definition`
/// names `mathlib.double` and `mathlib.helper` already sanitized to.
pub mod merged_name {
    /// The body that satisfies an import of `export_field` from
    /// `logical_module` — the closure root, and the only merged function an
    /// Inference declaration names.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn root(logical_module: &str, export_field: &str) -> String {
        format!("{logical_module}{}{export_field}", super::MERGED_SEPARATOR)
    }

    /// A function reachable from a merged root, keeping the debug name its own
    /// module gave it.
    ///
    /// Marked internal with a `#` after the module prefix because
    /// `source_name` comes from a foreign module and is unconstrained: it may
    /// be exactly the export field one of that module's roots is named after.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn callee(logical_module: &str, source_name: &str) -> String {
        format!(
            "{logical_module}{}{}{source_name}",
            super::MERGED_SEPARATOR,
            super::MERGED_INTERNAL_MARK
        )
    }

    /// A merged function whose source module carried no name section, named
    /// from its deterministic output index so the artifact stays reproducible.
    ///
    /// Internal for the same reason as [`callee`]: it is a body no declaration
    /// names, and `func_7` is a name a foreign module could equally have
    /// exported.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn anonymous(logical_module: &str, out_func_idx: u32) -> String {
        format!(
            "{logical_module}{}{}func_{out_func_idx}",
            super::MERGED_SEPARATOR,
            super::MERGED_INTERNAL_MARK
        )
    }
}

/// Structured key identifying a WASM function across the whole program.
///
/// The four variants partition the WASM function namespace so that
/// `Method { struct_name: "Foo", name: "bar" }` cannot textually collide with
/// `SpecFree { spec: "Foo", name: "bar" }` even though both would share the
/// `"Foo.bar"` string under a flat `String` key. The collision class is
/// eliminated by construction.
///
/// Every variant carries `module_path`: the source-root-relative segments of the
/// item's **defining** file. Two files may each define a `fn add`; qualifying the
/// key by the defining file keeps the two distinct. The entry file's
/// `module_path` is empty, so its items keep unqualified names — a single-file
/// program produces byte-identical output to before file qualification existed.
/// For a method the qualifier is the **struct's** defining file, not the call
/// site's.
///
/// The spec variants keep `module_path` and the **bare** `spec` name
/// structurally separate; they do **not** fold the file into the `spec` string.
/// This is what makes the key injective: `["lib", "checks"]` + `S` and
/// `["lib_checks"]` + `S` are distinct keys even though their lossy rendered name
/// ([`fold_spec_name`]) is identical. A recursive spec function therefore keeps
/// its own self-edge in the call graph instead of being masked by a same-folded
/// sibling. Construct the spec variants through [`Self::spec_free_folded`] /
/// [`Self::spec_method_folded`].
///
/// `Display` reproduces the mangled-string form: the free/method variants prefix
/// the `.`-joined module path when non-empty (`lib.arith.add`,
/// `lib.arith.Point.new`); the spec variants render the **folded** spec name
/// instead (`lib_checks_S.rec`) so the rendered string stays byte-identical to
/// the proof grammar and prior output. Display is therefore deliberately lossy
/// for spec keys (two injective keys can render the same string). It is used for
/// diagnostic messages and panic descriptions. The codegen↔analysis frame-size
/// interchange map (`CodegenOutput::frame_sizes`, `estimate_frame_sizes`) is
/// keyed by the structured [`FnKey`] itself, not by this rendering — were it
/// keyed by `Display`, two keys that render identically would collapse into
/// one slot, which is exactly what the structured key exists to prevent. This
/// rendering is also *not* what the WASM `name` section carries — and so not
/// what a `.wat` disassembly of the module shows: that is
/// [`Self::name_section_symbol`].
///
/// [`Self::name_section_symbol`] is the *other* rendering, and the two are
/// deliberately separate: the name section is a namespace shared with the
/// linker and read back by the proof translation, so what goes into it answers
/// to that namespace's rules rather than to what reads best in a diagnostic.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum FnKey {
    Free {
        module_path: Vec<String>,
        name: String,
    },
    Method {
        module_path: Vec<String>,
        struct_name: String,
        name: String,
    },
    SpecFree {
        module_path: Vec<String>,
        spec: String,
        name: String,
    },
    SpecMethod {
        module_path: Vec<String>,
        spec: String,
        struct_name: String,
        name: String,
    },
}

impl FnKey {
    /// A free function defined in the file named by `module_path`.
    #[must_use = "constructs a key"]
    pub fn free_in(module_path: Vec<String>, name: impl Into<String>) -> Self {
        Self::Free {
            module_path,
            name: name.into(),
        }
    }

    /// A method or associated function on `struct_name`, defined in the file
    /// named by `module_path` (the struct's defining file).
    #[must_use = "constructs a key"]
    pub fn method_in(
        module_path: Vec<String>,
        struct_name: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self::Method {
            module_path,
            struct_name: struct_name.into(),
            name: name.into(),
        }
    }

    /// A spec-inner free function with an empty `module_path` and the given
    /// `spec` stored verbatim.
    ///
    /// Prefer [`Self::spec_free_folded`], which keys by the real defining file.
    /// This bare form is for the entry file (empty module path) or a caller that
    /// holds only an already-rendered spec name; do not mix it with the folded
    /// form for the same logical function, or the two keys will differ.
    #[must_use = "constructs a key"]
    pub fn spec_free(spec: impl Into<String>, name: impl Into<String>) -> Self {
        Self::SpecFree {
            module_path: Vec::new(),
            spec: spec.into(),
            name: name.into(),
        }
    }

    /// A spec-inner free function, keyed by its defining `module_path` and the
    /// **bare** `spec` name (kept structurally separate for injectivity; the file
    /// is folded into the spec name only at [`Display`](Self) time).
    #[must_use = "constructs a key"]
    pub fn spec_free_folded(module_path: &[String], spec: &str, name: impl Into<String>) -> Self {
        Self::SpecFree {
            module_path: module_path.to_vec(),
            spec: spec.to_string(),
            name: name.into(),
        }
    }

    /// A spec-inner method with an empty `module_path` and the given `spec`
    /// stored verbatim.
    ///
    /// Prefer [`Self::spec_method_folded`], which keys by the real defining file.
    /// See [`Self::spec_free`] for when the bare form applies.
    #[must_use = "constructs a key"]
    pub fn spec_method(
        spec: impl Into<String>,
        struct_name: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self::SpecMethod {
            module_path: Vec::new(),
            spec: spec.into(),
            struct_name: struct_name.into(),
            name: name.into(),
        }
    }

    /// A spec-inner method, keyed by its defining `module_path` and the **bare**
    /// `spec` name (kept structurally separate for injectivity; the file is
    /// folded into the spec name only at [`Display`](Self) time).
    #[must_use = "constructs a key"]
    pub fn spec_method_folded(
        module_path: &[String],
        spec: &str,
        struct_name: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self::SpecMethod {
            module_path: module_path.to_vec(),
            spec: spec.to_string(),
            struct_name: struct_name.into(),
            name: name.into(),
        }
    }

    /// The symbol code generation writes into the WASM `name` section for this
    /// function.
    ///
    /// This is the program half of that namespace. Every component is an
    /// Inference identifier and `.` is the only joiner, so the result is drawn
    /// from `[A-Za-z0-9_.]` and can never equal a [`merged_name`], all of which
    /// carry [`MERGED_SEPARATOR`]. A proof-mode obligation minted from a
    /// source-level call therefore resolves to the program's own function even
    /// when a linked external's logical module is named after one of the
    /// program's source files.
    ///
    /// The free and method variants render their full [`Display`](Self) form,
    /// defining file included, so two files defining one bare name stay as
    /// distinct here as they already are as keys.
    ///
    /// The spec variants deliberately render **unqualified** — `SpecFree` as
    /// its bare `name`, `SpecMethod` as `{StructName}.{name}`. Spec membership
    /// does not travel through the name section at all: it travels as function
    /// indices in `inference.spec_funcs`, and the proof translation resolves a
    /// reachability obligation by stripping the folded spec prefix off the
    /// obligation symbol and looking the remaining *bare* name up in the
    /// section. Qualifying the spec variants would break that lookup, and would
    /// move the emitted Rocq `Definition` name of every specification function.
    /// The carve-out is safe for the same reason the rest of the scheme is: a
    /// spec-inner symbol is still an identifier join, still `:`-free, so it
    /// still cannot meet a merged name.
    #[must_use = "returns the function's name-section symbol"]
    pub fn name_section_symbol(&self) -> String {
        match self {
            Self::SpecFree { name, .. } => name.clone(),
            Self::SpecMethod {
                struct_name, name, ..
            } => format!("{struct_name}{METHOD_SEPARATOR}{name}"),
            Self::Free { .. } | Self::Method { .. } => self.to_string(),
        }
    }
}

impl std::fmt::Display for FnKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Free and method keys prefix the `.`-joined defining file.
            Self::Free { module_path, name } => write!(f, "{}", qualify_dotted(module_path, name)),
            Self::Method {
                module_path,
                struct_name,
                name,
            } => write!(
                f,
                "{}",
                qualify_dotted(
                    module_path,
                    &format!("{struct_name}{METHOD_SEPARATOR}{name}")
                )
            ),
            // Spec keys fold the defining file into the spec name with `_` (the
            // Rocq-legal, byte-identical-to-prior-output form), so they bypass the
            // generic `.`-join tail above — prefixing it would double-qualify
            // (`lib.checks.lib_checks_S.rec`).
            Self::SpecFree {
                module_path,
                spec,
                name,
            } => write!(f, "{}.{name}", fold_spec_name(module_path, spec)),
            Self::SpecMethod {
                module_path,
                spec,
                struct_name,
                name,
            } => write!(
                f,
                "{}.{struct_name}{METHOD_SEPARATOR}{name}",
                fold_spec_name(module_path, spec)
            ),
        }
    }
}

/// Prefixes a rendered key body with its `.`-joined defining file, or returns it
/// unchanged for the entry file (empty `module_path`).
fn qualify_dotted(module_path: &[String], rest: &str) -> String {
    if module_path.is_empty() {
        rest.to_string()
    } else {
        format!(
            "{}{METHOD_SEPARATOR}{rest}",
            module_path.join(METHOD_SEPARATOR)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segs: &[&str]) -> Vec<String> {
        segs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn display_entry_file_is_unqualified() {
        assert_eq!(FnKey::free_in(vec![], "add").to_string(), "add");
        assert_eq!(
            FnKey::method_in(vec![], "Point", "new").to_string(),
            "Point.new"
        );
        assert_eq!(FnKey::spec_free("MySpec", "f").to_string(), "MySpec.f");
        assert_eq!(
            FnKey::spec_method("MySpec", "Point", "new").to_string(),
            "MySpec.Point.new"
        );
    }

    #[test]
    fn display_non_entry_file_is_dot_qualified() {
        let p = path(&["lib", "geometry"]);
        assert_eq!(
            FnKey::free_in(p.clone(), "add").to_string(),
            "lib.geometry.add"
        );
        assert_eq!(
            FnKey::method_in(p, "Point", "new").to_string(),
            "lib.geometry.Point.new"
        );
    }

    #[test]
    fn fold_spec_name_underscore_joins_non_entry_file() {
        assert_eq!(fold_spec_name(&[], "S"), "S");
        assert_eq!(
            fold_spec_name(&path(&["lib", "checks"]), "S"),
            "lib_checks_S"
        );
        assert_eq!(fold_spec_name(&path(&["math"]), "Sp"), "math_Sp");
    }

    #[test]
    fn folded_spec_constructors_render_the_folded_name() {
        // The folded constructors keep the real module_path + bare spec, but
        // Display still renders the underscore-folded name byte-identically to
        // the proof grammar and prior output.
        let p = path(&["lib", "checks"]);
        assert_eq!(
            FnKey::spec_free_folded(&p, "S", "f").to_string(),
            "lib_checks_S.f"
        );
        assert_eq!(
            FnKey::spec_method_folded(&p, "S", "T", "m").to_string(),
            "lib_checks_S.T.m"
        );
    }

    #[test]
    fn folded_spec_key_is_not_the_bare_empty_path_key() {
        // The folded constructor now keeps the real module_path, so it is a
        // *different key* from the bare empty-path constructor even though the
        // bare one was once how codegen represented the same function. Display is
        // equal (lossy); identity is not.
        let p = path(&["lib", "checks"]);
        let folded = FnKey::spec_free_folded(&p, "S", "f");
        let bare = FnKey::spec_free(fold_spec_name(&p, "S"), "f");
        assert_ne!(
            folded, bare,
            "the folded key keeps a real module_path and must not equal the empty-path bare key"
        );
        assert_eq!(
            folded.to_string(),
            bare.to_string(),
            "both still render the same folded name (Display is lossy by design)"
        );
    }

    /// The spec fold is non-injective (`[lib,checks]+S` and `[lib_checks]+S` both
    /// render `lib_checks_S`), so spec identity must live in the structured key,
    /// not the folded string. Two such files must produce *distinct* keys (or a
    /// recursive spec function in one would have its call-graph self-edge masked by
    /// a same-folded sibling, escaping the recursion check).
    #[test]
    fn spec_key_distinguishes_files_that_fold_to_the_same_name() {
        let nested = FnKey::spec_free_folded(&path(&["lib", "checks"]), "S", "rec");
        let flat = FnKey::spec_free_folded(&path(&["lib_checks"]), "S", "rec");
        assert_ne!(
            nested, flat,
            "files `lib/checks` and `lib_checks` must be distinct spec keys"
        );
        assert_eq!(nested.to_string(), "lib_checks_S.rec");
        assert_eq!(
            flat.to_string(),
            "lib_checks_S.rec",
            "Display is lossy by design — both fold to the same rendered name"
        );

        // The same for spec methods.
        let nested_m = FnKey::spec_method_folded(&path(&["lib", "checks"]), "S", "T", "m");
        let flat_m = FnKey::spec_method_folded(&path(&["lib_checks"]), "S", "T", "m");
        assert_ne!(nested_m, flat_m);
        assert_eq!(nested_m.to_string(), "lib_checks_S.T.m");
        assert_eq!(flat_m.to_string(), "lib_checks_S.T.m");
    }

    #[test]
    fn name_section_symbol_qualifies_free_and_method_keys() {
        assert_eq!(
            FnKey::free_in(path(&["lib", "arith"]), "add").name_section_symbol(),
            "lib.arith.add"
        );
        assert_eq!(
            FnKey::method_in(path(&["lib", "geo"]), "Point", "dist").name_section_symbol(),
            "lib.geo.Point.dist"
        );
    }

    /// A single-file program's symbols are exactly what they were before the
    /// name section carried a defining file, so its artifacts do not move.
    #[test]
    fn name_section_symbol_of_an_entry_file_key_is_its_display() {
        let free = FnKey::free_in(vec![], "add");
        let method = FnKey::method_in(vec![], "Point", "new");
        assert_eq!(free.name_section_symbol(), "add");
        assert_eq!(method.name_section_symbol(), "Point.new");
        assert_eq!(free.name_section_symbol(), free.to_string());
        assert_eq!(method.name_section_symbol(), method.to_string());
    }

    /// Spec-inner functions are carved out of the file qualification: the proof
    /// translation resolves a reachability obligation by stripping the folded
    /// spec prefix and looking the remaining *bare* name up in this section, and
    /// spec membership travels as indices in `inference.spec_funcs` rather than
    /// through the name at all.
    #[test]
    fn name_section_symbol_leaves_spec_keys_unqualified() {
        let p = path(&["lib", "checks"]);
        assert_eq!(
            FnKey::spec_free_folded(&p, "S", "ex_double").name_section_symbol(),
            "ex_double"
        );
        assert_eq!(
            FnKey::spec_method_folded(&p, "S", "Point", "dist").name_section_symbol(),
            "Point.dist"
        );
        // The entry-file spec forms render the same, so a spec function's symbol
        // does not depend on where its file sits.
        assert_eq!(
            FnKey::spec_free("S", "ex_double").name_section_symbol(),
            "ex_double"
        );
        assert_eq!(
            FnKey::spec_method("S", "Point", "dist").name_section_symbol(),
            "Point.dist"
        );
        // Display still qualifies them; only the name section does not.
        assert_eq!(
            FnKey::spec_free_folded(&p, "S", "ex_double").to_string(),
            "lib_checks_S.ex_double"
        );
    }

    /// The disjointness invariant, stated over both producers: no compiled
    /// function's symbol contains `:`, and every merged name does. This is what
    /// lets the proof translation resolve an obligation symbol by string
    /// equality over the whole post-merge name section without a source module
    /// and a linked logical module of the same name answering for each other.
    #[test]
    fn program_symbols_are_colon_free_and_merged_names_are_not() {
        let program = [
            FnKey::free_in(vec![], "add"),
            FnKey::free_in(path(&["mathlib"]), "helper"),
            FnKey::method_in(vec![], "Point", "new"),
            FnKey::method_in(path(&["lib", "geo"]), "Point", "dist"),
            FnKey::spec_free_folded(&path(&["lib"]), "S", "ex_double"),
            FnKey::spec_method_folded(&path(&["lib"]), "S", "Point", "dist"),
        ];
        for key in &program {
            let sym = key.name_section_symbol();
            assert!(
                !sym.contains(':'),
                "`{sym}` is a compiled function's symbol and must carry no `:`"
            );
        }

        let merged = [
            merged_name::root("mathlib", "double"),
            merged_name::callee("mathlib", "helper"),
            merged_name::anonymous("mathlib", 7),
        ];
        for name in &merged {
            assert!(
                name.contains(MERGED_SEPARATOR),
                "`{name}` is a merged body's name and must carry `{MERGED_SEPARATOR}`"
            );
        }

        for key in &program {
            for name in &merged {
                assert_ne!(&key.name_section_symbol(), name);
            }
        }
    }

    #[test]
    fn merged_names_have_the_documented_shapes() {
        assert_eq!(merged_name::root("mathlib", "double"), "mathlib::double");
        assert_eq!(merged_name::callee("mathlib", "helper"), "mathlib::#helper");
        assert_eq!(merged_name::anonymous("mathlib", 7), "mathlib::#func_7");
    }

    /// A logical module is a `::`-joined identifier path, so it carries the
    /// merged separator itself and a merged name reads back as the source path.
    /// The shapes stay unambiguous: a root's field is an identifier, and an
    /// inner name — which is a foreign module's and so unconstrained — is
    /// pinned by the mark sitting immediately after the module prefix.
    #[test]
    fn a_path_joined_logical_module_keeps_the_shapes_unambiguous() {
        assert_eq!(
            merged_name::root("crypto::digest", "hash"),
            "crypto::digest::hash"
        );
        assert_eq!(
            merged_name::callee("crypto::digest", "compress"),
            "crypto::digest::#compress"
        );
        assert_eq!(
            merged_name::anonymous("crypto::digest", 4),
            "crypto::digest::#func_4"
        );

        // Module `a::b`'s inner `x` against module `a`'s inner named `b::#x`.
        assert_ne!(
            merged_name::callee("a::b", "x"),
            merged_name::callee("a", "b::#x")
        );
        // Module `a::b`'s root `x` against module `a`'s inner named `b::x`.
        assert_ne!(
            merged_name::root("a::b", "x"),
            merged_name::callee("a", "b::x")
        );
        // And a `::`-joined module's root is still `:`-bearing, so it stays out
        // of the program half.
        assert!(
            !FnKey::free_in(path(&["crypto", "digest"]), "hash")
                .name_section_symbol()
                .contains(':')
        );
    }

    /// The reproduced miscompile, in key terms: a source file `mathlib` and a
    /// linked external whose logical module is also `mathlib`, the external
    /// carrying a private inner function of the same bare name. Under one
    /// separator both were `mathlib.helper`, so an obligation about the
    /// program's `helper` resolved to the external's body — and a claim false of
    /// the program was emitted as a true, dischargeable obligation.
    #[test]
    fn source_module_cannot_be_answered_for_by_a_linked_module_of_the_same_name() {
        let own = FnKey::free_in(path(&["mathlib"]), "helper");
        assert_eq!(own.name_section_symbol(), "mathlib.helper");
        assert_ne!(
            own.name_section_symbol(),
            merged_name::callee("mathlib", "helper")
        );
        assert_ne!(
            own.name_section_symbol(),
            merged_name::root("mathlib", "helper")
        );

        // The same for a struct named after a bound logical module, which put two
        // `mathlib.double` entries in one section.
        let method = FnKey::method_in(vec![], "mathlib", "double");
        assert_eq!(method.name_section_symbol(), "mathlib.double");
        assert_ne!(
            method.name_section_symbol(),
            merged_name::root("mathlib", "double")
        );

        // And for a stripped external's synthesized name against a program
        // function that happens to be called `func_7`.
        assert_ne!(
            FnKey::free_in(path(&["mathlib"]), "func_7").name_section_symbol(),
            merged_name::anonymous("mathlib", 7)
        );
    }

    /// The adversarial external: a foreign module exporting `double` while also
    /// calling a private inner function it named `double`. The export field is
    /// an Inference identifier (the `external fn` declaration's own name) and so
    /// cannot begin with the internal mark, which is what keeps the two apart.
    #[test]
    fn merged_root_and_inner_callee_of_one_module_stay_distinct() {
        let root = merged_name::root("mathlib", "double");
        let inner = merged_name::callee("mathlib", "double");
        assert_ne!(root, inner);
        assert!(!root.contains(MERGED_INTERNAL_MARK));
        assert!(inner.contains(MERGED_INTERNAL_MARK));

        // An inner function a foreign module named `#double` still cannot forge
        // the root, since the root has no mark to forge.
        assert_ne!(merged_name::callee("mathlib", "#double"), root);
        // Nor can a stripped external's index-derived name meet a root, whatever
        // field that module exports.
        assert_ne!(
            merged_name::anonymous("mathlib", 7),
            merged_name::root("mathlib", "func_7")
        );
    }

    /// The FAMILY 2 regression: a struct associated/instance function and a
    /// same-named free function in a sibling file must be distinct keys. Under
    /// the old flat-string scheme both rendered to `a.mid.make` and the free node
    /// hijacked the method's call-graph slot, masking A035 recursion.
    #[test]
    fn method_does_not_collide_with_sibling_file_free_fn() {
        let method = FnKey::method_in(path(&["a"]), "mid", "make");
        let free = FnKey::free_in(path(&["a", "mid"]), "make");
        assert_ne!(
            method, free,
            "a struct method and a sibling-file free fn must be distinct keys"
        );
        // They do still render to the same Display string — Display is lossy by
        // design — which is exactly why identity must be the structured key, not
        // the string.
        assert_eq!(method.to_string(), free.to_string());
    }
}
