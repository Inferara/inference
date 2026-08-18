//! Tests for the shared `external fn` index: which declaration a bare name
//! means at a given point in the program.
//!
//! An `external fn`'s identity is the declaration a use site names, not its
//! bare name — two declarations may share a name and agree on nothing else.
//! Every consumer (analysis rule A024, the specification translator) resolves
//! through this one index, so these tests pin the resolution all of them get.
#[cfg(test)]
mod tests {
    use crate::utils::{build_ast, try_type_check_multi_file};
    use inference_ast::ids::DefId;
    use inference_ast::nodes::Def;
    use inference_type_checker::TypeCheckerBuilder;
    use inference_type_checker::typed_context::TypedContext;

    /// The entry file's module path.
    const ENTRY: &[String] = &[];

    fn type_check(source: &str) -> TypedContext {
        TypeCheckerBuilder::build_typed_context(build_ast(source.to_string()))
            .expect("test source should type-check")
            .typed_context()
    }

    fn module(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| (*s).to_string()).collect()
    }

    /// The [`DefId`] of the `external fn` named `name` declared directly in the
    /// given scope — the file at `module_path`, or the `spec` block of that name
    /// within it.
    ///
    /// Found by walking the arena, so an assertion comparing against it never
    /// leans on the index it is checking.
    fn declaration(
        ctx: &TypedContext,
        module_path: &[String],
        spec: Option<&str>,
        name: &str,
    ) -> DefId {
        let arena = ctx.arena();
        let file = ctx
            .source_files()
            .find(|sf| sf.module_path == module_path)
            .unwrap_or_else(|| panic!("the fixture has no file at {module_path:?}"));
        let defs: &[DefId] = match spec {
            None => &file.defs,
            Some(spec) => file
                .defs
                .iter()
                .find_map(|&def_id| match &arena[def_id].kind {
                    Def::Spec {
                        name: spec_name,
                        defs,
                        ..
                    } if arena[*spec_name].name == spec => Some(defs.as_slice()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the fixture has no `spec {spec}`")),
        };
        defs.iter()
            .copied()
            .find(|&def_id| match &arena[def_id].kind {
                Def::ExternFunction {
                    name: decl_name, ..
                } => arena[*decl_name].name == name,
                _ => false,
            })
            .unwrap_or_else(|| panic!("the fixture declares no `external fn {name}` in that scope"))
    }

    /// A top-level declaration resolves from the file's top level, and both
    /// entry points agree there: `lookup` with no enclosing spec is
    /// `lookup_top_level`.
    #[test]
    fn a_top_level_declaration_resolves_at_the_top_level() {
        let ctx = type_check("external fn print(val: i32) -> (); fn main() { }");
        let decl = declaration(&ctx, ENTRY, None, "print");
        let index = ctx.extern_index();
        assert_eq!(index.lookup(ENTRY, None, "print"), Some(decl));
        assert_eq!(index.lookup_top_level(ENTRY, "print"), Some(decl));
    }

    /// A name that is a plain function, not an extern, resolves to nothing —
    /// the index answers "which extern", never "which function".
    #[test]
    fn a_plain_function_is_not_an_extern() {
        let ctx = type_check("fn helper(x: i32) -> i32 { return x; }");
        assert_eq!(ctx.extern_index().lookup(ENTRY, None, "helper"), None);
    }

    /// Two same-named declarations, one per scope: a call inside the spec means
    /// the spec's, a call at the top level means the file's. This is the whole
    /// reason resolution cannot be name-keyed — the two differ in arity, type
    /// and binding.
    #[test]
    fn a_spec_declaration_shadows_a_same_named_top_level_one() {
        let ctx = type_check(
            "external fn sort(a: i32) -> i32; spec Ms { external fn sort(a: i64, b: i64) -> i64; }",
        );
        let top_level = declaration(&ctx, ENTRY, None, "sort");
        let spec_inner = declaration(&ctx, ENTRY, Some("Ms"), "sort");
        assert_ne!(
            top_level, spec_inner,
            "the fixture must declare two distinct `sort`s, or this test proves nothing"
        );
        let index = ctx.extern_index();
        assert_eq!(index.lookup(ENTRY, Some("Ms"), "sort"), Some(spec_inner));
        assert_eq!(index.lookup(ENTRY, None, "sort"), Some(top_level));
    }

    /// `lookup_top_level` never descends into a `spec`. A `use … from` clause is
    /// file-scoped and binds top-level declarations only, so a spec-inner
    /// declaration must stay invisible to it — otherwise a top-level `use` would
    /// silently bind an extern its own scope never declared.
    #[test]
    fn lookup_top_level_never_reaches_into_a_spec() {
        let ctx = type_check("spec Ms { external fn sort(a: i32) -> i32; }");
        let spec_inner = declaration(&ctx, ENTRY, Some("Ms"), "sort");
        let index = ctx.extern_index();
        assert_eq!(
            index.lookup(ENTRY, Some("Ms"), "sort"),
            Some(spec_inner),
            "the declaration must be reachable from inside its own spec"
        );
        assert_eq!(index.lookup_top_level(ENTRY, "sort"), None);
        assert_eq!(index.lookup(ENTRY, None, "sort"), None);
    }

    /// A spec that declares no extern of its own falls back to the file's top
    /// level: the walk is innermost-first, not innermost-only.
    #[test]
    fn a_spec_falls_back_to_the_file_top_level() {
        let ctx = type_check(
            "external fn sort(a: i32) -> i32; spec Ms { fn run(x: i32) -> i32 { return x; } }",
        );
        let top_level = declaration(&ctx, ENTRY, None, "sort");
        assert_eq!(
            ctx.extern_index().lookup(ENTRY, Some("Ms"), "sort"),
            Some(top_level)
        );
    }

    /// Sibling specs are isolated: one spec's declaration is not in scope in
    /// another.
    #[test]
    fn sibling_specs_do_not_see_each_others_declarations() {
        let ctx = type_check(
            "spec A { external fn f(x: i32) -> i32; } spec B { fn g(x: i32) -> i32 { return x; } }",
        );
        let in_a = declaration(&ctx, ENTRY, Some("A"), "f");
        let index = ctx.extern_index();
        assert_eq!(index.lookup(ENTRY, Some("A"), "f"), Some(in_a));
        assert_eq!(index.lookup(ENTRY, Some("B"), "f"), None);
    }

    /// Two files of one program may each declare an `external fn` of the same
    /// name. They are distinct declarations and each file resolves the name to
    /// its own — a program-wide map keyed by the bare name could hold only one
    /// of the two.
    #[test]
    fn same_named_declarations_in_two_files_stay_distinct() {
        let files = [
            (
                vec![],
                "use lib; external fn scale(x: i32) -> i32; pub fn main() -> i32 { return lib::helper(1); }",
            ),
            (
                vec!["lib"],
                "external fn scale(x: i32) -> i32; pub fn helper(x: i32) -> i32 { return x; }",
            ),
        ];
        let ctx =
            try_type_check_multi_file(&files).expect("the two-file fixture should type-check");
        let lib = module(&["lib"]);
        let in_entry = declaration(&ctx, ENTRY, None, "scale");
        let in_lib = declaration(&ctx, &lib, None, "scale");
        assert_ne!(
            in_entry, in_lib,
            "the fixture must declare two distinct `scale`s, or this test proves nothing"
        );
        let index = ctx.extern_index();
        assert_eq!(index.lookup_top_level(ENTRY, "scale"), Some(in_entry));
        assert_eq!(index.lookup_top_level(&lib, "scale"), Some(in_lib));
    }

    /// A declaration is visible only in the file that makes it: an extern
    /// declared in the entry file does not resolve from an imported one, and
    /// vice versa.
    #[test]
    fn a_declaration_is_invisible_from_another_file() {
        let files = [
            (
                vec![],
                "use lib; external fn scale(x: i32) -> i32; pub fn main() -> i32 { return lib::helper(1); }",
            ),
            (
                vec!["lib"],
                "external fn shift(x: i32) -> i32; pub fn helper(x: i32) -> i32 { return x; }",
            ),
        ];
        let ctx =
            try_type_check_multi_file(&files).expect("the two-file fixture should type-check");
        let index = ctx.extern_index();
        let lib = module(&["lib"]);
        assert!(index.lookup_top_level(ENTRY, "scale").is_some());
        assert!(index.lookup_top_level(&lib, "shift").is_some());
        assert_eq!(index.lookup_top_level(&lib, "scale"), None);
        assert_eq!(index.lookup_top_level(ENTRY, "shift"), None);
    }
}
