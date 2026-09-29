/// Integration tests for analysis rule A057.
///
/// - A057: AssertAlwaysFails — an `assert` in executable code whose condition is
///   false on every run that reaches it is an unconditional halt, and is
///   rejected. It closes the path A056 leaves open: a guard whose failing branch
///   halts instead of recovering.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::build_ast;
    use inference_analysis::errors::AnalysisDiagnostic;

    /// The line of every A057 finding in `source`.
    fn a057_lines(source: &str) -> Vec<u32> {
        let arena = build_ast(source.to_string());
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context();
        let Err(errors) = inference_analysis::analyze(&ctx) else {
            return Vec::new();
        };
        errors
            .errors()
            .iter()
            .filter_map(|diagnostic| match diagnostic {
                AnalysisDiagnostic::AssertAlwaysFails { location } => Some(location.start_line),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a057_assert_false_is_rejected() {
        let source = "pub fn f() {\n    assert(false);\n}";
        assert_eq!(a057_lines(source), vec![2]);
    }

    /// The case A056 would otherwise let through: the out-of-range branch of a
    /// guard halts instead of recovering.
    #[test]
    fn a057_halting_failure_branch_of_a_bounds_guard_is_rejected() {
        let source = "pub fn f(i: u32) -> i32 {\n\
                      let a: [i32; 4] = [1, 2, 3, 4];\n\
                      let mut r: i32 = 0;\n\
                      if i < 4 { r = a[i]; } else {\n\
                      assert(i < 4);\n\
                      }\n\
                      return r;\n\
                      }";
        assert_eq!(a057_lines(source), vec![5]);
    }

    #[test]
    fn a057_assert_contradicting_a_known_value_is_rejected() {
        let source = "pub fn f() {\n    let x: i32 = 4;\n    assert(x == 5);\n}";
        assert_eq!(a057_lines(source), vec![3]);
    }

    /// An assert that can hold on some run is a check, not a halt.
    #[test]
    fn a057_assert_that_can_hold_is_accepted() {
        assert_eq!(
            a057_lines("pub fn f(i: u32) { assert(i < 4); }"),
            Vec::<u32>::new()
        );
        assert_eq!(
            a057_lines("pub fn f() { assert(true); }"),
            Vec::<u32>::new()
        );
    }

    /// No run reaches the inner branch, so its assert never halts anything.
    #[test]
    fn a057_unreachable_assert_is_accepted() {
        let source = "pub fn f(i: u32) { if i < 4 { if i >= 4 { assert(false); } } }";
        assert_eq!(a057_lines(source), Vec::<u32>::new());
    }

    /// In a specification an `assert` states a claim; it is not a check the
    /// program makes.
    #[test]
    fn a057_specification_body_is_not_examined() {
        let source = "spec S { fn p() forall { let x: i32 = @; assume { assert(false); } } }";
        assert_eq!(a057_lines(source), Vec::<u32>::new());
    }
}
