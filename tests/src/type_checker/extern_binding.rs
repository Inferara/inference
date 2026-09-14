//! Tests for the `use … from` binding pass across files.
//!
//! A `use { f } from m;` clause names fields of a logical module and binds the
//! `external fn` declarations of **its own file**. That scope is what these
//! pin: a clause must not reach a sibling's declaration, a sibling's
//! declaration must not intercept a clause meant for a local one, and the
//! per-file consistency rule (one name, one binding) must stay a per-file rule
//! rather than a program-wide one.
//!
//! One rule about these clauses is deliberately program-wide, and the
//! cross-file half of it is pinned here: an import module has one provider. A
//! linked clause in one file and a `host::` one in another that reach the same
//! module are refused even though each file is internally consistent, because
//! the linker offers every export of a module it merged to satisfy any import
//! under that module string, so downstream the two cannot be told apart.
//!
//! The bindings are read back by declaration [`DefId`], because that is what a
//! binding attaches to. Two files may declare `external fn scale` and bind them
//! to different modules; asked by name the program has two answers, and only
//! the declaration says which is which.
#[cfg(test)]
mod tests {
    use crate::utils::{build_ast, build_multi_file_ast, try_type_check_multi_file};
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

    /// A malformed `from host::…;` clause is labelled with the file that wrote
    /// it, not with the entry document.
    ///
    /// The binding pass runs once at the root and walks every file's
    /// directives, so a diagnostic pushed without its owner's label reads as
    /// belonging to the entry file — and sends the reader to a `use` clause
    /// that is not there.
    ///
    /// Read structurally rather than out of the aggregated message: the label
    /// and the message are two substrings of one rendered blob, and asserting
    /// them separately would pass with the host diagnostic left unlabelled the
    /// moment any second labelled diagnostic joins it.
    #[test]
    fn a_malformed_host_clause_in_an_imported_file_names_that_file() {
        let outcome = check_with_diagnostics(build_multi_file_ast(&[
            (
                vec![],
                "use sib;\npub fn go(x: i32) -> i32 { return sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from host::vendor::v2;\npub \
                 fn via(x: i32) -> i32 { return scale(x); }",
            ),
        ]));
        assert_eq!(outcome.errors.len(), 1, "got: {:?}", outcome.errors);
        assert_eq!(
            outcome.errors[0].file_label.as_deref(),
            Some("sib"),
            "the offending file must be named"
        );
        assert!(
            matches!(
                outcome.errors[0].error,
                TypeCheckError::HostImportModuleNested { .. }
            ),
            "got: {:?}",
            outcome.errors[0].error
        );
    }

    /// One import module bound as a linked module by one file and as a host
    /// import by another is refused, naming both files.
    ///
    /// The per-file ambiguity rule cannot see this: each file holds exactly one
    /// clause for `scale` and is internally consistent. Code generation folds
    /// the two declarations onto a single `(import "vendor" "scale" …)` entry,
    /// which the linker then either satisfies and strips or leaves for an
    /// embedder — one answer for two files that asked for opposite ones.
    #[test]
    fn a_linked_and_a_host_binding_of_one_import_across_files_are_refused() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from host::vendor;\npub fn \
                 via(x: i32) -> i32 { return scale(x); }",
            ),
        ]);
        assert!(
            rendered.contains(
                "the import module `vendor` is bound to two providers: `scale` binds it as a \
                 linked module in the entry file, and `scale` binds it as a host import in `sib`"
            ),
            "both files must be named, and named by which clause each wrote, got: {rendered}"
        );
        assert!(
            rendered.contains("`from vendor;` to link the module, `from host::vendor;` to have \
                               the embedder supply it"),
            "the remedy has to spell both clauses, got: {rendered}"
        );
    }

    /// The same disagreement written the other way round still names each file
    /// by the clause it wrote.
    ///
    /// The two file names come from a two-arm match on the *second* binding's
    /// kind, and the fixture above reaches one arm only — it always puts the
    /// linked clause in the file walked first. An implementation that wrote the
    /// other arm as a copy of the first would ship green and print the files
    /// swapped, sending the reader to edit the file that wrote the clause the
    /// message says is elsewhere.
    #[test]
    fn the_provider_conflict_names_each_file_by_its_own_clause_in_either_order() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 host::vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from vendor;\npub fn \
                 via(x: i32) -> i32 { return scale(x); }",
            ),
        ]);
        assert!(
            rendered.contains(
                "the import module `vendor` is bound to two providers: `scale` binds it as a \
                 linked module in `sib`, and `scale` binds it as a host import in the entry file"
            ),
            "the roles must follow the clauses, not the walk order, got: {rendered}"
        );
    }

    /// The providers of one import module disagree even when no single field is
    /// bound twice: the entry file links `vendor` for `scale`, the sibling names
    /// `vendor` as a host import for `shift`.
    ///
    /// Keyed on the `(module, field)` pair this program would pass, and reach a
    /// linker whose candidate set for an import is every external bound under
    /// the module string — so a merged `vendor.wasm` that happens to export
    /// `shift` would satisfy the import the sibling's author left for an
    /// embedder, substituting one provider for the other with no diagnostic.
    #[test]
    fn two_fields_of_one_module_may_not_disagree_about_its_provider() {
        let rendered = errors(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn shift(a: i32) -> i32;\nuse { shift } from host::vendor;\npub fn \
                 via(x: i32) -> i32 { return shift(x); }",
            ),
        ]);
        assert!(
            rendered.contains(
                "the import module `vendor` is bound to two providers: `scale` binds it as a \
                 linked module in the entry file, and `shift` binds it as a host import in `sib`"
            ),
            "each half of the conflict must name its own extern, got: {rendered}"
        );
    }

    /// One conflicting import module earns one diagnostic, however many
    /// declarations the disagreeing side binds it through.
    ///
    /// The rule's unit is the module, so the second side's declaration count is
    /// not something the reader should be able to read off the output: two
    /// fields imported by one `host::vendor` clause are one mistake in one
    /// clause, and repeating the message per declaration would print the same
    /// pair of clauses twice.
    ///
    /// The location is checked in the same fixture because it is the same
    /// claim about the same thing: the diagnostic is about a `use … from`
    /// clause, so it is reported once *and* at that clause — line 3 of the
    /// sibling — rather than at either `external fn` declaration above it,
    /// which is the text the remedy never mentions.
    #[test]
    fn one_conflicting_module_is_reported_once_and_at_its_clause() {
        let outcome = check_with_diagnostics(build_multi_file_ast(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nexternal fn shift(a: i32) -> \
                 i32;\nuse { scale, shift } from host::vendor;\npub fn via(x: i32) -> i32 \
                 { return scale(x) + shift(x); }",
            ),
        ]));
        let conflicts: Vec<_> = outcome
            .errors
            .iter()
            .filter(|reported| {
                matches!(
                    reported.error,
                    TypeCheckError::ConflictingExternProvider { .. }
                )
            })
            .collect();
        assert_eq!(
            conflicts.len(),
            1,
            "one module, one diagnostic — got: {:?}",
            outcome.errors
        );
        assert!(
            conflicts[0]
                .error
                .to_string()
                .contains("the import module `vendor` is bound to two providers"),
            "got: {}",
            conflicts[0].error
        );
        assert_eq!(
            conflicts[0].error.location().start_line,
            3,
            "the diagnostic belongs on the `use … from host::vendor;` clause the remedy asks \
             the reader to rewrite, not on an `external fn` declaration: {}",
            conflicts[0].error
        );
    }

    /// Two files that link one module through different fields agree, and are
    /// left alone. The rule is about a *disagreement*; keyed on the module
    /// alone without comparing the two kinds it would reject every ordinary
    /// program that imports two functions from one library across two files.
    #[test]
    fn two_files_may_link_one_module_through_different_fields() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn shift(a: i32) -> i32;\nuse { shift } from vendor;\npub fn \
                 via(x: i32) -> i32 { return shift(x); }",
            ),
        ])
        .expect("two files may split one linked module's fields between them");
        assert_eq!(bound_module(&ctx, ENTRY, "scale").as_deref(), Some("vendor"));
        assert_eq!(
            bound_module(&ctx, &module(&["sib"]), "shift").as_deref(),
            Some("vendor")
        );
    }

    /// Two files agreeing on a host binding is not a conflict. The refusal is
    /// about a *disagreement*, so a rule keyed on the pair alone — rather than
    /// on the pair's two kinds differing — would reject the ordinary case of
    /// two files calling one embedder-supplied function.
    #[test]
    fn two_files_may_both_bind_one_host_import() {
        let ctx = try_type_check_multi_file(&[
            (
                vec![],
                "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from \
                 host::vendor;\npub fn go(x: i32) -> i32 { return scale(x) + sib::via(x); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from host::vendor;\npub fn \
                 via(x: i32) -> i32 { return scale(x); }",
            ),
        ])
        .expect("two files may agree on one host import");
        assert_eq!(bound_module(&ctx, ENTRY, "scale").as_deref(), Some("vendor"));
        assert_eq!(
            bound_module(&ctx, &module(&["sib"]), "scale").as_deref(),
            Some("vendor")
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
