/// Integration tests for analysis rule A035.
///
/// - A035: RecursionDetected — direct and mutual/indirect recursion is forbidden
///   (Power of 10, Rule 1).
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_type_check_multi_file};
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

    /// Returns true if any analysis error is a `RecursionDetected` (A035).
    /// Filters by variant rather than asserting a total error count, since a
    /// bare-function surface may also trip unrelated rules.
    fn has_recursion(source: &str) -> bool {
        match analyze(source) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. })),
        }
    }

    fn recursion_diag(source: &str) -> AnalysisDiagnostic {
        analyze(source)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. }))
            .expect("expected a RecursionDetected diagnostic")
            .clone()
    }

    #[test]
    fn a035_direct_recursion_rejected() {
        let source = "fn f() -> i32 { return f(); }";
        assert!(
            has_recursion(source),
            "expected RecursionDetected for direct self-recursion"
        );
    }

    #[test]
    fn a035_direct_recursion_names_cycle() {
        let diag = recursion_diag("fn f() -> i32 { return f(); }");
        let msg = diag.to_string();
        assert!(
            msg.contains("f -> f"),
            "diagnostic should name the cycle `f -> f`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    #[test]
    fn a035_mutual_recursion_rejected() {
        let source = "fn a() -> i32 { return b(); } fn b() -> i32 { return a(); }";
        assert!(
            has_recursion(source),
            "expected RecursionDetected for mutual recursion a <-> b"
        );
    }

    #[test]
    fn a035_non_recursive_accepted() {
        let source = "fn a() -> i32 { return b(); } fn b() -> i32 { return 0; }";
        assert!(
            !has_recursion(source),
            "non-recursive call chain must not trip A035"
        );
    }

    #[test]
    fn a035_recursion_nested_in_if_detected() {
        let source = r#"
            fn f(n: i32) -> i32 {
                if n > 0 {
                    let r: i32 = f(n);
                    return r;
                }
                return 0;
            }
        "#;
        assert!(
            has_recursion(source),
            "recursive call nested inside an if-block must be detected"
        );
    }

    #[test]
    fn a035_three_cycle_detected() {
        let source = r#"
            fn a() -> i32 { return b(); }
            fn b() -> i32 { return c(); }
            fn c() -> i32 { return a(); }
        "#;
        assert!(
            has_recursion(source),
            "expected RecursionDetected for the 3-cycle a -> b -> c -> a"
        );
    }

    #[test]
    fn a035_method_self_recursion_detected() {
        // End-to-end coverage of method-call resolution through real source:
        // `self.rec()` must resolve to the canonical key `S.rec` and form a cycle.
        let source = r#"
            struct S {
                v: i32;
                fn rec(self) -> i32 { return self.rec(); }
            }
            pub fn entry() -> i32 { let s: S = S { v: 1 }; return s.rec(); }
        "#;
        let diag = recursion_diag(source);
        assert!(
            diag.to_string().contains("S.rec -> S.rec"),
            "diagnostic should name the method cycle `S.rec -> S.rec`, got: {diag}"
        );
    }

    #[test]
    fn a035_recursion_inside_nondet_block_detected() {
        // A recursive call buried in a `forall` block body must still be caught;
        // the walker descends into non-deterministic blocks like any other block.
        let source = r#"
            fn r(n: i32) -> i32 { return r(n); }
            pub fn entry() -> i32 {
                forall {
                    let x: i32 = r(0);
                }
                return 0;
            }
        "#;
        assert!(
            has_recursion(source),
            "recursive call inside a forall block must be detected"
        );
    }

    // --- Cross-file recursion ----------------------------------------------------
    //
    // A `::`-qualified module call (`lib::b::pong()`) and a `root::`-qualified
    // call back into the entry file resolve to a function in another file. The
    // whole-program call graph must record those edges so a cycle spanning files
    // is rejected — a regression where qualified call edges were silently dropped
    // let cross-file mutual recursion compile and stack-overflow at runtime.

    /// Type-checks a multi-file program (entry first, empty module path) and runs
    /// the analysis pass, returning its result.
    fn analyze_multi(files: &[(Vec<&str>, &str)]) -> Result<AnalysisResult, AnalysisErrors> {
        let ctx = try_type_check_multi_file(files)
            .expect("multi-file type checking should succeed for analysis test input");
        inference_analysis::analyze(&ctx)
    }

    fn has_recursion_multi(files: &[(Vec<&str>, &str)]) -> bool {
        match analyze_multi(files) {
            Ok(_) => false,
            Err(errors) => errors
                .errors()
                .iter()
                .any(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. })),
        }
    }

    fn recursion_diag_multi(files: &[(Vec<&str>, &str)]) -> AnalysisDiagnostic {
        analyze_multi(files)
            .expect_err("expected analysis errors but got Ok")
            .errors()
            .iter()
            .find(|e| matches!(e, AnalysisDiagnostic::RecursionDetected { .. }))
            .expect("expected a RecursionDetected diagnostic")
            .clone()
    }

    /// Mutual recursion across files via a `::`-qualified call out (`lib::b::pong`)
    /// and a `root::`-qualified call back (`root::ping`) must be rejected, and the
    /// diagnostic chain must name both files.
    #[test]
    fn a035_cross_file_qualified_mutual_recursion_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b;
                    pub fn ping(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return lib::b::pong(n - 1);
                    }
                    pub fn main() -> i32 { return ping(5); }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use root;
                    pub fn pong(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return root::ping(n - 1);
                    }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("ping") && msg.contains("lib.b.pong"),
            "cross-file cycle diagnostic should name both files (`ping` and `lib.b.pong`), got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    /// The item-import form (`use lib::b::{pong}` / `use root::{ping}` with bare
    /// calls) was already caught before the qualified-call fix; this guards that
    /// it stays caught (the discriminator was purely the call *form*).
    #[test]
    fn a035_cross_file_item_import_mutual_recursion_still_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b::{pong};
                    pub fn ping(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return pong(n - 1);
                    }
                    pub fn main() -> i32 { return ping(5); }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use root::{ping};
                    pub fn pong(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return ping(n - 1);
                    }
                "#,
            ),
        ];
        assert!(
            has_recursion_multi(files),
            "item-import cross-file mutual recursion must stay rejected"
        );
    }

    /// A legitimate non-recursive cross-file chain `f0 -> lib::b::f1 ->
    /// lib::c::f2` must still compile: qualified edges are recorded, but they form
    /// no cycle.
    #[test]
    fn a035_non_recursive_cross_file_chain_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b;
                    pub fn f0(n: i32) -> i32 { return lib::b::f1(n) + 1; }
                    pub fn main() -> i32 { return f0(1); }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use lib::c;
                    pub fn f1(n: i32) -> i32 { return lib::c::f2(n) + 1; }
                "#,
            ),
            (
                vec!["lib", "c"],
                r#"
                    pub fn f2(n: i32) -> i32 { return n + 1; }
                "#,
            ),
        ];
        assert!(
            !has_recursion_multi(files),
            "a non-recursive cross-file chain must not trip A035"
        );
    }

    /// Two files each define a free `fn helper`; the entry `helper` is in a cycle
    /// via `lib::b::ping`, but `lib::x::helper` is innocent. The same-named
    /// innocent function must not be implicated — node identity is by defining
    /// file, so the two `helper`s are distinct nodes.
    #[test]
    fn a035_same_named_cross_file_collision_does_not_falsely_implicate() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::b;
                    use lib::x;
                    pub fn helper(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return lib::b::ping(n - 1);
                    }
                    pub fn main() -> i32 { return helper(3) + lib::x::helper(2); }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use root;
                    pub fn ping(n: i32) -> i32 {
                        if n <= 0 { return 0; }
                        return root::helper(n - 1);
                    }
                "#,
            ),
            (
                vec!["lib", "x"],
                r#"
                    pub fn helper(n: i32) -> i32 { return n + 1; }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("helper") && msg.contains("lib.b.ping"),
            "cycle should name the entry `helper` and `lib.b.ping`, got: {msg}"
        );
        assert!(
            !msg.contains("lib.x.helper"),
            "innocent same-named `lib.x.helper` must not be implicated, got: {msg}"
        );
    }

    // --- Cross-file recursion through methods and associated functions -----------
    //
    // An instance-method dispatch (`recv.m()`) and a bare/namespaced associated
    // call (`Type::assoc()`) resolve to a function whose defining file differs
    // from the call site's. The call graph records those edges from the type
    // checker's recorded call target, qualified by the method's/struct's defining
    // file. A regression where the instance-method and associated-function arms
    // did not record a target dropped those cross-file edges, letting a cycle
    // through methods compile and stack-overflow at runtime.

    /// Mutual recursion through cross-file *instance methods* (`x.ping()` calls
    /// `y.pong()` which calls back `z.ping()`) must be rejected, naming both files'
    /// methods.
    #[test]
    fn a035_cross_file_instance_method_mutual_recursion_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a::{A};
                    pub fn main() -> i32 {
                        let x: A = A::make();
                        return x.ping();
                    }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b::{B};
                    pub struct A {
                        v: i32;
                        pub fn make() -> A { return A { v: 1 }; }
                        pub fn ping(self) -> i32 {
                            let y: B = B::make();
                            return y.pong();
                        }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use lib::a::{A};
                    pub struct B {
                        v: i32;
                        pub fn make() -> B { return B { v: 2 }; }
                        pub fn pong(self) -> i32 {
                            let z: A = A::make();
                            return z.ping();
                        }
                    }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("lib.a.A.ping") && msg.contains("lib.b.B.pong"),
            "instance-method cycle should name both files' methods \
             (`lib.a.A.ping`, `lib.b.B.pong`), got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    /// Mutual recursion through cross-file bare *associated functions* — `A::ping()`
    /// where `A` is item-imported (a two-segment `A::ping` path, not a namespace
    /// path) — must be rejected. The bare associated arm records the call target so
    /// the edge resolves to the struct's defining file.
    #[test]
    fn a035_cross_file_bare_assoc_mutual_recursion_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a::{A};
                    pub fn main() -> i32 { return A::ping(); }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b::{B};
                    pub struct A {
                        v: i32;
                        pub fn ping() -> i32 { return B::pong(); }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use lib::a::{A};
                    pub struct B {
                        v: i32;
                        pub fn pong() -> i32 { return A::ping(); }
                    }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("lib.a.A.ping") && msg.contains("lib.b.B.pong"),
            "bare associated cycle should name both files' associated functions, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    /// Mutual recursion through cross-file *namespace-qualified associated*
    /// functions (`lib::b::B::pong()`) must be rejected. (The namespace-qualified
    /// arm already recorded a target; this guards it stays caught alongside the new
    /// method/bare-assoc arms.)
    #[test]
    fn a035_cross_file_namespaced_assoc_mutual_recursion_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a;
                    pub fn main() -> i32 { return lib::a::A::ping(); }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b;
                    pub struct A {
                        v: i32;
                        pub fn ping() -> i32 { return lib::b::B::pong(); }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    use lib::a;
                    pub struct B {
                        v: i32;
                        pub fn pong() -> i32 { return lib::a::A::ping(); }
                    }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("lib.a.A.ping") && msg.contains("lib.b.B.pong"),
            "namespaced associated cycle should name both files' functions, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    /// A mixed cycle: an entry free function `drive()` calls a cross-file instance
    /// method `x.step()`, which calls back into the entry via `root::drive()`. Both
    /// the method edge and the `root::` free-function edge must be recorded.
    #[test]
    fn a035_cross_file_mixed_root_and_method_recursion_rejected() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a;
                    pub fn drive() -> i32 {
                        let x: lib::a::A = lib::a::A::make();
                        return x.step();
                    }
                    pub fn main() -> i32 { return drive(); }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use root;
                    pub struct A {
                        v: i32;
                        pub fn make() -> A { return A { v: 3 }; }
                        pub fn step(self) -> i32 { return root::drive() + self.v; }
                    }
                "#,
            ),
        ];
        let diag = recursion_diag_multi(files);
        let msg = diag.to_string();
        assert!(
            msg.contains("drive") && msg.contains("lib.a.A.step"),
            "mixed `root::` + method cycle should name `drive` and `lib.a.A.step`, got: {msg}"
        );
        assert_eq!(diag.rule_id(), "A035");
    }

    /// A legitimate non-recursive cross-file chain through an associated function
    /// and instance methods must still compile: the method/assoc edges are
    /// recorded, but they form no cycle.
    #[test]
    fn a035_non_recursive_cross_file_method_chain_accepted() {
        let files: &[(Vec<&str>, &str)] = &[
            (
                vec![],
                r#"
                    use lib::a::{A};
                    pub fn main() -> i32 {
                        let x: A = A::make();
                        return x.value();
                    }
                "#,
            ),
            (
                vec!["lib", "a"],
                r#"
                    use lib::b::{B};
                    pub struct A {
                        v: i32;
                        pub fn make() -> A { return A { v: 10 }; }
                        pub fn value(self) -> i32 {
                            let y: B = B::make();
                            return self.v + y.get();
                        }
                    }
                "#,
            ),
            (
                vec!["lib", "b"],
                r#"
                    pub struct B {
                        w: i32;
                        pub fn make() -> B { return B { w: 5 }; }
                        pub fn get(self) -> i32 { return self.w; }
                    }
                "#,
            ),
        ];
        assert!(
            !has_recursion_multi(files),
            "a non-recursive cross-file method/assoc chain must not trip A035"
        );
    }
}
