//! Tests for the `use … from` binding pass across files.
//!
//! A `use { f } from m;` clause names fields of a logical module and binds the
//! `external fn` declarations of **its own file**. That scope is what these
//! pin: a clause must not reach a sibling's declaration, a sibling's
//! declaration must not intercept a clause meant for a local one, and the
//! per-file consistency rule (one name, one module) must stay a per-file rule
//! rather than a program-wide one.
//!
//! The bindings are read back by declaration [`DefId`], because that is what a
//! binding attaches to. Two files may declare `external fn scale` and bind them
//! to different modules; asked by name the program has two answers, and only
//! the declaration says which is which.
#[cfg(test)]
mod tests {
    use crate::utils::{build_ast, try_type_check_multi_file};
    use inference_ast::ids::DefId;
    use inference_ast::nodes::Def;
    use inference_type_checker::check_with_diagnostics;
    use inference_type_checker::errors::TypeCheckError;
    use inference_type_checker::typed_context::TypedContext;

    /// The entry file's module path.
    const ENTRY: &[String] = &[];

    fn module(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| (*s).to_string()).collect()
    }

    /// The [`DefId`] of the top-level `external fn name` declared by the file at
    /// `module_path`.
    ///
    /// Walks the arena rather than asking the extern index, so an assertion
    /// about a binding never leans on the resolver that produced it.
    fn declaration(ctx: &TypedContext, module_path: &[String], name: &str) -> DefId {
        let arena = ctx.arena();
        let file = ctx
            .source_files()
            .find(|sf| sf.module_path == module_path)
            .unwrap_or_else(|| panic!("the fixture has no file at {module_path:?}"));
        file.defs
            .iter()
            .copied()
            .find(|&def_id| match &arena[def_id].kind {
                Def::ExternFunction {
                    name: decl_name, ..
                } => arena[*decl_name].name == name,
                _ => false,
            })
            .unwrap_or_else(|| {
                panic!("the file at {module_path:?} declares no `external fn {name}`")
            })
    }

    /// The logical module the declaration of `name` in `module_path` is bound
    /// to, or `None` when that declaration is unbound.
    fn bound_module(ctx: &TypedContext, module_path: &[String], name: &str) -> Option<String> {
        let decl = declaration(ctx, module_path, name);
        ctx.extern_origin_by_decl(decl).map(|o| o.logical_module)
    }

    fn errors(files: &[(Vec<&str>, &str)]) -> String {
        match try_type_check_multi_file(files) {
            Ok(_) => panic!("the fixture should be rejected"),
            Err(e) => e.to_string(),
        }
    }

    /// A `use … from` clause binds a declaration in its own file only.
    ///
    /// The entry file names `scale` with nothing of that name declared in it;
    /// that the *sibling* declares one is irrelevant, and treating it as a
    /// candidate silently attaches the entry's clause to a declaration the entry
    /// file cannot even name.
    #[test]
    fn a_use_clause_does_not_reach_a_siblings_declaration() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\nuse { scale } from libA;\npub fn go(x: i32) -> i32 { return \
                 sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\npub fn via(x: i32) -> i32 { return scale(x); }",
            ),
        ]);
        assert!(
            rendered.contains("imports `scale` from module `libA`")
                && rendered.contains("no `external fn scale` is declared"),
            "binding across a file boundary must be a dangling import, got: {rendered}"
        );
    }

    /// A sibling's unrelated declaration does not intercept a binding.
    ///
    /// The sibling declares `scale` and never binds or calls it. The entry file
    /// declares and binds its own. A program-wide name table holds one entry, so
    /// whichever file it happens to keep decides which declaration the entry's
    /// clause attaches to — leaving the entry's own declaration unbound and its
    /// working call rejected.
    #[test]
    fn an_unrelated_sibling_declaration_does_not_intercept_a_binding() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn \
                 go(x: i32) -> i32 { return scale(x) + sib::helper(); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\npub fn helper() -> i32 { return 7; }",
            ),
        ])
        .expect("an unbound sibling declaration must not reject the entry's binding");
        assert_eq!(bound_module(&ctx, ENTRY, "scale").as_deref(), Some("libA"));
        assert_eq!(
            bound_module(&ctx, &module(&["sib"]), "scale"),
            None,
            "the sibling declares `scale` without binding it, so it stays unbound"
        );
    }

    /// Two files may each declare `external fn scale` and bind it to a
    /// *different* module. The declarations are distinct, so the two bindings
    /// are not a conflict — the linker names the merged roots per module.
    #[test]
    fn two_files_may_bind_one_name_to_different_modules() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn \
                 from_a(x: i32) -> i32 { return scale(x); }\npub fn from_b(x: i32) -> i32 { \
                 return sib::via_b(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from libB;\npub fn via_b(x: \
                 i32) -> i32 { return scale(x); }",
            ),
        ])
        .expect("two files binding one name to two modules is legal");
        assert_eq!(bound_module(&ctx, ENTRY, "scale").as_deref(), Some("libA"));
        assert_eq!(
            bound_module(&ctx, &module(&["sib"]), "scale").as_deref(),
            Some("libB")
        );
    }

    /// The commonest shape of all: two files both use the same library
    /// function. Each declares its own extern and binds it to the same module.
    #[test]
    fn two_files_may_bind_one_name_to_the_same_module() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn \
                 a(x: i32) -> i32 { return scale(x); }\npub fn b(x: i32) -> i32 { return \
                 sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn via(x: i32) \
                 -> i32 { return scale(x); }",
            ),
        ])
        .expect("two files binding one name to the same module is legal");
        assert_eq!(bound_module(&ctx, ENTRY, "scale").as_deref(), Some("libA"));
        assert_eq!(
            bound_module(&ctx, &module(&["sib"]), "scale").as_deref(),
            Some("libA")
        );
    }

    /// The per-file consistency rule survives the narrowing: one file naming one
    /// field from two modules is still ambiguous, and its declaration is left
    /// unbound rather than resolved to an arbitrary one of the two.
    #[test]
    fn one_file_binding_one_name_to_two_modules_is_ambiguous() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from libA;\nuse { \
                 scale } from libB;\npub fn go(x: i32) -> i32 { return scale(x) + sib::helper(); }",
            ),
            (vec!["sib"], "pub fn helper() -> i32 { return 7; }"),
        ]);
        assert!(
            rendered.contains("external function `scale` is bound to multiple modules")
                && rendered.contains("`libA`")
                && rendered.contains("`libB`"),
            "a within-file conflict must still be rejected, got: {rendered}"
        );
    }

    /// An ambiguity inside an imported file names that file. The clause, the
    /// declaration and the diagnostic all belong to one file now, so the label
    /// cannot drift to whichever file the scan reached first.
    #[test]
    fn an_ambiguity_in_an_imported_file_names_that_file() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\npub fn go(x: i32) -> i32 { return sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from libA;\nuse { scale } from \
                 libB;\npub fn via(x: i32) -> i32 { return scale(x); }",
            ),
        ]);
        assert!(
            rendered.contains("sib:") && rendered.contains("bound to multiple modules"),
            "the offending file must be named, got: {rendered}"
        );
    }

    /// Binding diagnostics are reported in source order.
    ///
    /// They are produced by draining a name-keyed map, so without an explicit
    /// ordering step four dangling imports written on consecutive lines report
    /// in hash order — a list the reader cannot follow back into the file.
    #[test]
    fn dangling_import_diagnostics_are_reported_in_source_order() {
        let source = "use { alpha } from libA;\nuse { beta } from libA;\nuse { gamma } from \
                      libA;\nuse { delta } from libA;\nfn main() -> i32 { return 0; }";
        let outcome = check_with_diagnostics(build_ast(source.to_string()));
        let reported: Vec<(u32, String)> = outcome
            .errors
            .iter()
            .filter_map(|d| match &d.error {
                TypeCheckError::ExternImportNotDeclared { name, location, .. } => {
                    Some((location.start_line, name.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            reported,
            vec![
                (1, "alpha".to_string()),
                (2, "beta".to_string()),
                (3, "gamma".to_string()),
                (4, "delta".to_string()),
            ],
            "dangling imports must report in the order they are written"
        );
    }
}
