//! Comprehensive multi-file type-checking matrix for the file-based module
//! hierarchy (issue #63).
//!
//! Where `multi_file.rs` pins the load-bearing smoke behaviors, this file crosses
//! the full grid: item kinds (fn / struct / enum / const
//! / type alias / method) × import forms (absolute path, file import, item import,
//! `pub use` namespace re-export, transitive re-export chains) × visibility (pub /
//! private), positive and negative, plus collisions, value cycles, specs, and the
//! dual-location private-access diagnostics.
//!
//! Every test drives [`crate::utils::try_type_check_multi_file`] over
//! `(module_path, source)` pairs (entry first, empty path = entry). Sources are
//! compact inline `.inf`; no filesystem access occurs.
//!
//! ## Behaviors pinned here that are surprising or limiting (probed from the
//! current implementation; asserted as the actual behavior):
//!
//! - **A `::`-qualified type resolves to its canonical identity in type
//!   position.** A cross-file type can be named directly by its qualified path
//!   (`let x: a::b::T`, a parameter, or a return type) — not only after an item
//!   import brings its bare name into scope. The qualified annotation resolves to
//!   the same nominal identity the value carries (see the
//!   `qualified_*_annotation_*` tests).
//! - **Type aliases are nominal, not transparent, even single-file** (`expected
//!   Id, found i32`). A pre-existing type-checker trait, not a multi-file
//!   regression. Pinned by [`type_alias_is_nominal_not_transparent`].
//! - **`pub use` of *items* does not surface them through the re-exporting file**;
//!   only `pub use` of a *namespace* (whole file) is traversable. Pinned by
//!   [`pub_use_of_item_is_not_surfaced_through_reexporter`].
//!
//! ## Cross-file semantics (#63):
//!
//! - **An item-imported type works in a function signature (param/return)
//!   position**, the same as in a `let` binding. Pinned by
//!   [`imported_struct_usable_in_signature_position`].
//! - **`const` is an importable item** and is reachable by a qualified `a::b::C`
//!   path; private consts are rejected at the boundary. Pinned by
//!   [`const_is_an_importable_item`], [`const_reachable_via_namespace_qualified_path`],
//!   and their private twins.
//! - **A private associated method is *not* callable cross-file**, and a private
//!   type-alias item import is rejected like any other item kind. Pinned by
//!   [`private_associated_method_rejected_cross_file`] and
//!   [`private_type_alias_item_import_rejected`].
//! - **Instance methods resolve on a cross-file imported struct** (and a private
//!   one is rejected with a visibility error). Pinned by
//!   [`instance_method_resolves_on_imported_struct`] and
//!   [`private_instance_method_rejected_on_imported_struct`].
#[cfg(test)]
mod tests {
    use crate::utils::try_type_check_multi_file;

    /// Convenience: assert the multi-file program type-checks cleanly.
    fn assert_ok(files: &[(Vec<&str>, &str)]) {
        let r = try_type_check_multi_file(files);
        assert!(
            r.is_ok(),
            "expected type-check success, got: {:?}",
            r.err().map(|e| e.to_string())
        );
    }

    /// Convenience: assert the program fails and return the aggregated message.
    fn assert_err(files: &[(Vec<&str>, &str)]) -> String {
        try_type_check_multi_file(files)
            .err()
            .expect("expected type-check failure")
            .to_string()
    }

    // ---------------------------------------------------------------------
    // Axis 1 — absolute cross-file paths × item visibility. The absolute
    // `a::b::fn` spelling is the long form of a namespace the accessing file
    // imported: a file (the entry included) reaches another file's surface only
    // through its own `use`, so each absolute path is paired with the `use` that
    // licenses it. A `pub` fn is then reachable by the path; a private one is
    // rejected with the dual-location diagnostic.
    // ---------------------------------------------------------------------

    #[test]
    fn absolute_path_pub_fn_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::arith; pub fn main() -> i32 { return lib::arith::add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn absolute_path_private_fn_rejected_dual_location() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::arith; pub fn main() -> i32 { return lib::arith::secret(); }",
            ),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("cannot access private function `lib::arith::secret`"),
            "use-site names the file-qualified private function, got: {msg}"
        );
        assert!(
            msg.contains("note: function `lib::arith::secret` is defined at"),
            "note points at the definition, got: {msg}"
        );
        assert!(
            msg.contains("in file `lib::arith`; add `pub` to export it"),
            "note names the defining file and the fix, got: {msg}"
        );
    }

    #[test]
    fn absolute_path_deep_namespace_three_dirs() {
        assert_ok(&[
            (
                vec![],
                "use a::b::c; pub fn main() -> i32 { return a::b::c::add(1, 2); }",
            ),
            (vec!["a", "b", "c"], "pub fn add(x: i32, y: i32) -> i32 { return x + y; }"),
        ]);
    }

    #[test]
    fn absolute_call_path_from_non_importing_file_rejected() {
        // A non-entry file reaching another file's surface by an absolute
        // `dir::file::fn` path it never imported is an encapsulation leak: there is
        // no ambient cross-file visibility (the leaf-alias `geom::val()` is already
        // rejected, and the long spelling is not an exception). The diagnostic
        // names the unimported namespace and points at the `use` to add.
        let msg = assert_err(&[
            (vec![], "use lib::geom; use helper; pub fn main() -> i32 { return helper::go(); }"),
            (vec!["lib", "geom"], "pub fn val() -> i32 { return 7; }"),
            (vec!["helper"], "pub fn go() -> i32 { return lib::geom::val(); }"),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;"),
            "the leak names the unimported namespace and the fix, got: {msg}"
        );
    }

    #[test]
    fn absolute_type_path_from_non_importing_file_rejected() {
        // The same discipline on the type path: a non-entry file using a
        // `::`-qualified cross-file type it never imported is rejected.
        let msg = assert_err(&[
            (vec![], "use lib::geom; use helper; pub fn main() -> i32 { return helper::go(); }"),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
            (
                vec!["helper"],
                "pub fn go() -> i32 { let p: lib::geom::Point = lib::geom::Point { x: 5, y: 6 }; return p.x; }",
            ),
        ]);
        assert!(
            msg.contains("lib::geom::Point"),
            "the leaked type path is rejected, got: {msg}"
        );
    }

    #[test]
    fn absolute_path_from_importing_non_entry_file_resolves() {
        // The complete fix licenses the long spelling for a file that DID import
        // the namespace: `lib/helper.inf` writes `use lib::geom;` and may then
        // spell the deep `lib::geom::val()` it already holds.
        assert_ok(&[
            (vec![], "use helper; pub fn main() -> i32 { return helper::go(); }"),
            (vec!["lib", "geom"], "pub fn val() -> i32 { return 7; }"),
            (vec!["helper"], "use lib::geom; pub fn go() -> i32 { return lib::geom::val(); }"),
        ]);
    }

    #[test]
    fn absolute_path_from_entry_file_resolves() {
        // The entry file imports the namespace and may then spell its absolute
        // path, exactly as a non-entry file does (this is the entry's own imported
        // path, which the gate must never over-reject).
        assert_ok(&[
            (vec![], "use lib::geom; pub fn main() -> i32 { return lib::geom::val(); }"),
            (vec!["lib", "geom"], "pub fn val() -> i32 { return 7; }"),
        ]);
    }

    #[test]
    fn entry_absolute_path_to_namespace_imported_only_by_another_file_rejected() {
        // The entry is held to the same import discipline as every other file: it
        // may absolute-spell `lib::secret::val()` only if the *entry itself*
        // imported a covering namespace. Here the entry imports `lib::a`, and only
        // `lib/a.inf` imports `lib::secret` — so `lib::secret` is in the closure but
        // is not the entry's to spell. Without this, the entry could borrow a
        // namespace some other file (even via a private `use`) dragged in, defeating
        // the rule that a file's `use` list is its complete dependency manifest.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a; pub fn entry() -> i32 { return lib::secret::val(); }",
            ),
            (vec!["lib", "a"], "use lib::secret;"),
            (vec!["lib", "secret"], "pub fn val() -> i32 { return 99; }"),
        ]);
        assert!(
            msg.contains("namespace `lib::secret` is not imported")
                && msg.contains("use lib::secret;"),
            "the entry's unlicensed absolute path names the unimported namespace and the fix, got: {msg}"
        );
    }

    #[test]
    fn entry_absolute_path_to_self_imported_namespace_resolves() {
        // The companion to the rejection: when the *entry* writes the `use`, its
        // own absolute path resolves — the import licenses the long spelling.
        assert_ok(&[
            (
                vec![],
                "use lib::secret; pub fn entry() -> i32 { return lib::secret::val(); }",
            ),
            (vec!["lib", "secret"], "pub fn val() -> i32 { return 99; }"),
        ]);
    }

    #[test]
    fn entry_absolute_path_to_namespace_imported_only_privately_by_another_file_rejected() {
        // The discipline holds even when the other file's import is private (a
        // plain `use`, not `pub use`): a private import never re-exports the
        // namespace to the entry, so the entry still needs its own `use`.
        let msg = assert_err(&[
            (
                vec![],
                "use helper; pub fn entry() -> i32 { return lib::secret::val() + helper::go(); }",
            ),
            (vec!["helper"], "use lib::secret; pub fn go() -> i32 { return 0; }"),
            (vec!["lib", "secret"], "pub fn val() -> i32 { return 99; }"),
        ]);
        assert!(
            msg.contains("namespace `lib::secret` is not imported")
                && msg.contains("use lib::secret;"),
            "another file's private import does not license the entry's absolute path, got: {msg}"
        );
    }

    #[test]
    fn entry_qualified_path_to_own_root_spec_still_resolves() {
        // Removing the entry's blanket root-anchor must not break the entry
        // reaching its *own* root-scope definitions by qualified path. An entry
        // `spec` is a non-directory root child, not another file's surface, so
        // `Check::verify_inner()` still resolves (and is rejected as proof-only, not
        // as an unknown method) without any `use`.
        let msg = assert_err(&[(
            vec![],
            "spec Check { fn verify_inner() -> i32 { return 42; } } \
             pub fn run() -> i32 { return Check::verify_inner(); }",
        )]);
        assert!(
            msg.contains("cannot call spec function `Check::verify_inner`")
                && msg.contains("proof-only"),
            "an entry's own root-scope spec resolves by qualified path (no use needed), got: {msg}"
        );
    }

    #[test]
    fn entry_item_not_bare_visible_but_reachable_via_use_root() {
        // A non-entry file does NOT see an entry item by bare name: there is no
        // ambient cross-file visibility. The entry's `pub` items are reached only
        // through the reserved `use root;` handle, as `root::item`.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::helper; pub fn entry_fn() -> i32 { return 1; } pub fn main() {}",
            ),
            (vec!["lib", "helper"], "pub fn run() -> i32 { return entry_fn(); }"),
        ]);
        assert!(
            msg.contains("call to undefined function `entry_fn`"),
            "an entry item is not bare-visible from a non-entry file, got: {msg}"
        );

        assert_ok(&[
            (
                vec![],
                "use lib::helper; pub fn entry_fn() -> i32 { return 1; } pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "use root; pub fn run() -> i32 { return root::entry_fn(); }",
            ),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 2 — file import (`use a::b;`) binds the namespace `b`, reached with
    // `::`. Item kinds: fn (callable), const (use-site), struct/enum (as `let`
    // type via a cross-file constructor).
    // ---------------------------------------------------------------------

    #[test]
    fn file_import_namespace_call_pub_fn() {
        assert_ok(&[
            (vec![], "use lib::arith; pub fn main() -> i32 { return lib::arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn file_import_namespace_call_private_fn_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::arith; pub fn main() -> i32 { return lib::arith::secret(); }"),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("cannot access private function `lib::arith::secret`"),
            "private fn rejected through file-import namespace, got: {msg}"
        );
    }

    // The two-segment namespace call (`use util; util::helper()`) is the
    // shortest cross-file call shape: a single-directory file bound as a
    // namespace and reached with one qualifier. It must not be confused with the
    // two-segment `Enum::Variant` or `Type::assoc_fn()` forms, which keep their
    // own resolution.

    #[test]
    fn two_segment_namespace_call_pub_fn() {
        assert_ok(&[
            (vec![], "use util; pub fn main() -> i32 { return util::helper(); }"),
            (vec!["util"], "pub fn helper() -> i32 { return 7; }"),
        ]);
    }

    #[test]
    fn two_segment_namespace_call_private_fn_rejected() {
        let msg = assert_err(&[
            (vec![], "use util; pub fn main() -> i32 { return util::secret(); }"),
            (vec!["util"], "fn secret() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("cannot access private function `util::secret`"),
            "private fn rejected through two-segment namespace call, got: {msg}"
        );
    }

    #[test]
    fn two_segment_namespace_binding_from_nested_path() {
        // `use a::b;` binds the last segment `b` as the namespace, even though the
        // file lives in a subdirectory; the call is still two segments (`b::fn`).
        assert_ok(&[
            (vec![], "use lib::arith; pub fn main() -> i32 { return arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn two_segment_namespace_call_missing_fn_rejected() {
        // The head is a bound namespace but the final segment names nothing in it;
        // this is the call's error, not a silent fall-through to enum/method code.
        let msg = assert_err(&[
            (vec![], "use util; pub fn main() -> i32 { return util::nope(); }"),
            (vec!["util"], "pub fn helper() -> i32 { return 7; }"),
        ]);
        assert!(
            msg.contains("util::nope"),
            "missing namespace fn names the bad path, got: {msg}"
        );
    }

    #[test]
    fn two_segment_enum_variant_still_resolves_when_head_is_not_a_namespace() {
        // `Color::Red` is also a two-segment `A::b`, but `Color` is a local enum,
        // not a bound namespace, so it keeps variant resolution and is unaffected
        // by the namespace-call path.
        assert_ok(&[(
            vec![],
            "enum Color { Red, Green } pub fn main() -> i32 { let c: Color = Color::Red; return 0; }",
        )]);
    }

    #[test]
    fn two_segment_assoc_fn_still_resolves_when_head_is_a_type() {
        // `Foo::make()` is a two-segment `Type::assoc_fn()`; `Foo` is a local
        // type, not a namespace, so associated-function resolution is unaffected.
        assert_ok(&[(
            vec![],
            "struct Foo { x: i32; fn make() -> i32 { return 0; } } pub fn main() -> i32 { return Foo::make(); }",
        )]);
    }

    #[test]
    fn cross_file_const_consumed_inside_its_own_file() {
        // A top-level const is resolved at its use site within its defining file;
        // a public fn there returns it, and that fn is callable cross-file once the
        // entry imports the namespace.
        assert_ok(&[
            (
                vec![],
                "use lib::vals; pub fn main() -> i32 { return lib::vals::get_max(); }",
            ),
            (
                vec!["lib", "vals"],
                "pub const MAX: i32 = 10; pub fn get_max() -> i32 { return MAX; }",
            ),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 3 — item import (`use a::b::{x}`) × item kind × visibility.
    // The import itself must resolve (exist + pub) for fn/struct/enum.
    // ---------------------------------------------------------------------

    #[test]
    fn item_import_pub_fn_bare_call() {
        assert_ok(&[
            (vec![], "use lib::arith::{add}; pub fn main() -> i32 { return add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn item_import_pub_struct_literal_and_field() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn item_import_pub_enum_construct_via_constructor() {
        // `match` is unimplemented in the parser, so the variant is constructed
        // and the value bound; the enum type is usable as a `let` type after the
        // item import, fed by a cross-file constructor fn returning the enum. The
        // item import binds `Color`; the absolute call `lib::col::first()` needs
        // the namespace import too, so the entry imports both.
        assert_ok(&[
            (
                vec![],
                "use lib::col; use lib::col::{Color}; pub fn main() -> i32 { let c: Color = lib::col::first(); return 0; }",
            ),
            (
                vec!["lib", "col"],
                "pub enum Color { Red, Green, Blue } pub fn first() -> Color { return Color::Red; }",
            ),
        ]);
    }

    #[test]
    fn item_import_pub_enum_construct_variant_directly() {
        // The imported enum's variant is constructible by bare name in a `let`.
        assert_ok(&[
            (
                vec![],
                "use lib::col::{Color}; pub fn main() -> i32 { let c: Color = Color::Red; return 0; }",
            ),
            (vec!["lib", "col"], "pub enum Color { Red, Green, Blue }"),
        ]);
    }

    #[test]
    fn item_import_private_fn_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::arith::{secret}; pub fn main() {}"),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("item `secret` in file `lib::arith` is private"),
            "private fn item import rejected, got: {msg}"
        );
        assert!(
            msg.contains("note: `secret` is defined at") && msg.contains("add `pub` to export it"),
            "ImportedItemPrivate carries a dual-location note, got: {msg}"
        );
    }

    #[test]
    fn item_import_private_struct_rejected_dual_location() {
        let msg = assert_err(&[
            (vec![], "use lib::geo::{Point}; pub fn main() {}"),
            (vec!["lib", "geo"], "struct Point { x: i32; }"),
        ]);
        assert!(
            msg.contains("item `Point` in file `lib::geo` is private"),
            "private struct item import rejected, got: {msg}"
        );
        assert!(
            msg.contains("note: `Point` is defined at") && msg.contains("in file `lib::geo`"),
            "note points at the struct definition site and file, got: {msg}"
        );
    }

    #[test]
    fn item_import_private_enum_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::col::{Color}; pub fn main() {}"),
            (vec!["lib", "col"], "enum Color { Red, Green }"),
        ]);
        assert!(
            msg.contains("item `Color` in file `lib::col` is private"),
            "private enum item import rejected, got: {msg}"
        );
    }

    #[test]
    fn item_import_missing_item_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::arith::{nope}; pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
        assert!(
            msg.contains("item `nope` not found in file `lib::arith`"),
            "missing item names item and file, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 4 — `const` is an importable item × visibility. A `pub const` crosses
    // the file boundary both as a braced item import and as a qualified `::` path;
    // a private const is rejected at the boundary with a dual-location note.
    // ---------------------------------------------------------------------

    #[test]
    fn const_is_an_importable_item() {
        // A `pub const` is item-importable and usable bare in the importing file.
        assert_ok(&[
            (vec![], "use lib::vals::{MAX}; pub fn main() -> i32 { return MAX; }"),
            (vec!["lib", "vals"], "pub const MAX: i32 = 10;"),
        ]);
    }

    #[test]
    fn private_const_item_import_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::vals::{MAX}; pub fn main() -> i32 { return MAX; }"),
            (vec!["lib", "vals"], "const MAX: i32 = 10;"),
        ]);
        assert!(
            msg.contains("item `MAX` in file `lib::vals` is private"),
            "a private const item import is rejected, got: {msg}"
        );
        assert!(
            msg.contains("note: `MAX` is defined at") && msg.contains("add `pub` to export it"),
            "ImportedItemPrivate carries a dual-location note, got: {msg}"
        );
    }

    #[test]
    fn const_reachable_via_namespace_qualified_path() {
        // `lib::vals::MAX` resolves as a qualified path to the pub const, once the
        // entry imports the namespace that licenses the absolute spelling.
        assert_ok(&[
            (
                vec![],
                "use lib::vals; pub fn main() -> i32 { return lib::vals::MAX; }",
            ),
            (vec!["lib", "vals"], "pub const MAX: i32 = 10;"),
        ]);
    }

    #[test]
    fn private_const_qualified_path_rejected_dual_location() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::vals; pub fn main() -> i32 { return lib::vals::MAX; }",
            ),
            (vec!["lib", "vals"], "const MAX: i32 = 10;"),
        ]);
        assert!(
            msg.contains("cannot access private constant `lib::vals::MAX`"),
            "a private const is not reachable by a qualified path, got: {msg}"
        );
        assert!(
            msg.contains("note: constant `lib::vals::MAX` is defined at")
                && msg.contains("in file `lib::vals`; add `pub` to export it"),
            "the diagnostic names the definition site and the fix, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 5 — type alias imports. The import resolves; the alias is nominal.
    // ---------------------------------------------------------------------

    #[test]
    fn type_alias_pub_item_import_resolves() {
        // A `pub type` alias imported as an item resolves the import without error
        // even if unused; the alias name is bound in the importing file.
        assert_ok(&[
            (vec![], "use lib::ty::{Id}; pub fn main() {}"),
            (vec!["lib", "ty"], "pub type Id = i32;"),
        ]);
    }

    #[test]
    fn type_alias_is_nominal_not_transparent() {
        // Pre-existing single-file behavior: a `type Id = i32;` alias is treated
        // nominally, so `let x: Id = 5;` is a mismatch. Pinned here so the
        // multi-file matrix records that aliasing is not transparency (the
        // failure below is NOT a multi-file regression).
        let msg = assert_err(&[(
            vec![],
            "type Id = i32; pub fn main() -> i32 { let x: Id = 5; return 0; }",
        )]);
        assert!(
            msg.contains("type mismatch") && msg.contains("expected `Id`, found `i32`"),
            "an alias is nominal, not transparent, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 6 — structs: nested types, and methods cross-file.
    // ---------------------------------------------------------------------

    #[test]
    fn nested_struct_imported_field_type() {
        // A local struct has fields of an imported struct type; literal
        // construction and nested field access both type-check.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
                 struct Line { a: Point; b: Point; } \
                 pub fn main() -> i32 { \
                     let l: Line = Line { a: Point { x: 0, y: 0 }, b: Point { x: 1, y: 1 } }; \
                     return l.a.x; \
                 }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn nested_struct_typed_field_access_when_entry_imports_only_outer() {
        // The entry imports `Outer` but NOT the inner `Mid` that `Outer` nests.
        // Reading `o.mid.a` must type-check: `Mid` is `pub` and reached through the
        // accessible `Outer`, so the field's type resolves through `Outer`'s
        // defining file even though the entry cannot name `Mid` by itself. A
        // field-type resolution against the *access site* would reject this as
        // "member access requires a struct type, found Mid" (Rule 4) (#63).
        assert_ok(&[
            (
                vec![],
                "use lib::a::{Outer}; use lib::a; \
                 pub fn main() -> i32 { let o: Outer = a::make(); return o.mid.a + o.mid.b; }",
            ),
            (
                vec!["lib", "a"],
                "use lib::b::{Mid}; \
                 pub struct Outer { head: i32; mid: Mid; tail: i32; } \
                 pub fn make() -> Outer { return Outer { head: 1, mid: Mid { a: 2, b: 3 }, tail: 4 }; }",
            ),
            (vec!["lib", "b"], "pub struct Mid { a: i32; b: i32; }"),
        ]);
    }

    #[test]
    fn pub_associated_method_callable_cross_file() {
        // `Type::assoc()` of a public associated method on an imported struct.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point::make(); return 0; }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; pub fn make() -> Point { return Point { x: 0 }; } }",
            ),
        ]);
    }

    #[test]
    fn private_associated_method_callable_within_own_file() {
        // Control: a private associated method is freely callable inside its own
        // file (`Point::secret()` from a sibling pub fn in the same struct).
        assert_ok(&[
            (vec![], "use lib::geo; pub fn main() {}"),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; \
                 fn secret() -> Point { return Point { x: 0 }; } \
                 pub fn use_secret() -> Point { return Point::secret(); } }",
            ),
        ]);
    }

    #[test]
    fn private_associated_method_rejected_cross_file() {
        // A *private* associated method is not callable across the file boundary:
        // a method enforces its own visibility even on a `pub` struct, so
        // `Point::secret()` from another file is a PrivateAccessViolation with a
        // dual-location note.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point::secret(); return 0; }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; \
                 pub fn pub_make() -> Point { return Point { x: 0 }; } \
                 fn secret() -> Point { return Point { x: 0 }; } }",
            ),
        ]);
        assert!(
            msg.contains("cannot access private method `secret` on type `Point`"),
            "a private associated method is rejected cross-file, got: {msg}"
        );
        assert!(
            msg.contains("note: method `secret` on type `Point` is defined at")
                && msg.contains("in file `lib::geo`; add `pub` to export it"),
            "the diagnostic names the definition site and the fix, got: {msg}"
        );
    }

    #[test]
    fn pub_associated_method_with_private_sibling_callable_cross_file() {
        // Control: the *public* associated method on the same struct is callable
        // cross-file even though the struct also has a private associated method —
        // visibility is per-method, not per-struct.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point::pub_make(); return 0; }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; \
                 pub fn pub_make() -> Point { return Point { x: 0 }; } \
                 fn secret() -> Point { return Point { x: 0 }; } }",
            ),
        ]);
    }

    #[test]
    fn instance_method_resolves_on_imported_struct() {
        // A pub instance method (`self` receiver) on an imported struct resolves
        // through the struct's defining scope and is callable cross-file.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point { x: 3, y: 4 }; return p.get_x(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; pub fn get_x(self) -> i32 { return self.x; } }",
            ),
        ]);
    }

    #[test]
    fn private_instance_method_rejected_on_imported_struct() {
        // A *private* instance method on an imported struct resolves but is
        // rejected at the file boundary with a visibility error — the same
        // per-method rule the associated-method test enforces.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point { x: 3, y: 4 }; return p.get_x(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; fn get_x(self) -> i32 { return self.x; } }",
            ),
        ]);
        assert!(
            msg.contains("cannot access private method `get_x` on type `Point`"),
            "a private instance method is rejected on an imported struct, got: {msg}"
        );
    }

    #[test]
    fn instance_method_resolves_same_file() {
        // Control for the cross-file case above: the instance method resolves
        // fine when the struct and the call site are in the same file.
        assert_ok(&[(
            vec![],
            "struct Point { x: i32; fn get_x(self) -> i32 { return self.x; } } \
             pub fn main() -> i32 { let p: Point = Point { x: 3 }; return p.get_x(); }",
        )]);
    }

    #[test]
    fn method_resolves_on_receiver_canonical_identity_not_call_site_name() {
        // The receiver `o.inner` carries the canonical identity `lib::geo::Inner`,
        // whose `get()` exists. The entry defines its own same-named `Inner` (also
        // with `get`), but resolution must follow the receiver's canonical struct,
        // not the bare name at the call site. The program type-checks because the
        // *receiver's* struct genuinely has the method.
        assert_ok(&[
            (
                vec![],
                "struct Inner { a: i32; pub fn get(self) -> i32 { return self.a; } } \
                 use lib::geo::{Outer, build}; \
                 pub fn main() -> i32 { let o: Outer = build(); return o.inner.get(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Inner { v: i32; pub fn get(self) -> i32 { return self.v; } } \
                 pub struct Outer { inner: Inner; } \
                 pub fn build() -> Outer { return Outer { inner: Inner { v: 1 } }; }",
            ),
        ]);
    }

    #[test]
    fn method_missing_on_receiver_canonical_struct_rejected_despite_same_named_local() {
        // The receiver `o.inner` is a `lib::geo::Inner` that has *no* `get` method.
        // The entry defines a same-named `Inner` that *does* have `get`. Dispatch by
        // bare name would silently hijack the entry's `get` and mis-compile; the
        // type checker must instead reject the call because the receiver's canonical
        // struct lacks the method.
        let msg = assert_err(&[
            (
                vec![],
                "struct Inner { a: i32; pub fn get(self) -> i32 { return self.a; } } \
                 use lib::geo::{Outer, build}; \
                 pub fn main() -> i32 { let o: Outer = build(); return o.inner.get(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Inner { v: i32; } \
                 pub struct Outer { inner: Inner; } \
                 pub fn build() -> Outer { return Outer { inner: Inner { v: 1 } }; }",
            ),
        ]);
        assert!(
            msg.contains("method `get` not found on type `Inner`"),
            "a method absent on the receiver's canonical struct must be rejected even \
             when a same-named local struct has it, got: {msg}"
        );
    }

    #[test]
    fn method_on_imported_fn_return_value_resolves_by_canonical_identity() {
        // The receiver is an imported function's return value (`pt()`), not a let
        // binding. Its canonical type is `lib::geo::Point`; `.sum()` resolves on
        // that struct even though the entry defines a same-named `Point` without
        // `sum`. Resolving by the call-site bare name would find the entry's
        // method-less `Point` and wrongly reject — or, worse, hijack a same-named
        // method. The receiver's canonical identity drives resolution.
        assert_ok(&[
            (
                vec![],
                "struct Point { a: i32; } \
                 use lib::geo::{pt}; \
                 pub fn run() -> i32 { return pt().sum(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; pub fn sum(self) -> i32 { return self.x; } } \
                 pub fn pt() -> Point { return Point { x: 5 }; }",
            ),
        ]);
    }

    #[test]
    fn same_named_method_different_arity_checks_against_receiver_canonical_signature() {
        // Both files define `Inner::get`, but with DIFFERENT arities: the entry's
        // takes an extra `i32`, the canonical `lib::geo::Inner`'s takes none. The
        // call `o.inner.get()` passes zero user args. If dispatch resolved by the
        // call-site bare name, the arg-count check would fire against the entry's
        // one-extra-arg signature ("expected 1, found 0"). It type-checks because
        // the fix dispatches to the receiver's canonical `lib::geo::Inner::get`
        // first, and the arg-count check then validates against THAT signature.
        // This pins that the fix — not the arity check — selects the body; the
        // arity check merely validates the already-correct signature.
        assert_ok(&[
            (
                vec![],
                "struct Inner { a: i32; pub fn get(self, extra: i32) -> i32 { return self.a + extra; } } \
                 use lib::geo::{Outer, build}; \
                 pub fn main() -> i32 { let o: Outer = build(); return o.inner.get(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Inner { v: i32; pub fn get(self) -> i32 { return self.v; } } \
                 pub struct Outer { inner: Inner; } \
                 pub fn build() -> Outer { return Outer { inner: Inner { v: 5 } }; }",
            ),
        ]);
    }

    #[test]
    fn same_named_method_different_arity_rejects_wrong_arg_count_for_canonical_sig() {
        // The negative twin: the same divergent-arity setup, but the call now
        // passes ONE arg. The canonical `lib::geo::Inner::get` takes zero, so the
        // arg-count check must reject against the canonical signature — proving the
        // arity is validated on the body the fix selected, not the entry's
        // one-arg `get` (which would have wrongly accepted the call).
        let msg = assert_err(&[
            (
                vec![],
                "struct Inner { a: i32; pub fn get(self, extra: i32) -> i32 { return self.a + extra; } } \
                 use lib::geo::{Outer, build}; \
                 pub fn main() -> i32 { let o: Outer = build(); return o.inner.get(7); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Inner { v: i32; pub fn get(self) -> i32 { return self.v; } } \
                 pub struct Outer { inner: Inner; } \
                 pub fn build() -> Outer { return Outer { inner: Inner { v: 5 } }; }",
            ),
        ]);
        assert!(
            msg.contains("Inner::get") && msg.contains("expects 0 arguments"),
            "the call must be checked against the canonical zero-arg `get`, got: {msg}"
        );
    }

    #[test]
    fn method_argument_of_cross_file_struct_type_checks() {
        // A method whose ARGUMENT is itself a cross-file struct type, with the
        // receiver also a cross-file struct. `Sink::take` (on the imported `Sink`)
        // accepts a `lib::geo::Item`; the entry item-imports both `Sink` and `Item`,
        // constructs the item via an imported constructor, and passes it. Both the
        // receiver dispatch (`s.take`) and the argument typing must resolve through
        // canonical identity for the call to check. (No same-named entry `Item` is
        // introduced: an entry-local same-named struct would item-import-collide and
        // trip the pre-existing `Custom`-vs-`Struct` nominal-equality quirk, which is
        // a separate limitation, not part of the dispatch fix.)
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Sink, Item, make_item, make_sink}; \
                 pub fn run() -> i32 { let s: Sink = make_sink(); let it: Item = make_item(); return s.take(it); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Item { v: i32; } \
                 pub struct Sink { acc: i32; pub fn take(self, it: Item) -> i32 { return it.v; } } \
                 pub fn make_item() -> Item { return Item { v: 9 }; } \
                 pub fn make_sink() -> Sink { return Sink { acc: 0 }; }",
            ),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 7 — imported types in signature positions (limitation).
    // ---------------------------------------------------------------------

    #[test]
    fn imported_struct_usable_as_let_type() {
        // The supported shape: an imported struct as a `let` binding type.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn imported_struct_usable_in_signature_position() {
        // An item-imported type is recognized in a function signature (return or
        // param) position, the same as in a `let` binding: signature types are
        // validated after imports resolve.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; pub fn main() -> Point { return Point { x: 1, y: 2 }; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn imported_struct_usable_as_param_type() {
        // The param-position twin of the return-position test above: an
        // item-imported type is recognized as a parameter type and its fields are
        // accessible through the parameter.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
                 pub fn read_x(p: Point) -> i32 { return p.x; } \
                 pub fn main() {}",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn unknown_type_in_signature_still_rejected() {
        // The validation pass still fires for a genuinely unknown type — moving it
        // after import resolution must not silence real errors.
        let msg = assert_err(&[(
            vec![],
            "pub fn main() -> Nonexistent { return 0; }",
        )]);
        assert!(
            msg.contains("unknown type `Nonexistent`"),
            "an unresolved signature type is still rejected, got: {msg}"
        );
    }

    #[test]
    fn single_file_struct_usable_as_return_type_control() {
        // Control: a same-file struct IS usable as a return type, so the gap above
        // is specific to imported (cross-file) types.
        assert_ok(&[(
            vec![],
            "struct Point { x: i32; } \
             pub fn make() -> Point { return Point { x: 1 }; } \
             pub fn main() -> i32 { let p: Point = make(); return p.x; }",
        )]);
    }

    // ---------------------------------------------------------------------
    // Axis 8 — `pub use` re-export: namespaces traverse, items do not.
    // ---------------------------------------------------------------------

    #[test]
    fn pub_use_namespace_one_hop_resolves() {
        assert_ok(&[
            (vec![], "use math; pub fn main() -> i32 { return math::arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "pub use lib::arith;"),
        ]);
    }

    #[test]
    fn pub_use_namespace_two_hop_chain_resolves() {
        // main -> m1 -> m2 -> lib, two chained re-exports, leaf fn at the end.
        assert_ok(&[
            (vec![], "use m1; pub fn main() -> i32 { return m1::m2::lib::add(1, 2); }"),
            (vec!["lib"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["m2"], "pub use lib;"),
            (vec!["m1"], "pub use m2;"),
        ]);
    }

    #[test]
    fn plain_use_blocks_traversal_at_last_hop() {
        // The re-exporting file uses a plain (non-pub) `use`, so the chain through
        // it does not resolve. The leaf `add` exists; only the plain `use` hides
        // it, so the diagnostic names the path and steers toward `pub use`.
        let msg = assert_err(&[
            (vec![], "use math; pub fn main() -> i32 { return math::arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "use lib::arith;"),
        ]);
        assert!(
            msg.contains("call to `math::arith::add` is blocked") && msg.contains("pub use"),
            "plain use must surface the re-export hint, got: {msg}"
        );
    }

    #[test]
    fn plain_use_blocks_traversal_at_first_hop() {
        // Break the 3-hop chain at the FIRST hop: m1 uses plain `use m2;`.
        let msg = assert_err(&[
            (vec![], "use m1; pub fn main() -> i32 { return m1::m2::lib::add(1, 2); }"),
            (vec!["lib"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["m2"], "pub use lib;"),
            (vec!["m1"], "use m2;"),
        ]);
        assert!(
            msg.contains("call to `m1::m2::lib::add` is blocked") && msg.contains("pub use"),
            "breaking the first hop surfaces the re-export hint, got: {msg}"
        );
    }

    #[test]
    fn plain_use_blocks_traversal_at_middle_hop() {
        // Break the 3-hop chain at the MIDDLE hop: m2 uses plain `use lib;`.
        let msg = assert_err(&[
            (vec![], "use m1; pub fn main() -> i32 { return m1::m2::lib::add(1, 2); }"),
            (vec!["lib"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["m2"], "use lib;"),
            (vec!["m1"], "pub use m2;"),
        ]);
        assert!(
            msg.contains("call to `m1::m2::lib::add` is blocked") && msg.contains("pub use"),
            "breaking the middle hop surfaces the re-export hint, got: {msg}"
        );
    }

    #[test]
    fn genuinely_undefined_namespace_function_stays_undefined() {
        // The companion to the re-export hint: when the leaf truly does not exist
        // in the reachable namespace, the diagnostic must remain the plain
        // "undefined function" rather than wrongly suggesting a missing `pub use`.
        let msg = assert_err(&[
            (vec![], "use math; pub fn main() -> i32 { return math::nope(1, 2); }"),
            (vec!["math"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
        assert!(
            msg.contains("call to undefined function `math::nope`") && !msg.contains("pub use"),
            "a genuinely absent leaf must stay an undefined-function error, got: {msg}"
        );
    }

    #[test]
    fn reexport_blocked_non_function_leaf_stays_undefined() {
        // The leaf reached gate-free is a STRUCT, not a function, so calling it is
        // nonsense regardless of re-export. The "add `pub use`" hint must NOT fire;
        // the call falls back to the plain undefined-function diagnostic.
        let msg = assert_err(&[
            (vec![], "use math; pub fn main() -> i32 { return math::Thing(1, 2); }"),
            (vec!["lib", "geo"], "pub struct Thing { x: i32; }"),
            (vec!["math"], "use lib::geo::{Thing};"),
        ]);
        assert!(
            msg.contains("`math::Thing`") && !msg.contains("pub use"),
            "a non-function leaf must not trigger the re-export hint, got: {msg}"
        );
    }

    #[test]
    fn pub_use_of_item_is_reimportable_through_reexporter() {
        // `pub use lib::arith::{add};` re-exports the ITEM, so an importer of
        // `math` reaches `add` BOTH ways, consistently: a bare item re-import
        // `use math::{add};` and a namespace-qualified call `math::add(...)`.
        // Item-import resolution runs to a fixpoint, so `math`'s own re-export
        // binding is available when `main`'s re-import resolves regardless of the
        // order scopes are visited (#63).
        assert_ok(&[
            (vec![], "use math::{add}; pub fn main() -> i32 { return add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "pub use lib::arith::{add};"),
        ]);
        assert_ok(&[
            (vec![], "use math; pub fn main() -> i32 { return math::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "pub use lib::arith::{add};"),
        ]);
    }

    #[test]
    fn plain_use_of_item_is_not_reimportable_through_reexporter() {
        // A PLAIN `use lib::arith::{add};` keeps `add` private to `math`, so a
        // re-import from `math` must fail — the consistent counterpart to
        // [`pub_use_of_item_is_reimportable_through_reexporter`].
        let msg = assert_err(&[
            (vec![], "use math::{add}; pub fn main() -> i32 { return add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "use lib::arith::{add};"),
        ]);
        assert!(
            msg.contains("item `add` not found in file `math`"),
            "a plain-use'd item is not re-importable from the re-exporter, got: {msg}"
        );
    }

    #[test]
    fn item_reexport_chain_three_hops_resolves() {
        // A 3-hop item re-export chain — `a` pub-uses `b` pub-uses `c` (which
        // defines `deep`) — resolves when the entry item-imports `deep` from `a`.
        // The fixpoint walks the chain to a stable point regardless of scope
        // visitation order (#63).
        assert_ok(&[
            (vec![], "use lib::a::{deep}; pub fn main() -> i32 { return deep(5); }"),
            (vec!["lib", "c"], "pub fn deep(a: i32) -> i32 { return a + 100; }"),
            (vec!["lib", "b"], "pub use lib::c::{deep};"),
            (vec!["lib", "a"], "pub use lib::b::{deep};"),
        ]);
    }

    #[test]
    fn item_reexport_chain_broken_by_plain_hop_rejected() {
        // The same chain with a PLAIN middle hop (`b` plain-uses `c`) does not
        // re-export `deep`, so `a`'s `pub use lib::b::{deep};` cannot find it and
        // the entry re-import is rejected.
        let msg = assert_err(&[
            (vec![], "use lib::a::{deep}; pub fn main() -> i32 { return deep(5); }"),
            (vec!["lib", "c"], "pub fn deep(a: i32) -> i32 { return a + 100; }"),
            (vec!["lib", "b"], "use lib::c::{deep};"),
            (vec!["lib", "a"], "pub use lib::b::{deep};"),
        ]);
        assert!(
            msg.contains("not found"),
            "a plain hop breaks the re-export chain, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 9 — same-named items across files.
    // ---------------------------------------------------------------------

    #[test]
    fn same_named_private_fns_each_used_in_own_file() {
        // Three files each define a private `helper` and call it from a sibling.
        // File scopes keep them distinct; no collision.
        assert_ok(&[
            (
                vec![],
                "use lib::a; use lib::b; fn helper() -> i32 { return 0; } pub fn main() -> i32 { return helper(); }",
            ),
            (vec!["lib", "a"], "fn helper() -> i32 { return 1; } pub fn run() -> i32 { return helper(); }"),
            (vec!["lib", "b"], "fn helper() -> i32 { return 2; } pub fn run() -> i32 { return helper(); }"),
        ]);
    }

    #[test]
    fn same_named_pub_fns_via_different_namespaces() {
        // Two files export `f`; both are file-imported and called by their
        // distinct qualified paths without ambiguity.
        assert_ok(&[
            (
                vec![],
                "use lib::a; use lib::b; pub fn main() -> i32 { return lib::a::f() + lib::b::f(); }",
            ),
            (vec!["lib", "a"], "pub fn f() -> i32 { return 1; }"),
            (vec!["lib", "b"], "pub fn f() -> i32 { return 2; }"),
        ]);
    }

    #[test]
    fn same_named_structs_constructed_in_each_file_distinct_keys() {
        // Each file constructs its own same-named struct; canonical keys are
        // distinct (bare for entry, file-qualified otherwise) and fetch distinct
        // layouts.
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "struct Buffer { x: i32; } pub fn main() -> i32 { let b: Buffer = Buffer { x: 0 }; return b.x; }",
            ),
            (
                vec!["lib", "buf"],
                "pub struct Buffer { y: i32; z: i32; } pub fn use_it() -> i32 { let b: Buffer = Buffer { y: 1, z: 2 }; return b.y; }",
            ),
        ])
        .expect("same-named structs in separate files should type-check");

        let entry_key = ctx.canonical_struct_key("Buffer", &[]).expect("entry Buffer");
        let lib_key = ctx
            .canonical_struct_key("Buffer", &["lib".to_string(), "buf".to_string()])
            .expect("lib::buf Buffer");
        assert_eq!(entry_key, "Buffer");
        assert_eq!(lib_key, "lib::buf::Buffer");
        assert_ne!(entry_key, lib_key);
        assert_eq!(ctx.lookup_struct(&entry_key).unwrap().fields.len(), 1);
        assert_eq!(ctx.lookup_struct(&lib_key).unwrap().fields.len(), 2);
    }

    #[test]
    fn same_named_enums_distinct_canonical_keys() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "enum Color { Red } pub fn main() -> i32 { let c: Color = Color::Red; return 0; }",
            ),
            (
                vec!["lib", "col"],
                "pub enum Color { A, B, C } pub fn use_it() -> i32 { let c: Color = Color::A; return 0; }",
            ),
        ])
        .expect("same-named enums in separate files should type-check");

        let entry_key = ctx.canonical_enum_key("Color", &[]).expect("entry Color");
        let lib_key = ctx
            .canonical_enum_key("Color", &["lib".to_string(), "col".to_string()])
            .expect("lib::col Color");
        assert_eq!(entry_key, "Color");
        assert_eq!(lib_key, "lib::col::Color");
        assert_ne!(entry_key, lib_key);
        assert_eq!(ctx.lookup_enum(&entry_key).unwrap().variants.len(), 1);
        assert_eq!(ctx.lookup_enum(&lib_key).unwrap().variants.len(), 3);
    }

    // ---------------------------------------------------------------------
    // Axis 10 — specs and visibility.
    // ---------------------------------------------------------------------

    #[test]
    fn spec_sees_own_file_private_fn() {
        assert_ok(&[
            (vec![], "use lib::api; pub fn main() {}"),
            (
                vec!["lib", "api"],
                "fn helper() -> i32 { return 1; } spec ApiSpec { fn check() -> i32 { return helper(); } }",
            ),
        ]);
    }

    #[test]
    fn spec_references_imported_pub_fn() {
        assert_ok(&[
            (
                vec![],
                "use lib::arith; spec S { fn check() -> i32 { return lib::arith::add(1, 2); } } pub fn main() {}",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn spec_references_imported_pub_struct() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; spec S { fn check() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; } } pub fn main() {}",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn spec_references_other_file_private_fn_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::arith; spec S { fn check() -> i32 { return lib::arith::secret(); } } pub fn main() {}",
            ),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("cannot access private function `lib::arith::secret`"),
            "a spec cannot reach another file's private fn, got: {msg}"
        );
    }

    #[test]
    fn spec_cannot_reach_other_file_private_struct() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::buf; spec EntrySpec { fn check() -> i32 { let b: Buffer = Buffer { x: 0 }; return b.x; } } pub fn main() {}",
            ),
            (vec!["lib", "buf"], "struct Buffer { x: i32; }"),
        ]);
        assert!(
            msg.contains("struct `Buffer` is not defined")
                || msg.contains("unknown type `Buffer`"),
            "a spec must not reach another file's private struct by bare name, got: {msg}"
        );
    }

    #[test]
    fn spec_helper_struct_does_not_collide_with_other_file_top_level() {
        // A spec-inner helper `struct Tmp` in `lib::a` and an unrelated top-level
        // `struct Tmp` in the entry file are distinct types in distinct files, so
        // both register cleanly. Spec types key by their enclosing file, not
        // project-globally, so this no longer over-rejects.
        assert_ok(&[
            (
                vec![],
                "use lib::a; struct Tmp { x: i32; } pub fn main() -> i32 { return a::add(1, 2); }",
            ),
            (
                vec!["lib", "a"],
                "spec S { struct Tmp { v: i32; } } pub fn add(p: i32, q: i32) -> i32 { return p + q; }",
            ),
        ]);
    }

    #[test]
    fn spec_helper_structs_in_two_files_do_not_collide() {
        // Two spec-inner helpers both named `Tmp`, one per file, key by their own
        // files and never conflict.
        assert_ok(&[
            (
                vec![],
                "use lib::a; spec S { struct Tmp { v: i32; } } pub fn main() -> i32 { return a::add(1, 2); }",
            ),
            (
                vec!["lib", "a"],
                "spec SA { struct Tmp { x: i32; } } pub fn add(p: i32, q: i32) -> i32 { return p + q; }",
            ),
        ]);
    }

    #[test]
    fn same_named_spec_cannot_reach_other_files_spec_private_fn() {
        // Two files each declare `spec Sp`. File `b`'s spec must NOT see file
        // `a`'s spec-private `priv_a`: spec scopes are keyed by their file, so
        // the two `Sp` scopes are distinct. A bare reference to `priv_a` from
        // b's spec is an undefined-name error, never a cross-file privacy leak
        // (and never a codegen panic in proof mode) (#63).
        let msg = assert_err(&[
            (vec![], "use lib::a; use lib::b; pub fn main() -> i32 { return b::run() + a::helper(); }"),
            (
                vec!["lib", "a"],
                "pub fn helper() -> i32 { return 0; } spec Sp { fn priv_a() -> i32 { return 1; } fn use_a() -> i32 { return priv_a(); } }",
            ),
            (
                vec!["lib", "b"],
                "pub fn run() -> i32 { return 2; } spec Sp { fn check() -> i32 { return priv_a(); } }",
            ),
        ]);
        assert!(
            msg.contains("undefined function `priv_a`"),
            "b's spec must not reach a's spec-private fn, got: {msg}"
        );
    }

    #[test]
    fn same_named_specs_with_same_named_inner_fns_compile() {
        // Two files each declare `spec Invariant` containing same-named inner fns
        // `check`/`driver`. File-qualified spec scope keys keep the two `Invariant`
        // scopes distinct, so the inner names never collide and each spec's inner
        // fn is callable only within its own file's spec (#63).
        assert_ok(&[
            (vec![], "use lib::a; use lib::b; pub fn main() -> i32 { return a::helper() + b::run(); }"),
            (
                vec!["lib", "a"],
                "pub fn helper() -> i32 { return 0; } spec Invariant { fn check() -> i32 { return 1; } fn driver() -> i32 { return check(); } }",
            ),
            (
                vec!["lib", "b"],
                "pub fn run() -> i32 { return 2; } spec Invariant { fn check() -> i32 { return 3; } fn driver() -> i32 { return check(); } }",
            ),
        ]);
    }

    #[test]
    fn spec_helper_struct_collides_within_same_file() {
        // The same-file collision must still be rejected: two specs in ONE file
        // each declaring `struct Tmp` would map to the same canonical key.
        let msg = assert_err(&[(
            vec![],
            "spec S1 { struct Tmp { v: i32; } } spec S2 { struct Tmp { w: i32; } } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering struct `Tmp`")
                && msg.contains("within a file's spec scopes"),
            "two same-named spec helpers in one file must still collide, got: {msg}"
        );
    }

    #[test]
    fn spec_helper_enum_does_not_collide_with_other_file_top_level() {
        // The enum twin of the struct case: a spec-inner helper `enum E` and an
        // unrelated top-level `enum E` in another file do not collide.
        assert_ok(&[
            (
                vec![],
                "use lib::a; enum E { A } pub fn main() -> i32 { return a::add(1, 2); }",
            ),
            (
                vec!["lib", "a"],
                "spec S { enum E { B } } pub fn add(p: i32, q: i32) -> i32 { return p + q; }",
            ),
        ]);
    }

    #[test]
    fn spec_helper_enum_collides_within_same_file() {
        // The same-file enum collision is rejected, exactly like the struct twin.
        let msg = assert_err(&[(
            vec![],
            "spec S1 { enum E { A } } spec S2 { enum E { B } } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering enum `E`")
                && msg.contains("within a file's spec scopes"),
            "two same-named spec helper enums in one file must still collide, got: {msg}"
        );
    }

    // A spec-inner struct/enum and a top-level one with the same bare name in the
    // same file map to one canonical key, so accepting both would let codegen index
    // one layout for two definitions. The collision is rejected regardless of which
    // is written first: top-level types register before the file's spec blocks, so
    // the spec definition is the one rejected either way. A same-named type in a
    // *different* file is keyed distinctly and never collides (the no-collision
    // control above).

    #[test]
    fn spec_struct_collides_with_top_level_struct_spec_first() {
        let msg = assert_err(&[(
            vec![],
            "spec S { struct Point { a: i32; } } pub struct Point { v: i32; } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering struct `Point`")
                && msg.contains("within a file's spec scopes"),
            "spec-before-top-level same-name struct must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_struct_collides_with_top_level_struct_top_level_first() {
        let msg = assert_err(&[(
            vec![],
            "pub struct Point { v: i32; } spec S { struct Point { a: i32; } } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering struct `Point`")
                && msg.contains("within a file's spec scopes"),
            "top-level-before-spec same-name struct must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_enum_collides_with_top_level_enum_spec_first() {
        let msg = assert_err(&[(
            vec![],
            "spec S { enum Color { Red } } pub enum Color { Blue } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering enum `Color`")
                && msg.contains("within a file's spec scopes"),
            "spec-before-top-level same-name enum must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_enum_collides_with_top_level_enum_top_level_first() {
        let msg = assert_err(&[(
            vec![],
            "pub enum Color { Blue } spec S { enum Color { Red } } pub fn main() {}",
        )]);
        assert!(
            msg.contains("error registering enum `Color`")
                && msg.contains("within a file's spec scopes"),
            "top-level-before-spec same-name enum must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_struct_collides_with_top_level_struct_cross_file_spec_first() {
        // The collision is per-file, so it is rejected the same way in a non-entry
        // file as in the entry file.
        let msg = assert_err(&[
            (vec![], "use lib::a; pub fn main() -> i32 { return lib::a::make(); }"),
            (
                vec!["lib", "a"],
                "spec MySpec { struct Point { a: i32; } } pub struct Point { v: i32; } \
                 pub fn make() -> i32 { let p: Point = Point { v: 41 }; return p.v; }",
            ),
        ]);
        assert!(
            msg.contains("error registering struct `Point`")
                && msg.contains("within a file's spec scopes"),
            "cross-file spec-before-top-level same-name struct must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_struct_collides_with_top_level_struct_cross_file_top_level_first() {
        let msg = assert_err(&[
            (vec![], "use lib::a; pub fn main() -> i32 { return lib::a::make(); }"),
            (
                vec!["lib", "a"],
                "pub struct Point { v: i32; } spec MySpec { struct Point { a: i32; } } \
                 pub fn make() -> i32 { let p: Point = Point { v: 41 }; return p.v; }",
            ),
        ]);
        assert!(
            msg.contains("error registering struct `Point`")
                && msg.contains("within a file's spec scopes"),
            "cross-file top-level-before-spec same-name struct must collide, got: {msg}"
        );
    }

    #[test]
    fn spec_helper_and_top_level_distinct_names_compile() {
        // The no-collision control: a spec helper whose name differs from the
        // top-level type registers cleanly and the program type-checks.
        assert_ok(&[(
            vec![],
            "spec S { struct Inner { a: i32; } } pub struct Point { v: i32; } \
             pub fn main() -> i32 { let p: Point = Point { v: 41 }; return p.v; }",
        )]);
    }

    // ---------------------------------------------------------------------
    // Axis — spec-inner / top-level function shadowing is a SAME-FILE relation.
    // A spec-inner fn shadows a top-level fn only when both are in the same file;
    // the colliding top-level name is resolved in the spec's own file scope, never
    // the entry file's. The diagnostic carries the file the collision lives in.
    // ---------------------------------------------------------------------

    #[test]
    fn spec_inner_fn_matching_entry_top_level_fn_in_another_file_ok() {
        // A spec in an imported file whose inner fn shares a name with an
        // entry-file top-level fn is NOT a shadow: they live in distinct files. The
        // shadow check is keyed to the spec's own file scope, so this must compile
        // rather than be wrongly rejected against the entry's top-level surface.
        assert_ok(&[
            (
                vec![],
                "use lib::checks; \
                 pub fn compute() -> i32 { return 1; } \
                 pub fn main() -> i32 { return compute(); }",
            ),
            (
                vec!["lib", "checks"],
                "spec S { fn compute() -> i32 { return 2; } \
                 fn check() -> i32 { return compute(); } }",
            ),
        ]);
    }

    #[test]
    fn same_file_top_level_and_spec_inner_shadow_in_imported_file_rejected() {
        // A genuine same-file collision inside a *non-entry* file: a top-level fn
        // and a spec-inner fn of the same name in `lib::checks`. The entry-scoped
        // check missed this because it only consulted the entry file's root; the
        // per-file check catches it, and the diagnostic carries the `lib::checks`
        // label so it is attributed to the file that owns the collision.
        let msg = assert_err(&[
            (vec![], "use lib::checks; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "checks"],
                "pub fn helper() -> i32 { return 1; } \
                 spec S { fn helper() -> i32 { return 2; } }",
            ),
        ]);
        assert!(
            msg.contains("function `helper` inside spec `S` shadows a top-level function"),
            "a same-file shadow in an imported file must be rejected, got: {msg}"
        );
        assert!(
            msg.contains("lib::checks:"),
            "the non-entry shadow diagnostic must carry its file label, got: {msg}"
        );
    }

    #[test]
    fn entry_file_same_file_shadow_still_rejected_and_bare() {
        // The entry-file case is unchanged: a same-file top-level + spec-inner
        // collision is still rejected, and its diagnostic stays bare (the entry
        // file's label is `None`, matching every other entry diagnostic).
        let msg = assert_err(&[(
            vec![],
            "pub fn helper() -> i32 { return 1; } \
             spec S { fn helper() -> i32 { return 2; } } \
             pub fn main() -> i32 { return 0; }",
        )]);
        assert!(
            msg.contains("function `helper` inside spec `S` shadows a top-level function"),
            "an entry-file same-file shadow must still be rejected, got: {msg}"
        );
        assert!(
            !msg.contains("::"),
            "the entry-file shadow diagnostic must stay bare (no file label), got: {msg}"
        );
    }

    // A spec function is proof-only: calling one through a qualified path
    // (`Check::fn`, `lib::Check::fn`, `lib::checks::Check::fn`) is rejected by the
    // type checker rather than type-checking and then panicking in codegen, which
    // assigns spec functions no executable index. Rejection is wholesale — any
    // caller, any path length — because the qualified form never lowers; only a
    // bare-name call from within the same spec is supported.

    #[test]
    fn spec_fn_called_from_executable_via_qualified_path_rejected() {
        let msg = assert_err(&[(
            vec![],
            "spec Check { fn verify_inner() -> i32 { return 42; } } \
             pub fn run() -> i32 { return Check::verify_inner(); }",
        )]);
        assert!(
            msg.contains("cannot call spec function `Check::verify_inner`")
                && msg.contains("proof-only"),
            "executable code must not call a spec fn by qualified path, got: {msg}"
        );
    }

    #[test]
    fn spec_fn_called_from_executable_cross_file_via_qualified_path_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::checks; \
                 pub fn run() -> i32 { return lib::checks::Check::verify_inner(); }",
            ),
            (
                vec!["lib", "checks"],
                "spec Check { fn verify_inner() -> i32 { return 42; } }",
            ),
        ]);
        assert!(
            msg.contains("cannot call spec function `lib::checks::Check::verify_inner`")
                && msg.contains("proof-only"),
            "executable code must not call a cross-file spec fn by qualified path, got: {msg}"
        );
    }

    #[test]
    fn spec_fn_called_from_executable_three_segment_path_rejected() {
        // The struct of the path (`lib::Check::verify_inner`) where the spec lives
        // directly in `lib.inf` — a three-segment path that the namespace-qualified
        // associated-call handler sees first; it must fall through (a spec is not a
        // struct) to the qualified-call rejection rather than mis-resolving.
        let msg = assert_err(&[
            (
                vec![],
                "use lib; pub fn run() -> i32 { return lib::Check::verify_inner(); }",
            ),
            (
                vec!["lib"],
                "spec Check { fn verify_inner() -> i32 { return 42; } }",
            ),
        ]);
        assert!(
            msg.contains("cannot call spec function `lib::Check::verify_inner`")
                && msg.contains("proof-only"),
            "executable code must not call a spec fn by a three-segment path, got: {msg}"
        );
    }

    #[test]
    fn spec_fn_called_from_another_spec_via_qualified_path_rejected() {
        // The qualified form is rejected regardless of caller — even a spec calling
        // a sibling spec fn must use the bare name, since `Check::inner()` has no
        // emittable callee in proof mode either.
        let msg = assert_err(&[(
            vec![],
            "spec Check { fn inner() -> i32 { return 42; } \
             fn outer() -> i32 { return Check::inner(); } } pub fn main() {}",
        )]);
        assert!(
            msg.contains("cannot call spec function `Check::inner`")
                && msg.contains("proof-only"),
            "a spec must not call a sibling spec fn by qualified path, got: {msg}"
        );
    }

    #[test]
    fn spec_fn_called_by_bare_name_from_sibling_spec_fn_ok() {
        // The supported intra-spec call form: a bare-name call to a sibling spec
        // function. This must keep type-checking — it is the path proof-mode codegen
        // relies on, resolved inside the spec scope.
        assert_ok(&[(
            vec![],
            "spec Check { fn inner() -> i32 { return 42; } \
             fn outer() -> i32 { return inner(); } } pub fn main() {}",
        )]);
    }

    #[test]
    fn spec_fn_called_from_executable_with_wrong_arg_count_rejected_as_spec() {
        // The spec rejection takes precedence over the arity check: a qualified
        // spec call with the wrong number of arguments is still rejected as a
        // proof-only-boundary violation (not `ArgumentCountMismatch`), because the
        // qualified form never lowers regardless of how it is called. The point is
        // that it rejects with a coherent diagnostic and never reaches codegen — a
        // mismatched-arity qualified spec call must not slip past into a panic.
        let msg = assert_err(&[(
            vec![],
            "spec Check { fn verify_inner(a: i32) -> i32 { return a; } } \
             pub fn run() -> i32 { return Check::verify_inner(); }",
        )]);
        assert!(
            msg.contains("cannot call spec function `Check::verify_inner`")
                && msg.contains("proof-only"),
            "wrong-arity qualified spec call must reject as a spec violation, got: {msg}"
        );
    }

    #[test]
    fn nonexistent_fn_under_spec_name_rejected_as_undefined() {
        // A qualified path whose head names a real spec but whose leaf does not
        // exist must fall through to the plain undefined-function diagnostic — the
        // spec rejection only fires when resolution actually lands on a spec-inner
        // function. The contrast with `spec_fn_called_from_executable_*` confirms
        // the spec branch is keyed on a resolved spec callee, not merely on the
        // prefix matching a spec name, and that the miss is reported rather than
        // carried into codegen.
        let msg = assert_err(&[(
            vec![],
            "spec Check { fn verify_inner() -> i32 { return 42; } } \
             pub fn run() -> i32 { return Check::does_not_exist(); }",
        )]);
        assert!(
            msg.contains("undefined function `Check::does_not_exist`"),
            "nonexistent leaf under a spec name must be an undefined-function error, got: {msg}"
        );
        assert!(
            !msg.contains("proof-only"),
            "a nonexistent leaf must not borrow the spec proof-only diagnostic, got: {msg}"
        );
    }

    #[test]
    fn spec_inner_struct_assoc_fn_called_from_executable_rejected() {
        // A three-segment `Spec::Struct::assoc()` reaches an associated function on
        // a struct *inside* a spec. That function is proof-only — codegen assigns
        // it no executable index — so an executable-code call must be a clean
        // type-check rejection, not a codegen panic. The two-segment spec-fn form is
        // already rejected; this pins the three-segment spec-inner-struct path.
        let msg = assert_err(&[(
            vec![],
            "spec Check { struct Helper { v: i32; pub fn make() -> i32 { return 1; } } } \
             pub fn run() -> i32 { return Check::Helper::make(); }",
        )]);
        assert!(
            msg.contains("cannot call spec function `Check::Helper::make`")
                && msg.contains("proof-only"),
            "a spec-inner-struct assoc fn must be rejected from executable code, got: {msg}"
        );
    }

    #[test]
    fn spec_inner_struct_assoc_fn_called_cross_file_rejected() {
        // The cross-file form of the spec-inner-struct assoc rejection: the path
        // walks the file namespace into the spec and onto the struct's associated
        // function. It resolves through the namespace-qualified associated-call
        // handler (distinct from the single-file qualified-call handler), so both
        // handlers must enforce the proof-only boundary.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::specs; \
                 pub fn run() -> i32 { return lib::specs::Check::Helper::make(); }",
            ),
            (
                vec!["lib", "specs"],
                "spec Check { struct Helper { v: i32; pub fn make() -> i32 { return 1; } } }",
            ),
        ]);
        assert!(
            msg.contains("cannot call spec function `lib::specs::Check::Helper::make`")
                && msg.contains("proof-only"),
            "a cross-file spec-inner-struct assoc fn must be rejected, got: {msg}"
        );
    }

    #[test]
    fn top_level_struct_assoc_fn_still_callable_alongside_spec_rejection() {
        // The spec rejection must not over-fire: a legitimate top-level
        // `Type::assoc()` is not inside any spec and stays callable, both bare and
        // namespace-qualified across files. This is the positive control for the
        // proof-only boundary — it draws the line at spec membership, not at the
        // associated-call form.
        assert_ok(&[
            (
                vec![],
                "use lib::geo; \
                 pub fn run() -> i32 { return lib::geo::Counter::seed() + Helper::base(); } \
                 struct Helper { v: i32; pub fn base() -> i32 { return 10; } }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Counter { n: i32; pub fn seed() -> i32 { return 7; } }",
            ),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 11 — CircularDefinition over const / type-alias value graphs.
    // File-import cycles are legal; only value cycles are rejected.
    // ---------------------------------------------------------------------

    #[test]
    fn self_referential_const_rejected() {
        // `const A: i32 = A;` is a degenerate self-cycle: its value depends on
        // itself, with no evaluation order. The value graph records the self-edge
        // and rejects it, exactly like the two-node and longer cycles below.
        let msg = assert_err(&[(vec![], "const A: i32 = A; pub fn main() {}")]);
        assert!(
            msg.contains("circular definition detected"),
            "a self-referential const is rejected, got: {msg}"
        );
        assert!(
            msg.contains("A -> A"),
            "the cycle names the self-loop, got: {msg}"
        );
    }

    #[test]
    fn type_alias_only_cycle_rejected() {
        let msg = assert_err(&[(vec![], "type A = B; type B = A; pub fn main() {}")]);
        assert!(
            msg.contains("circular definition detected"),
            "a type-alias-only cycle is rejected, got: {msg}"
        );
        assert!(msg.contains('A') && msg.contains('B'), "names the cycle, got: {msg}");
    }

    #[test]
    fn const_then_type_alias_mixed_cycle_rejected() {
        let msg = assert_err(&[(vec![], "const A: i32 = B; type B = A; pub fn main() {}")]);
        assert!(
            msg.contains("circular definition detected"),
            "a const->type-alias mixed cycle is rejected, got: {msg}"
        );
    }

    #[test]
    fn four_node_cross_file_cycle_rejected() {
        let msg = assert_err(&[
            (vec![], "const A: i32 = lib::b::B; pub fn main() {}"),
            (vec!["lib", "b"], "pub const B: i32 = lib::c::C;"),
            (vec!["lib", "c"], "pub const C: i32 = lib::d::D;"),
            (vec!["lib", "d"], "pub const D: i32 = A;"),
        ]);
        assert!(
            msg.contains("circular definition detected"),
            "a four-node cross-file value cycle is rejected, got: {msg}"
        );
        assert!(
            msg.contains('A') && msg.contains('B') && msg.contains('C') && msg.contains('D'),
            "names all four members, got: {msg}"
        );
    }

    #[test]
    fn const_cycle_entirely_within_non_entry_file_carries_file_label() {
        // A value cycle confined to one non-entry file must be attributed to that
        // file. The definition graph runs at the root cursor, so without stamping
        // the cycle's home file the diagnostic would render a bare `line:col` and
        // misattribute the cycle to the entry. Here `A`/`B` cycle entirely within
        // `lib::consts`, so the diagnostic carries the `lib::consts` label.
        let msg = assert_err(&[
            (vec![], "use lib::consts; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "consts"],
                "pub const A: i32 = B; pub const B: i32 = A;",
            ),
        ]);
        assert!(
            msg.contains("circular definition detected"),
            "a non-entry-file value cycle is rejected, got: {msg}"
        );
        assert!(
            msg.contains("lib::consts:"),
            "the cycle diagnostic must carry its home-file label, got: {msg}"
        );
    }

    #[test]
    fn diamond_dag_yields_valid_topological_order() {
        // D <- B, D <- C, B <- A, C <- A. No cycle. A must follow B and C; B and
        // C must follow D. Order property, not exact sequence.
        let ctx = try_type_check_multi_file(&[(
            vec![],
            "const D: i32 = 1; const B: i32 = D; const C: i32 = D; const A: i32 = B; pub fn main() {}",
        )])
        .expect("a diamond DAG should type-check");
        let order = ctx.definition_order();
        let pos = |name: &str| {
            use inference_ast::nodes::Def;
            order
                .iter()
                .position(|&id| {
                    matches!(
                        &ctx.arena()[id].kind,
                        Def::Constant { name: n, .. } if ctx.arena()[*n].name == name
                    )
                })
                .unwrap_or_else(|| panic!("const {name} missing from definition order"))
        };
        assert!(pos("D") < pos("B"), "D before B");
        assert!(pos("D") < pos("C"), "D before C");
        assert!(pos("B") < pos("A"), "B before A");
    }

    // ---------------------------------------------------------------------
    // Axis 12 — import-name collisions.
    // ---------------------------------------------------------------------

    #[test]
    fn file_import_collides_with_local_fn() {
        let msg = assert_err(&[
            (vec![], "use lib::arith; fn arith() -> i32 { return 0; } pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
        assert!(
            msg.contains("imported name `arith` collides with a local definition"),
            "file import collides with a local fn, got: {msg}"
        );
    }

    #[test]
    fn file_import_collides_with_local_struct() {
        let msg = assert_err(&[
            (vec![], "use lib::geo; struct geo { x: i32; } pub fn main() {}"),
            (vec!["lib", "geo"], "pub struct Point { x: i32; }"),
        ]);
        assert!(
            msg.contains("imported name `geo` collides with a local definition"),
            "file import collides with a local struct, got: {msg}"
        );
    }

    #[test]
    fn two_file_imports_same_last_segment_collide() {
        let msg = assert_err(&[
            (vec![], "use lib::a::geo; use lib::b::geo; pub fn main() {}"),
            (vec!["lib", "a", "geo"], "pub fn f() -> i32 { return 1; }"),
            (vec!["lib", "b", "geo"], "pub fn g() -> i32 { return 2; }"),
        ]);
        assert!(
            msg.contains("imported name `geo` collides with another import"),
            "two file imports with the same last segment collide, got: {msg}"
        );
    }

    #[test]
    fn item_import_collides_with_file_import() {
        let msg = assert_err(&[
            (vec![], "use lib::arith; use lib::other::{arith}; pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["lib", "other"], "pub fn arith() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("imported name `arith` collides with another import"),
            "an item import collides with a file import of the same name, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 13 — empty import list and other malformed imports.
    // ---------------------------------------------------------------------

    #[test]
    fn empty_import_list_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::arith::{}; pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
        assert!(
            msg.contains("empty import list"),
            "empty braced import list is rejected, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 14 — entry reachability edge: `use main;` cannot name the entry.
    // ---------------------------------------------------------------------

    #[test]
    fn use_main_does_not_name_the_entry_file() {
        // The entry has the empty module path, not `["main"]`, so a non-entry file
        // writing `use main;` cannot resolve it. Pinned as the current behavior:
        // the import fails to resolve. Entry items are reached through the reserved
        // `use root;` handle, not bare or via `use main;` — see
        // `entry_item_not_bare_visible_but_reachable_via_use_root`.
        let msg = assert_err(&[
            (vec![], "use lib::helper; pub fn entry_fn() -> i32 { return 1; } pub fn main() {}"),
            (
                vec!["lib", "helper"],
                "use main; pub fn run() -> i32 { return main::entry_fn(); }",
            ),
        ]);
        assert!(
            msg.contains("cannot resolve import path: main"),
            "`use main;` does not name the entry file, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 15 — type-alias visibility.
    // ---------------------------------------------------------------------

    #[test]
    fn private_type_alias_item_import_rejected() {
        // Importing a *private* type alias as an item is rejected, the same as a
        // private fn / struct / enum item import: type aliases carry real
        // visibility in the symbol table now.
        let msg = assert_err(&[
            (vec![], "use lib::ty::{Id}; pub fn main() {}"),
            (vec!["lib", "ty"], "type Id = i32;"),
        ]);
        assert!(
            msg.contains("item `Id` in file `lib::ty` is private"),
            "a private type-alias item import is rejected, got: {msg}"
        );
        assert!(
            msg.contains("note: `Id` is defined at") && msg.contains("add `pub` to export it"),
            "ImportedItemPrivate carries a dual-location note for an alias, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 16 — entry-file boundary: a non-entry file reaches NO entry item by
    // bare name — neither private (soundness) nor public (no ambient cross-file
    // visibility). The entry's `pub` items are reachable only through the reserved
    // `use root;` handle, as `root::item` (pinned by
    // `entry_item_not_bare_visible_but_reachable_via_use_root` in Axis 1 and the
    // `pub_entry_*_via_use_root_item` tests below).
    // ---------------------------------------------------------------------

    #[test]
    fn private_entry_struct_not_reachable_by_bare_name() {
        // The entry file's private `struct Secret` must not be constructible by
        // bare name in an imported file — previously it leaked (no diagnostic),
        // letting an importer read its private fields.
        let msg = assert_err(&[
            (vec![], "struct Secret { password: i32; } pub fn main() {}"),
            (
                vec!["lib", "helper"],
                "pub fn steal() -> i32 { let s: Secret = Secret { password: 99 }; return s.password; }",
            ),
        ]);
        assert!(
            msg.contains("struct `Secret` is not defined")
                || msg.contains("unknown type `Secret`"),
            "a private entry struct is not in scope in an imported file, got: {msg}"
        );
    }

    #[test]
    fn private_entry_fn_not_reachable_by_bare_name() {
        let msg = assert_err(&[
            (vec![], "fn secret() -> i32 { return 7; } pub fn main() {}"),
            (vec!["lib", "helper"], "pub fn run() -> i32 { return secret(); }"),
        ]);
        assert!(
            msg.contains("call to undefined function `secret`"),
            "a private entry fn is not callable by bare name from an imported file, got: {msg}"
        );
    }

    #[test]
    fn private_entry_const_not_reachable_by_bare_name() {
        // A top-level const registers as a root-scope variable for intra-file use;
        // that variable must not leak to a non-entry file either.
        let msg = assert_err(&[
            (vec![], "const MAX: i32 = 5; pub fn main() {}"),
            (vec!["lib", "helper"], "pub fn run() -> i32 { return MAX; }"),
        ]);
        assert!(
            msg.contains("use of undeclared variable `MAX`"),
            "a private entry const is not reachable by bare name, got: {msg}"
        );
    }

    #[test]
    fn private_entry_enum_not_reachable_by_bare_name() {
        let msg = assert_err(&[
            (vec![], "enum Color { Red } pub fn main() {}"),
            (
                vec!["lib", "helper"],
                "pub fn run() -> i32 { let c: Color = Color::Red; return 0; }",
            ),
        ]);
        assert!(
            msg.contains("enum `Color` is not defined")
                || msg.contains("unknown type `Color`"),
            "a private entry enum is not reachable by bare name, got: {msg}"
        );
    }

    #[test]
    fn private_entry_type_alias_not_reachable_by_bare_name() {
        // A private `type` alias in the entry file is not visible by bare name in
        // an imported file (an alias is nominal, so referencing it as a `let` type
        // would be `unknown type`).
        let msg = assert_err(&[
            (vec![], "type Id = i32; pub fn main() {}"),
            (
                vec!["lib", "helper"],
                "pub fn run() -> i32 { let x: Id = 0; return 0; }",
            ),
        ]);
        assert!(
            msg.contains("unknown type `Id`") || msg.contains("`Id`"),
            "a private entry type alias is not reachable by bare name, got: {msg}"
        );
    }

    #[test]
    fn pub_entry_struct_not_bare_visible_but_reachable_via_use_root_item() {
        // A `pub` entry struct is NOT constructible by bare name from an imported
        // file — there is no ambient cross-file visibility, even for `pub` items.
        let msg = assert_err(&[
            (
                vec![],
                "pub struct Shared { v: i32; } use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "pub fn use_it() -> i32 { let s: Shared = Shared { v: 1 }; return s.v; }",
            ),
        ]);
        assert!(
            msg.contains("unknown type `Shared`")
                || msg.contains("struct `Shared` is not defined"),
            "a pub entry struct is not bare-visible from an imported file, got: {msg}"
        );

        // It IS reachable through the reserved `use root::{Shared};` item import.
        assert_ok(&[
            (
                vec![],
                "pub struct Shared { v: i32; } use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "use root::{Shared}; pub fn use_it() -> i32 { let s: Shared = Shared { v: 1 }; return s.v; }",
            ),
        ]);
    }

    #[test]
    fn pub_entry_struct_assoc_fn_not_bare_visible_from_non_entry_file() {
        // The type-member twin of the literal/`let` boundary above: a bare
        // `Gizmo::magic()` written in an imported file must NOT resolve to the
        // entry's same-named struct's associated function. The struct resolver's
        // first branch used to walk to root ungated and leak it (returning the
        // entry's value); it is now boundary-aware (#63).
        let msg = assert_err(&[
            (
                vec![],
                "pub struct Gizmo { v: i32; pub fn magic() -> i32 { return 1234; } } \
                 use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "pub fn use_it() -> i32 { return Gizmo::magic(); }",
            ),
        ]);
        assert!(
            msg.contains("method `magic` not found on type `Gizmo`")
                || msg.contains("Gizmo"),
            "a bare entry assoc fn is not reachable from an imported file, got: {msg}"
        );

        // It IS reachable through the reserved `use root;` namespace handle.
        assert_ok(&[
            (
                vec![],
                "pub struct Gizmo { v: i32; pub fn magic() -> i32 { return 1234; } } \
                 use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "use root; pub fn use_it() -> i32 { return root::Gizmo::magic(); }",
            ),
        ]);
    }

    #[test]
    fn namespace_qualified_type_member_does_not_fall_through_to_entry() {
        // `lib::geo::Color::Green` where `geo` has NO `Color` but the entry does.
        // The namespace resolver's first branch used to walk past `geo` into root
        // and silently bind the entry's `Color` (a Rule-8/M4 leak); it must now
        // error rather than fall through (#63).
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; pub enum Color { Red, Green } \
                 pub fn main() -> i32 { let c: Color = lib::geo::Color::Green; return 0; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
        assert!(
            msg.contains("cannot resolve `lib::geo::Color`")
                || msg.contains("Color"),
            "a namespace type-member must not fall through to the entry, got: {msg}"
        );
    }

    #[test]
    fn non_entry_file_own_item_import_wins_over_same_named_entry_type() {
        // Mirror of the leak fix: a non-entry file item-imports its own `Inner`
        // while the entry defines a same-named `struct Inner`. The file's own
        // import must win in its own file — gating the entry must not also block
        // the legitimate own-import (#63).
        assert_ok(&[
            (
                vec![],
                "use container; pub struct Inner { a: i32; b: i32; } pub fn main() {}",
            ),
            (
                vec!["container"],
                "use lib::types::{Inner}; \
                 pub fn mk() -> i32 { let i: Inner = Inner { v: 6 }; return i.v; }",
            ),
            (vec!["lib", "types"], "pub struct Inner { v: i32; }"),
        ]);
    }

    #[test]
    fn non_entry_file_bare_assoc_fn_binds_own_import_not_entry() {
        // N-1: a non-entry file imports its own `Inner` and calls a bare
        // `Inner::tag()`. With a same-named entry `Inner`, the bare call must bind
        // the file's OWN imported `Inner`, not the entry's (#63).
        assert_ok(&[
            (
                vec![],
                "use container; \
                 pub struct Inner { a: i32; pub fn tag() -> i32 { return 99; } } pub fn main() {}",
            ),
            (
                vec!["container"],
                "use lib::types::{Inner}; pub fn run() -> i32 { return Inner::tag(); }",
            ),
            (
                vec!["lib", "types"],
                "pub struct Inner { v: i32; pub fn tag() -> i32 { return 1; } }",
            ),
        ]);
    }

    #[test]
    fn plain_import_type_member_stays_blocked_even_with_same_named_entry_type() {
        // N-2 (M-C bypass): `lib::mid::Thing::assoc()` where `mid` PLAINLY imports
        // `Thing` (not `pub use`) must stay blocked even when the entry defines a
        // same-named `Thing`. The gated first branch must not leak the entry's
        // `Thing`, so the re-export gate in the second branch is actually reached
        // and rejects the plain import (#63).
        let msg = assert_err(&[
            (
                vec![],
                "use lib::mid; \
                 pub struct Thing { a: i32; pub fn assoc() -> i32 { return 999; } } \
                 pub fn main() -> i32 { return lib::mid::Thing::assoc(); }",
            ),
            (
                vec!["lib", "mid"],
                "use lib::source::{Thing}; pub fn placeholder() -> i32 { return 0; }",
            ),
            (
                vec!["lib", "source"],
                "pub struct Thing { v: i32; pub fn assoc() -> i32 { return 5; } }",
            ),
        ]);
        assert!(
            !msg.is_empty(),
            "a plainly-imported type-member must stay blocked, got success"
        );

        // The `pub use` form IS reachable (the re-export gate permits it).
        assert_ok(&[
            (
                vec![],
                "use lib::mid; \
                 pub struct Thing { a: i32; pub fn assoc() -> i32 { return 999; } } \
                 pub fn main() -> i32 { return lib::mid::Thing::assoc(); }",
            ),
            (
                vec!["lib", "mid"],
                "pub use lib::source::{Thing}; pub fn placeholder() -> i32 { return 0; }",
            ),
            (
                vec!["lib", "source"],
                "pub struct Thing { v: i32; pub fn assoc() -> i32 { return 5; } }",
            ),
        ]);
    }

    #[test]
    fn non_entry_file_uses_its_own_struct_in_type_member() {
        // MUST-NOT-break control: a non-entry file calls an associated function on
        // its OWN struct. The boundary gate hides the *entry's* items, never the
        // file's own definitions, so this resolves cleanly (#63).
        assert_ok(&[
            (vec![], "use lib::b; pub fn main() -> i32 { return lib::b::run(); }"),
            (
                vec!["lib", "b"],
                "pub struct Widget { v: i32; pub fn build() -> i32 { return 77; } } \
                 pub fn run() -> i32 { return Widget::build(); }",
            ),
        ]);
    }

    #[test]
    fn pub_entry_const_not_bare_visible_but_reachable_via_use_root_item() {
        // A `pub` entry const is NOT reachable by bare name from an imported file;
        // it is reached through the reserved `use root::{MAX};` item import.
        let msg = assert_err(&[
            (
                vec![],
                "pub const MAX: i32 = 5; use lib::helper; pub fn main() {}",
            ),
            (vec!["lib", "helper"], "pub fn use_it() -> i32 { return MAX; }"),
        ]);
        assert!(
            msg.contains("use of undeclared variable `MAX`"),
            "a pub entry const is not bare-visible from an imported file, got: {msg}"
        );

        assert_ok(&[
            (
                vec![],
                "pub const MAX: i32 = 5; use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "use root::{MAX}; pub fn use_it() -> i32 { return MAX; }",
            ),
        ]);
    }

    #[test]
    fn entry_file_sees_its_own_private_items() {
        // Control: the entry file itself reaches its own private items by bare
        // name — the boundary filter only applies to *imported* files.
        assert_ok(&[(
            vec![],
            "struct Secret { v: i32; } \
             fn helper() -> i32 { return 1; } \
             const MAX: i32 = 9; \
             pub fn main() -> i32 { let s: Secret = Secret { v: MAX }; return s.v + helper(); }",
        )]);
    }

    #[test]
    fn spec_in_non_entry_file_cannot_reach_entry_private_struct() {
        // A spec lives in a sub-scope of its file. From a non-entry file's spec,
        // the boundary filter still hides the entry file's private struct by bare
        // name.
        let msg = assert_err(&[
            (vec![], "struct Secret { v: i32; } pub fn main() {}"),
            (
                vec!["lib", "api"],
                "spec ApiSpec { fn check() -> i32 { let s: Secret = Secret { v: 0 }; return s.v; } }",
            ),
        ]);
        assert!(
            msg.contains("struct `Secret` is not defined")
                || msg.contains("unknown type `Secret`"),
            "a non-entry spec must not reach the entry file's private struct, got: {msg}"
        );
    }

    #[test]
    fn spec_in_entry_file_sees_entry_private_struct() {
        // Control: a spec in the *entry* file (a descendant of root) still reaches
        // the entry file's own private struct — no boundary is crossed.
        assert_ok(&[(
            vec![],
            "struct Secret { v: i32; } \
             spec S { fn check() -> i32 { let s: Secret = Secret { v: 1 }; return s.v; } } \
             pub fn main() {}",
        )]);
    }

    // ---------------------------------------------------------------------
    // Axis 17 — cross-file struct literal in a function signature position.
    // A signature whose param or return type is an item-imported struct accepts a
    // struct literal of that type: signatures are re-resolved to `Struct` after
    // imports, matching what call sites infer.
    // ---------------------------------------------------------------------

    #[test]
    fn cross_file_struct_literal_passed_as_param() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
                 pub fn read_x(p: Point) -> i32 { return p.x; } \
                 pub fn main() -> i32 { return read_x(Point { x: 1, y: 2 }); }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn cross_file_struct_literal_returned_then_consumed() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
                 pub fn make() -> Point { return Point { x: 1, y: 2 }; } \
                 pub fn main() -> i32 { let p: Point = make(); return p.x; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn cross_file_struct_literal_method_param() {
        // A method param of an imported struct type accepts a literal of that type.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
                 struct Box { v: i32; } \
                 struct Box2 { v: i32; pub fn take(self, p: Point) -> i32 { return p.x; } } \
                 pub fn main() -> i32 { let b: Box2 = Box2 { v: 0 }; return b.take(Point { x: 5, y: 6 }); }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn single_file_struct_literal_param_control() {
        // Control: passing a struct literal as a param of a same-file struct type
        // is unchanged by the re-resolution pass.
        assert_ok(&[(
            vec![],
            "struct Point { x: i32; } \
             fn read_x(p: Point) -> i32 { return p.x; } \
             pub fn main() -> i32 { return read_x(Point { x: 1 }); }",
        )]);
    }

    // ---------------------------------------------------------------------
    // Axis 18 — const initializers referencing cross-file consts. An acyclic
    // chain type-checks (the initializer check runs after imports resolve); a
    // cycle still reports only `CircularDefinition`; `definition_order()` orders
    // the chain dependency-first.
    // ---------------------------------------------------------------------

    #[test]
    fn const_initializer_references_item_imported_const() {
        assert_ok(&[
            (
                vec![],
                "use lib::limits::{BASE}; const DERIVED: i32 = BASE; pub fn main() -> i32 { return DERIVED; }",
            ),
            (vec!["lib", "limits"], "pub const BASE: i32 = 10;"),
        ]);
    }

    #[test]
    fn const_initializer_references_absolute_qualified_const() {
        assert_ok(&[
            (
                vec![],
                "use lib::limits; const DERIVED: i32 = lib::limits::BASE; pub fn main() -> i32 { return DERIVED; }",
            ),
            (vec!["lib", "limits"], "pub const BASE: i32 = 10;"),
        ]);
    }

    #[test]
    fn const_initializer_references_namespace_qualified_const() {
        // The two-segment file-import form `limits::BASE`.
        assert_ok(&[
            (
                vec![],
                "use lib::limits; const DERIVED: i32 = limits::BASE; pub fn main() -> i32 { return DERIVED; }",
            ),
            (vec!["lib", "limits"], "pub const BASE: i32 = 10;"),
        ]);
    }

    #[test]
    fn const_initializer_references_private_cross_file_const_rejected() {
        // A const initializer cannot reach another file's *private* const, even
        // when the entry imports the namespace (the import licenses the path; the
        // const's privacy is what rejects it).
        let msg = assert_err(&[
            (
                vec![],
                "use lib::limits; const DERIVED: i32 = lib::limits::BASE; pub fn main() -> i32 { return DERIVED; }",
            ),
            (vec!["lib", "limits"], "const BASE: i32 = 10;"),
        ]);
        assert!(
            msg.contains("cannot access private constant `lib::limits::BASE`"),
            "a private cross-file const is rejected in an initializer, got: {msg}"
        );
    }

    #[test]
    fn acyclic_cross_file_const_chain_orders_dependency_first() {
        // C <- B <- A across three files; the chain type-checks and
        // `definition_order()` puts each dependency before its dependent. Every
        // file — the entry included — imports the namespace it reads from, since a
        // const initializer obeys the same import discipline as any other
        // cross-file reference: a file may not reach another file's surface without
        // a `use`.
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use lib::b; const A: i32 = lib::b::B; pub fn main() -> i32 { return A; }",
            ),
            (vec!["lib", "b"], "use lib::c; pub const B: i32 = lib::c::C;"),
            (vec!["lib", "c"], "pub const C: i32 = 1;"),
        ])
        .expect("an acyclic cross-file const chain should type-check");
        let order = ctx.definition_order();
        let pos = |name: &str| {
            use inference_ast::nodes::Def;
            order
                .iter()
                .position(|&id| {
                    matches!(
                        &ctx.arena()[id].kind,
                        Def::Constant { name: n, .. } if ctx.arena()[*n].name == name
                    )
                })
                .unwrap_or_else(|| panic!("const {name} missing from definition order"))
        };
        assert!(pos("C") < pos("B"), "C before B");
        assert!(pos("B") < pos("A"), "B before A");
    }

    #[test]
    fn const_initializer_absolute_path_without_use_rejected() {
        // A const initializer in a non-entry file reaching another file's const by
        // an absolute `dir::file::const` path it never imported is the same
        // encapsulation leak the call and type paths forbid: no file reads another
        // file's surface without a `use`. The fix is to add the import.
        let msg = assert_err(&[
            (vec![], "use lib::b; pub fn main() -> i32 { return lib::b::B; }"),
            (vec!["lib", "b"], "pub const B: i32 = lib::c::C;"),
            (vec!["lib", "c"], "pub const C: i32 = 1;"),
        ]);
        assert!(
            msg.contains("lib::c"),
            "the rejection names the unimported namespace `lib::c`, got: {msg}"
        );
    }

    #[test]
    fn cyclic_cross_file_const_reports_only_circular_definition() {
        // The cross-file value cycle must report `CircularDefinition` and nothing
        // else — the const-initializer check is skipped when a cycle is present,
        // so no secondary resolution error leaks.
        let msg = assert_err(&[
            (vec![], "const A: i32 = lib::b::B; pub fn main() {}"),
            (vec!["lib", "b"], "pub const B: i32 = lib::c::C;"),
            (vec!["lib", "c"], "pub const C: i32 = lib::d::D;"),
            (vec!["lib", "d"], "pub const D: i32 = A;"),
        ]);
        assert_eq!(
            msg.matches("circular definition detected").count(),
            1,
            "exactly one circular-definition error, got: {msg}"
        );
        assert!(
            !msg.contains("undeclared")
                && !msg.contains("enum `")
                && !msg.contains("is not defined"),
            "no secondary resolution error accompanies the cycle, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 19 — qualified-path diagnostics. A `::` path through a known namespace
    // whose final segment is not a value reports `cannot resolve <path>` (naming a
    // function when that is what the segment is), never the misleading
    // "enum `lib` is not defined".
    // ---------------------------------------------------------------------

    #[test]
    fn qualified_path_unknown_final_segment_diagnostic() {
        // The entry imports the namespace so the path is licensed; the diagnostic
        // under test is the *bad final segment* one, not the missing-import one.
        let msg = assert_err(&[
            (vec![], "use lib::vals; const X: i32 = lib::vals::NOPE; pub fn main() {}"),
            (vec!["lib", "vals"], "pub const BASE: i32 = 10;"),
        ]);
        assert!(
            msg.contains("cannot resolve `lib::vals::NOPE`"),
            "an unknown final segment through a namespace names the path, got: {msg}"
        );
        assert!(
            !msg.contains("enum `lib`"),
            "the misleading enum message is gone, got: {msg}"
        );
    }

    #[test]
    fn qualified_path_names_function_not_value_diagnostic() {
        // The entry imports the namespace so the path is licensed; the diagnostic
        // under test is the *function-in-value-position* one.
        let msg = assert_err(&[
            (vec![], "use lib::vals; const X: i32 = lib::vals::add; pub fn main() {}"),
            (vec!["lib", "vals"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
        assert!(
            msg.contains("cannot resolve `lib::vals::add`")
                && msg.contains("names a function, not a value"),
            "a function named in value position is diagnosed precisely, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_path_unknown_final_segment_diagnostic() {
        // The two-segment file-import form also gets the precise message.
        let msg = assert_err(&[
            (vec![], "use lib::vals; const X: i32 = vals::NOPE; pub fn main() {}"),
            (vec!["lib", "vals"], "pub const BASE: i32 = 10;"),
        ]);
        assert!(
            msg.contains("cannot resolve `vals::NOPE`") && !msg.contains("enum `vals`"),
            "a two-segment namespace path with a bad final segment is diagnosed, got: {msg}"
        );
    }

    #[test]
    fn enum_variant_access_unaffected_by_qualified_path_diagnostic() {
        // Control: a genuine single-qualifier `Enum::Variant` is left to the
        // variant code — its prefix is not a namespace, so the qualified-path
        // diagnostic never fires. A bad variant still reports `variant not found`.
        let msg = assert_err(&[(
            vec![],
            "enum Color { Red, Green } pub fn main() -> i32 { let c: Color = Color::Blue; return 0; }",
        )]);
        assert!(
            msg.contains("variant `Blue` not found on enum `Color`"),
            "enum-variant diagnostics are unchanged, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis 20 — namespace-qualified type-member access: `geo::Point::new(...)`,
    // `geo::Color::Green`, `geo::Point { .. }` reach a struct/enum *inside* an
    // imported file, with the type's cross-file `pub`-ness enforced.
    // ---------------------------------------------------------------------

    #[test]
    fn namespace_qualified_assoc_fn_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Point}; \
                 pub fn main() -> i32 { let p: Point = geo::Point::new(3, 4); return p.sum(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn new(a: i32, b: i32) -> Point { return Point { x: a, y: b }; } \
                 pub fn sum(self) -> i32 { return self.x + self.y; } }",
            ),
        ]);
    }

    #[test]
    fn namespace_qualified_assoc_fn_private_type_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; pub fn main() -> i32 { return geo::Point::new(1, 2); }",
            ),
            (
                vec!["lib", "geo"],
                "struct Point { x: i32; y: i32; \
                 pub fn new(a: i32, b: i32) -> i32 { return a + b; } }",
            ),
        ]);
        assert!(
            msg.contains("cannot access private struct `Point`"),
            "a private type's assoc fn is not reachable through a namespace, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_assoc_fn_private_method_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; pub fn main() -> i32 { return geo::Point::secret(1, 2); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; \
                 fn secret(a: i32, b: i32) -> i32 { return a + b; } }",
            ),
        ]);
        assert!(
            msg.contains("Point") && msg.contains("secret"),
            "a private method through a namespace is rejected, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_enum_variant_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Color}; \
                 pub fn main() -> i32 { let c: Color = geo::Color::Green; return 0; }",
            ),
            (vec!["lib", "geo"], "pub enum Color { Red, Green, Blue }"),
        ]);
    }

    #[test]
    fn namespace_qualified_enum_variant_private_enum_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Color}; \
                 pub fn main() -> i32 { let c: Color = geo::Color::Green; return 0; }",
            ),
            (vec!["lib", "geo"], "enum Color { Red, Green, Blue }"),
        ]);
        assert!(
            msg.contains("item `Color` in file `lib::geo` is private"),
            "a private enum's variant is not reachable through a namespace, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_enum_variant_bad_variant_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Color}; \
                 pub fn main() -> i32 { let c: Color = geo::Color::Purple; return 0; }",
            ),
            (vec!["lib", "geo"], "pub enum Color { Red, Green, Blue }"),
        ]);
        assert!(
            msg.contains("Purple"),
            "an unknown variant through a namespace names the bad variant, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_struct_literal_constructs_and_reads_field() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Point}; \
                 pub fn main() -> i32 { let p: Point = geo::Point { x: 7, y: 8 }; return p.x; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn namespace_qualified_struct_literal_private_struct_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geo; pub fn main() -> i32 { let p: i32 = geo::Point { x: 7, y: 8 }.x; return p; }",
            ),
            (vec!["lib", "geo"], "struct Point { x: i32; y: i32; }"),
        ]);
        assert!(
            msg.contains("cannot access private struct `Point`"),
            "a private struct is not constructible through a namespace, got: {msg}"
        );
    }

    #[test]
    fn namespace_struct_literal_interops_with_item_imported_type() {
        // A value built via `geo::Point { .. }` has the same canonical key as one
        // built from the item-imported `Point`, so the two are mutually assignable.
        assert_ok(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Point}; \
                 pub fn main() -> i32 { \
                 let a: Point = geo::Point { x: 1, y: 2 }; \
                 let b: Point = Point { x: 3, y: 4 }; \
                 let c: Point = a; \
                 return b.x + c.y; }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    #[test]
    fn namespace_struct_literal_distinct_from_same_named_other_file() {
        // A `Point` from file `a` reached via its namespace is NOT assignable where
        // a same-named `Point` from file `b` is expected — different files,
        // different canonical keys.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a; use lib::b::{Point}; \
                 pub fn read(p: Point) -> i32 { return p.x; } \
                 pub fn main() -> i32 { return read(a::Point { x: 1, y: 2 }); }",
            ),
            (vec!["lib", "a"], "pub struct Point { x: i32; y: i32; }"),
            (vec!["lib", "b"], "pub struct Point { x: i32; y: i32; }"),
        ]);
        assert!(
            msg.contains("expected `lib::b::Point`, found `lib::a::Point`"),
            "a namespace-built Point from file a is not a file-b Point, got: {msg}"
        );
    }

    #[test]
    fn namespace_qualified_assoc_fn_via_absolute_path() {
        // The absolute form `lib::geo::Point::new(...)` resolves the type member
        // through the namespace path. The item import binds `Point`; the absolute
        // assoc call needs the namespace import too, so the entry imports both.
        assert_ok(&[
            (
                vec![],
                "use lib::geo; use lib::geo::{Point}; \
                 pub fn main() -> i32 { let p: Point = lib::geo::Point::new(3, 4); return p.sum(); }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn new(a: i32, b: i32) -> Point { return Point { x: a, y: b }; } \
                 pub fn sum(self) -> i32 { return self.x + self.y; } }",
            ),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 20b — namespace type-member access through an INTERMEDIATE file
    // honors the re-export gate exactly as the free-function path does: a plain
    // (non-`pub use`) intermediate import blocks traversal to its type members;
    // a `pub use` re-export permits it. Confirmed at depth-1 (`mid::Point::raw`)
    // and depth-3 (`lib::sub::mid::Point::raw`) (#63, Rule 5).
    // ---------------------------------------------------------------------

    #[test]
    fn namespace_type_member_through_plain_import_blocked_depth1() {
        // `mid` plain-imports `Point` (`use lib::a::{Point};`), so the struct is
        // private to `mid`; reaching `mid::Point::raw()` from another file is a
        // public-surface leak and must be rejected — consistent with a plain-
        // imported free function being blocked.
        let msg = assert_err(&[
            (vec![], "use lib::mid; pub fn main() -> i32 { return lib::mid::Point::raw(); }"),
            (
                vec!["lib", "a"],
                "pub struct Point { x: i32; pub fn raw() -> i32 { return 99; } }",
            ),
            (vec!["lib", "mid"], "use lib::a::{Point};"),
        ]);
        assert!(
            msg.contains("lib::mid::Point::raw"),
            "a plain-imported type's member is not reachable through a namespace, got: {msg}"
        );
    }

    #[test]
    fn namespace_type_member_through_pub_use_import_resolves_depth1() {
        // With `pub use lib::a::{Point};` the intermediate re-exports the type, so
        // `mid::Point::raw()` resolves — the positive counterpart to the plain-
        // import block.
        assert_ok(&[
            (vec![], "use lib::mid; pub fn main() -> i32 { return lib::mid::Point::raw(); }"),
            (
                vec!["lib", "a"],
                "pub struct Point { x: i32; pub fn raw() -> i32 { return 99; } }",
            ),
            (vec!["lib", "mid"], "pub use lib::a::{Point};"),
        ]);
    }

    #[test]
    fn namespace_type_member_through_plain_import_blocked_depth3() {
        // The same gate holds at depth-3 (`lib::sub::mid::Point::raw`): a plain
        // intermediate import blocks the deeper type-member path.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::sub::mid; pub fn main() -> i32 { return lib::sub::mid::Point::raw(); }",
            ),
            (
                vec!["lib", "a"],
                "pub struct Point { x: i32; pub fn raw() -> i32 { return 99; } }",
            ),
            (vec!["lib", "sub", "mid"], "use lib::a::{Point};"),
        ]);
        assert!(
            msg.contains("lib::sub::mid::Point::raw"),
            "a depth-3 plain-imported type member is blocked, got: {msg}"
        );
    }

    #[test]
    fn namespace_type_member_through_pub_use_import_resolves_depth3() {
        assert_ok(&[
            (
                vec![],
                "use lib::sub::mid; pub fn main() -> i32 { return lib::sub::mid::Point::raw(); }",
            ),
            (
                vec!["lib", "a"],
                "pub struct Point { x: i32; pub fn raw() -> i32 { return 99; } }",
            ),
            (vec!["lib", "sub", "mid"], "pub use lib::a::{Point};"),
        ]);
    }

    #[test]
    fn namespace_enum_variant_through_plain_import_blocked() {
        // The enum-variant type-member path honors the same gate: a plain
        // intermediate import of the enum blocks `mid::Color::Red`.
        let msg = assert_err(&[
            (
                vec![],
                "use lib::mid; use lib::a::{Color}; \
                 pub fn main() -> i32 { let c: Color = lib::mid::Color::Red; return 0; }",
            ),
            (vec!["lib", "a"], "pub enum Color { Red, Green }"),
            (vec!["lib", "mid"], "use lib::a::{Color};"),
        ]);
        assert!(
            msg.contains("cannot resolve `lib::mid::Color`"),
            "a plain-imported enum's variant is not reachable through a namespace, got: {msg}"
        );
    }

    #[test]
    fn namespace_enum_variant_through_pub_use_import_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::mid; use lib::a::{Color}; \
                 pub fn main() -> i32 { let c: Color = lib::mid::Color::Red; return 0; }",
            ),
            (vec!["lib", "a"], "pub enum Color { Red, Green }"),
            (vec!["lib", "mid"], "pub use lib::a::{Color};"),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis 9b — cross-file struct/enum TYPE CONFUSION (the B1 soundness
    // guard). Same-named struct/enum types from different files are DISTINCT
    // (identity keyed on the canonical file path, not the bare name), so a
    // value of one is never assignable where the other is expected — at every
    // boundary. Each negative compiled-as-bug before the identity fix; if any
    // starts passing, the type checker has regressed to bare-name unification
    // and a 12-byte struct can flow into a 4-byte slot (OOB) or a value's
    // private behavior can run on a forged same-named public twin.
    // ---------------------------------------------------------------------

    /// `T{x;y;z}` from one file passed where the other file's `T{x}` param is
    /// expected — the canonical confirmed repro (argument boundary).
    #[test]
    fn b1_argument_boundary_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{read_a}; use lib::b::{T}; \
                 pub fn main() -> i32 { let v: T = T { x: 1, y: 2, z: 3 }; return read_a(v); }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; } pub fn read_a(v: T) -> i32 { return v.x; }"),
            (vec!["lib", "b"], "pub struct T { x: i32; y: i32; z: i32; }"),
        ]);
        assert!(msg.contains("type mismatch"), "must reject the cross-file confusion, got: {msg}");
        assert!(
            msg.contains("lib::a::T") && msg.contains("lib::b::T"),
            "diagnostic must name both file-qualified types, got: {msg}"
        );
    }

    /// A function returning one file's `T` assigned into a `let` annotated with
    /// the other file's same-named `T` (return/let-annotation boundary).
    #[test]
    fn b1_return_into_other_typed_let_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{make_a}; use lib::b::{T}; \
                 pub fn main() -> i32 { let v: T = make_a(); return v.x; }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; } pub fn make_a() -> T { return T { x: 1 }; }"),
            (vec!["lib", "b"], "pub struct T { x: i32; y: i32; }"),
        ]);
        assert!(msg.contains("type mismatch"), "got: {msg}");
    }

    /// Calling one file's method on a value of the other file's same-named type
    /// (instance-method-receiver boundary).
    #[test]
    fn b1_method_receiver_boundary_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{T}; use lib::b::{make_b}; \
                 pub fn main() -> i32 { let v: T = make_b(); return v.deep(); }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; pub fn deep(self) -> i32 { return self.x; } }"),
            (vec!["lib", "b"], "pub struct T { x: i32; y: i32; z: i32; } pub fn make_b() -> T { return T { x: 1, y: 2, z: 3 }; }"),
        ]);
        assert!(msg.contains("type mismatch") || msg.contains("not defined") || msg.contains("no method"), "got: {msg}");
    }

    /// Same-named enums with DIFFERENT variant order across files — passing one
    /// where the other is expected would silently flip the discriminant.
    #[test]
    fn b1_enum_variant_order_boundary_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{Color}; use lib::b::{classify}; \
                 pub fn main() -> i32 { let c: Color = Color::Red; return classify(c); }",
            ),
            (vec!["lib", "a"], "pub enum Color { Red, Green, Blue }"),
            (vec!["lib", "b"], "pub enum Color { Blue, Green, Red } pub fn classify(c: Color) -> i32 { return 0; }"),
        ]);
        assert!(msg.contains("type mismatch"), "got: {msg}");
    }

    /// A struct whose field type is a same-named-but-different imported struct:
    /// crossing the OUTER struct must be rejected (compounded-offset guard).
    #[test]
    fn b1_nested_struct_boundary_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{Wrap}; use lib::b::{make_b_wrap}; \
                 pub fn main() -> i32 { let w: Wrap = make_b_wrap(); return w.tag; }",
            ),
            (vec!["lib", "a"], "pub struct Small { x: i32; } pub struct Wrap { inner: Small; tag: i32; }"),
            (vec!["lib", "b"], "pub struct Small { x: i32; y: i32; z: i32; } pub struct Wrap { inner: Small; tag: i32; } pub fn make_b_wrap() -> Wrap { return Wrap { inner: Small { x: 1, y: 2, z: 3 }, tag: 9 }; }"),
        ]);
        assert!(msg.contains("type mismatch"), "got: {msg}");
    }

    /// Nominal-by-file, not structural: even when the two same-named structs
    /// have IDENTICAL layout, they remain distinct types and do not interoperate.
    #[test]
    fn b1_identical_layout_still_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a::{read_a}; use lib::b::{T}; \
                 pub fn main() -> i32 { let v: T = T { x: 1 }; return read_a(v); }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; } pub fn read_a(v: T) -> i32 { return v.x; }"),
            (vec!["lib", "b"], "pub struct T { x: i32; }"),
        ]);
        assert!(msg.contains("type mismatch"), "identical layout must still be nominally distinct, got: {msg}");
    }

    /// Positive control: the SAME type reached via item-import in two files
    /// interoperates (no false rejection) — guards against over-strictness.
    #[test]
    fn b1_same_type_via_import_interoperates() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; use lib::ops::{flip}; \
                 pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return flip(p); }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
            (vec!["lib", "ops"], "use lib::geo::{Point}; pub fn flip(p: Point) -> i32 { return p.y; }"),
        ]);
    }

    /// Positive control: a single-file program with one `T` is unaffected.
    #[test]
    fn b1_single_file_one_type_unaffected() {
        assert_ok(&[(
            vec![],
            "struct T { x: i32; } fn read(v: T) -> i32 { return v.x; } \
             pub fn main() -> i32 { return read(T { x: 7 }); }",
        )]);
    }

    /// The same imported type passed to a non-entry function reached through a
    /// *namespace-qualified* call (`ops::flip(p)`) — not an item-imported bare
    /// call — must also interoperate. The non-entry param is canonicalized at its
    /// defining file, so the qualified-call reader sees the same key as the value.
    #[test]
    fn b1_same_type_via_namespace_qualified_call_interoperates() {
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; use lib::ops; \
                 pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return ops::flip(p); }",
            ),
            (vec!["lib", "geo"], "pub struct Point { x: i32; y: i32; }"),
            (vec!["lib", "ops"], "use lib::geo::{Point}; pub fn flip(p: Point) -> i32 { return p.y; }"),
        ]);
    }

    /// A non-entry function whose param is a same-named type imported from a
    /// *different* file than the caller imports: the param resolves through the
    /// *defining* file's import (`lib::a::T`), the argument is the caller's
    /// `lib::b::T`, so the call is still rejected — the fix canonicalizes against
    /// the definer's scope, never collapsing distinct same-named types.
    #[test]
    fn b1_non_entry_param_cross_imported_type_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::b::{T}; use lib::ops::{flip}; \
                 pub fn main() -> i32 { let v: T = T { x: 1, y: 2 }; return flip(v); }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; }"),
            (vec!["lib", "b"], "pub struct T { x: i32; y: i32; }"),
            (vec!["lib", "ops"], "use lib::a::{T}; pub fn flip(p: T) -> i32 { return p.x; }"),
        ]);
        assert!(msg.contains("type mismatch"), "got: {msg}");
        assert!(
            msg.contains("lib::a::T") && msg.contains("lib::b::T"),
            "diagnostic must name both file-qualified types, got: {msg}"
        );
    }

    /// Counterpart positive control: when the non-entry function and the caller
    /// import the *same* file's type, the param resolves to that one canonical key
    /// and the call interoperates.
    #[test]
    fn b1_non_entry_param_same_imported_type_interoperates() {
        assert_ok(&[
            (
                vec![],
                "use lib::a::{T}; use lib::ops::{flip}; \
                 pub fn main() -> i32 { let v: T = T { x: 1 }; return flip(v); }",
            ),
            (vec!["lib", "a"], "pub struct T { x: i32; }"),
            (vec!["lib", "ops"], "use lib::a::{T}; pub fn flip(p: T) -> i32 { return p.x; }"),
        ]);
    }

    /// A non-entry function whose param is an item-imported *enum* must also
    /// interoperate when called from the entry file — the param canonicalizes the
    /// same way a struct param does.
    #[test]
    fn b1_non_entry_enum_param_via_import_interoperates() {
        assert_ok(&[
            (
                vec![],
                "use lib::col::{Color}; use lib::ops::{paint}; \
                 pub fn main() -> i32 { let c: Color = Color::Green; return paint(c); }",
            ),
            (vec!["lib", "col"], "pub enum Color { Red, Green, Blue }"),
            (vec!["lib", "ops"], "use lib::col::{Color}; pub fn paint(c: Color) -> i32 { return 0; }"),
        ]);
    }

    // ---------------------------------------------------------------------
    // Axis — `::`-qualified type annotations resolve to canonical identity.
    //
    // A qualified type (`geo::Level`, `root::T`, `lib::geom::Point`) names a
    // cross-file type through its namespace chain. The annotation must resolve
    // to the same canonical nominal identity a constructor or bare reference
    // produces, so it *equals* the matching value type rather than staying an
    // opaque qualified-name. These cover let / parameter / return / receiver
    // positions, for both struct and enum, at 2- and 3-segment depths and via
    // `root::`, plus the cross-form identity, visibility, and negatives.
    // ---------------------------------------------------------------------

    /// A namespace-qualified enum annotation in a `let` binding equals the
    /// qualified value, so the binding type-checks and the `==` unifies.
    #[test]
    fn qualified_enum_annotation_in_let_resolves() {
        assert_ok(&[
            (
                vec![],
                "use geo; \
                 pub fn run() -> i32 { \
                   let x: geo::Level = geo::Level::High; \
                   if x == geo::Level::High { return 2; } return 0; }",
            ),
            (vec!["geo"], "pub enum Level { Low, Med, High }"),
        ]);
    }

    /// A namespace-qualified struct annotation in a `let` binding equals the
    /// qualified constructor's type.
    #[test]
    fn qualified_struct_annotation_in_let_resolves() {
        assert_ok(&[
            (
                vec![],
                "use geo; \
                 pub fn run() -> i32 { \
                   let p: geo::Point = geo::Point { x: 5 }; return p.x; }",
            ),
            (vec!["geo"], "pub struct Point { x: i32; }"),
        ]);
    }

    /// A 3-segment qualified struct annotation (`lib::geom::Point`) resolves
    /// through the two-hop namespace chain bound by `use lib::geom;`.
    #[test]
    fn three_segment_qualified_struct_annotation_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; \
                 pub fn run() -> i32 { \
                   let p: lib::geom::Point = lib::geom::Point { x: 8, y: 9 }; return p.x; }",
            ),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// A 2-segment qualified type annotation (`let p: g::Pt`) must resolve to the
    /// type `g.inf` defines even when a sibling file `g/Pt.inf` is pulled into the
    /// import closure by another file. The sibling's presence must not turn the
    /// leaf `Pt` into a sub-file namespace; the type defined in `g` wins, mirroring
    /// the precedence the associated-function path already honors. Without this, the
    /// annotation fails with a self-contradictory `expected g::Pt, found g::Pt`.
    #[test]
    fn two_segment_qualified_annotation_resolves_with_same_named_sibling_file() {
        assert_ok(&[
            (
                vec![],
                "use g; use z; \
                 pub fn main() -> i32 { let p: g::Pt = g::Pt::make(); return p.x + z::touch(); }",
            ),
            (vec!["g"], "pub struct Pt { x: i32; pub fn make() -> Pt { return Pt { x: 5 }; } }"),
            (vec!["z"], "use g::Pt; pub fn touch() -> i32 { return 0; }"),
            (vec!["g", "Pt"], "pub fn make() -> i32 { return 999; }"),
        ]);
    }

    /// The 3-level twin: `a::b::c::Node` resolves to the type `a/b/c.inf` defines
    /// even though a sibling `a/b/c/Node.inf` is in the closure. The leaf type
    /// resolution is independent of how deep the namespace prefix runs.
    #[test]
    fn three_level_qualified_annotation_resolves_with_same_named_sibling_file() {
        assert_ok(&[
            (
                vec![],
                "use a::b::c; use z; \
                 pub fn main() -> i32 { \
                   let n: a::b::c::Node = a::b::c::Node::make(); return n.v + z::touch(); }",
            ),
            (
                vec!["a", "b", "c"],
                "pub struct Node { v: i32; pub fn make() -> Node { return Node { v: 7 }; } }",
            ),
            (vec!["z"], "use a::b::c::Node; pub fn touch() -> i32 { return 0; }"),
            (vec!["a", "b", "c", "Node"], "pub fn make() -> i32 { return 999; }"),
        ]);
    }

    /// The associated-function form of the same path must keep resolving with the
    /// sibling file present: `g::Pt::make()` is the struct's associated function,
    /// not the sibling `g/Pt.inf`'s free `make`. This is the value-position twin of
    /// the annotation test and guards that the leaf-segment precedence change does
    /// not alter the (already-correct) non-leaf assoc behavior.
    #[test]
    fn qualified_assoc_call_resolves_with_same_named_sibling_file() {
        assert_ok(&[
            (
                vec![],
                "use g; use z; pub fn main() -> i32 { return g::Pt::make().x + z::touch(); }",
            ),
            (vec!["g"], "pub struct Pt { x: i32; pub fn make() -> Pt { return Pt { x: 5 }; } }"),
            (vec!["z"], "use g::Pt; pub fn touch() -> i32 { return 0; }"),
            (vec!["g", "Pt"], "pub fn make() -> i32 { return 999; }"),
        ]);
    }

    /// A `root::`-qualified annotation names a type in the *entry* file from a
    /// non-entry file; its canonical key is the bare name (entry = empty path).
    #[test]
    fn root_qualified_annotation_resolves_to_entry_type() {
        assert_ok(&[
            (
                vec![],
                "use lib::b::{describe}; \
                 pub struct Pt { x: i32; } \
                 pub fn run() -> i32 { let p: Pt = Pt { x: 4 }; return describe(p); }",
            ),
            (
                vec!["lib", "b"],
                "use root; pub fn describe(p: root::Pt) -> i32 { return p.x; }",
            ),
        ]);
    }

    /// A 2-segment qualified type in *parameter* position resolves and the
    /// caller's matching value interoperates.
    #[test]
    fn qualified_struct_annotation_in_param_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::b; use lib::shapes::{Q}; \
                 pub fn run() -> i32 { let q: Q = Q { x: 3 }; return lib::b::describe(q); }",
            ),
            (vec!["lib", "shapes"], "pub struct Q { x: i32; }"),
            (
                vec!["lib", "b"],
                "use lib::shapes; pub fn describe(q: lib::shapes::Q) -> i32 { return q.x; }",
            ),
        ]);
    }

    /// A qualified type in *return* position resolves and unifies with the
    /// returned constructor value.
    #[test]
    fn qualified_struct_annotation_in_return_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; \
                 pub fn make() -> lib::geom::Point { return lib::geom::Point { x: 1, y: 2 }; } \
                 pub fn run() -> i32 { let p: lib::geom::Point = make(); return p.y; }",
            ),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// A qualified type used as a *method parameter* (a method is defined inside
    /// its struct body) resolves the same as a free-function parameter, so the
    /// `self`-bearing method can accept a cross-file value named by qualifier.
    #[test]
    fn qualified_struct_annotation_in_method_param_resolves() {
        assert_ok(&[
            (
                vec![],
                "use geo; \
                 pub struct Holder { \
                   v: i32; \
                   pub fn add(self, p: geo::Point) -> i32 { return self.v + p.x; } \
                 } \
                 pub fn run() -> i32 { \
                   let h: Holder = Holder { v: 1 }; \
                   let p: geo::Point = geo::Point { x: 6 }; \
                   return h.add(p); }",
            ),
            (vec!["geo"], "pub struct Point { x: i32; }"),
        ]);
    }

    /// A namespace-qualified annotation and the matching *item-imported* value
    /// name the same canonical type, so the two forms are interchangeable.
    #[test]
    fn qualified_annotation_equals_item_imported_value() {
        assert_ok(&[
            (
                vec![],
                "use geo; use geo::{Point}; \
                 pub fn run() -> i32 { \
                   let a: geo::Point = Point { x: 7 }; \
                   let b: Point = geo::Point { x: 8 }; \
                   return a.x + b.x; }",
            ),
            (vec!["geo"], "pub struct Point { x: i32; }"),
        ]);
    }

    /// Reaching a *private* entry-file type through `root::` is rejected with a
    /// visibility diagnostic pointing at the declaration — never silently
    /// accepted.
    #[test]
    fn root_qualified_private_type_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::b::{describe}; \
                 struct Secret { x: i32; } \
                 pub fn run() -> i32 { return 0; }",
            ),
            (
                vec!["lib", "b"],
                "use root; pub fn describe(s: root::Secret) -> i32 { return s.x; }",
            ),
        ]);
        assert!(
            msg.contains("cannot access private struct `Secret`"),
            "private entry type reached via `root::` must be rejected, got: {msg}"
        );
    }

    /// A qualifier naming a real namespace but a non-existent leaf type fails
    /// with a clean `unknown type` diagnostic naming the full path.
    #[test]
    fn qualified_annotation_unknown_leaf_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use geo; \
                 pub fn run() -> i32 { let x: geo::Nope = geo::Level::Low; return 0; }",
            ),
            (vec!["geo"], "pub enum Level { Low, High }"),
        ]);
        assert!(
            msg.contains("unknown type `geo::Nope`"),
            "unknown qualified leaf must be reported, got: {msg}"
        );
    }

    /// A qualifier naming a non-existent namespace fails with a clean `unknown
    /// type` diagnostic rather than a silent acceptance.
    #[test]
    fn qualified_annotation_unknown_namespace_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use geo; \
                 pub fn run() -> i32 { let x: nope::Level = geo::Level::Low; return 0; }",
            ),
            (vec!["geo"], "pub enum Level { Low, High }"),
        ]);
        assert!(
            msg.contains("unknown type `nope::Level`"),
            "unknown qualified namespace must be reported, got: {msg}"
        );
    }

    /// An *uncalled* non-entry function whose parameter is a qualified type still
    /// type-checks: the signature's qualified annotation resolves the same way a
    /// used one does, regardless of call sites.
    #[test]
    fn uncalled_fn_with_qualified_param_type_checks() {
        assert_ok(&[
            (
                vec![],
                "use lib::b::{unused}; \
                 pub struct Pt { x: i32; } \
                 pub fn run() -> i32 { return 1; }",
            ),
            (
                vec!["lib", "b"],
                "use root; pub fn unused(o: root::Pt) -> i32 { return 7; }",
            ),
        ]);
    }

    /// Two files each define a same-named struct. A qualified annotation that
    /// names one file's `Cell` must not be accepted for the other file's `Cell`
    /// value — nominal-by-file identity holds through the qualified form.
    #[test]
    fn qualified_annotation_keeps_same_named_types_distinct() {
        let msg = assert_err(&[
            (
                vec![],
                "use a; use b; \
                 pub fn run() -> i32 { let c: a::Cell = b::Cell { v: 1 }; return c.v; }",
            ),
            (vec!["a"], "pub struct Cell { v: i32; }"),
            (vec!["b"], "pub struct Cell { v: i32; }"),
        ]);
        assert!(
            msg.contains("type mismatch") || msg.contains("a::Cell"),
            "same-named cross-file structs must stay distinct through a qualified \
             annotation, got: {msg}"
        );
    }

    /// A struct *field* declared with a `::`-qualified cross-file type resolves to
    /// the field type's canonical identity, so the struct definition type-checks
    /// and the nested field is readable.
    #[test]
    fn qualified_struct_field_type_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; \
                 pub struct Wrapper { p: lib::geom::Point; } \
                 pub fn run() -> i32 { \
                   let w: Wrapper = Wrapper { p: lib::geom::Point { x: 3, y: 4 } }; \
                   return w.p.x; }",
            ),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// A struct field declared with a qualified path whose leaf does not exist is
    /// rejected with a clean `unknown type` diagnostic — never silently accepted
    /// (which previously let a bad field type reach codegen and panic).
    #[test]
    fn qualified_struct_field_unknown_type_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geom; \
                 pub struct Wrapper { p: lib::geom::Nope; } \
                 pub fn run() -> i32 { return 0; }",
            ),
            (vec!["lib", "geom"], "pub struct Point { x: i32; }"),
        ]);
        assert!(
            msg.contains("unknown type `lib::geom::Nope`"),
            "a bad qualified field type must be reported, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Axis — file-namespace bindings are private to the file that wrote them.
    // A brace-free `use a::b;` binds `b` only within its own file; a different
    // file (including a non-entry one) never resolves a bare qualified call
    // through another file's binding, even though their scope chains share an
    // ancestor (#63).
    // ---------------------------------------------------------------------

    /// The leak's root: an entry-file `use lib::Point;` must not make a bare
    /// `Point::new()` inside `lib.inf` mean the sibling file `lib/Point.inf`. The
    /// program type-checks because `Point::new()` binds the local struct's
    /// associated function whose signature matches the use; resolving the leaked
    /// file's `i32`-returning `new` instead would surface a return-type mismatch.
    #[test]
    fn entry_namespace_import_does_not_leak_into_lib_bare_assoc_call() {
        assert_ok(&[
            (vec![], "use lib; use lib::Point; pub fn main() -> i32 { return lib::build(); }"),
            (
                vec!["lib"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn new() -> Point { return Point { x: 1, y: 2 }; } } \
                 pub fn build() -> i32 { let p: Point = Point::new(); return p.x + p.y; }",
            ),
            (vec!["lib", "Point"], "pub fn new() -> i32 { return 0; }"),
        ]);
    }

    /// A non-entry file that does not itself import a namespace `n` cannot reach an
    /// entry-file `use a::n;` binding: the binding is private to the entry. The bare
    /// `n::value()` in `lib.inf` must fail to resolve, never silently bind the
    /// entry's `n`. With the binding blocked, `n` is no longer a bound namespace in
    /// `lib.inf` — but `a::n` is a real file in the project, so the call is
    /// rejected as a missing import (the head names a namespace, not a type),
    /// pointing at the fix rather than at a nonexistent method. This is the
    /// negative twin of the same-alias independence test.
    #[test]
    fn non_entry_file_cannot_use_entry_namespace_binding() {
        let msg = assert_err(&[
            (vec![], "use lib; use a::n; pub fn main() -> i32 { return lib::run(); }"),
            (vec!["lib"], "pub fn run() -> i32 { return n::value(); }"),
            (vec!["a", "n"], "pub fn value() -> i32 { return 1; }"),
        ]);
        assert!(
            msg.contains("namespace `n` is not imported") && msg.contains("n::value"),
            "the leaked binding must not resolve; expected a missing-import rejection \
             for `n`, got: {msg}"
        );
    }

    #[test]
    fn qualified_call_via_unimported_namespace_reports_missing_import() {
        // A bare `util::helper()` whose head names a real file in the project but is
        // not imported in the calling file is a missing-import error, not a "method
        // not found on type `util`". The head is a namespace, so the fix is a `use`;
        // routing it to method dispatch would point at a nonexistent method on a
        // type that does not exist. The target is reachable (another file imports
        // it), so it is in the project's namespace set.
        let msg = assert_err(&[
            (vec![], "use lib::other; pub fn main() -> i32 { return util::helper(); }"),
            (vec!["lib", "other"], "use lib::util; pub fn bridge() -> i32 { return util::helper(); }"),
            (vec!["lib", "util"], "pub fn helper() -> i32 { return 1; }"),
        ]);
        assert!(
            msg.contains("namespace `util` is not imported")
                && msg.contains("util::helper"),
            "an unimported-namespace call must report the missing import, got: {msg}"
        );
        assert!(
            !msg.contains("not found on type"),
            "the call must not be routed to method dispatch, got: {msg}"
        );
    }

    #[test]
    fn qualified_call_on_genuine_unknown_type_still_method_not_found() {
        // The contrast: a head that names neither a type nor a project file is a
        // genuine `Type::method()` miss and still reports `method not found on type`
        // — the missing-import diagnostic only fires when the head names a real
        // file namespace.
        let msg = assert_err(&[(
            vec![],
            "pub fn main() -> i32 { return Bogus::method(); }",
        )]);
        assert!(
            msg.contains("method `method` not found on type `Bogus`"),
            "a genuine unknown type must still be a method-not-found error, got: {msg}"
        );
    }

    /// Two non-entry files each bind the same local alias `n` to a *different* file
    /// (`use a::n;` vs `use b::n;`). Each file's bare `n::value()` resolves against
    /// its own import; neither sees the other's binding through the shared root
    /// ancestor. Both callable paths type-check.
    #[test]
    fn same_alias_in_two_non_entry_files_resolves_independently() {
        assert_ok(&[
            (
                vec![],
                "use left; use right; \
                 pub fn main() -> i32 { return left::pick() + right::pick(); }",
            ),
            (vec!["left"], "use a::n; pub fn pick() -> i32 { return n::value(); }"),
            (vec!["right"], "use b::n; pub fn pick() -> i32 { return n::value(); }"),
            (vec!["a", "n"], "pub fn value() -> i32 { return 11; }"),
            (vec!["b", "n"], "pub fn value() -> i32 { return 22; }"),
        ]);
    }

    /// A type defined in a file wins over a same-named sibling file when a qualified
    /// `parent::Name::member` path is resolved: zero-argument `lib::Point::new()` is
    /// the struct's associated function, not the sibling file `lib/Point.inf`'s free
    /// `new`. The sibling takes one argument, so a leak to the file would surface an
    /// argument-count mismatch on the zero-argument call; a clean type-check pins the
    /// struct-precedence choice.
    #[test]
    fn qualified_path_prefers_struct_over_same_named_sibling_file() {
        assert_ok(&[
            (vec![], "use lib; use lib::Point; pub fn main() -> i32 { return lib::Point::new(); }"),
            (vec!["lib"], "pub struct Point { v: i32; pub fn new() -> i32 { return 1; } }"),
            (vec!["lib", "Point"], "pub fn new(unused: i32) -> i32 { return 1000; }"),
        ]);
    }

    /// The negative complement: when the qualified path's leaf names a member that
    /// the struct lacks, resolution does *not* silently fall back to the same-named
    /// sibling file's free function. `lib::Point::missing()` is rejected even though
    /// `lib/Point.inf` defines a `missing` — the struct's identity is what the path
    /// addresses (#63).
    #[test]
    fn qualified_path_struct_member_miss_does_not_fall_back_to_sibling_file() {
        let msg = assert_err(&[
            (vec![], "use lib; use lib::Point; pub fn main() -> i32 { return lib::Point::missing(); }"),
            (vec!["lib"], "pub struct Point { v: i32; pub fn new() -> i32 { return 1; } }"),
            (vec!["lib", "Point"], "pub fn missing() -> i32 { return 9; }"),
        ]);
        assert!(
            msg.contains("missing"),
            "a struct member miss must not fall back to the sibling file, got: {msg}"
        );
    }

    // ---------------------------------------------------------------------
    // Head precedence (the 2-segment call gate): a struct/enum defined in the
    // accessing file pre-empts a same-named sibling FILE at the *head* of a
    // qualified call, so `foo::pick()` / `Vec::new()` / `Color::Red` mean the
    // local type even when an unrelated sibling drags the same-named file into the
    // import closure. This is the value-position counterpart to the leaf/non-leaf
    // type-path precedence above, decided through the shared head-precedence
    // helper so the two resolvers never disagree (#63).
    // ---------------------------------------------------------------------

    /// A local `struct Vec` with associated `new() -> Vec`, used in a `let v: Vec =
    /// Vec::new()`, must type-check even when a sibling pulls a root-child `Vec.inf`
    /// (free `new() -> i32`) into the closure. Resolving the call to the file would
    /// give `i32` and surface a self-contradictory `expected Vec, found i32`.
    #[test]
    fn local_struct_assoc_call_wins_over_sibling_file_in_let() {
        assert_ok(&[
            (vec![], "use bar; use puller; pub fn main() -> i32 { return bar::make(); }"),
            (
                vec!["bar"],
                "pub struct Vec { len: i32; pub fn new() -> Vec { return Vec { len: 7 }; } } \
                 pub fn make() -> i32 { let v: Vec = Vec::new(); return v.len; }",
            ),
            (vec!["Vec"], "pub fn new() -> i32 { return 999; }"),
            (vec!["puller"], "use Vec; pub fn p() -> i32 { return Vec::new(); }"),
        ]);
    }

    /// A local `enum Color { Red }`, referenced as `Color::Red`, must resolve to the
    /// variant — not be mistaken for a sibling `Color.inf`'s free `Red()` — when the
    /// sibling is in the closure. Resolving to the file would reject the variant use
    /// with `Color::Red names a function, not a value`.
    #[test]
    fn local_enum_variant_wins_over_sibling_file() {
        assert_ok(&[
            (vec![], "use bar; use puller; pub fn main() -> i32 { return bar::make(); }"),
            (
                vec!["bar"],
                "pub enum Color { Red, Green } \
                 pub fn make() -> i32 { let c: Color = Color::Red; return 5; }",
            ),
            (vec!["Color"], "pub fn Red() -> i32 { return 999; }"),
            (vec!["puller"], "use Color; pub fn p() -> i32 { return Color::Red(); }"),
        ]);
    }

    /// A control without the sibling file: `bar`'s `Vec::new()` resolves to its local
    /// struct on its own. Paired with [`local_struct_assoc_call_wins_over_sibling_file_in_let`],
    /// it pins that adding the sibling to the closure does not change resolution.
    #[test]
    fn local_struct_assoc_call_resolves_without_sibling_file_present() {
        assert_ok(&[
            (vec![], "use bar; pub fn main() -> i32 { return bar::make(); }"),
            (
                vec!["bar"],
                "pub struct Vec { len: i32; pub fn new() -> Vec { return Vec { len: 7 }; } } \
                 pub fn make() -> i32 { let v: Vec = Vec::new(); return v.len; }",
            ),
        ]);
    }

    /// A legitimate two-segment namespace call (`util::helper()`, no local type
    /// shadowing the head) must still resolve through the import after the head-veto
    /// is added — the veto fires only when the accessing file defines the head type.
    #[test]
    fn plain_two_segment_namespace_call_still_resolves_after_head_veto() {
        assert_ok(&[
            (vec![], "use util; pub fn main() -> i32 { return util::helper(); }"),
            (vec!["util"], "pub fn helper() -> i32 { return 99; }"),
        ]);
    }

    /// A legitimate multi-segment namespace traversal from a *non-entry* file whose
    /// head is a `use`-bound namespace (not a locally-defined type) must keep
    /// resolving: the head-veto must not fire for a namespace head. `math` re-exports
    /// `lib::arith`, and a non-entry `caller` reaches `math::arith::add` through it.
    #[test]
    fn namespace_traversal_from_non_entry_file_unaffected_by_head_veto() {
        assert_ok(&[
            (vec![], "use caller; pub fn main() -> i32 { return caller::go(); }"),
            (vec!["caller"], "use math; pub fn go() -> i32 { return math::arith::add(3, 4); }"),
            (vec!["math"], "pub use lib::arith;"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    // ---------------------------------------------------------------------
    // Intermediate-segment precedence: a type defined in the accessing file
    // pre-empts a same-named sibling file only when the type interpretation is
    // *viable* for the remaining path (the type is the leaf, or is followed by a
    // single member). A type name colliding with an *intermediate* `::`-segment —
    // one followed by a further `::`-segment — cannot be a type-member access, so
    // the namespace walk must continue and the type must not stop it (#63).
    // ---------------------------------------------------------------------

    /// `lib::geom::Point` where the parent file `lib.inf` defines a `struct geom`
    /// that collides with the intermediate `geom` segment. The type interpretation
    /// of `geom` is impossible (it is followed by `::Point`, another type), so the
    /// walk consumes `geom` as the sub-file namespace and the qualified annotation
    /// resolves to the real `Point`.
    #[test]
    fn intermediate_segment_collision_with_struct_resolves_namespace() {
        assert_ok(&[
            (
                vec![],
                "use lib; use lib::geom; \
                 pub fn main() -> i32 { let p: lib::geom::Point = lib::geom::Point { x: 1, y: 2 }; return p.x + lib::tagval(); }",
            ),
            (vec!["lib"], "struct geom { tag: i32; } pub fn tagval() -> i32 { return 0; }"),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// The enum twin: an `enum geom` in the parent file likewise has no member
    /// type, so the intermediate `geom` segment of `lib::geom::Point` is the
    /// sub-file namespace and the path resolves.
    #[test]
    fn intermediate_segment_collision_with_enum_resolves_namespace() {
        assert_ok(&[
            (
                vec![],
                "use lib; use lib::geom; \
                 pub fn main() -> i32 { let p: lib::geom::Point = lib::geom::Point { x: 1, y: 2 }; return p.y + lib::tagval(); }",
            ),
            (vec!["lib"], "enum geom { A, B } pub fn tagval() -> i32 { return 0; }"),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// The three-level twin: `lib::sub::geom::Point` where the *mid* intermediate
    /// segment `sub` collides with a `struct sub` in `lib.inf`. `sub` is followed
    /// by two more segments, so it cannot be a type-access and the walk continues.
    #[test]
    fn intermediate_segment_collision_at_three_levels_resolves_namespace() {
        assert_ok(&[
            (
                vec![],
                "use lib; use lib::sub::geom; \
                 pub fn main() -> i32 { let p: lib::sub::geom::Point = lib::sub::geom::Point { x: 1, y: 2 }; return p.x + lib::tagval(); }",
            ),
            (vec!["lib"], "struct sub { tag: i32; } pub fn tagval() -> i32 { return 0; }"),
            (vec!["lib", "sub"], "pub fn placeholder() -> i32 { return 0; }"),
            (vec!["lib", "sub", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// Even under an intermediate-segment collision, a genuinely unknown leaf type
    /// still errors cleanly: `lib::geom::Nope` names a real namespace but no type,
    /// so it is `unknown type`, not silently accepted.
    #[test]
    fn intermediate_segment_collision_unknown_leaf_still_rejected() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib; use lib::geom; \
                 pub fn main() -> i32 { let p: lib::geom::Nope = lib::geom::Nope { x: 1 }; return 0; }",
            ),
            (vec!["lib"], "struct geom { tag: i32; } pub fn tagval() -> i32 { return 0; }"),
            (vec!["lib", "geom"], "pub struct Point { x: i32; }"),
        ]);
        assert!(
            msg.contains("unknown type `lib::geom::Nope`"),
            "an unknown leaf under an intermediate collision must still be reported, got: {msg}"
        );
    }

    /// The head case of the same rule from a *non-entry* accessing file: `caller`
    /// defines `struct geom` and writes `geom::sub::Point`, while a same-named
    /// sibling `geom.inf` is reachable. The head `geom` is followed by two more
    /// segments, so the type cannot be the target and the head-veto must not fire —
    /// the absolute namespace path resolves to the real `Point`. This pins that the
    /// head precedence is remaining-path-aware, not position-agnostic.
    #[test]
    fn head_collision_with_two_trailing_segments_resolves_namespace() {
        assert_ok(&[
            (
                vec![],
                "use caller; pub fn main() -> i32 { return caller::go(); }",
            ),
            (
                vec!["caller"],
                "use geom::sub; \
                 struct geom { tag: i32; } \
                 pub fn go() -> i32 { let p: geom::sub::Point = geom::sub::Point { x: 4 }; return p.x; }",
            ),
            (vec!["geom"], "pub fn touch() -> i32 { return 0; }"),
            (vec!["geom", "sub"], "pub struct Point { x: i32; }"),
        ]);
    }

    // ---------------------------------------------------------------------
    // Recursive-struct detection is by canonical key, not bare name: distinct
    // same-named cross-file structs are not a cycle, and a genuine cross-file
    // cycle is caught at type-check (before codegen) (#63).
    // ---------------------------------------------------------------------

    /// A genuine cross-file struct cycle — `root::Outer` contains `lib::m::Inner`,
    /// which contains `root::Outer` back — must be rejected at type-check with the
    /// `recursive struct definition` diagnostic, not slip through to a codegen
    /// layout failure.
    #[test]
    fn genuine_cross_file_struct_cycle_rejected_at_type_check() {
        let msg = assert_err(&[
            (vec![], "use lib::m; pub struct Outer { inner: lib::m::Inner; } pub fn main() -> i32 { return 0; }"),
            (vec!["lib", "m"], "use root; pub struct Inner { back: root::Outer; }"),
        ]);
        assert!(
            msg.contains("recursive struct definition"),
            "a genuine cross-file struct cycle must be rejected at type-check, got: {msg}"
        );
    }

    /// Distinct same-named cross-file structs are *not* a cycle: the entry `Wrap`
    /// has a field typed as a different `lib::m::Wrap`. The bare-name comparison
    /// would falsely flag this; keying by canonical identity accepts it.
    #[test]
    fn distinct_same_named_cross_file_struct_field_is_not_a_cycle() {
        assert_ok(&[
            (vec![], "use lib::m; pub struct Wrap { inner: lib::m::Wrap; tag: i32; } pub fn main() -> i32 { let w: Wrap = Wrap { inner: lib::m::Wrap { v: 5 }, tag: 9 }; return w.inner.v + w.tag; }"),
            (vec!["lib", "m"], "pub struct Wrap { v: i32; }"),
        ]);
    }

    /// A cross-file cycle that closes through an ARRAY field must be caught: the
    /// `Array` recursion arm of the cycle check must thread the canonical key as
    /// the direct-field arm does. `Outer` holds `[lib::m::Inner; 2]`, and `Inner`
    /// holds `root::Outer` back.
    #[test]
    fn cross_file_struct_cycle_through_array_field_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::m; pub struct Outer { items: [lib::m::Inner; 2]; } pub fn main() -> i32 { return 0; }"),
            (vec!["lib", "m"], "use root; pub struct Inner { back: root::Outer; }"),
        ]);
        assert!(
            msg.contains("recursive struct definition"),
            "a cross-file cycle through an array field must be rejected, got: {msg}"
        );
    }

    /// The array-field control: a distinct same-named cross-file struct reached
    /// through an array field is not a cycle, so it must type-check — the array arm
    /// must not re-introduce the bare-name false positive.
    #[test]
    fn distinct_same_named_cross_file_struct_through_array_is_not_a_cycle() {
        assert_ok(&[
            (vec![], "use lib::m; pub struct Wrap { inners: [lib::m::Wrap; 2]; } pub fn main() -> i32 { return 0; }"),
            (vec!["lib", "m"], "pub struct Wrap { v: i32; }"),
        ]);
    }

    /// A three-file cross-file cycle (`root::A` -> `lib::b::B` -> `lib::c::C` ->
    /// `root::A`) must be caught: the multi-hop `lookup_struct_by_key` traversal
    /// with its `visited` set has to close the loop across three distinct files.
    #[test]
    fn three_file_cross_file_struct_cycle_rejected() {
        let msg = assert_err(&[
            (vec![], "use lib::b; pub struct A { b: lib::b::B; } pub fn main() -> i32 { return 0; }"),
            (vec!["lib", "b"], "use lib::c; pub struct B { c: lib::c::C; }"),
            (vec!["lib", "c"], "use root; pub struct C { a: root::A; }"),
        ]);
        assert!(
            msg.contains("recursive struct definition"),
            "a three-file struct cycle must be rejected at type-check, got: {msg}"
        );
    }

    // =====================================================================
    // FIX-17 — round-13 ambient-visibility defects (#63).
    //
    // Defect 1: the absolute-anchor gate licensed a path if ANY imported
    // namespace key was a *prefix* of the whole path, so `use lib::geom;`
    // leaked `lib::geom::sub::deep` — a deeper namespace only another file
    // dragged into the closure. The gate now licenses iff an imported key
    // *equals* the path's Deepest Registered Namespace Prefix (DRNP): each
    // `use` imports exactly one namespace, never its sub-namespaces.
    //
    // Defect 5: the qualified-type and qualified-struct-literal positions
    // routed an unimported-namespace leak through a generic "unknown
    // type" / "struct X is not defined", hiding the missing import. Both now
    // mirror the call/const sites — uniform missing-import diagnostics.
    //
    // Defects 6/7: when the target sibling file is uncompiled, the rev-scan
    // fell back to a shorter directory key (`lib`), suggesting the
    // unparseable `use lib;`. The hedged diagnostic now offers the path's
    // namespace portion (always a parseable file namespace) instead.
    // =====================================================================

    // ---- Defect 1: anchor-gate leak is closed (DRNP-equality) ----

    /// The leak repro: the entry imports only `lib::geom`, while `helper`
    /// imports `lib::geom::sub`, dragging the deeper namespace into the closure.
    /// `use lib::geom;` must NOT license the entry to spell `lib::geom::sub::deep`
    /// — `sub` is a deeper namespace the entry never imported. The deeper file IS
    /// in the closure, so the diagnostic is the confident missing-import naming
    /// the exact `use`.
    #[test]
    fn deeper_namespace_not_licensed_by_shallower_import() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geom; use helper; pub fn entry() -> i32 { return lib::geom::sub::deep() + helper::go(); }",
            ),
            (vec!["lib", "geom"], "pub fn area() -> i32 { return 1; }"),
            (vec!["lib", "geom", "sub"], "pub fn deep() -> i32 { return 99; }"),
            (
                vec!["helper"],
                "use lib::geom::sub; pub fn go() -> i32 { return lib::geom::sub::deep(); }",
            ),
        ]);
        assert!(
            msg.contains("namespace `lib::geom::sub` is not imported")
                && msg.contains("use lib::geom::sub;")
                && msg.contains("lib::geom::sub::deep"),
            "a shallower `use lib::geom;` must not license the deeper `lib::geom::sub`, got: {msg}"
        );
    }

    /// Accept: a surface call into the imported namespace itself. `lib::geom::area`
    /// has DRNP `lib::geom` (its leaf `area` is a function, not a namespace), which
    /// the entry imported, so it resolves.
    #[test]
    fn direct_surface_call_into_imported_namespace_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; pub fn entry() -> i32 { return lib::geom::area(); }",
            ),
            (vec!["lib", "geom"], "pub fn area() -> i32 { return 7; }"),
        ]);
    }

    /// Accept: an associated call whose type segment is NOT a namespace.
    /// `lib::geom::Point::new` has DRNP `lib::geom` — `Point` is a type, never a
    /// `mod_scopes` key — so `use lib::geom;` licenses it without needing the
    /// nonexistent `use lib::geom::Point;`.
    #[test]
    fn assoc_call_through_type_in_imported_namespace_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; pub fn entry() -> i32 { return lib::geom::Point::make(); }",
            ),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; pub fn make() -> i32 { return 5; } }",
            ),
        ]);
    }

    /// Accept: a qualified struct literal whose namespace the entry imported.
    /// `lib::geom::Point { .. }` has the pure-namespace prefix `lib::geom`, the
    /// whole of which the entry imported, so the literal resolves (the inclusive
    /// DRNP scan handles the pre-split-prefix shape).
    #[test]
    fn qualified_struct_literal_into_imported_namespace_resolves() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; pub fn entry() -> i32 { let p: lib::geom::Point = lib::geom::Point { x: 3, y: 4 }; return p.x; }",
            ),
            (vec!["lib", "geom"], "pub struct Point { x: i32; y: i32; }"),
        ]);
    }

    /// Reject: a file that imported NEITHER the namespace nor any covering parent
    /// may not spell the absolute path. `helper` imports nothing under `lib`, so
    /// `lib::geom::area()` is an encapsulation leak even though the entry's
    /// `use lib::geom;` put it in the closure.
    #[test]
    fn no_parent_import_does_not_license_absolute_path() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geom; use helper; pub fn entry() -> i32 { return helper::go(); }",
            ),
            (vec!["lib", "geom"], "pub fn area() -> i32 { return 1; }"),
            (vec!["helper"], "pub fn go() -> i32 { return lib::geom::area(); }"),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;"),
            "a file importing nothing under `lib` may not spell `lib::geom::area`, got: {msg}"
        );
    }

    /// Reject (intended-stricter): importing the CHILD namespace does not license
    /// spelling the PARENT surface. `use lib::geom::sub;` imports exactly
    /// `lib::geom::sub`; the entry may not borrow its parent `lib::geom` to call
    /// `lib::geom::area` — that parent is a different namespace it never imported.
    #[test]
    fn child_import_does_not_license_parent_surface() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geom::sub; pub fn entry() -> i32 { return lib::geom::area() + lib::geom::sub::deep(); }",
            ),
            (vec!["lib", "geom"], "pub fn area() -> i32 { return 1; }"),
            (vec!["lib", "geom", "sub"], "pub fn deep() -> i32 { return 2; }"),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;"),
            "importing the child `lib::geom::sub` must not license the parent surface `lib::geom::area`, got: {msg}"
        );
    }

    /// Reject: an *item* import contributes no namespace key, so it never
    /// licenses an absolute path. `use lib::geom::{area};` brings `area` into bare
    /// scope but does not let the entry spell `lib::geom::other()` in long form.
    #[test]
    fn item_import_does_not_license_absolute_sibling_path() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::geom::{area}; pub fn entry() -> i32 { return lib::geom::other(); }",
            ),
            (
                vec!["lib", "geom"],
                "pub fn area() -> i32 { return 1; } pub fn other() -> i32 { return 2; }",
            ),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;"),
            "an item import grants no namespace key, so it must not license `lib::geom::other`, got: {msg}"
        );
    }

    // ---- Defect 5: type-annotation & struct-literal positions report the
    // missing import, not a generic unknown-type / struct-not-defined ----

    /// A leaked qualified TYPE annotation (`let p: lib::geom::Point`) reports the
    /// missing import — not "struct Point is not defined". `helper` drags
    /// `lib::geom` into the closure; the entry never imported it.
    #[test]
    fn leaked_qualified_type_annotation_reports_missing_import() {
        let msg = assert_err(&[
            (
                vec![],
                "use helper; pub fn entry() -> i32 { let p: lib::geom::Point = helper::mk(); return p.x; }",
            ),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; }",
            ),
            (
                vec!["helper"],
                "use lib::geom; pub fn mk() -> lib::geom::Point { return lib::geom::Point { x: 1, y: 2 }; }",
            ),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;")
                && msg.contains("lib::geom::Point"),
            "a leaked qualified type annotation must name the missing import, got: {msg}"
        );
        assert!(
            !msg.contains("struct `Point` is not defined"),
            "the annotation must not cascade a misleading struct-not-defined, got: {msg}"
        );
    }

    /// A leaked qualified STRUCT LITERAL (`lib::geom::Point { .. }`) reports the
    /// missing import — not "struct Point is not defined" — restoring full
    /// call/type/literal/const uniformity. This is the residual the annotation fix
    /// alone left, since the literal position is a separate consumer.
    #[test]
    fn leaked_qualified_struct_literal_reports_missing_import() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::wrap; pub fn entry() -> i32 { return lib::geom::mk(lib::geom::Point { x: 3, y: 4 }); }",
            ),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; } pub fn mk(p: Point) -> i32 { return p.x; }",
            ),
            (vec!["lib", "wrap"], "use lib::geom; pub fn t() -> i32 { return 0; }"),
        ]);
        assert!(
            msg.contains("namespace `lib::geom` is not imported")
                && msg.contains("use lib::geom;")
                && msg.contains("lib::geom::Point"),
            "a leaked qualified struct literal must name the missing import, got: {msg}"
        );
        assert!(
            !msg.contains("struct `Point` is not defined"),
            "the struct literal must not cascade a misleading struct-not-defined, got: {msg}"
        );
    }

    /// The corrected program — once the suggested `use lib::geom;` is added — type
    /// checks cleanly, proving the hint the leak diagnostics emit is itself
    /// parseable and resolves the reference.
    #[test]
    fn corrected_qualified_struct_literal_compiles_with_suggested_use() {
        assert_ok(&[
            (
                vec![],
                "use lib::geom; pub fn entry() -> i32 { let p: lib::geom::Point = lib::geom::Point { x: 3, y: 4 }; return lib::geom::mk(p); }",
            ),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; } pub fn mk(p: Point) -> i32 { return p.x; }",
            ),
        ]);
    }

    // ---- Defect 2 wording: single-segment unimported namespace CALL ----

    /// A bare `other::thing()` call from a file that did not import `other`, where
    /// another file (`bridge`) dragged `other` into the closure, reports the
    /// reworded `UnimportedNamespaceCall`: `add use other; to call other::thing`
    /// — no unparseable `...::` placeholder.
    #[test]
    fn unimported_single_segment_namespace_call_parseable_hint() {
        let msg = assert_err(&[
            (
                vec![],
                "use bridge; pub fn entry() -> i32 { return other::thing(); }",
            ),
            (vec!["bridge"], "use other; pub fn b() -> i32 { return other::thing(); }"),
            (vec!["other"], "pub fn thing() -> i32 { return 5; }"),
        ]);
        assert!(
            msg.contains("namespace `other` is not imported")
                && msg.contains("add `use other;` to call `other::thing`")
                && !msg.contains("..."),
            "the unimported-call hint must read `add use other; to call other::thing` with no `...`, got: {msg}"
        );
    }

    /// The corrected call — once `use other;` is added — type checks, proving the
    /// suggested import is parseable.
    #[test]
    fn corrected_single_segment_namespace_call_compiles() {
        assert_ok(&[
            (
                vec![],
                "use other; pub fn entry() -> i32 { return other::thing(); }",
            ),
            (vec!["other"], "pub fn thing() -> i32 { return 5; }"),
        ]);
    }

    // ---- Defects 6/7: uncompiled-file fallback gives a parseable hedged hint ----

    /// When the target sibling file is not in the closure (uncompiled), the rev-
    /// scan would otherwise fall back to the shallow directory key `lib`,
    /// suggesting the unparseable `use lib;`. The hedged diagnostic instead offers
    /// the namespace portion `lib::other` (a parseable file namespace), with the
    /// "could not resolve" / "if … names a source file" hedge.
    #[test]
    fn uncompiled_namespace_call_gives_hedged_parseable_hint() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a; pub fn entry() -> i32 { return lib::other::val(); }",
            ),
            (vec!["lib", "a"], "pub fn helper() -> i32 { return 1; }"),
        ]);
        assert!(
            msg.contains("could not resolve `lib::other::val`")
                && msg.contains("`lib::other` is not an imported namespace")
                && msg.contains("import it with `use lib::other;`"),
            "an uncompiled-target call must give the hedged hint naming `lib::other`, got: {msg}"
        );
        assert!(
            !msg.contains("use lib;"),
            "the hedged hint must not suggest the unparseable directory `use lib;`, got: {msg}"
        );
    }

    /// The deeper uncompiled case (`lib::b::g`, `b.inf` absent): the hedged hint
    /// names `lib::b`, not the directory `lib`.
    #[test]
    fn uncompiled_deeper_namespace_call_names_namespace_portion() {
        let msg = assert_err(&[
            (
                vec![],
                "use lib::a; pub fn entry() -> i32 { return lib::b::g(); }",
            ),
            (vec!["lib", "a"], "pub fn helper() -> i32 { return 1; }"),
        ]);
        assert!(
            msg.contains("could not resolve `lib::b::g`")
                && msg.contains("`lib::b` is not an imported namespace")
                && msg.contains("import it with `use lib::b;`")
                && !msg.contains("use lib;"),
            "the hedged hint must name `lib::b`, not the directory `lib`, got: {msg}"
        );
    }

}
