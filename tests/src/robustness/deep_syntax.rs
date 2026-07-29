//! Deeply nested and deeply chained input survives the recursive phases (#322).
//!
//! Every phase of the front end — grammar, AST lowering, type check, analysis
//! and code generation — descends once per level of the input's syntactic
//! nesting, so the stack a phase runs on decides how deep an input it survives.
//! The phases used to run on whatever stack the host thread happened to have,
//! which is how a 350-operand operator chain and a 900-arm `else if` chain both
//! ended a compile with `fatal runtime error: stack overflow` — a signal kill,
//! not a diagnostic, because an overflow aborts the process instead of
//! unwinding. Both now compile: every in-process driver runs the pipeline
//! through [`inference::with_compiler_stack`], which reserves
//! [`inference_parser::MIN_COMPILE_STACK`].
//!
//! The tests below must route through that helper themselves. Cargo's test
//! harness gives each test thread a stack an order of magnitude smaller than the
//! phases need, so a test that called a phase directly would overflow long
//! before it reached the behaviour under test — it would be measuring the
//! harness, not the compiler.
//!
//! What is pinned here is *survival*, not rejection. There is no explicit
//! syntactic depth limit yet, so input deeper than the reserved stack allows
//! still aborts; asserting a diagnostic for over-deep input would assert
//! something the compiler does not do. Every depth below is therefore a depth
//! the compiler is claimed to accept, kept several times under the measured
//! ceiling so that platforms whose stack frames are larger than the ones these
//! were measured on still clear it.

use crate::utils::try_build_ast;

// Source-shape generators. Each emits one compact line, per the contributing
// guide: whitespace is irrelevant to the parser and the interesting parameter is
// the depth, not the layout.

/// `pub fn f(a: i64) -> i64 { return a + a + … + a; }` with `n` operands, so the
/// parsed expression is a left-leaning `Binary` spine `n - 1` deep.
fn operand_chain(n: usize) -> String {
    let chain = std::iter::repeat_n("a", n).collect::<Vec<_>>().join(" + ");
    format!("pub fn f(a: i64) -> i64 {{ return {chain}; }}")
}

/// `pub fn f(a: i64) -> i64 { return ((…a…)); }` with `n` nested parentheses.
fn paren_nest(n: usize) -> String {
    let open = "(".repeat(n);
    let close = ")".repeat(n);
    format!("pub fn f(a: i64) -> i64 {{ return {open}a{close}; }}")
}

/// `pub fn f(a: i64) -> i64 { return ----a; }` with `n` prefix `-` operators.
///
/// `n` is kept even so the value is unchanged; the point of the shape is the
/// `PrefixUnary` spine, not the arithmetic.
fn prefix_unary_nest(n: usize) -> String {
    let ops = "-".repeat(n);
    format!("pub fn f(a: i64) -> i64 {{ return {ops}a; }}")
}

/// An `if` / `else if` chain of `k` arms closed by a final `else`.
///
/// The source is flat but the walked tree is not, and the amplification is the
/// reason this shape is worth a generator of its own: the grammar parses the chain
/// into a single node, and lowering then desugars it into nested `if` statements at
/// **two** levels per arm — an `if` plus the `else` block wrapping its successor.
/// So `k` arms cost `2k` levels to every phase that walks the lowered tree, which is
/// why a flat 900-arm chain used to abort while 900 lines of straight-line code do
/// not. Measured: an arm costs exactly what one level of true `if` nesting costs
/// (identical ceilings in the type checker, analysis and codegen).
fn else_if_chain(k: usize) -> String {
    let arms: String = (0..k)
        .map(|i| format!("if a == {i} {{ return {i}; }} else "))
        .collect();
    format!("pub fn f(a: i64) -> i64 {{ {arms}{{ return 0; }} }}")
}

/// `pub fn f() { { { … } } }` with `n` nested bare block statements.
fn block_nest(n: usize) -> String {
    let open = "{".repeat(n);
    let close = "}".repeat(n);
    format!("pub fn f() {{ {open} {close} }}")
}

/// A function taking an `n`-deep nested array type, e.g. `[[[i64; 1]; 1]; 1]`,
/// so the recursion under test is the type grammar rather than the expression
/// grammar.
fn nested_array_type(n: usize) -> String {
    let ty = format!("{}i64{}", "[".repeat(n), "; 1]".repeat(n));
    format!("pub fn f(a: {ty}) -> i64 {{ return 0; }}")
}

/// `spec Deep { fn g() -> i32 { return ((…1…)); } }` — the same expression nest
/// as [`paren_nest`], but inside a `spec`, which the phases walk through a
/// different set of entry points than a plain top-level function.
fn spec_expression_nest(n: usize) -> String {
    let open = "(".repeat(n);
    let close = ")".repeat(n);
    format!("spec Deep {{ fn g() -> i32 {{ return {open}1{close}; }} }}")
}

// Pipeline helpers. Each runs one prefix of the pipeline on a compiler-sized
// thread and returns a `Result` carrying the rendered failure, so a phase that
// rejects the input fails the assertion with its own diagnostics rather than
// with a bare `false`. Everything the phase allocates is also dropped inside the
// closure, so no deep structure is carried back to the harness thread.

/// Parses `source`, reporting every syntax error.
fn parses(source: &str) -> Result<(), String> {
    inference::with_compiler_stack(|| {
        try_build_ast(source.to_string())
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Parses and type-checks `source`.
fn type_checks(source: &str) -> Result<(), String> {
    inference::with_compiler_stack(|| {
        let arena = try_build_ast(source.to_string()).map_err(|e| e.to_string())?;
        inference::type_check(arena)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Parses, type-checks and analyses `source`.
fn passes_front_end(source: &str) -> Result<(), String> {
    inference::with_compiler_stack(|| {
        let arena = try_build_ast(source.to_string()).map_err(|e| e.to_string())?;
        let typed_context = inference::type_check(arena).map_err(|e| e.to_string())?;
        inference::analyze(&typed_context)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    })
}

/// Runs the whole pipeline and returns the size of the emitted module, so a
/// caller can assert something was actually generated.
fn compiles(source: &str) -> Result<usize, String> {
    inference::with_compiler_stack(|| {
        let arena = try_build_ast(source.to_string()).map_err(|e| e.to_string())?;
        let typed_context = inference::type_check(arena).map_err(|e| e.to_string())?;
        inference::analyze(&typed_context).map_err(|e| format!("{e:?}"))?;
        inference::codegen(&typed_context, "deep")
            .map(|output| output.wasm().len())
            .map_err(|e| e.to_string())
    })
}

/// The exact input reported in issue #322: a 350-operand operator chain, which
/// used to abort the type checker on the platform default stack while 300
/// survived. It is asserted through analysis, not merely parsing, because the
/// type checker is the phase that aborted.
#[test]
fn reported_350_operand_chain_passes_front_end() {
    let source = operand_chain(350);
    assert_eq!(passes_front_end(&source), Ok(()));
}

/// A 900-arm `else if` chain compiles end to end. This was the lowest known
/// abort threshold — 800 arms survived, 900 did not — so it is driven all the
/// way through code generation rather than stopping at the type checker.
#[test]
fn reported_900_arm_else_if_chain_compiles() {
    let source = else_if_chain(900);
    let wasm_len = compiles(&source).expect("a 900-arm else-if chain must compile");
    assert!(wasm_len > 0, "codegen produced an empty module");
}

/// The phase's acceptance bar for chained operands. The type-check ceiling on
/// the reserved stack measures at roughly 5,200 operands, so 2,000 leaves well
/// over a factor of two for platforms with larger frames.
#[test]
fn operand_chain_of_2000_type_checks() {
    let source = operand_chain(2_000);
    assert_eq!(type_checks(&source), Ok(()));
}

/// The phase's acceptance bar for `else if` arms. Statement frames are lighter than
/// expression frames, so this shape's own measured type-check ceiling is 14,069 arms
/// — 2,000 sits seven times under it, a wider margin than the operand chain's.
#[test]
fn else_if_chain_of_2000_arms_type_checks() {
    let source = else_if_chain(2_000);
    assert_eq!(type_checks(&source), Ok(()));
}

/// Parenthesis nesting is the cheapest deep shape to build and the most direct
/// probe of the grammar's own recursion. 1,000 levels is a fifth of the measured
/// type-check ceiling and orders of magnitude past anything a program contains.
#[test]
fn paren_nest_of_1000_passes_front_end() {
    let source = paren_nest(1_000);
    assert_eq!(passes_front_end(&source), Ok(()));
}

/// A 1,000-deep prefix-unary spine survives the grammar, lowering and the type
/// checker. It stops at the type checker because analysis rejects adjacent
/// prefix unary operators outright (A033), which is a decision about the
/// program's meaning and independent of how deep the spine is.
#[test]
fn prefix_unary_nest_of_1000_type_checks() {
    let source = prefix_unary_nest(1_000);
    assert_eq!(type_checks(&source), Ok(()));
}

/// 1,000 nested bare blocks — statement-level rather than expression-level
/// nesting, which the phases descend through separate recursions.
#[test]
fn block_nest_of_1000_passes_front_end() {
    let source = block_nest(1_000);
    assert_eq!(passes_front_end(&source), Ok(()));
}

/// A 500-deep nested array type exercises the type grammar and the type
/// checker's type resolution, neither of which is reached by the expression
/// shapes. The depth is lower than the expression cases because a level here
/// costs a level in the parser, in lowering, and in the resolved type it builds;
/// 500 is still far past any type a program declares.
#[test]
fn nested_array_type_of_500_type_checks() {
    let source = nested_array_type(500);
    assert_eq!(type_checks(&source), Ok(()));
}

/// The same 1,000-level expression nest inside a `spec`, which the phases enter
/// through their spec-handling paths rather than the top-level function paths.
#[test]
fn spec_expression_nest_of_1000_passes_front_end() {
    let source = spec_expression_nest(1_000);
    assert_eq!(passes_front_end(&source), Ok(()));
}

/// The grammar and lowering reach considerably further than the phases behind
/// them, which is what makes one shared floor for all of them worth having:
/// 10,000 nested parentheses parse, twice the depth the type checker accepts and
/// twice what the parse phase managed on the platform default stack.
#[test]
fn paren_nest_of_10000_parses() {
    let source = paren_nest(10_000);
    assert_eq!(parses(&source), Ok(()));
}
