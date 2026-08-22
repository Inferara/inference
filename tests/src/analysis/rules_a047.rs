/// Integration tests for analysis rule A047.
///
/// - A047: `ExternWriteThroughImmutableArgument` — a compound argument at a
///   `mut` `external fn` parameter must be rooted at a `mut` binding. A linked
///   external shares the caller's single linear memory, so a struct or array
///   argument reaches it as a raw pointer into the caller's own frame; `mut` on
///   the declaration states that the foreign body may store through it. The
///   store lives in a `.wasm` the type checker never reads, so the call site is
///   the only place the language can require the author to say the value may
///   change.
///
/// `mut` on an `external fn` parameter parses today and is otherwise inert, so
/// these tests are the first thing that gives it meaning. Three properties carry
/// most of the weight. The first is that the rule reads the *declaration*, not
/// the argument: the same call is rejected or accepted purely by whether the
/// parameter it lands on is `mut`, and only for a parameter that passes a memory
/// region — a scalar or an enum is a value, and the documented
/// `external fn store_at(mut ptr: i32, ..)` idiom must stay untouched. The
/// second is that resolution is scope-aware: a `spec` may declare its own
/// `external fn` under a name a top-level declaration already uses, and each
/// call must be measured against the declaration visible from where it stands.
/// The third is that mutability is read from the argument's *root* binding, so a
/// projection (`o.inner`, `grid[0]`, `(p)`) is judged by the binding it reaches
/// into.
///
/// One shape is deliberately not covered: a call whose arity does not match the
/// declaration. The rule guards its positional index so a surplus argument is
/// skipped, but such a program never reaches analysis — the type checker rejects
/// it first, so a test could only assert the type error.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::{AnalysisDiagnostic, AnalysisErrors, AnalysisResult};
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn analyze(source: &str) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = type_check(source);
        inference_analysis::analyze(&ctx)
    }

    /// Collects this rule's diagnostics by variant rather than by count. Several
    /// of these programs also trip A024 (an extern declared inside a `spec` can
    /// never be bound) or A012 (a compound literal as an argument), and neither
    /// may perturb the result.
    fn a047_diags(source: &str) -> Vec<AnalysisDiagnostic> {
        match analyze(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        AnalysisDiagnostic::ExternWriteThroughImmutableArgument { .. }
                    )
                })
                .cloned()
                .collect(),
        }
    }

    fn assert_a047(source: &str) {
        assert!(
            !a047_diags(source).is_empty(),
            "expected A047 for {source:?}, got: {:?}",
            analyze(source).err()
        );
    }

    fn assert_no_a047(source: &str) {
        assert!(
            a047_diags(source).is_empty(),
            "did not expect A047 for {source:?}, got: {:?}",
            a047_diags(source)
        );
    }

    /// The struct shape every test below shares: a writing external declared over
    /// a two-field struct, bound to a source module.
    const PAIR_PRELUDE: &str = "\
external fn sort_pair(mut p: Pair);
use { sort_pair } from sortlib;
struct Pair { a: i32; b: i32; }
";

    /// The same external declared over an array instead, for the cases where the
    /// region is an array rather than a struct.
    const ARRAY_PRELUDE: &str = "\
external fn sort_pair(mut p: [i32; 2]);
use { sort_pair } from sortlib;
";

    // --- Fires: the argument's root binding is not `mut` ---

    #[test]
    fn a047_rejects_a_non_mut_local() {
        assert_a047(&format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    let p: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(p);
}}
"
        ));
    }

    #[test]
    fn a047_rejects_a_non_mut_parameter() {
        assert_a047(&format!(
            "{PAIR_PRELUDE}
fn touch(p: Pair) {{ sort_pair(p); }}
pub fn main() {{
    let mut p: Pair = Pair {{ a: 5, b: 2 }};
    touch(p);
}}
"
        ));
    }

    #[test]
    fn a047_rejects_a_non_mut_self_receiver() {
        assert_a047(
            "\
external fn sort_pair(mut p: Pair);
use { sort_pair } from sortlib;
struct Pair {
    a: i32;
    b: i32;

    fn touch(self) { sort_pair(self); }
}
pub fn main() {
    let mut p: Pair = Pair { a: 5, b: 2 };
    p.touch();
}
",
        );
    }

    #[test]
    fn a047_rejects_a_non_mut_array_local() {
        assert_a047(&format!(
            "{ARRAY_PRELUDE}
pub fn main() {{
    let arr: [i32; 2] = [5, 2];
    sort_pair(arr);
}}
"
        ));
    }

    #[test]
    fn a047_rejects_a_field_of_a_non_mut_binding() {
        assert_a047(
            "\
external fn sort_pair(mut p: Inner);
use { sort_pair } from sortlib;
struct Inner { a: i32; b: i32; }
struct Outer { inner: Inner; }
pub fn main() {
    let o: Outer = Outer { inner: Inner { a: 5, b: 2 } };
    sort_pair(o.inner);
}
",
        );
    }

    #[test]
    fn a047_rejects_an_element_of_a_non_mut_array() {
        assert_a047(&format!(
            "{ARRAY_PRELUDE}
pub fn main() {{
    let grid: [[i32; 2]; 2] = [[5, 2], [1, 0]];
    sort_pair(grid[0]);
}}
"
        ));
    }

    #[test]
    fn a047_rejects_a_parenthesized_non_mut_binding() {
        assert_a047(&format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    let p: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair((p));
}}
"
        ));
    }

    // --- Does not fire: the root binding says the value may change ---

    #[test]
    fn a047_accepts_a_mut_local() {
        assert_no_a047(&format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    let mut p: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(p);
}}
"
        ));
    }

    #[test]
    fn a047_accepts_a_mut_parameter() {
        assert_no_a047(&format!(
            "{PAIR_PRELUDE}
fn touch(mut p: Pair) {{ sort_pair(p); }}
pub fn main() {{
    let mut p: Pair = Pair {{ a: 5, b: 2 }};
    touch(p);
}}
"
        ));
    }

    #[test]
    fn a047_accepts_a_mut_self_receiver() {
        assert_no_a047(
            "\
external fn sort_pair(mut p: Pair);
use { sort_pair } from sortlib;
struct Pair {
    a: i32;
    b: i32;

    fn touch(mut self) { sort_pair(self); }
}
pub fn main() {
    let mut p: Pair = Pair { a: 5, b: 2 };
    p.touch();
}
",
        );
    }

    #[test]
    fn a047_accepts_a_projection_of_a_mut_binding() {
        assert_no_a047(
            "\
external fn sort_pair(mut p: Inner);
use { sort_pair } from sortlib;
struct Inner { a: i32; b: i32; }
struct Outer { inner: Inner; }
pub fn main() {
    let mut o: Outer = Outer { inner: Inner { a: 5, b: 2 } };
    sort_pair(o.inner);
}
",
        );
    }

    #[test]
    fn a047_accepts_a_mut_array_element() {
        assert_no_a047(&format!(
            "{ARRAY_PRELUDE}
pub fn main() {{
    let mut grid: [[i32; 2]; 2] = [[5, 2], [1, 0]];
    sort_pair(grid[0]);
}}
"
        ));
    }

    // --- Does not fire: the declaration never claimed the right to write ---

    #[test]
    fn a047_accepts_a_non_mut_extern_parameter() {
        // The common case, and the one every read-only external has.
        assert_no_a047(
            "\
external fn read_pair(p: Pair);
use { read_pair } from sortlib;
struct Pair { a: i32; b: i32; }
pub fn main() {
    let p: Pair = Pair { a: 5, b: 2 };
    read_pair(p);
}
",
        );
    }

    #[test]
    fn a047_reports_only_the_mut_position() {
        // Two compound parameters, one `mut`. The rule is per-position, so the
        // second argument is untouched even though its binding is identical.
        let source = "\
external fn merge(mut dst: Pair, src: Pair);
use { merge } from sortlib;
struct Pair { a: i32; b: i32; }
pub fn main() {
    let x: Pair = Pair { a: 5, b: 2 };
    let y: Pair = Pair { a: 1, b: 0 };
    merge(x, y);
}
";
        let diags = a047_diags(source);
        let args: Vec<&str> = diags
            .iter()
            .filter_map(|d| match d {
                AnalysisDiagnostic::ExternWriteThroughImmutableArgument { arg, .. } => {
                    Some(arg.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            args,
            vec!["x"],
            "only the argument at the `mut` position may be reported, got: {diags:?}"
        );
    }

    // --- Does not fire: the parameter passes a value, not a region ---

    #[test]
    fn a047_accepts_a_scalar_mut_extern_parameter() {
        // `external fn store_at(ptr: i32, ..)` is the documented raw-pointer
        // idiom. An `i32` argument is a value: there is no region of the
        // caller's memory for the callee to write into, so no binding of the
        // caller's can change through it.
        let prelude = "\
external fn store_at(mut ptr: i32, val: i32);
use { store_at } from memlib;
";
        assert_no_a047(&format!(
            "{prelude}
pub fn main() {{
    let p: i32 = 128;
    store_at(p, 5);
}}
"
        ));
        assert_no_a047(&format!(
            "{prelude}
pub fn main() {{ store_at(128, 5); }}
"
        ));
    }

    #[test]
    fn a047_accepts_an_enum_typed_mut_extern_parameter() {
        // An enum lowers to a bare `i32` tag, so it is passed by value like any
        // other scalar.
        assert_no_a047(
            "\
external fn paint(mut c: Color);
use { paint } from gfx;
enum Color { Red, Green }
pub fn main() {
    let c: Color = Color::Red;
    paint(c);
}
",
        );
    }

    #[test]
    fn a047_accepts_a_native_callee_with_a_mut_parameter() {
        // `mut` on a native parameter says the callee may reassign its own copy.
        // Inference value semantics keep that copy private, so the caller's
        // binding is unaffected and nothing needs declaring.
        assert_no_a047(
            "\
struct Pair { a: i32; b: i32; }
fn native(mut p: Pair) -> i32 { return p.a; }
pub fn main() -> i32 {
    let p: Pair = Pair { a: 5, b: 2 };
    return native(p);
}
",
        );
    }

    #[test]
    fn a047_judges_each_binding_separately() {
        // Two functions forward an identically shaped argument to the same
        // external, differing only in `mut`. Mutability is recorded per
        // identifier *occurrence*, so exactly one of them is reported — a
        // name-keyed or last-writer-wins record would report both or neither.
        let source = &format!(
            "{PAIR_PRELUDE}
fn read_only(x: Pair) {{ sort_pair(x); }}
fn read_write(mut y: Pair) {{ sort_pair(y); }}
pub fn main() {{
    let mut p: Pair = Pair {{ a: 5, b: 2 }};
    read_only(p);
    read_write(p);
}}
"
        );
        let diags = a047_diags(source);
        let args: Vec<&str> = diags
            .iter()
            .filter_map(|d| match d {
                AnalysisDiagnostic::ExternWriteThroughImmutableArgument { arg, .. } => {
                    Some(arg.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            args,
            vec!["x"],
            "only the non-`mut` parameter may be reported, got: {diags:?}"
        );
    }

    // --- Scope: a `spec` may declare its own external ---

    #[test]
    fn a047_reaches_a_spec_inner_extern_declaration() {
        // A047 must walk `spec` bodies with the spec threaded through, or this
        // declaration is invisible to it. (A024 also fires here — a spec-inner
        // extern can never be bound — which is why the diagnostics are filtered
        // by variant.)
        assert_a047(
            "\
struct Pair { a: i32; b: i32; }
spec Ms {
    external fn sort_pair(mut p: Pair);

    fn run() {
        let p: Pair = Pair { a: 5, b: 2 };
        sort_pair(p);
    }
}
",
        );
    }

    #[test]
    fn a047_resolves_each_scope_to_its_own_declaration() {
        // One bare name, two declarations: a bound top-level one whose parameter
        // is not `mut`, and a spec-inner one whose parameter is. A name-keyed
        // lookup would report both call sites or neither; scope-aware resolution
        // reports exactly the call inside the spec.
        let source = "\
external fn sort_pair(p: Pair);
use { sort_pair } from sortlib;
struct Pair { a: i32; b: i32; }
pub fn main() {
    let p: Pair = Pair { a: 5, b: 2 };
    sort_pair(p);
}
spec Ms {
    external fn sort_pair(mut p: Pair);

    fn run() {
        let q: Pair = Pair { a: 1, b: 0 };
        sort_pair(q);
    }
}
";
        let diags = a047_diags(source);
        let args: Vec<&str> = diags
            .iter()
            .filter_map(|d| match d {
                AnalysisDiagnostic::ExternWriteThroughImmutableArgument { arg, .. } => {
                    Some(arg.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            args,
            vec!["q"],
            "only the spec-inner call resolves to the `mut` declaration, got: {diags:?}"
        );
    }

    #[test]
    fn a047_spec_inner_declaration_does_not_reach_the_top_level() {
        // The mirror image: the `mut` declaration is the spec-inner one, and the
        // top-level call cannot see it. Without the enclosing-scope thread this
        // program would be rejected at a call the language considers correct.
        assert_no_a047(
            "\
external fn sort_pair(p: Pair);
use { sort_pair } from sortlib;
struct Pair { a: i32; b: i32; }
pub fn main() {
    let p: Pair = Pair { a: 5, b: 2 };
    sort_pair(p);
}
spec Ms {
    external fn sort_pair(mut p: Pair);
}
",
        );
    }

    #[test]
    fn a047_accepts_a_mut_binding_inside_a_spec_body() {
        // The acceptance half of the scope-aware walk, and the only test that
        // pairs a `spec` with a `mut` binding at a `mut` extern parameter.
        //
        // Every other spec fixture here is a *rejection*: each one reaches the
        // report before the mutability of the root is ever consulted, so gating
        // that check on being outside a spec leaves them all green. This program
        // is the one that goes red — inside a spec, the binding is `mut`, and the
        // call must be accepted for the same reason the top-level `mut` local is.
        //
        // (A024 also fires — a spec-inner extern can never be bound — which is
        // why `a047_diags` filters by variant.)
        assert_no_a047(
            "\
struct Pair { a: i32; b: i32; }
spec Ms {
    external fn sort_pair(mut p: Pair);

    fn run() {
        let mut p: Pair = Pair { a: 5, b: 2 };
        sort_pair(p);
    }
}
",
        );
    }

    #[test]
    fn a047_accepts_a_mut_parameter_of_a_spec_inner_function() {
        // The same property through the other binding form a spec body can
        // introduce, so the acceptance does not rest on `let` alone.
        assert_no_a047(
            "\
struct Pair { a: i32; b: i32; }
spec Ms {
    external fn sort_pair(mut p: Pair);

    fn touch(mut p: Pair) { sort_pair(p); }
}
",
        );
    }

    // --- A `const` root: reported, but no `mut` can be asked of it ---

    #[test]
    fn a047_rejects_a_const_root() {
        // A `const` is not a `mut` binding, so the write is refused for the same
        // reason as any other immutable root.
        assert_a047(&format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    const P: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(P);
}}
"
        ));
    }

    #[test]
    fn a047_does_not_ask_a_const_for_a_mut_the_grammar_rejects() {
        // `const mut P` is a parse error, so the fix a non-`mut` `let` is given
        // is unspellable here. The message must offer the repair that exists.
        let source = &format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    const P: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(P);
}}
"
        );
        let text = a047_diags(source)
            .first()
            .expect("a `const` at a `mut` extern parameter is reported")
            .to_string();
        assert!(
            text.contains("`P` is a `const`"),
            "the message must say the root is a `const`, got: {text}"
        );
        assert!(
            text.contains("copy `P` into a `mut` binding and pass that instead"),
            "the message must offer the repair a `const` can take, got: {text}"
        );
        assert!(
            !text.contains("declare it `mut P`"),
            "the message must not ask for `const mut P`, which does not parse, got: {text}"
        );
    }

    #[test]
    fn a047_still_asks_a_non_mut_let_for_mut_where_it_is_bound() {
        // The control for the test above: the direct repair survives for the
        // binding form that can actually take it. Without this, deleting the
        // `mut` advice outright would leave the `const` assertions green.
        let source = &format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    let p: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(p);
}}
"
        );
        let text = a047_diags(source)
            .first()
            .expect("a non-`mut` local at a `mut` extern parameter is reported")
            .to_string();
        assert!(
            text.contains("declare it `mut p` where it is bound"),
            "a `let` keeps the direct repair, got: {text}"
        );
        assert!(
            !text.contains("is a `const`"),
            "a `let` must not be described as a `const`, got: {text}"
        );
    }

    // --- The message ---

    #[test]
    fn a047_message_names_the_binding_the_parameter_and_the_callee() {
        let source = &format!(
            "{PAIR_PRELUDE}
pub fn main() {{
    let p: Pair = Pair {{ a: 5, b: 2 }};
    sort_pair(p);
}}
"
        );
        let diags = a047_diags(source);
        let diag = diags.first().expect("expected an A047 diagnostic");
        let text = diag.to_string();
        assert!(
            text.contains("cannot pass `p`"),
            "message must name the binding, got: {text}"
        );
        assert!(
            text.contains("parameter `p: Pair`"),
            "message must name the parameter and its declared type, got: {text}"
        );
        assert!(
            text.contains("`external fn sort_pair`"),
            "message must name the external function, got: {text}"
        );
        assert!(
            text.contains("declare it `mut p` where it is bound"),
            "message must spell the fix out, got: {text}"
        );
        assert_eq!(diag.rule_id(), "A047");
    }

    #[test]
    fn a047_message_names_the_root_of_a_projection() {
        // `o.inner` is not a binding; `o` is, and `o` is what the author must
        // declare `mut`. The caret still sits on the argument as written.
        let diags = a047_diags(
            "\
external fn sort_pair(mut inner: Inner);
use { sort_pair } from sortlib;
struct Inner { a: i32; b: i32; }
struct Outer { inner: Inner; }
pub fn main() {
    let o: Outer = Outer { inner: Inner { a: 5, b: 2 } };
    sort_pair(o.inner);
}
",
        );
        let diag = diags.first().expect("expected an A047 diagnostic");
        let text = diag.to_string();
        assert!(
            text.contains("cannot pass `o`") && text.contains("declare it `mut o`"),
            "message must name the root binding, not the projection, got: {text}"
        );
    }

    // --- An argument with no binding behind it ---

    #[test]
    fn a047_rejects_an_argument_with_no_root_binding() {
        // A compound literal at a `mut` position is rooted at nothing, so there
        // is no binding whose declaration could say the value may change. A047
        // must not silently accept it. This shape is *always* reported by A012 as
        // well — a compound literal may not be an argument at all — so the
        // assertion is that both fire, never that A047 fires alone.
        let source = &format!(
            "{PAIR_PRELUDE}
pub fn main() {{ sort_pair(Pair {{ a: 5, b: 2 }}); }}
"
        );
        assert_a047(source);
        let errors = analyze(source).expect_err("expected analysis errors");
        assert!(
            errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::CompoundLiteralAsArgument { .. })),
            "the rootless shape must remain an A012 error too, got: {:?}",
            errors.errors()
        );
    }
}
