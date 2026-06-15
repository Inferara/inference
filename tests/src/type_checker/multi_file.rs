//! Multi-file type-checking smoke tests for the file-based module hierarchy.
//!
//! Each test assembles an arena from `(module_path, source)` pairs with
//! [`crate::utils::try_type_check_multi_file`] (entry first, then imported files
//! in canonical order) and checks resolution across file scopes, imports, and
//! re-exports. The comprehensive matrix lives in `multi_file_matrix.rs`; these
//! pin the load-bearing behaviors.
#[cfg(test)]
mod tests {
    use crate::utils::try_type_check_multi_file;

    /// The corrected three-file example from the issue: `main` imports `math`,
    /// `math` re-exports `lib::arith`, and `main` reaches `math::arith::add`
    /// transitively. Resolution must succeed end to end.
    #[test]
    fn three_file_reexport_chain_resolves() {
        let files = [
            (
                vec![],
                "use math; pub fn main() { let r: i32 = math::arith::add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            (vec!["math"], "pub use lib::arith;"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(result.is_ok(), "three-file re-export chain should type-check, got: {:?}", result.err().map(|e| e.to_string()));
    }

    /// A plain (non-`pub`) import is private to the importing file, so a chained
    /// access that would traverse through it across a file boundary must fail.
    #[test]
    fn non_reexported_chain_access_errors() {
        let files = [
            (
                vec![],
                "use math; pub fn main() { let r: i32 = math::arith::add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
            // plain `use` (not `pub use`): `arith` is not re-exported from math.
            (vec!["math"], "use lib::arith;"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_err(),
            "a chain through a non-re-exported import must not resolve"
        );
    }

    /// A direct absolute path to a public function in another file resolves
    /// through the file scope tree, once the accessing file imports the namespace
    /// that licenses the absolute spelling.
    #[test]
    fn absolute_path_to_public_function_resolves() {
        let files = [
            (
                vec![],
                "use lib::arith; pub fn main() { let r: i32 = lib::arith::add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(result.is_ok(), "absolute cross-file path should resolve, got: {:?}", result.err().map(|e| e.to_string()));
    }

    /// An item import binds the item for bare use in the importing file.
    #[test]
    fn item_import_binds_for_bare_use() {
        let files = [
            (
                vec![],
                "use lib::arith::{add}; pub fn main() { let r: i32 = add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        // Note: bare-use resolution of imported items at call sites is wired
        // through resolved imports; the import itself must resolve (exists +
        // pub) without error.
        let result = try_type_check_multi_file(&files);
        assert!(result.is_ok(), "item import of a public function should resolve, got: {:?}", result.err().map(|e| e.to_string()));
    }

    /// Importing a private item by name is rejected, naming the item and file.
    #[test]
    fn item_import_of_private_item_errors() {
        let files = [
            (vec![], "use lib::arith::{secret}; pub fn main() {}"),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ];
        let result = try_type_check_multi_file(&files);
        let msg = result.err().expect("type check should fail").to_string();
        assert!(
            msg.contains("item `secret`") && msg.contains("private"),
            "error should name the private item, got: {msg}"
        );
    }

    /// Importing a non-existent item is rejected, naming the item and file.
    #[test]
    fn item_import_of_missing_item_errors() {
        let files = [
            (vec![], "use lib::arith::{nope}; pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        let result = try_type_check_multi_file(&files);
        let msg = result.err().expect("type check should fail").to_string();
        assert!(
            msg.contains("item `nope` not found in file `lib::arith`"),
            "error should name the missing item and file, got: {msg}"
        );
    }

    /// An empty braced import list is rejected with an educational message.
    #[test]
    fn empty_import_list_errors() {
        let files = [
            (vec![], "use lib::arith::{}; pub fn main() {}"),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        let result = try_type_check_multi_file(&files);
        let msg = result.err().expect("type check should fail").to_string();
        assert!(
            msg.contains("empty import list"),
            "error should explain the empty import list, got: {msg}"
        );
    }

    /// A file import whose bound name collides with a local definition is
    /// rejected.
    #[test]
    fn file_import_name_collides_with_local_definition() {
        let files = [
            (
                vec![],
                "use lib::arith; fn arith() -> i32 { return 0; } pub fn main() {}",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        let result = try_type_check_multi_file(&files);
        let msg = result.err().expect("type check should fail").to_string();
        assert!(
            msg.contains("collides"),
            "error should report the name collision, got: {msg}"
        );
    }

    /// Two files each defining a private struct of the same name register in
    /// distinct file scopes, so each file resolves its own without conflict.
    #[test]
    fn same_named_private_structs_in_two_files_coexist() {
        let files = [
            (
                vec![],
                "struct Buffer { x: i32; } pub fn main() { let b: Buffer = Buffer { x: 0 }; }",
            ),
            (
                vec!["lib", "buf"],
                "struct Buffer { y: i32; } pub fn use_it() { let b: Buffer = Buffer { y: 1 }; }",
            ),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(result.is_ok(), "same-named private structs in separate files should not conflict, got: {:?}", result.err().map(|e| e.to_string()));
    }

    /// A non-entry file may reference its own file-private function — the file
    /// scope keeps private items visible within their own file.
    #[test]
    fn non_entry_file_sees_its_own_privates() {
        let files = [
            (vec![], "use lib::api; pub fn main() {}"),
            (
                vec!["lib", "api"],
                "fn helper() -> i32 { return 1; } pub fn run() -> i32 { return helper(); }",
            ),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(result.is_ok(), "a non-entry file should see its own private functions, got: {:?}", result.err().map(|e| e.to_string()));
    }

    /// A spec in a non-entry file sees that file's private items: its scope hangs
    /// off the file scope, so a parent-chain lookup reaches the file's privates.
    #[test]
    fn spec_in_non_entry_file_sees_own_file_privates() {
        let files = [
            (vec![], "use lib::api; pub fn main() {}"),
            (
                vec!["lib", "api"],
                "fn helper() -> i32 { return 1; } \
                 spec ApiSpec { fn check() -> i32 { return helper(); } }",
            ),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_ok(),
            "a spec should see its own file's private functions, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    /// Two files each defining a same-named struct resolve to distinct canonical
    /// keys: the entry file's bare name and the non-entry file's file-qualified
    /// name. Codegen and analysis fetch layouts by these keys, so the two never
    /// collapse to one layout.
    #[test]
    fn same_named_structs_resolve_to_distinct_canonical_keys() {
        let files = [
            (
                vec![],
                "struct Buffer { x: i32; } pub fn main() { let b: Buffer = Buffer { x: 0 }; }",
            ),
            (
                vec!["lib", "buf"],
                "pub struct Buffer { y: i32; z: i32; } \
                 pub fn use_it() { let b: Buffer = Buffer { y: 1, z: 2 }; }",
            ),
        ];
        let ctx = try_type_check_multi_file(&files).expect("should type-check");

        let entry_key = ctx
            .canonical_struct_key("Buffer", &[])
            .expect("entry Buffer resolves");
        let lib_key = ctx
            .canonical_struct_key("Buffer", &["lib".to_string(), "buf".to_string()])
            .expect("lib::buf Buffer resolves");
        assert_eq!(entry_key, "Buffer", "entry-file struct keeps a bare key");
        assert_eq!(lib_key, "lib::buf::Buffer", "non-entry struct is file-qualified");
        assert_ne!(entry_key, lib_key, "same-named structs get distinct keys");

        // Each key fetches the layout of its own file: distinct field counts.
        assert_eq!(ctx.lookup_struct(&entry_key).unwrap().fields.len(), 1);
        assert_eq!(ctx.lookup_struct(&lib_key).unwrap().fields.len(), 2);
    }

    /// A cross-file call to a private function is rejected with a dual-location
    /// diagnostic naming the use site, the definition site, and the defining file.
    #[test]
    fn cross_file_private_function_call_rejected_with_dual_location() {
        let files = [
            (
                vec![],
                "use lib::arith; pub fn main() { let r: i32 = lib::arith::secret(); }",
            ),
            (vec!["lib", "arith"], "fn secret() -> i32 { return 0; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("calling a private cross-file function must fail")
            .to_string();
        assert!(
            msg.contains("cannot access private function `lib::arith::secret`"),
            "error should name the private function at the use site, got: {msg}"
        );
        assert!(
            msg.contains("is defined at") && msg.contains("in file `lib::arith`"),
            "error should point at the definition site and file, got: {msg}"
        );
        assert!(
            msg.contains("add `pub` to export it"),
            "error should suggest the fix, got: {msg}"
        );
    }

    /// A `pub` cross-file function called by absolute path resolves — the public
    /// surface crosses the file boundary once the accessing file imports the
    /// namespace.
    #[test]
    fn cross_file_public_function_call_resolves() {
        let files = [
            (
                vec![],
                "use lib::arith; pub fn main() { let r: i32 = lib::arith::add(1, 2); }",
            ),
            (vec!["lib", "arith"], "pub fn add(a: i32, b: i32) -> i32 { return a + b; }"),
        ];
        assert!(
            try_type_check_multi_file(&files).is_ok(),
            "a public cross-file function should be callable by absolute path"
        );
    }

    /// A `pub` struct's field is accessible cross-file (field rule = struct rule):
    /// because the struct is public and imported, its fields come along.
    #[test]
    fn pub_struct_field_accessible_cross_file() {
        let files = [
            (
                vec![],
                "use lib::geo::{Point}; \
                 pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; }",
            ),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; }",
            ),
        ];
        assert!(
            try_type_check_multi_file(&files).is_ok(),
            "a field of an imported pub struct should be accessible without per-field visibility"
        );
    }

    /// A spec in the entry file must not see another file's private struct: the
    /// `_anywhere` escape hatch no longer leaks private types across files.
    #[test]
    fn spec_cannot_see_another_files_private_struct() {
        let files = [
            (
                vec![],
                "use lib::buf; \
                 spec EntrySpec { fn check() -> i32 { let b: Buffer = Buffer { x: 0 }; return b.x; } } \
                 pub fn main() {}",
            ),
            (vec!["lib", "buf"], "struct Buffer { x: i32; }"),
        ];
        assert!(
            try_type_check_multi_file(&files).is_err(),
            "a spec must not reach another file's private struct by bare name"
        );
    }

    /// An intra-file value cycle between two consts is a hard error naming the
    /// cycle. File-import cycles are allowed; only value cycles are rejected.
    #[test]
    fn const_value_cycle_intra_file_rejected() {
        let files = [(
            vec![],
            "const A: i32 = B; const B: i32 = A; pub fn main() {}",
        )];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a const value cycle must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
        assert!(
            msg.contains('A') && msg.contains('B'),
            "the cycle message should name both members, got: {msg}"
        );
    }

    /// A value cycle that spans two files via absolute paths is rejected. The
    /// import graph is acyclic; the cycle is purely in the values.
    #[test]
    fn const_value_cycle_cross_file_rejected() {
        let files = [
            (
                vec![],
                "const A: i32 = lib::vals::V; pub fn main() {}",
            ),
            (vec!["lib", "vals"], "pub const V: i32 = A;"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a cross-file value cycle must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition across files, got: {msg}"
        );
    }

    /// A three-node value cycle (A -> B -> C -> A) is rejected and the full cycle
    /// is named.
    #[test]
    fn const_value_cycle_three_nodes_rejected() {
        let files = [(
            vec![],
            "const A: i32 = B; const B: i32 = C; const C: i32 = A; pub fn main() {}",
        )];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a three-node value cycle must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
        assert!(
            msg.contains('A') && msg.contains('B') && msg.contains('C'),
            "the cycle message should name all three members, got: {msg}"
        );
    }

    /// An acyclic chain of consts type-checks and yields a dependency-first
    /// definition order for a later phase.
    #[test]
    fn acyclic_const_chain_yields_topological_order() {
        let files = [(
            vec![],
            "const A: i32 = 1; const B: i32 = A; const C: i32 = B; pub fn main() {}",
        )];
        let ctx = try_type_check_multi_file(&files).expect("an acyclic chain should type-check");
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
        assert!(pos("A") < pos("B"), "A must come before B");
        assert!(pos("B") < pos("C"), "B must come before C");
    }

    /// A cross-file type-alias value cycle expressed through item imports must be
    /// rejected. `::` does not parse in type position, so an item import is the
    /// *only* way to write a cross-file type-alias reference; before the
    /// edge-discovery fix this cycle escaped detection entirely (#63).
    #[test]
    fn type_alias_cycle_cross_file_via_item_import_rejected() {
        let files = [
            (vec![], "use lib::t::{B}; pub type A = B;"),
            (vec!["lib", "t"], "use main::{A}; pub type B = A;"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a cross-file type-alias cycle via item import must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
        assert!(
            msg.contains('A') && msg.contains('B'),
            "the cycle message should name both members, got: {msg}"
        );
    }

    /// The confirmed three-file mutually-recursive type-alias cycle (M3 repro):
    /// `main::A = lib::t::B`, `lib::t::B = lib::u::A`, `lib::u::A = lib::t::B`.
    /// Each edge crosses a file boundary only through an item import; the cycle
    /// must be caught at type-check, before codegen.
    #[test]
    fn type_alias_cycle_three_file_via_item_import_rejected() {
        let files = [
            (vec![], "use lib::t::{B}; pub type A = B;"),
            (vec!["lib", "t"], "use lib::u::{A}; pub type B = A;"),
            (vec!["lib", "u"], "use lib::t::{B}; pub type A = B;"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a three-file type-alias cycle via item imports must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
    }

    /// An acyclic cross-file type-alias chain expressed through item imports
    /// (`A = B`, `B = C`, `C = i32`) must type-check: the cycle check follows the
    /// import edges but finds no back-edge, so the chain is accepted.
    #[test]
    fn type_alias_chain_cross_file_via_item_import_accepted() {
        let files = [
            (vec![], "use lib::t::{B}; pub type A = B; pub fn main() {}"),
            (vec!["lib", "t"], "use lib::u::{C}; pub type B = C;"),
            (vec!["lib", "u"], "pub type C = i32;"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_ok(),
            "an acyclic cross-file type-alias chain must type-check, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    /// A const that references itself through a namespace import of its own file
    /// (`use lib::v; const C = v::C;`) closes a degenerate self-edge that is
    /// discoverable only by canonicalizing the namespace-qualified reference
    /// through the import binding. It must be rejected as a circular definition,
    /// exercising the namespace branch of import edge discovery (#63).
    #[test]
    fn const_self_edge_via_namespace_import_rejected() {
        let files = [
            (vec![], "use lib::v; pub fn main() {}"),
            (vec!["lib", "v"], "use lib::v; pub const C: i32 = v::C;"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a self-referential const through a namespace import must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
    }

    /// A type alias that aliases its own imported name (`use other::{X}; type X =
    /// X;`) closes a degenerate self-edge: the local name `X` collides with the
    /// import, resolving to its own node. It must be rejected as a circular
    /// definition.
    #[test]
    fn type_alias_self_edge_via_item_import_rejected() {
        let files = [
            (vec![], "use other::{X}; pub type X = X;"),
            (vec!["other"], "pub type X = i32;"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a self-referential alias through an import must fail")
            .to_string();
        assert!(
            msg.contains("circular definition detected"),
            "error should report a circular definition, got: {msg}"
        );
    }

    /// Two files that import each other but share no definition-value dependency
    /// form a file-import cycle, which is explicitly allowed (#63). The cycle
    /// check must NOT flag it: the edges are import edges, not value edges. Each
    /// file imports a *function* from the other, so the import graph is cyclic
    /// while the const/type-alias value graph stays empty.
    #[test]
    fn file_import_cycle_without_value_dependency_not_flagged() {
        let files = [
            (vec![], "use lib::a; pub fn main() {}"),
            (
                vec!["lib", "a"],
                "use lib::b::{pong}; pub fn ping() -> i32 { return pong(); }",
            ),
            (
                vec!["lib", "b"],
                "use lib::a::{ping}; pub fn pong() -> i32 { return 7; }",
            ),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_ok(),
            "a file-import cycle with no value dependency must not be a CircularDefinition, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    /// A brace-free file import in the entry (`use lib::Point;`) must not leak its
    /// file-namespace binding across the file boundary: inside `lib.inf`, a bare
    /// `Point::new()` is the local struct's associated function, not the sibling
    /// file `lib/Point.inf`. The struct's `new` returns `Point`, matching the
    /// `let p: Point` binding; the leaked file's `new` returns `i32`, which would
    /// surface a type mismatch — so a clean type-check pins the boundary (#63).
    #[test]
    fn entry_file_namespace_import_does_not_leak_into_lib() {
        let files = [
            (
                vec![],
                "use lib; use lib::Point; pub fn main() -> i32 { return lib::build(); }",
            ),
            (
                vec!["lib"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn new() -> Point { return Point { x: 1, y: 2 }; } } \
                 pub fn build() -> i32 { let p: Point = Point::new(); return p.x + p.y; }",
            ),
            (vec!["lib", "Point"], "pub fn new() -> i32 { return 0; }"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_ok(),
            "the entry's `use lib::Point;` must not hijack `lib.inf`'s bare `Point::new()`, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    /// Two item imports of the *same* canonical target under one name — `f`
    /// imported directly from `orig` and again through `proxy`'s `pub use
    /// orig::{f}` re-export — are a benign duplicate, not a collision. Both name
    /// the identical function `orig::f`, so binding it once and resolving the
    /// bare call is correct.
    #[test]
    fn duplicate_item_imports_of_same_target_are_benign() {
        let files = [
            (
                vec![],
                "use orig::{f}; use proxy::{f}; pub fn main() -> i32 { return f(); }",
            ),
            (vec!["orig"], "pub fn f() -> i32 { return 42; }"),
            (vec!["proxy"], "pub use orig::{f};"),
        ];
        let result = try_type_check_multi_file(&files);
        assert!(
            result.is_ok(),
            "two imports of the same canonical target must not collide, got: {:?}",
            result.err().map(|e| e.to_string())
        );
    }

    /// Two item imports binding one name to *different* canonical targets — `f`
    /// from `one` and a different `f` from `two` — are a genuine clash and must be
    /// rejected; the benign-duplicate rule applies only to identical targets.
    #[test]
    fn duplicate_item_imports_of_different_targets_collide() {
        let files = [
            (
                vec![],
                "use one::{f}; use two::{f}; pub fn main() -> i32 { return f(); }",
            ),
            (vec!["one"], "pub fn f() -> i32 { return 1; }"),
            (vec!["two"], "pub fn f() -> i32 { return 2; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("two different targets under one name must collide")
            .to_string();
        assert!(
            msg.contains("collides with another import"),
            "different-target name clash must still be reported, got: {msg}"
        );
    }

    /// An item import binding a name equal to a builtin type is rejected, and the
    /// rejection is identical whether the import sits in the entry file or a
    /// non-entry file. Builtins live only in the entry (root) scope, so without a
    /// uniform check the entry rejects while a non-entry file silently lets the
    /// builtin shadow the imported struct.
    #[test]
    fn import_of_builtin_type_name_rejected_in_entry_file() {
        let files = [
            (vec![], "use lib::types::{string}; pub fn main() {}"),
            (vec!["lib", "types"], "pub struct string { v: i32; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("importing a builtin-named type must be rejected")
            .to_string();
        assert!(
            msg.contains("collides with a builtin type"),
            "entry-file builtin-named import must be rejected, got: {msg}"
        );
    }

    /// Companion to [`import_of_builtin_type_name_rejected_in_entry_file`]: the
    /// byte-identical directive in a *non-entry* file is rejected with the same
    /// message, so entry and non-entry files behave consistently.
    #[test]
    fn import_of_builtin_type_name_rejected_in_non_entry_file() {
        let files = [
            (vec![], "use lib::mid; pub fn main() -> i32 { return lib::mid::go(); }"),
            (
                vec!["lib", "mid"],
                "use lib::types::{string}; \
                 pub fn go() -> i32 { let s: string = string { v: 9 }; return s.v; }",
            ),
            (vec!["lib", "types"], "pub struct string { v: i32; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("importing a builtin-named type must be rejected in a non-entry file")
            .to_string();
        assert!(
            msg.contains("collides with a builtin type"),
            "non-entry-file builtin-named import must be rejected the same way, got: {msg}"
        );
    }

    /// A method call on a re-export-qualified struct value whose method genuinely
    /// does not exist produces the *accurate* "method not found" diagnostic — the
    /// canonical-key method dispatch reports the real absence rather than a
    /// spurious miss or silent acceptance. The struct does define `sum`, but not
    /// the called `nope`.
    #[test]
    fn reexport_qualified_value_missing_method_reports_accurately() {
        let files = [
            (
                vec![],
                "use math; pub fn run() -> i32 { \
                 let p: math::geo::Point = math::geo::Point { x: 1, y: 2 }; return p.nope(); }",
            ),
            (vec!["math"], "pub use lib::geo;"),
            (
                vec!["lib", "geo"],
                "pub struct Point { x: i32; y: i32; \
                 pub fn sum(self) -> i32 { return self.x + self.y; } }",
            ),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("calling a non-existent method must fail")
            .to_string();
        assert!(
            msg.contains("method `nope` not found on type `Point`"),
            "a genuinely-absent method must be reported accurately, got: {msg}"
        );
    }

    /// A cyclic, unresolvable item re-export (`main` imports `lib::a::{deep}`,
    /// `a` re-exports from `b`, `b` re-exports from `a`) reports the
    /// `deep`-not-found-in-`lib::a` diagnostic exactly once, even though two
    /// distinct import sites (main's import and `b`'s `pub use`) both fail to find
    /// `deep` in `lib::a`. Deduping by `(item, file)` collapses the identical text.
    #[test]
    fn cyclic_unresolvable_reexport_reports_each_target_once() {
        let files = [
            (vec![], "use lib::a::{deep}; pub fn run() -> i32 { return deep(); }"),
            (vec!["lib", "a"], "pub use lib::b::{deep};"),
            (vec!["lib", "b"], "pub use lib::a::{deep};"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a cyclic unresolvable re-export must fail")
            .to_string();
        let a_count = msg.matches("item `deep` not found in file `lib::a`").count();
        assert_eq!(
            a_count, 1,
            "the `deep`-not-found-in-`lib::a` diagnostic must appear exactly once, got {a_count} in: {msg}"
        );
    }

    /// An unresolvable re-export raised by a *non-entry* file carries that file's
    /// `::`-joined label (e.g. `lib::a:`), not a bare `line:col`, matching every
    /// other diagnostic produced in that file. Here `lib::a`'s `pub use
    /// lib::b::{x}` cannot find `x` in `lib::b`, so its diagnostic is labeled
    /// `lib::a`.
    #[test]
    fn cyclic_unresolvable_reexport_carries_importing_file_label() {
        let files = [
            (vec![], "use lib::a::{x}; pub fn run() -> i32 { return x(); }"),
            (vec!["lib", "a"], "pub use lib::b::{x};"),
            (vec!["lib", "b"], "pub use lib::a::{x};"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a cyclic unresolvable re-export must fail")
            .to_string();
        assert!(
            msg.contains("lib::a:") && msg.contains("item `x` not found in file `lib::b`"),
            "a non-entry file's unresolvable re-export must carry its file label, got: {msg}"
        );
    }

    /// An unresolvable *file* import (a Plain `use` naming a missing namespace)
    /// raised by a non-entry file also carries that file's label, exercising the
    /// file-import diagnostic channel rather than the item-import one.
    #[test]
    fn unresolvable_file_import_in_non_entry_file_carries_label() {
        let files = [
            (vec![], "use lib::a; pub fn run() -> i32 { return lib::a::go(); }"),
            (vec!["lib", "a"], "use lib::missing; pub fn go() -> i32 { return 0; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("an unresolvable file import must fail")
            .to_string();
        assert!(
            msg.contains("lib::a:"),
            "a non-entry file's unresolvable file import must carry its file label, got: {msg}"
        );
    }

    /// Two different imported files each call an undefined `missing()`. The
    /// undefined-function diagnostic is per call site, so BOTH files must report
    /// it: a name-only dedup key would swallow the second file's error (leaving
    /// only a downstream type-mismatch there) and misdirect the user.
    #[test]
    fn undefined_function_in_two_files_reports_both() {
        let files = [
            (
                vec![],
                "use lib::a; use lib::b; \
                 pub fn main() -> i32 { return lib::a::fa() + lib::b::fb(); }",
            ),
            (vec!["lib", "a"], "pub fn fa() -> i32 { return missing(); }"),
            (vec!["lib", "b"], "pub fn fb() -> i32 { return missing(); }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("two undefined-function calls must fail type check")
            .to_string();
        assert!(
            msg.contains("lib::a:") && msg.contains("call to undefined function `missing`"),
            "the first file's undefined-function error must be reported, got: {msg}"
        );
        // The second file's error is the one the old name-only dedup dropped.
        assert!(
            msg.contains("lib::b:")
                && msg
                    .matches("call to undefined function `missing`")
                    .count()
                    >= 2,
            "both files' undefined-function errors must be reported, got: {msg}"
        );
    }

    /// A single file calling the same undefined function twice still reports it
    /// once: the registration and inference passes both visit the call, and the
    /// file-aware dedup key collapses the same-site repeat. This guards against
    /// the file-folding fix accidentally splitting a genuine single-site
    /// duplicate.
    #[test]
    fn undefined_function_same_site_still_dedups_once() {
        let files = [(
            vec![],
            "pub fn main() -> i32 { \
             let a: i32 = missing(); let b: i32 = missing(); return a + b; }",
        )];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("an undefined-function call must fail type check")
            .to_string();
        let count = msg.matches("call to undefined function `missing`").count();
        assert_eq!(
            count, 1,
            "a same-site repeated undefined function must report exactly once, got {count} in: {msg}"
        );
    }

    /// Two different files each directly import the same missing item from the same
    /// target (`use shared::data::{Missing};` in both `lib1::a` and `lib2::b`).
    /// The item-not-found diagnostic is per importing file, so BOTH must report —
    /// a target-only dedup key would swallow the second importer's error. The two
    /// distinct importing-file labels are what keep them apart.
    #[test]
    fn same_missing_item_imported_by_two_files_reports_both() {
        let files = [
            (vec![], "use lib1::a; use lib2::b; pub fn main() -> i32 { return 0; }"),
            (vec!["lib1", "a"], "use shared::data::{Missing}; pub fn fa() -> i32 { return 1; }"),
            (vec!["lib2", "b"], "use shared::data::{Missing}; pub fn fb() -> i32 { return 2; }"),
            (vec!["shared", "data"], "pub fn present() -> i32 { return 0; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("two missing item imports must fail type check")
            .to_string();
        assert!(
            msg.contains("lib1::a:") && msg.contains("lib2::b:"),
            "both importers' labels must appear, got: {msg}"
        );
        let count = msg
            .matches("item `Missing` not found in file `shared::data`")
            .count();
        assert_eq!(
            count, 2,
            "each of the two importers must report the missing item, got {count} in: {msg}"
        );
    }

    /// A single importer naming the same missing item twice in one `use` still
    /// reports it once: the importer-keyed dedup collapses the same file's repeat,
    /// so the two-importers fix does not split a genuine single-importer duplicate.
    #[test]
    fn same_missing_item_imported_twice_by_one_file_dedups_once() {
        let files = [
            (vec![], "use lib::a; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "a"],
                "use shared::data::{Missing}; use shared::data::{Missing}; \
                 pub fn fa() -> i32 { return 1; }",
            ),
            (vec!["shared", "data"], "pub fn present() -> i32 { return 0; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("a missing item import must fail type check")
            .to_string();
        let count = msg
            .matches("item `Missing` not found in file `shared::data`")
            .count();
        assert_eq!(
            count, 1,
            "one importer naming the same missing item twice must report once, got {count} in: {msg}"
        );
    }

    /// An import collision reported in a NON-entry file carries that file's label
    /// (`lib::mid:`), consistent with every other diagnostic in the same file. The
    /// collision is reported outside the per-file inference loop (at the root), so
    /// it must be stamped with the importing scope's own file rather than rendered
    /// bare.
    #[test]
    fn non_entry_import_collision_is_file_labeled() {
        let files = [
            (vec![], "use lib::mid; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "mid"],
                "pub use lib::thing::Thing; struct Thing { x: i32; } pub fn use_it() -> i32 { return 0; }",
            ),
            (vec!["lib", "thing"], "pub struct Thing { y: i32; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("an import colliding with a local definition must fail")
            .to_string();
        assert!(
            msg.contains("imported name `Thing` collides"),
            "the collision must be reported, got: {msg}"
        );
        assert!(
            msg.contains("lib::mid:") && msg.contains("imported name `Thing` collides"),
            "a non-entry import collision must carry its file label, got: {msg}"
        );
    }

    /// An import collision in the ENTRY file stays bare (no file prefix), matching
    /// every other entry-file diagnostic — the user named the entry, so labelling
    /// it would add only noise.
    #[test]
    fn entry_import_collision_stays_bare() {
        let files = [
            (
                vec![],
                "use lib::thing::Thing; struct Thing { x: i32; } pub fn main() -> i32 { return 0; }",
            ),
            (vec!["lib", "thing"], "pub struct Thing { y: i32; }"),
        ];
        let msg = try_type_check_multi_file(&files)
            .err()
            .expect("an entry import colliding with a local definition must fail")
            .to_string();
        assert!(
            msg.contains("imported name `Thing` collides"),
            "the collision must be reported, got: {msg}"
        );
        // The entry collision must not be prefixed by any non-entry file label.
        assert!(
            !msg.contains("lib::thing:") && !msg.contains("::Thing:"),
            "an entry-file import collision must stay bare, got: {msg}"
        );
    }
}
