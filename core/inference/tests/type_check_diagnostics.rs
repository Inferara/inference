//! Integration tests for the lossless structured type-check entry point
//! (`inference::type_check_with_diagnostics` / `inference_type_checker::check_with_diagnostics`,
//! issue #33).
//!
//! The suite pins three properties the LSP layer depends on:
//!
//! 1. **Structured, parse-free diagnostics** — each error keeps its
//!    [`TypeCheckError`] variant, its per-file-local source location, and its
//!    optional module-path file label, so no string parsing is needed.
//! 2. **Legacy parity** — the existing string-joining path
//!    ([`inference::type_check`]) renders byte-identically to rendering the
//!    structured errors the same way, guarding against drift between the two.
//! 3. **Partial-context usefulness** — when some definitions fail to check, the
//!    returned context still answers whole-program queries (`lookup_struct`,
//!    `lookup_enum`, `lookup_method`) and per-node queries
//!    (`get_node_typeinfo`, `call_target`) for the definitions that did check.
//!
//! Sources are compact inline `.inf` per the contributing guide; multi-file
//! programs are folded with `inference_parser::parse_into` (no filesystem).

use inference::{
    TypeCheckDiagnostic, TypeCheckError, TypedContext, type_check, type_check_with_diagnostics,
};
use inference_ast::arena::AstArena;
use inference_ast::ids::{DefId, ExprId, NodeId};
use inference_ast::nodes::{Def, Expr, Stmt};

/// Parses a single-file source into an arena, asserting it is syntactically
/// valid (these tests exercise type checking, not parsing).
fn parse_arena(source: &str) -> AstArena {
    let parsed = inference_parser::parse(source);
    assert!(
        parsed.errors.is_empty(),
        "test source has syntax errors: {:?}",
        parsed.errors
    );
    parsed.arena
}

/// Folds `(module_path, source)` pairs into one multi-file arena, entry file
/// (empty module path) first. Mirrors what the project front end builds at
/// runtime, without touching the filesystem.
fn parse_multi_file(files: &[(&[&str], &str)]) -> AstArena {
    let mut arena = AstArena::default();
    for (module_path, source) in files {
        let segments: Vec<String> = module_path.iter().map(|s| (*s).to_string()).collect();
        let parsed = inference_parser::parse_into(arena, source, segments);
        assert!(
            parsed.errors.is_empty(),
            "multi-file test source {module_path:?} has syntax errors: {:?}",
            parsed.errors
        );
        arena = parsed.arena;
    }
    arena
}

/// Renders structured diagnostics exactly as the legacy aggregated
/// [`inference::type_check`] message does: `label:line:col: message` per error
/// (bare `line:col: message` for the entry file), joined by `"; "`. Kept in
/// lockstep with the render in `TypeCheckerBuilder::build_typed_context`; the
/// parity tests fail if they ever diverge.
fn render_like_legacy(errors: &[TypeCheckDiagnostic]) -> String {
    errors
        .iter()
        .map(|diagnostic| match &diagnostic.file_label {
            Some(label) => format!("{label}:{}", diagnostic.error),
            None => diagnostic.error.to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Finds the top-level function named `name`, panicking if it is absent.
fn function_def_id_by_name(ctx: &TypedContext, name: &str) -> DefId {
    for def_id in ctx.function_def_ids() {
        if let Def::Function { name: name_id, .. } = &ctx.arena()[def_id].kind
            && ctx.arena()[*name_id].name == name
        {
            return def_id;
        }
    }
    panic!("function `{name}` not found in typed context");
}

/// Returns the id of the first `FunctionCall` expression in the body of the
/// given function, panicking if there is none. Used to probe `get_node_typeinfo`
/// (on the call expression, which carries the callee's return type) and
/// `call_target` (on the call's function sub-expression).
fn first_call_expr_in_body(ctx: &TypedContext, func: DefId) -> ExprId {
    let Def::Function { body, .. } = &ctx.arena()[func].kind else {
        panic!("`{func:?}` is not a function");
    };
    for &stmt_id in &ctx.arena()[*body].stmts {
        let candidate = match &ctx.arena()[stmt_id].kind {
            Stmt::VarDef {
                value: Some(expr), ..
            } => Some(*expr),
            Stmt::Return { expr } => Some(*expr),
            Stmt::Expr(expr) => Some(*expr),
            _ => None,
        };
        if let Some(expr) = candidate
            && matches!(ctx.arena()[expr].kind, Expr::FunctionCall { .. })
        {
            return expr;
        }
    }
    panic!("no function call in body of `{func:?}`");
}

// --- Single-error structured shape ------------------------------------------

#[test]
fn single_type_mismatch_has_structured_variant_and_location() {
    let source = r#"fn main() -> i32 { return true; }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert_eq!(
        outcome.errors.len(),
        1,
        "expected exactly one error, got {:?}",
        outcome.errors
    );
    let diagnostic = &outcome.errors[0];
    assert_eq!(
        diagnostic.file_label, None,
        "an entry-file error carries no file label"
    );
    let TypeCheckError::TypeMismatch { location, .. } = &diagnostic.error else {
        panic!("expected TypeMismatch, got {:?}", diagnostic.error);
    };
    // The return-type mismatch anchors at the returning statement; assert the
    // location is on line 1, in bounds, non-empty, and covers the `true` value.
    assert_eq!(location.start_line, 1, "single-line source");
    assert!(
        location.offset_end > location.offset_start,
        "location spans a non-empty range"
    );
    assert!(
        (location.offset_end as usize) <= source.len(),
        "location stays within the source"
    );
    let slice = &source[location.offset_start as usize..location.offset_end as usize];
    assert!(
        slice.contains("true"),
        "the location `{slice}` should cover the offending `true`"
    );
    assert_eq!(
        location.start_column,
        location.offset_start + 1,
        "1-based byte column on a single line equals offset + 1"
    );
}

#[test]
fn undefined_function_error_carries_name_and_location() {
    let source = r#"fn main() -> i32 { return missing(); }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    let undefined = outcome
        .errors
        .iter()
        .find_map(|d| match &d.error {
            TypeCheckError::UndefinedFunction { name, location } => Some((name, location)),
            _ => None,
        })
        .expect("expected an UndefinedFunction error");
    assert_eq!(undefined.0, "missing");
    assert_eq!(undefined.1.start_line, 1);
    assert!(
        undefined.1.offset_end > undefined.1.offset_start,
        "location spans a non-empty range"
    );
    assert!(
        (undefined.1.offset_end as usize) <= source.len(),
        "location stays within the source"
    );
}

#[test]
fn unknown_type_error_variant_is_preserved() {
    let source = r#"fn main(x: Nope) -> i32 { return 0; }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    let has_unknown_type = outcome.errors.iter().any(|d| {
        matches!(
            &d.error,
            TypeCheckError::UnknownType { name, .. } if name == "Nope"
        )
    });
    assert!(
        has_unknown_type,
        "expected UnknownType for `Nope`, got {:?}",
        outcome.errors
    );
}

// --- Multiple errors: order and dedup ---------------------------------------

#[test]
fn multiple_errors_preserve_order_and_render_like_legacy() {
    let source = r#"fn a() -> i32 { return true; } fn b() -> i32 { return missing(); }"#;
    let legacy = type_check(parse_arena(source))
        .err()
        .expect("program has type errors")
        .to_string();
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert!(
        outcome.errors.len() >= 2,
        "expected at least two errors, got {:?}",
        outcome.errors
    );
    assert_eq!(
        render_like_legacy(&outcome.errors),
        legacy,
        "structured render must reproduce the legacy aggregated string byte-for-byte"
    );
}

#[test]
fn dedup_behavior_matches_legacy() {
    // The same undefined type referenced twice can be emitted by more than one
    // pass; the structured path must dedup identically to the legacy path, which
    // the render-equality below enforces.
    let source = r#"fn f(a: Nope, b: Nope) -> i32 { return 0; }"#;
    let legacy = type_check(parse_arena(source))
        .err()
        .expect("program has type errors")
        .to_string();
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert_eq!(render_like_legacy(&outcome.errors), legacy);
    assert_eq!(
        legacy.matches("; ").count() + 1,
        outcome.errors.len(),
        "structured error count matches the number of `; `-joined legacy segments"
    );
}

// --- Multi-file file labels -------------------------------------------------

#[test]
fn error_in_imported_module_is_labeled_with_module_path() {
    let arena = parse_multi_file(&[
        (
            &[],
            "use lib::math; pub fn main() -> i32 { return lib::math::add(1, 2); }",
        ),
        (
            &["lib", "math"],
            "pub fn add(a: i32, b: i32) -> i32 { return wrong; }",
        ),
    ]);
    let outcome = type_check_with_diagnostics(arena);

    let labeled = outcome
        .errors
        .iter()
        .find(|d| matches!(&d.error, TypeCheckError::UnknownIdentifier { name, .. } if name == "wrong"))
        .expect("expected the undeclared-variable error from the imported file");
    assert_eq!(
        labeled.file_label.as_deref(),
        Some("lib::math"),
        "an imported-file error names its module path"
    );
}

#[test]
fn multi_file_render_matches_legacy_with_labels() {
    let files: &[(&[&str], &str)] = &[
        (
            &[],
            "use lib::math; pub fn main() -> i32 { return lib::math::add(1); }",
        ),
        (
            &["lib", "math"],
            "pub fn add(a: i32, b: i32) -> i32 { return wrong; }",
        ),
    ];
    let legacy = type_check(parse_multi_file(files))
        .err()
        .expect("program has type errors")
        .to_string();
    let outcome = type_check_with_diagnostics(parse_multi_file(files));

    assert!(
        !outcome.errors.is_empty(),
        "expected cross-file errors, got none"
    );
    assert_eq!(
        render_like_legacy(&outcome.errors),
        legacy,
        "labels and ordering must match the legacy aggregated string"
    );
    assert!(
        legacy.contains("lib::math:"),
        "the legacy string labels the imported file, sanity-checking the fixture"
    );
}

// --- Clean program ----------------------------------------------------------

#[test]
fn clean_program_has_no_errors_and_fully_usable_context() {
    let source = r#"struct Point { x: i32; y: i32; } fn area(p: Point) -> i32 { return p.x + p.y; } pub fn main() -> i32 { let p: Point = Point { x: 3, y: 4 }; return area(p); }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert!(
        outcome.errors.is_empty(),
        "expected a clean program, got {:?}",
        outcome.errors
    );
    let ctx = &outcome.typed_context;
    assert!(
        ctx.lookup_struct("Point").is_some(),
        "the struct index is populated for a clean program"
    );
    let main = function_def_id_by_name(ctx, "main");
    let call = first_call_expr_in_body(ctx, main);
    assert!(
        ctx.get_node_typeinfo(NodeId::Expr(call)).is_some(),
        "the call expression is typed"
    );
    let Expr::FunctionCall { function, .. } = &ctx.arena()[call].kind else {
        unreachable!("first_call_expr_in_body returns a FunctionCall");
    };
    assert!(
        ctx.call_target(*function).is_some(),
        "the resolved call target is recorded"
    );
}

#[test]
fn clean_program_agrees_with_legacy_type_check() {
    let source = r#"struct Point { x: i32; y: i32; } pub fn main() -> i32 { let p: Point = Point { x: 1, y: 2 }; return p.x; }"#;
    let legacy = type_check(parse_arena(source)).expect("clean program type-checks");
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert!(outcome.errors.is_empty());
    assert_eq!(
        legacy.lookup_struct("Point").is_some(),
        outcome.typed_context.lookup_struct("Point").is_some(),
        "both entry points build the same struct index on success"
    );
}

// --- Partial context after an error -----------------------------------------

#[test]
fn partial_context_serves_checked_parts_after_error_in_sibling() {
    // `broken` fails to type-check; every other definition is well-formed. The
    // returned context must still answer whole-program and per-node queries for
    // the well-formed parts.
    let source = r#"struct Point { x: i32; y: i32; fn sum(self) -> i32 { return self.x + self.y; } } enum Color { Red, Green, Blue } fn helper(n: i32) -> i32 { return n; } pub fn good() -> i32 { let s: i32 = helper(1); return s; } fn broken() -> i32 { return true; }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert!(
        !outcome.errors.is_empty(),
        "the broken sibling must produce an error"
    );
    let ctx = &outcome.typed_context;

    // Whole-program tables survive the error in `broken`.
    assert!(
        ctx.lookup_struct("Point").is_some(),
        "struct index survives a sibling error"
    );
    assert!(
        ctx.lookup_enum("Color").is_some(),
        "enum index survives a sibling error"
    );
    assert!(
        ctx.lookup_method("Point", "sum").is_some(),
        "method lookup survives a sibling error"
    );

    // Per-node results answer for the well-formed sibling `good`.
    let good = function_def_id_by_name(ctx, "good");
    let call = first_call_expr_in_body(ctx, good);
    assert!(
        ctx.get_node_typeinfo(NodeId::Expr(call)).is_some(),
        "a node in a checked sibling is typed despite an error elsewhere"
    );
    let Expr::FunctionCall { function, .. } = &ctx.arena()[call].kind else {
        unreachable!("first_call_expr_in_body returns a FunctionCall");
    };
    assert!(
        ctx.call_target(*function).is_some(),
        "a resolved call in a checked sibling has a recorded target"
    );

    // The arena is always fully present, regardless of errors.
    assert!(
        ctx.arena().source_files().len() >= 1,
        "the parsed arena is preserved"
    );
}

#[test]
fn partial_context_struct_used_correctly_alongside_error() {
    // A struct is defined and used correctly in one function while another
    // function has an unrelated error; the struct stays resolvable.
    let source = r#"struct Vec2 { x: i32; y: i32; } fn ok(v: Vec2) -> i32 { return v.x; } fn bad() -> i32 { return undefined_name; }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    assert!(!outcome.errors.is_empty(), "the `bad` function must error");
    assert!(
        outcome.typed_context.lookup_struct("Vec2").is_some(),
        "a correctly-used struct is resolvable despite an unrelated error"
    );
    let ok = function_def_id_by_name(&outcome.typed_context, "ok");
    if let Def::Function { args, .. } = &outcome.typed_context.arena()[ok].kind {
        assert_eq!(
            args.len(),
            1,
            "the well-formed function is intact in the arena"
        );
    } else {
        panic!("`ok` is not a function");
    }
}

// --- Extern-import diagnostics from an imported file ------------------------

#[test]
fn dangling_extern_import_in_imported_file_is_labeled_with_module_path() {
    // Extern-binding collection scans the `use { .. } from <module>;` directives
    // of every file in the closure while the cursor sits at the root (no open
    // file). A dangling extern import that lives in an imported file must still
    // name that file: its source location is per-file-local, so a `None` label
    // makes the LSP render it against the entry document at out-of-bounds,
    // entry-file-local offsets.
    let arena = parse_multi_file(&[
        (&[], "use lib; fn main() -> i32 { return 0; }"),
        (
            &["lib"],
            "use { foo_undeclared } from env; pub fn helper() -> i32 { return 1; }",
        ),
    ]);
    let outcome = type_check_with_diagnostics(arena);

    let dangling = outcome
        .errors
        .iter()
        .find(|d| {
            matches!(&d.error, TypeCheckError::ExternImportNotDeclared { name, .. } if name == "foo_undeclared")
        })
        .expect("expected the dangling extern-import error from the imported file");
    assert_eq!(
        dangling.file_label.as_deref(),
        Some("lib"),
        "an extern-import error names the file that owns the `use ... from` directive"
    );
}

#[test]
fn ambiguous_extern_import_in_imported_file_is_labeled_with_module_path() {
    // The ambiguous-module push shares the same root-cursor scan as the dangling
    // one, so it must anchor to the file holding the first conflicting directive.
    let arena = parse_multi_file(&[
        (&[], "use lib; fn main() -> i32 { return 0; }"),
        (
            &["lib"],
            "use { blend } from collections; use { blend } from algorithms; external fn blend(a: i32) -> i32; pub fn helper() -> i32 { return 1; }",
        ),
    ]);
    let outcome = type_check_with_diagnostics(arena);

    let ambiguous = outcome
        .errors
        .iter()
        .find(|d| {
            matches!(&d.error, TypeCheckError::AmbiguousExternModule { name, .. } if name == "blend")
        })
        .expect("expected the ambiguous extern-import error from the imported file");
    assert_eq!(
        ambiguous.file_label.as_deref(),
        Some("lib"),
        "an ambiguous extern-import error names the file that owns the first `use ... from`"
    );
}

// --- Annotated-let number-literal mismatch: source-level double emission -----

#[test]
fn annotated_let_number_literal_mismatch_double_emits_and_renders_in_legacy_twice() {
    // A `let p: Point = 3;` reports the same variable-definition mismatch twice:
    // once from the number-literal-vs-non-numeric-target check and once from the
    // general initializer comparison it falls through to. This is deliberately
    // left as-is at the type-checker source, because the legacy aggregated string
    // renders the duplicate too (they share one error list), so deduping at the
    // source would silently change pinned compiler-output. The user-visible
    // squiggle duplication is instead collapsed downstream in the IDE/LSP
    // diagnostics layer. This test pins the current source-level contract so any
    // future change to emission is a conscious, coordinated one.
    let source = r#"struct Point { x: i32; } fn main() -> i32 { let p: Point = 3; return 0; }"#;
    let outcome = type_check_with_diagnostics(parse_arena(source));

    let mismatches: Vec<_> = outcome
        .errors
        .iter()
        .filter(|d| {
            matches!(&d.error, TypeCheckError::TypeMismatch { .. })
                && d.error.to_string().contains("in variable definition")
        })
        .collect();
    assert_eq!(
        mismatches.len(),
        2,
        "the number-literal path emits the variable-definition mismatch twice, got {:?}",
        outcome.errors
    );
    assert_eq!(
        mismatches[0].error.location(),
        mismatches[1].error.location(),
        "both mismatches anchor at the same location"
    );

    let legacy = type_check(parse_arena(source))
        .err()
        .expect("program has a type error")
        .to_string();
    assert_eq!(
        legacy
            .matches("type mismatch in variable definition")
            .count(),
        2,
        "the legacy aggregated string renders the duplicate too, so source-level \
         emission must not be deduped without coordinating the pinned output: {legacy}"
    );
}
