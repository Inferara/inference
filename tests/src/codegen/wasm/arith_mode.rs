//! The `checked(…)` / `wrapping(…)` surface at code generation.
//!
//! Two properties are pinned here, and both are stated relative to the
//! language's default arithmetic mode rather than to a keyword. An annotation
//! naming the default is byte-transparent: it names what the emitter already
//! does, so an annotated source is byte-for-byte the same module as the
//! unannotated one, at every width and for every governed operator. An
//! annotation naming the *other* mode changes the module — which is what the
//! cases below measure: a governed operator under it changes, an ungoverned one
//! under the same annotation does not, and an operator outside the parentheses
//! is unaffected.
//!
//! Writing them that way is what lets the default flip without rewriting this
//! file: [`default_mode`], [`other_mode`] and [`assert_differs_from_default`]
//! each read [`ArithMode::DEFAULT`], so the polarity lives in the language and
//! not in these assertions.
//!
//! What a guard actually computes is the `checked_arith` fixture's business;
//! what is measured here is which operators an annotation reaches.

use crate::utils::{
    AnalysisMode, CodegenAttempt, assert_wasms_modules_equivalence, codegen_attempt,
    codegen_output_with_mode, wasm_codegen, wasm_codegen_no_analysis,
};
use inference_ast::nodes::ArithMode;
use inference_wasm_codegen::CompilationMode;

/// The eight integer widths, and whether unary `-` is legal at each.
const WIDTHS: [(&str, bool); 8] = [
    ("i8", true),
    ("i16", true),
    ("i32", true),
    ("i64", true),
    ("u8", false),
    ("u16", false),
    ("u32", false),
    ("u64", false),
];

/// Whether the language's default arithmetic mode is the checked one.
///
/// Read off the language's own constant rather than restated, so this file
/// tracks the default instead of pinning it.
fn default_is_checked() -> bool {
    matches!(ArithMode::DEFAULT, ArithMode::Checked)
}

/// The keyword naming the mode every unannotated operator already has.
fn default_mode() -> &'static str {
    ArithMode::DEFAULT.spelling()
}

/// The keyword naming the mode the language does not default to.
fn other_mode() -> &'static str {
    if default_is_checked() {
        ArithMode::Wrapping.spelling()
    } else {
        ArithMode::Checked.spelling()
    }
}

/// A two-parameter function whose body is `expr`.
fn binary_source(width: &str, expr: &str) -> String {
    format!("pub fn op(a: {width}, b: {width}) -> {width} {{ return {expr}; }}")
}

/// A one-parameter function whose body is `expr`.
fn unary_source(width: &str, expr: &str) -> String {
    format!("pub fn op(a: {width}) -> {width} {{ return {expr}; }}")
}

/// Asserts that the two sources compile to the same bytes, naming the pair when
/// they do not.
fn assert_same_module(plain: &str, annotated: &str) {
    let expected = wasm_codegen(plain);
    let actual = wasm_codegen(annotated);
    assert_eq!(
        expected, actual,
        "annotating changed the emitted module\n  plain: {plain}\n  annotated: {annotated}"
    );
}

/// [`assert_same_module`] with the analysis pass skipped, for a pair whose
/// annotated half has no operator to govern.
///
/// Analysis rule A053 rejects such an annotation, and rightly: an author who
/// wrote it meant it somewhere else. The question here is a different one and
/// has to be asked anyway — whether the *emitter* leaves the module alone when
/// an annotation reaches nothing — because that is what makes A053's severity a
/// choice rather than a necessity, and what would go silently wrong if the node
/// ever emitted an instruction of its own.
fn assert_same_module_without_analysis(plain: &str, annotated: &str) {
    let expected = wasm_codegen_no_analysis(plain);
    let actual = wasm_codegen_no_analysis(annotated);
    assert_eq!(
        expected, actual,
        "annotating changed the emitted module\n  plain: {plain}\n  annotated: {annotated}"
    );
}

/// The module `source` compiles to, insisting that code generation neither
/// refused nor panicked — a panic would satisfy a weaker reading of "the guard
/// was not emitted".
fn module(source: &str) -> Vec<u8> {
    match codegen_attempt(source, AnalysisMode::Skip) {
        CodegenAttempt::Ok(output) => {
            let wasm = output.wasm().to_vec();
            inf_wasmparser::validate(&wasm)
                .unwrap_or_else(|e| panic!("module is invalid: {source}\n{e}"));
            wasm
        }
        CodegenAttempt::Rejected(message) => {
            panic!("expected a module for: {source}\nrefused with: {message}")
        }
        CodegenAttempt::Panicked(message) => {
            panic!("code generation panicked for: {source}\n{message}")
        }
    }
}

/// Asserts that an annotation naming the language default reproduces the
/// unannotated module byte for byte.
fn assert_same_as_default(plain: &str, annotated: &str) {
    assert_same_module(plain, annotated);
}

/// Asserts that an annotation naming the mode the language does *not* default
/// to changes the emitted module.
///
/// Size is the observable side of the difference, and which way it points is
/// decided by the default rather than by the call sites. Under the checked
/// default the non-default annotation is the modular one, so it can only make a
/// module shorter — it removes a guard and the scratch locals the function
/// declared for it. Under the other default every one of these cases is the
/// same measurement in the opposite direction, which is why the comparison is
/// written once, here, and no case below spells a direction.
fn assert_differs_from_default(plain: &str, annotated: &str) {
    let plain_module = module(plain);
    let annotated_module = module(annotated);
    let (shorter, longer, which) = if default_is_checked() {
        (
            annotated_module.len(),
            plain_module.len(),
            "the non-default annotation must drop the guard the default emits",
        )
    } else {
        (
            plain_module.len(),
            annotated_module.len(),
            "the non-default annotation must add a guard the default does not emit",
        )
    };
    assert!(
        longer > shorter,
        "{which}\n  plain: {plain} ({} bytes)\n  annotated: {annotated} ({} bytes)",
        plain_module.len(),
        annotated_module.len()
    );
}

/// The number of `unreachable` instructions in `wat`.
fn unreachable_count(wat: &str) -> usize {
    wat.lines().filter(|line| line.trim() == "unreachable").count()
}

/// The number of `if <cond>; unreachable; end` sequences in `wat`.
///
/// Every guard traps through exactly that shape, and nothing else the emitter
/// produces does: an ordinary body's `unreachable` is the unreachable tail after
/// its `return`, which no `if` opens and no `end` closes.
fn guard_trap_shapes(wat: &str) -> usize {
    let lines: Vec<&str> = wat
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .windows(3)
        .filter(|window| {
            window[0].starts_with("if") && window[1] == "unreachable" && window[2] == "end"
        })
        .count()
}

/// The WAT of the module `source` compiles to.
fn wat(source: &str) -> String {
    wasmprinter::print_bytes(module(source)).expect("printable module")
}

#[test]
fn the_default_annotation_is_byte_transparent_for_every_binary_operator_at_every_width() {
    // The mark proves the annotated half really did lower through the
    // annotation arm: without it the pair could agree because the parser
    // dropped the node.
    cov_mark::check!(wasm_codegen_emit_arith_mode_expression);
    let mode = default_mode();
    for (width, _) in WIDTHS {
        for op in ["+", "-", "*"] {
            assert_same_as_default(
                &binary_source(width, &format!("a {op} b")),
                &binary_source(width, &format!("{mode}(a {op} b)")),
            );
        }
    }
}

#[test]
fn the_default_annotation_is_byte_transparent_for_unary_minus_at_every_signed_width() {
    let mode = default_mode();
    for (width, neg_is_legal) in WIDTHS {
        if !neg_is_legal {
            continue;
        }
        assert_same_as_default(
            &unary_source(width, "-a"),
            &unary_source(width, &format!("{mode}(-a)")),
        );
    }
}

#[test]
fn the_default_annotation_is_byte_transparent_around_a_whole_nested_expression() {
    // One annotation over a tree of governed operators, and one annotation per
    // operator, both reproduce the unannotated module.
    let mode = default_mode();
    let plain = "pub fn op(a: i64, b: i64, c: i64) -> i64 { return (a + b) * c - a; }";
    assert_same_as_default(
        plain,
        &format!("pub fn op(a: i64, b: i64, c: i64) -> i64 {{ return {mode}((a + b) * c - a); }}"),
    );
    assert_same_as_default(
        plain,
        &format!(
            "pub fn op(a: i64, b: i64, c: i64) -> i64 {{ \
             return {mode}({mode}({mode}(a + b) * c) - a); }}"
        ),
    );
}

#[test]
fn both_annotations_are_byte_transparent_over_the_ungoverned_operators() {
    // Division, remainder, the shifts and the bitwise operators are governed by
    // neither annotation, so both spellings reproduce the unannotated module —
    // scratch declarations included, since a pool reserved for a guard that is
    // never emitted would show up in these bytes. Unlike every other case here
    // this one is not stated against the default, because neither mode reaches
    // these operators at all.
    for mode in ["wrapping", "checked"] {
        for op in ["/", "%", "<<", ">>", "&", "|", "^"] {
            assert_same_module_without_analysis(
                &binary_source("u32", &format!("a {op} b")),
                &binary_source("u32", &format!("{mode}(a {op} b)")),
            );
        }
    }
}

#[test]
fn a_default_annotation_in_a_let_initializer_is_byte_transparent() {
    // The annotation sits where an expected type has to reach the literals
    // inside it; if it did not, the narrow literals below would be typed at a
    // different width and the module would differ.
    let mode = default_mode();
    assert_same_as_default(
        "pub fn op() -> i8 { let n: i8 = 100 + 27; return n; }",
        &format!("pub fn op() -> i8 {{ let n: i8 = {mode}(100 + 27); return n; }}"),
    );
}

#[test]
fn a_proof_build_of_an_annotated_source_matches_its_compile_build() {
    // The annotation is a source-level policy, not a mode-gated one: a spec-free
    // program emits the same instructions under both modes, exactly as it does
    // without the annotation.
    let mode = default_mode();
    let annotated = format!("pub fn op(a: i64, b: i64) -> i64 {{ return {mode}(a * b + a); }}");
    let plain = "pub fn op(a: i64, b: i64) -> i64 { return a * b + a; }";
    let compile = codegen_output_with_mode(&annotated, CompilationMode::Compile);
    let proof = codegen_output_with_mode(&annotated, CompilationMode::Proof);
    assert_wasms_modules_equivalence(compile.wasm(), proof.wasm());
    assert_wasms_modules_equivalence(&wasm_codegen(plain), proof.wasm());
}

#[test]
fn the_non_default_annotation_changes_every_governed_operator_at_every_width() {
    let mode = other_mode();
    for (width, neg_is_legal) in WIDTHS {
        for op in ["+", "-", "*"] {
            assert_differs_from_default(
                &binary_source(width, &format!("a {op} b")),
                &binary_source(width, &format!("{mode}(a {op} b)")),
            );
        }
        if neg_is_legal {
            assert_differs_from_default(
                &unary_source(width, "-a"),
                &unary_source(width, &format!("{mode}(-a)")),
            );
        }
    }
}

#[test]
fn every_guard_traps_through_unreachable_and_nothing_else() {
    // The trap instruction is part of the contract, not an implementation
    // detail: a guard that reached wasm's own integer-overflow trap would report
    // a different trap kind to the host and would be a trap role the Rocq
    // contract does not enumerate. `unreachable` is the only trap any guard
    // introduces, and the arithmetic that could trap natively — the multiply
    // guard's division — is fenced off before it runs.
    //
    // Counted rather than searched for. Every emitted body already ends with the
    // unreachable tail after its `return`, so asking whether the WAT contains
    // the word answers yes for a module with no guard in it at all. What
    // separates a guarded module from an unguarded one is how many
    // `unreachable`s it has and what encloses them: each guard's trap sits alone
    // inside an `if` of its own, and every `unreachable` the checked build adds
    // over the plain one is one of those.
    //
    // Which of the two spellings carries the guard comes from the language's
    // default, so the pair is built the same way the byte-cost rows below build
    // theirs and the measurement is guarded minus unguarded at either polarity.
    for (width, neg_is_legal) in WIDTHS {
        let mut expressions = vec!["a + b", "a - b", "a * b"];
        if neg_is_legal {
            expressions.push("-a");
        }
        for expression in expressions {
            let unary = expression == "-a";
            let build = |body: &str| {
                if unary {
                    unary_source(width, body)
                } else {
                    binary_source(width, body)
                }
            };
            let (guarded, unguarded) = guarded_and_unguarded(expression);
            let (plain, checked) = (build(&unguarded), build(&guarded));
            let plain_wat = wat(&plain);
            let checked_wat = wat(&checked);
            assert_eq!(
                guard_trap_shapes(&plain_wat),
                0,
                "an unguarded body must open no `if` over an `unreachable`: {plain}\n{plain_wat}"
            );
            let added = unreachable_count(&checked_wat)
                .checked_sub(unreachable_count(&plain_wat))
                .unwrap_or_else(|| panic!("the guard removed a trap site: {checked}"));
            assert!(
                added > 0,
                "the guard added no trap site: {checked}\n{checked_wat}"
            );
            assert_eq!(
                guard_trap_shapes(&checked_wat),
                added,
                "every trap the guard adds must sit alone inside an `if` of its own: \
                 {checked}\n{checked_wat}"
            );
        }
    }
}

#[test]
fn checked_leaves_the_ungoverned_operators_alone() {
    // The checked spelling in particular, whatever the default is: a `checked`
    // around an operator no mode governs must reserve no pool and emit no
    // guard, and the pool would show up in these bytes if it did.
    for op in ["/", "%", "<<", ">>", "&", "|", "^"] {
        assert_same_module_without_analysis(
            &binary_source("u32", &format!("a {op} b")),
            &binary_source("u32", &format!("checked(a {op} b)")),
        );
    }
}

#[test]
fn an_annotation_governs_only_the_operators_inside_its_own_parentheses() {
    // `checked(a)` closes at its `)`, so the `+` that follows keeps whatever
    // mode it had and the module is unchanged.
    for mode in ["wrapping", "checked"] {
        assert_same_module_without_analysis(
            "pub fn op(a: i32, b: i32) -> i32 { return a + b; }",
            &format!("pub fn op(a: i32, b: i32) -> i32 {{ return {mode}(a) + b; }}"),
        );
        assert_same_module_without_analysis(
            "pub fn op(a: i32, b: i32) -> i32 { let x: i32 = a; return x + b; }",
            &format!("pub fn op(a: i32, b: i32) -> i32 {{ let x: i32 = {mode}(a); return x + b; }}"),
        );
    }
}

#[test]
fn the_innermost_annotation_decides_each_operator() {
    // The two nestings govern different operators, so they must produce
    // different modules — and each must differ from the unannotated one.
    let outer = default_mode();
    let inner = other_mode();
    const PLAIN: &str = "pub fn op(a: i32, b: i32, c: i32) -> i32 { return a * (b + c); }";
    let inner_annotated = format!(
        "pub fn op(a: i32, b: i32, c: i32) -> i32 {{ return {outer}(a * {inner}(b + c)); }}"
    );
    let outer_annotated = format!(
        "pub fn op(a: i32, b: i32, c: i32) -> i32 {{ return {inner}(a * {outer}(b + c)); }}"
    );
    assert_differs_from_default(PLAIN, &inner_annotated);
    assert_differs_from_default(PLAIN, &outer_annotated);
    assert_ne!(
        module(&inner_annotated),
        module(&outer_annotated),
        "the two nestings govern different operators"
    );
    // Nesting one annotation inside another leaves the module unchanged when
    // both name the language default.
    assert_same_as_default(
        "pub fn op(a: i32, b: i32, c: i32) -> i32 { return a + (b + c); }",
        &format!(
            "pub fn op(a: i32, b: i32, c: i32) -> i32 {{ return {outer}(a + {outer}(b + c)); }}"
        ),
    );
}

#[test]
fn an_annotation_over_a_comparison_governs_the_arithmetic_inside_it() {
    // The boundary shape: the annotation encloses the comparison, and what it
    // governs is the sum on the comparison's left.
    const PLAIN: &str = "pub fn op(a: i32, b: i32, c: i32) -> bool { return a + b > c; }";
    assert_same_as_default(
        PLAIN,
        &format!(
            "pub fn op(a: i32, b: i32, c: i32) -> bool {{ return {}(a + b > c); }}",
            default_mode()
        ),
    );
    assert_differs_from_default(
        PLAIN,
        &format!(
            "pub fn op(a: i32, b: i32, c: i32) -> bool {{ return {}(a + b > c); }}",
            other_mode()
        ),
    );
    // The comparison itself is not arithmetic, so an annotation whose only
    // operator is the comparison asks for nothing, under either spelling.
    for mode in ["wrapping", "checked"] {
        assert_same_module_without_analysis(
            "pub fn op(a: i32, b: i32) -> bool { return a > b; }",
            &format!("pub fn op(a: i32, b: i32) -> bool {{ return {mode}(a > b); }}"),
        );
    }
}

#[test]
fn an_annotation_in_a_const_initializer_governs_its_operator() {
    // A constant initializer is lowered like any other expression, so the
    // annotation means there exactly what it means elsewhere. The sum lands on
    // `i32::MAX` rather than one past it: an initializer whose result leaves the
    // type is refused by analysis before this question can be asked, and the
    // question is about the annotation, not about the value.
    const PLAIN: &str = "pub fn op() -> i32 { const K: i32 = 2147483646 + 1; return K; }";
    assert_same_as_default(
        PLAIN,
        &format!(
            "pub fn op() -> i32 {{ const K: i32 = {}(2147483646 + 1); return K; }}",
            default_mode()
        ),
    );
    // Code generation folds nothing, so the annotation reaches this `+` exactly
    // as it reaches one over parameters.
    assert_differs_from_default(
        PLAIN,
        &format!(
            "pub fn op() -> i32 {{ const K: i32 = {}(2147483646 + 1); return K; }}",
            other_mode()
        ),
    );
}

#[test]
fn whitespace_between_the_keyword_and_the_parenthesis_changes_nothing() {
    // The keyword and its `(` are separate tokens, so the form is not one of
    // the language's glue-sensitive spellings and layout cannot change the
    // emitted module. Compared against the same annotation written closed up,
    // so the property is about layout alone and holds under either spelling.
    for mode in ["wrapping", "checked"] {
        let closed = format!("pub fn op(a: i32, b: i32) -> i32 {{ return {mode}(a + b); }}");
        for spaced in [
            format!("pub fn op(a: i32, b: i32) -> i32 {{ return {mode} (a + b); }}"),
            format!("pub fn op(a: i32, b: i32) -> i32 {{ return {mode}\n    (a + b); }}"),
        ] {
            assert_same_module(&closed, &spaced);
        }
    }
}

#[test]
fn an_annotated_operand_leaves_the_operator_outside_it_alone() {
    // `wrapping(a) + b` annotates the operand, not the sum: the `+` is outside
    // the parentheses and keeps the mode it had.
    for mode in ["wrapping", "checked"] {
        assert_same_module_without_analysis(
            "pub fn op(a: i32, b: i32) -> i32 { return a + b; }",
            &format!("pub fn op(a: i32, b: i32) -> i32 {{ return {mode}(a) + b; }}"),
        );
    }
}

#[test]
fn an_annotation_does_not_reach_into_a_callee() {
    // The annotation is lexical: `checked(f(a))` governs no operator of its own,
    // and `f`'s body keeps the mode it would have had.
    //
    // The annotated caller is written *first* so it is also compiled first. A
    // mode that leaked past its own subtree would then still be in force while
    // the callee's body is lowered, and the callee's `+` would pick it up — the
    // one arrangement in which this comparison can see such a leak at all.
    for mode in ["wrapping", "checked"] {
        assert_same_module_without_analysis(
            "pub fn op(a: i32, b: i32) -> i32 { return add(a, b); } \
             fn add(x: i32, y: i32) -> i32 { return x + y; }",
            &format!(
                "pub fn op(a: i32, b: i32) -> i32 {{ return {mode}(add(a, b)); }} \
                 fn add(x: i32, y: i32) -> i32 {{ return x + y; }}"
            ),
        );
    }
}

/// The lowering site a catalogue row belongs to, which decides how many
/// parameters its source needs.
#[derive(Clone, Copy)]
enum GuardedForm {
    /// A binary operator, spelled as source spells it.
    Binary(&'static str),
    /// Unary minus.
    Negation,
}

use GuardedForm::{Binary, Negation};

/// One row of the overflow-guard catalogue: an operator at a width, and the
/// bytes its guard adds to the body that carries it.
struct GuardRow {
    width: &'static str,
    form: GuardedForm,
    /// Bytes one occurrence of the guard adds, the scratch declaration aside.
    guard: usize,
}

/// Every row of the guard catalogue, with its measured cost.
///
/// The list is the one the `checked_arith` fixture compiles, function for
/// function: three binary operators at each of the eight widths, and unary minus
/// at the four signed ones. Four numbers carry the design. The narrow widths
/// cost 13 whatever the operator, because a narrow guard is one shape — the
/// promoted result compared against its own re-narrowing — rather than one shape
/// per operator. Unsigned addition costs 13 against signed addition's 23,
/// because the unsigned test reads one operand where the signed test reads two.
/// Multiplication costs what it does because it checks by dividing the product
/// back, and the signed rows carry the extra arm that keeps `MIN * -1` away from
/// the machine's own division trap. And negation, which the emitter lowers as a
/// subtraction from zero, costs the width of the constant it compares against.
const CATALOGUE: &[GuardRow] = &[
    GuardRow { width: "i8", form: Binary("+"), guard: 13 },
    GuardRow { width: "i8", form: Binary("-"), guard: 13 },
    GuardRow { width: "i8", form: Binary("*"), guard: 13 },
    GuardRow { width: "i8", form: Negation, guard: 13 },
    GuardRow { width: "i16", form: Binary("+"), guard: 13 },
    GuardRow { width: "i16", form: Binary("-"), guard: 13 },
    GuardRow { width: "i16", form: Binary("*"), guard: 13 },
    GuardRow { width: "i16", form: Negation, guard: 13 },
    GuardRow { width: "u8", form: Binary("+"), guard: 13 },
    GuardRow { width: "u8", form: Binary("-"), guard: 13 },
    GuardRow { width: "u8", form: Binary("*"), guard: 13 },
    GuardRow { width: "u16", form: Binary("+"), guard: 13 },
    GuardRow { width: "u16", form: Binary("-"), guard: 13 },
    GuardRow { width: "u16", form: Binary("*"), guard: 13 },
    GuardRow { width: "i32", form: Binary("+"), guard: 23 },
    GuardRow { width: "i32", form: Binary("-"), guard: 23 },
    GuardRow { width: "i32", form: Binary("*"), guard: 52 },
    GuardRow { width: "i32", form: Negation, guard: 15 },
    GuardRow { width: "i64", form: Binary("+"), guard: 23 },
    GuardRow { width: "i64", form: Binary("-"), guard: 23 },
    GuardRow { width: "i64", form: Binary("*"), guard: 57 },
    GuardRow { width: "i64", form: Negation, guard: 20 },
    GuardRow { width: "u32", form: Binary("+"), guard: 13 },
    GuardRow { width: "u32", form: Binary("-"), guard: 15 },
    GuardRow { width: "u32", form: Binary("*"), guard: 30 },
    GuardRow { width: "u64", form: Binary("+"), guard: 13 },
    GuardRow { width: "u64", form: Binary("-"), guard: 15 },
    GuardRow { width: "u64", form: Binary("*"), guard: 30 },
];

/// What a function's local declarations grow by when a guard first reserves
/// scratch of one width class.
///
/// The pool is a fixed three slots per class, and a local vector run-length
/// encodes a class as one `(count, valtype)` pair: two bytes, whatever the
/// slots are used for. A second guard of the same class therefore costs
/// nothing, and a class the body never guards at costs nothing at all.
const POOL_DECLARATION_BYTES: usize = 2;

/// The expression written with its arithmetic guarded, and written without it.
///
/// Which spelling carries the guard follows the language's default: under the
/// checked default the bare operator carries one and `wrapping(...)` is what
/// takes it away; under the other, `checked(...)` is what adds one. Every
/// measurement below is guarded minus unguarded, so the catalogue's costs stay
/// positive at either polarity.
fn guarded_and_unguarded(expr: &str) -> (String, String) {
    let annotated = format!("{}({expr})", other_mode());
    if default_is_checked() {
        (expr.to_string(), annotated)
    } else {
        (annotated, expr.to_string())
    }
}

/// The source for `row` holding `count` occurrences of `expr`.
///
/// Two occurrences are joined with `|`, which no mode governs: it costs the same
/// in the guarded source and the unguarded one, and the same at one occurrence
/// as at two, so the difference between the two counts is guard bytes and
/// nothing else.
fn row_source(row: &GuardRow, expr: &str, count: usize) -> String {
    let body = if count == 1 {
        expr.to_string()
    } else {
        format!("{expr} | {expr}")
    };
    match row.form {
        GuardedForm::Binary(_) => binary_source(row.width, &body),
        GuardedForm::Negation => unary_source(row.width, &body),
    }
}

/// The operators of `row`, as source spells them.
fn row_expression(row: &GuardRow) -> &'static str {
    match row.form {
        GuardedForm::Binary("+") => "a + b",
        GuardedForm::Binary("-") => "a - b",
        GuardedForm::Binary("*") => "a * b",
        GuardedForm::Binary(other) => panic!("no catalogue row spells `{other}`"),
        GuardedForm::Negation => "-a",
    }
}

/// The total size of the module's function bodies, each without its own length
/// prefix.
///
/// The prefix is left out because it is a LEB128 length: a body that crosses 127
/// bytes grows its own prefix by one and the code section's by one more, and
/// those bytes belong to no guard. What the catalogue counts is instructions and
/// local declarations, which is exactly what this sums.
fn function_body_bytes(source: &str) -> usize {
    use inf_wasmparser::{Parser, Payload};
    let wasm = module(source);
    let mut total = 0;
    for payload in Parser::new(0).parse_all(&wasm) {
        if let Ok(Payload::CodeSectionEntry(body)) = payload {
            let range = body.range();
            total += range.end - range.start;
        }
    }
    assert!(total > 0, "no function body to measure: {source}");
    total
}

/// The bytes `guarded` spends over `unguarded`, which differ only in whether the
/// arithmetic between them is guarded.
fn guard_delta(guarded: &str, unguarded: &str) -> isize {
    let with = isize::try_from(function_body_bytes(guarded)).expect("body size fits");
    let without = isize::try_from(function_body_bytes(unguarded)).expect("body size fits");
    with - without
}

#[test]
fn every_guard_row_costs_the_bytes_the_catalogue_records() {
    // The per-row costs are quoted as prose in three places — the changelog
    // entry, the book's guard table and the emitter's own module documentation —
    // and prose does not turn red when the emitter moves under it. This is where
    // it does.
    //
    // Each row is measured twice, at one occurrence of the operator and at two,
    // because a single measurement cannot separate the guard from the scratch
    // the function declares for it: the first guard of a width class pays for
    // both and every later one pays for the guard alone. The difference between
    // the two counts is therefore one guard, and what is left over once that is
    // subtracted is the declaration.
    let mut wrong: Vec<String> = Vec::new();
    for row in CATALOGUE {
        let expr = row_expression(row);
        let (guarded, unguarded) = guarded_and_unguarded(expr);
        let once = guard_delta(
            &row_source(row, &guarded, 1),
            &row_source(row, &unguarded, 1),
        );
        let twice = guard_delta(
            &row_source(row, &guarded, 2),
            &row_source(row, &unguarded, 2),
        );
        let guard = twice - once;
        let declaration = once * 2 - twice;
        let expected_guard = isize::try_from(row.guard).expect("catalogue entry fits");
        let expected_declaration =
            isize::try_from(POOL_DECLARATION_BYTES).expect("declaration cost fits");
        if guard != expected_guard || declaration != expected_declaration {
            wrong.push(format!(
                "  {} `{expr}`: guard {guard} (catalogue says {expected_guard}), \
                 declaration {declaration} (expected {expected_declaration}); \
                 one occurrence cost {once}, two cost {twice}",
                row.width
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the emitted guards no longer cost what the catalogue records:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_scratch_pool_is_declared_once_per_width_class() {
    // The other half of the arithmetic above, measured where a single row cannot
    // show it: what the pool costs is decided by which width classes a body
    // guards at, not by how many guards it has. Two guards of one class share
    // the class's slots, a narrow guard rides in the i32 class because its
    // operands are promoted there, and only a body that guards at both classes
    // pays twice.
    let body = |left_width: &str, right_width: &str, left: &str, right: &str| {
        format!(
            "pub fn op(a: {left_width}, b: {left_width}, c: {right_width}, d: {right_width}) \
             -> {right_width} {{ let x: {left_width} = {left}; let y: {right_width} = {right}; \
             if x > 0 {{ return y; }} return y; }}"
        )
    };
    let (guarded, unguarded) = guarded_and_unguarded("a + b");
    let (guarded_right, unguarded_right) = guarded_and_unguarded("c + d");
    for (left, right, guards, classes) in [
        ("i32", "i32", 23 + 23, 1),
        ("i8", "i32", 13 + 23, 1),
        ("i32", "i64", 23 + 23, 2),
    ] {
        let expected = isize::try_from(guards + classes * POOL_DECLARATION_BYTES)
            .expect("expected cost fits");
        let measured = guard_delta(
            &body(left, right, &guarded, &guarded_right),
            &body(left, right, &unguarded, &unguarded_right),
        );
        assert_eq!(
            measured, expected,
            "a body guarding at {left} and {right} declares {classes} class(es) of scratch"
        );
    }
}
