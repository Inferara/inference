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
//! [`merged_name`], which is the second scheme composed over the same `.`
//! separator. Both schemes live here so the one namespace they write into can
//! be read in one place.

#![warn(clippy::pedantic)]

/// Separator used in mangled method names: `"{StructName}.{method_name}"`.
///
/// Dot is used because it matches Zig's convention and is standard across the
/// WASM ecosystem. Since `.` is a syntax token in Inference (member access), it
/// cannot appear in user-defined identifiers, so no single *identifier* can
/// forge a join.
///
/// That argument covers identifiers, and it is no longer the whole story: a
/// second scheme, [`merged_name`], composes `{logical_module}.{export_field}`
/// over this same separator. Two joins over one separator can meet — a struct
/// named `mathlib` and a `use { double } from mathlib;` binding both render
/// `mathlib.double` — so a collision surfaces in the *name section*, between
/// schemes, where it cannot within either one. See [`merged_name`] for what
/// does and does not catch it.
const METHOD_SEPARATOR: &str = ".";

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
/// These share the WASM name section with [`FnKey`]'s own rendering, so they
/// live beside it: the section is one namespace, and collocating its producers
/// is what makes that namespace *reviewable*. It does not make it injective.
/// All three keep the `logical_module` prefix because the export field alone is
/// not unique — two externals bound under different logical modules may export
/// the same field, and their merged bodies would then collide — but that prefix
/// only closes the collisions *within* this scheme.
///
/// Across the two schemes injectivity does not hold, because both join over the
/// same `.`: `{StructName}.{method}` and `{logical_module}.{export_field}`
/// render to one string whenever a struct is named after a bound logical
/// module. A `struct mathlib` carrying a `double` method, beside
/// `use { double } from mathlib;`, compiles and links with two functions named
/// `mathlib.double` in the name section.
///
/// The guard that exists is `wasm-to-v`'s `resolve_app_symbols`, whose
/// ambiguity arm refuses a symbol several defined functions share. It fires
/// only when an obligation *applies* the name, so it covers the obligation path
/// and nothing else: in compile mode the duplicate ships unremarked.
///
/// [`merged_name::root`] additionally carries a *contract*: a proof-mode
/// obligation that applies a linked `external fn` writes exactly this string,
/// and `wasm-to-v` resolves it verbatim against the linked module's name
/// section. Code generation and the linker therefore both call it rather than
/// formatting the name themselves — a drift between the two would leave the
/// obligation naming a function the module does not carry, or a different one.
pub mod merged_name {
    /// The body that satisfies an import of `export_field` from
    /// `logical_module` — the closure root, and the only merged function an
    /// Inference declaration names.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn root(logical_module: &str, export_field: &str) -> String {
        format!("{logical_module}{}{export_field}", super::METHOD_SEPARATOR)
    }

    /// A function reachable from a merged root, keeping the debug name its own
    /// module gave it.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn callee(logical_module: &str, source_name: &str) -> String {
        format!("{logical_module}{}{source_name}", super::METHOD_SEPARATOR)
    }

    /// A merged function whose source module carried no name section, named
    /// from its deterministic output index so the artifact stays reproducible.
    #[must_use = "returns the merged body's name-section symbol"]
    pub fn anonymous(logical_module: &str, out_func_idx: u32) -> String {
        format!(
            "{logical_module}{}func_{out_func_idx}",
            super::METHOD_SEPARATOR
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
/// diagnostic messages, `.wat` output, panic descriptions, and the
/// codegen↔analysis frame-size interchange map.
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
    pub fn spec_free_folded(
        module_path: &[String],
        spec: &str,
        name: impl Into<String>,
    ) -> Self {
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
                qualify_dotted(module_path, &format!("{struct_name}{METHOD_SEPARATOR}{name}"))
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
        format!("{}{METHOD_SEPARATOR}{rest}", module_path.join(METHOD_SEPARATOR))
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
        assert_eq!(FnKey::method_in(vec![], "Point", "new").to_string(), "Point.new");
        assert_eq!(FnKey::spec_free("MySpec", "f").to_string(), "MySpec.f");
        assert_eq!(
            FnKey::spec_method("MySpec", "Point", "new").to_string(),
            "MySpec.Point.new"
        );
    }

    #[test]
    fn display_non_entry_file_is_dot_qualified() {
        let p = path(&["lib", "geometry"]);
        assert_eq!(FnKey::free_in(p.clone(), "add").to_string(), "lib.geometry.add");
        assert_eq!(
            FnKey::method_in(p, "Point", "new").to_string(),
            "lib.geometry.Point.new"
        );
    }

    #[test]
    fn fold_spec_name_underscore_joins_non_entry_file() {
        assert_eq!(fold_spec_name(&[], "S"), "S");
        assert_eq!(fold_spec_name(&path(&["lib", "checks"]), "S"), "lib_checks_S");
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
