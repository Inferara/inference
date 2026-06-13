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
    /// through the file scope tree without any import.
    #[test]
    fn absolute_path_to_public_function_resolves() {
        let files = [
            (
                vec![],
                "pub fn main() { let r: i32 = lib::arith::add(1, 2); }",
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
                "pub fn main() { let r: i32 = lib::arith::secret(); }",
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
    /// surface crosses the file boundary.
    #[test]
    fn cross_file_public_function_call_resolves() {
        let files = [
            (
                vec![],
                "pub fn main() { let r: i32 = lib::arith::add(1, 2); }",
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
}
