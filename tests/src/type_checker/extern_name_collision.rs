//! A top-level `external fn` and a top-level function may not share a name.
//!
//! The rejection is not a resolution failure. A bare call resolves in the scope
//! it is written in, so such a program has one callee per call site and would
//! run correctly; the pair is rejected because a local function shadowing a
//! foreign-boundary declaration is hard to read — a call site does not say
//! whether the callee is compiled here or linked in. That is a property of the
//! spelling, so the rule spans the whole program. Within one file it also
//! replaces the symbol table's refusal of the second insert, whose message named
//! neither declaration.
//!
//! Both declarations and both locations are named, because neither is at fault
//! on its own — renaming either resolves it. The negative controls below matter
//! as much as the positive ones: the rule is about *these two* top-level kinds,
//! and every neighbouring shape (two externs, two functions, a method, a
//! `spec`-inner function) stays outside it.

use crate::utils::{build_ast, build_multi_file_ast, try_type_check_multi_file};
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::TypeCheckError;

/// Every collision reported for `files`, as
/// `(file the diagnostic belongs to, extern line, function line, file named in the note)`.
fn collisions(files: &[(Vec<&str>, &str)]) -> Vec<(Option<String>, u32, u32, Option<String>)> {
    check_with_diagnostics(build_multi_file_ast(files))
        .errors
        .into_iter()
        .filter_map(|d| match d.error {
            TypeCheckError::ExternFunctionNameCollision {
                location,
                function_location,
                function_file,
                ..
            } => Some((
                d.file_label,
                location.start_line,
                function_location.start_line,
                function_file,
            )),
            _ => None,
        })
        .collect()
}

fn single_file_errors(source: &str) -> Vec<TypeCheckError> {
    check_with_diagnostics(build_ast(source.to_string()))
        .errors
        .into_iter()
        .map(|d| d.error)
        .collect()
}

/// The declaration order must not decide which site is reported: the extern is
/// always the error and the function always the note, so the message reads the
/// same whichever the user wrote first.
#[test]
fn one_file_declaring_both_is_rejected_in_either_order() {
    const EXTERN_FIRST: &str = "external fn scale(a: i32) -> i32;\nfn scale(x: i32) -> i32 { \
                                return x * 10; }\npub fn run(x: i32) -> i32 { return scale(x); }";
    const FUNCTION_FIRST: &str = "fn scale(x: i32) -> i32 { return x * 10; }\nexternal fn \
                                  scale(a: i32) -> i32;\npub fn run(x: i32) -> i32 { return \
                                  scale(x); }";

    assert_eq!(
        collisions(&[(vec![], EXTERN_FIRST)]),
        vec![(None, 1, 2, None)],
        "the extern on line 1 collides with the function on line 2"
    );
    assert_eq!(
        collisions(&[(vec![], FUNCTION_FIRST)]),
        vec![(None, 2, 1, None)],
        "the extern on line 2 collides with the function on line 1"
    );
}

/// The collision replaces the symbol table's generic refusal, which named a
/// symbol and a scope but neither declaration's location.
#[test]
fn the_collision_is_the_whole_report() {
    let errors = single_file_errors(
        "external fn scale(a: i32) -> i32;\nfn scale(x: i32) -> i32 { return x * 10; }\npub fn \
         run(x: i32) -> i32 { return scale(x); }",
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeCheckError::ExternFunctionNameCollision { .. }]
        ),
        "one purpose-built diagnostic and nothing else: {errors:?}"
    );
}

/// A duplicate that has nothing to do with the extern still reports. The
/// colliding extern is kept out of the symbol table so it cannot raise a second
/// message of its own, and that removal must not swallow an unrelated one.
#[test]
fn an_unrelated_duplicate_function_still_reports() {
    let errors = single_file_errors(
        "external fn scale(a: i32) -> i32;\nfn scale(x: i32) -> i32 { return x; }\nfn scale(x: \
         i32) -> i32 { return x + 1; }\npub fn run() -> i32 { return 0; }",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::ExternFunctionNameCollision { .. })),
        "the collision is reported: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::RegistrationFailed { .. })),
        "the second `fn scale` is still a duplicate in its own right: {errors:?}"
    );
}

/// The cross-file half of the rule: the entry file defines `scale` and calls it,
/// an imported file declares and binds an `external fn scale`. Each call
/// resolves in its own file, so nothing rejected this shape — the rule is what
/// rejects it, and it must reach across the file boundary or the two spellings
/// still meet in one program.
#[test]
fn a_function_and_a_siblings_extern_are_rejected() {
    assert_eq!(
        collisions(&[
            (
                vec![],
                "use sib;\nfn scale(x: i32) -> i32 { return x * 10; }\npub fn run(x: i32) -> i32 \
                 { return scale(x); }\npub fn via(v: i32) -> i32 { return sib::doubled(v); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn doubled(v: \
                 i32) -> i32 { return scale(v); }",
            ),
        ]),
        vec![(Some("sib".to_string()), 1, 2, None)],
        "the diagnostic belongs to the declaring file and names the entry file's function"
    );
}

/// Both sites are readable in the rendered message, which is all a command-line
/// user sees. A second location renders as a bare `line:col`, so the file it
/// belongs to has to be spelled out.
#[test]
fn the_rendered_message_names_both_declarations() {
    let Err(err) = try_type_check_multi_file(&[
        (
            vec![],
            "use sib;\npub fn via(v: i32) -> i32 { return sib::doubled(v); }",
        ),
        (
            vec!["sib"],
            "external fn scale(a: i32) -> i32;\nfn scale(x: i32) -> i32 { return x; }\npub fn \
             doubled(v: i32) -> i32 { return scale(v); }",
        ),
    ]) else {
        panic!("a function and an extern of one name must be rejected");
    };
    let message = err.to_string();
    assert!(
        message.contains("sib:1:1: `external fn scale` and the function `scale` share one name")
            && message.contains("note: the function `scale` is defined at 2:1 in file `sib`"),
        "both declarations are named with their locations: {message}"
    );
}

/// Visibility is irrelevant: a private function is written as the same bare name
/// inside its own file, which is where a reader meets the two spellings.
#[test]
fn a_private_function_collides_as_well() {
    assert_eq!(
        collisions(&[
            (
                vec![],
                "use sib;\npub fn via(v: i32) -> i32 { return sib::doubled(v); }",
            ),
            (
                vec!["sib"],
                "external fn scale(a: i32) -> i32;\nfn scale(x: i32) -> i32 { return x; }\npub \
                 fn doubled(v: i32) -> i32 { return scale(v); }",
            ),
        ]),
        vec![(Some("sib".to_string()), 1, 2, Some("sib".to_string()))],
        "a private function in an imported file collides just as a `pub` one does"
    );
}

/// Two files each declaring `external fn scale` stay legal — the declarations
/// are distinct, each file's calls reach its own, and the linker resolves them
/// per module. Narrowing this to a name-level rule would reject the commonest
/// multi-file shape there is.
#[test]
fn two_files_may_each_declare_an_extern_of_one_name() {
    try_type_check_multi_file(&[
        (
            vec![],
            "use sib;\nexternal fn scale(a: i32) -> i32;\nuse { scale } from libA;\npub fn \
             from_a(x: i32) -> i32 { return scale(x); }\npub fn from_b(x: i32) -> i32 { return \
             sib::via_b(x); }",
        ),
        (
            vec!["sib"],
            "external fn scale(a: i32) -> i32;\nuse { scale } from libB;\npub fn via_b(x: i32) \
             -> i32 { return scale(x); }",
        ),
    ])
    .expect("two files each declaring and binding their own `scale` is legal");
}

/// Two files each defining `fn scale` stay legal: same-named items in different
/// files are different entities, reached by their own file's namespace.
#[test]
fn two_files_may_each_define_a_function_of_one_name() {
    try_type_check_multi_file(&[
        (
            vec![],
            "use sib;\npub fn scale(x: i32) -> i32 { return x * 10; }\npub fn both(x: i32) -> \
             i32 { return scale(x) + sib::scale(x); }",
        ),
        (vec!["sib"], "pub fn scale(x: i32) -> i32 { return x * 2; }"),
    ])
    .expect("two files each defining their own `scale` is legal");
}

/// A method is written with a receiver, so `p.scale()` and `scale(1)` are two
/// visibly different call forms and the rule must not reach it.
#[test]
fn a_struct_method_may_share_a_name_with_an_extern() {
    try_type_check_multi_file(&[(
        vec![],
        "external fn scale(a: i32) -> i32;\nuse { scale } from libA;\nstruct P { v: i32; fn \
         scale(self) -> i32 { return self.v; } }\npub fn run() -> i32 { let p: P = P { v: 3 }; \
         return p.scale() + scale(1); }",
    )])
    .expect("a method named `scale` beside an `external fn scale` is legal");
}

/// The rule is about two *top-level* declarations. A `spec`-inner function is
/// governed by the separate spec/top-level shadowing rule, which rejects this
/// program on its own terms; the collision check must stay out of it rather than
/// stack a second message on top.
#[test]
fn a_spec_inner_function_is_not_a_collision() {
    let errors = single_file_errors(
        "external fn scale(a: i32) -> i32;\nuse { scale } from libA;\nspec S { fn scale(x: i32) \
         -> i32 { return x; } }\npub fn run() -> i32 { return scale(1); }",
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::SpecFunctionShadowsTopLevel { .. })),
        "the spec/top-level shadowing rule owns this program: {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, TypeCheckError::ExternFunctionNameCollision { .. })),
        "a `spec`-inner function is not a top-level one: {errors:?}"
    );
}

/// Several collisions report in the order the user reads them — by file in
/// arena order, then by declaration within a file. Reporting them from a
/// name-keyed map would order them by hash, which is a list no reader can follow
/// back into the source.
#[test]
fn collisions_are_reported_in_file_then_source_order() {
    assert_eq!(
        collisions(&[
            (
                vec![],
                "use sib;\nfn alpha(x: i32) -> i32 { return x; }\nfn beta(x: i32) -> i32 { \
                 return x; }\npub fn run(x: i32) -> i32 { return alpha(x) + beta(x) + \
                 sib::go(x); }",
            ),
            (
                vec!["sib"],
                "external fn alpha(a: i32) -> i32;\nexternal fn beta(a: i32) -> i32;\npub fn \
                 go(x: i32) -> i32 { return x; }",
            ),
        ]),
        vec![
            (Some("sib".to_string()), 1, 2, None),
            (Some("sib".to_string()), 2, 3, None),
        ],
        "the sibling's two externs report in the order they are declared"
    );
}
