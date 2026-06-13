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
//! - **`::` does not parse in type position.** A cross-file type can be named in a
//!   `let` binding only after an *item import* brings its bare name into scope;
//!   `let x: a::b::T` is a parse error, so those forms are never written here.
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
    // Axis 1 — absolute cross-file paths (no import) × item visibility.
    // A `pub` fn in another file is reachable by its absolute `a::b::fn` path;
    // a private one is rejected with the dual-location diagnostic.
    // ---------------------------------------------------------------------

    #[test]
    fn absolute_path_pub_fn_resolves() {
        assert_ok(&[
            (vec![], "pub fn main() -> i32 { return lib::arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ]);
    }

    #[test]
    fn absolute_path_private_fn_rejected_dual_location() {
        let msg = assert_err(&[
            (vec![], "pub fn main() -> i32 { return lib::arith::secret(); }"),
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
            (vec![], "pub fn main() -> i32 { return a::b::c::add(1, 2); }"),
            (vec!["a", "b", "c"], "pub fn add(x: i32, y: i32) -> i32 { return x + y; }"),
        ]);
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
        // a public fn there returns it, and that fn is callable cross-file.
        assert_ok(&[
            (vec![], "pub fn main() -> i32 { return lib::vals::get_max(); }"),
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
        // item import, fed by a cross-file constructor fn returning the enum.
        assert_ok(&[
            (
                vec![],
                "use lib::col::{Color}; pub fn main() -> i32 { let c: Color = lib::col::first(); return 0; }",
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
        // `lib::vals::MAX` resolves as a qualified path to the pub const.
        assert_ok(&[
            (vec![], "pub fn main() -> i32 { return lib::vals::MAX; }"),
            (vec!["lib", "vals"], "pub const MAX: i32 = 10;"),
        ]);
    }

    #[test]
    fn private_const_qualified_path_rejected_dual_location() {
        let msg = assert_err(&[
            (vec![], "pub fn main() -> i32 { return lib::vals::MAX; }"),
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
                "spec S { fn check() -> i32 { return lib::arith::secret(); } } pub fn main() {}",
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
                "const DERIVED: i32 = lib::limits::BASE; pub fn main() -> i32 { return DERIVED; }",
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
        // A const initializer cannot reach another file's *private* const.
        let msg = assert_err(&[
            (
                vec![],
                "const DERIVED: i32 = lib::limits::BASE; pub fn main() -> i32 { return DERIVED; }",
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
        // `definition_order()` puts each dependency before its dependent.
        let ctx = try_type_check_multi_file(&[
            (vec![], "const A: i32 = lib::b::B; pub fn main() -> i32 { return A; }"),
            (vec!["lib", "b"], "pub const B: i32 = lib::c::C;"),
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
        let msg = assert_err(&[
            (vec![], "const X: i32 = lib::vals::NOPE; pub fn main() {}"),
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
        let msg = assert_err(&[
            (vec![], "const X: i32 = lib::vals::add; pub fn main() {}"),
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
        // The absolute form `lib::geo::Point::new(...)` (no file import) also
        // resolves the type member through the namespace path.
        assert_ok(&[
            (
                vec![],
                "use lib::geo::{Point}; \
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
}
