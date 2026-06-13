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
    fn absolute_path_to_entry_item_from_non_entry_file() {
        // A non-entry file reaches an entry-file item by bare name (entry is the
        // program root; its public items are visible to the closure).
        assert_ok(&[
            (
                vec![],
                "use lib::helper; pub fn entry_fn() -> i32 { return 1; } pub fn main() {}",
            ),
            (vec!["lib", "helper"], "pub fn run() -> i32 { return entry_fn(); }"),
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
        // it does not resolve.
        let msg = assert_err(&[
            (vec![], "use math; pub fn main() -> i32 { return math::arith::add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "use lib::arith;"),
        ]);
        assert!(!msg.is_empty(), "plain use must not be traversable, got: {msg}");
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
            msg.contains("call to undefined function `m1::m2::lib::add`"),
            "breaking the first hop blocks the whole chain, got: {msg}"
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
            msg.contains("call to undefined function `m1::m2::lib::add`"),
            "breaking the middle hop blocks the whole chain, got: {msg}"
        );
    }

    #[test]
    fn pub_use_of_item_is_not_surfaced_through_reexporter() {
        // `pub use lib::arith::{add};` re-exports the ITEM, but neither
        // `use math::{add};` nor `math::add(...)` reaches it: only namespace
        // re-exports are traversable. Pinned as the current behavior (item
        // re-exports are effectively private to the re-exporting file).
        let msg = assert_err(&[
            (vec![], "use math::{add}; pub fn main() -> i32 { return add(1, 2); }"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "pub use lib::arith::{add};"),
        ]);
        assert!(
            msg.contains("item `add` not found in file `math`"),
            "a pub-use'd item is not re-importable from the re-exporter, got: {msg}"
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
            !msg.is_empty(),
            "a spec must not reach another file's private struct by bare name, got: {msg}"
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
        // the import fails to resolve. (Entry items remain reachable by bare name
        // from the closure — see `absolute_path_to_entry_item_from_non_entry_file`.)
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
    // Axis 16 — entry-file privacy: a non-entry file must NOT reach the entry
    // file's *private* items by bare name. The entry file is the program root,
    // so a bare lookup from an imported file walks into root; private root items
    // are filtered at the file boundary (soundness). Public entry items stay
    // reachable by bare name (pinned by
    // `absolute_path_to_entry_item_from_non_entry_file` in Axis 1).
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
    fn pub_entry_struct_reachable_by_bare_name() {
        // Control: a `pub` entry struct IS constructible by bare name from an
        // imported file — only privacy is gated at the boundary.
        assert_ok(&[
            (
                vec![],
                "pub struct Shared { v: i32; } use lib::helper; pub fn main() {}",
            ),
            (
                vec!["lib", "helper"],
                "pub fn use_it() -> i32 { let s: Shared = Shared { v: 1 }; return s.v; }",
            ),
        ]);
    }

    #[test]
    fn pub_entry_const_reachable_by_bare_name() {
        // Control: a `pub` entry const IS reachable by bare name from an imported
        // file (through the const-symbol path, which gates on `pub`).
        assert_ok(&[
            (
                vec![],
                "pub const MAX: i32 = 5; use lib::helper; pub fn main() {}",
            ),
            (vec!["lib", "helper"], "pub fn use_it() -> i32 { return MAX; }"),
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
            !msg.is_empty()
                && (msg.contains("struct `Secret` is not defined")
                    || msg.contains("unknown type `Secret`")),
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
}
