//! End-to-end tests that diagnostics in a multi-file program name the file they
//! belong to.
//!
//! Source locations are per-file-local in the merged arena, so a bare `line:col`
//! from an imported file would be misread as the entry file the user invoked.
//! Both the type-check channel and the analysis channel must prefix a non-entry
//! finding with the file's `::`-joined module path (`lib::geom:line:col`), while
//! the entry file stays a bare `line:col` so single-file programs are unchanged.

#[cfg(test)]
mod tests {
    use crate::utils::try_type_check_multi_file;

    /// Type-checks a multi-file program and returns the aggregated diagnostic
    /// string. The first pair is the entry file (empty module path).
    fn type_check_error(files: &[(Vec<&str>, &str)]) -> String {
        try_type_check_multi_file(files)
            .err()
            .map(|e| e.to_string())
            .expect("program was expected to fail type checking")
    }

    /// Analyzes a multi-file program and returns the rendered findings string.
    /// The program must type-check; analysis findings are what we assert on.
    /// Both the error channel (`AnalysisErrors`) and the non-fatal channel
    /// (`AnalysisResult`, returned when no `Error`-severity finding exists) render
    /// through the same file-naming path, so either is rendered identically.
    fn analysis_findings(files: &[(Vec<&str>, &str)]) -> String {
        let ctx = try_type_check_multi_file(files)
            .expect("program was expected to type-check before analysis");
        match inference_analysis::analyze(&ctx) {
            Ok(result) => result.to_string(),
            Err(errors) => errors.to_string(),
        }
    }

    /// Whether the `line:col`-bearing tail of `message` (the part after an
    /// optional `Type checking failed: ` prefix) begins with a bare line number
    /// rather than an `ident::…:` file label. A bare diagnostic looks like
    /// `12:34: …`; a file-named one looks like `lib::geom:12:34: …`. This is
    /// precise about the *prefix* rather than rejecting any `::` in the message,
    /// since `::` appears legitimately inside many messages (method paths, cycle
    /// chains).
    fn starts_with_bare_location(message: &str) -> bool {
        let tail = message
            .strip_prefix("Type checking failed: ")
            .unwrap_or(message);
        // The first `:`-delimited segment of a bare diagnostic is the line number;
        // a file-named diagnostic's first segment is a module-path identifier.
        tail.split(':')
            .next()
            .is_some_and(|head| !head.is_empty() && head.bytes().all(|b| b.is_ascii_digit()))
    }

    // type-check channel

    /// A return-type mismatch in an imported file names that file, not the entry
    /// file the user invoked.
    #[test]
    fn type_error_in_return_position_names_imported_file() {
        let files = [
            (vec![], "use lib::geom; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; }\npub fn bad() -> i32 { return Point { x: 1, y: 2 }; }",
            ),
        ];
        let message = type_check_error(&files);
        assert!(
            message.contains("lib::geom:"),
            "return-type error must name the imported file `lib::geom`, got: {message}"
        );
        assert!(
            message.contains("type mismatch"),
            "expected a type-mismatch diagnostic, got: {message}"
        );
    }

    /// A `let`-binding type mismatch in an imported file names that file.
    #[test]
    fn type_error_in_let_binding_names_imported_file() {
        let files = [
            (vec![], "use lib::vals; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "vals"],
                "pub fn bad() -> i32 { let x: i32 = true; return x; }",
            ),
        ];
        let message = type_check_error(&files);
        assert!(
            message.contains("lib::vals:"),
            "let-binding error must name the imported file `lib::vals`, got: {message}"
        );
    }

    /// An argument-type / call error in an imported file names that file.
    #[test]
    fn type_error_in_call_argument_names_imported_file() {
        let files = [
            (vec![], "use lib::call; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "call"],
                "fn takes(a: i32) -> i32 { return a; }\npub fn bad() -> i32 { return takes(true); }",
            ),
        ];
        let message = type_check_error(&files);
        assert!(
            message.contains("lib::call:"),
            "call-argument error must name the imported file `lib::call`, got: {message}"
        );
    }

    /// A single-file program's type-check diagnostic is a bare `line:col` with no
    /// file prefix — the entry file is the one the user invoked.
    #[test]
    fn type_error_in_entry_file_stays_bare() {
        let files = [(vec![], "pub fn main() -> i32 { return true; }")];
        let message = type_check_error(&files);
        assert!(
            starts_with_bare_location(&message),
            "entry-file type error must start with a bare line:col, got: {message}"
        );
        assert!(
            !message.contains("<entry>"),
            "entry-file type error must not carry the parse-channel `<entry>` label, got: {message}"
        );
        assert!(
            message.contains("type mismatch"),
            "expected a type-mismatch diagnostic, got: {message}"
        );
    }

    /// A type error in a deeply nested imported file names the full module path.
    #[test]
    fn type_error_in_deep_module_path_names_full_path() {
        let files = [
            (vec![], "use a::b::c; pub fn main() -> i32 { return 0; }"),
            (vec!["a", "b", "c"], "pub fn bad() -> i32 { return true; }"),
        ];
        let message = type_check_error(&files);
        assert!(
            message.contains("a::b::c:"),
            "deep-path type error must name `a::b::c`, got: {message}"
        );
    }

    /// Two identical type errors at the same line:col in two different imported
    /// files render distinguishably, each named by its own file.
    #[test]
    fn type_errors_at_same_location_in_two_files_are_distinguishable() {
        let files = [
            (
                vec![],
                "use lib::a; use lib::b; pub fn main() -> i32 { return 0; }",
            ),
            (vec!["lib", "a"], "pub fn bad() -> i32 { return true; }"),
            (vec!["lib", "b"], "pub fn bad() -> i32 { return true; }"),
        ];
        let message = type_check_error(&files);
        assert!(
            message.contains("lib::a:"),
            "expected `lib::a` to be named, got: {message}"
        );
        assert!(
            message.contains("lib::b:"),
            "expected `lib::b` to be named, got: {message}"
        );
    }

    // analysis channel

    /// A037 (constant array index out of bounds) in an imported file names that
    /// file.
    #[test]
    fn analysis_a037_in_imported_file_names_file() {
        let files = [
            (vec![], "use lib::a; pub fn main() -> i32 { return lib::a::oob(); }"),
            (
                vec!["lib", "a"],
                "pub fn oob() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; }",
            ),
        ];
        let findings = analysis_findings(&files);
        assert!(
            findings.contains("lib::a:") && findings.contains("[A037]"),
            "A037 in an imported file must name `lib::a`, got: {findings}"
        );
    }

    /// A035 (recursion) for a cross-file cycle names the file holding the call
    /// site that closes the cycle.
    #[test]
    fn analysis_a035_cross_file_recursion_names_file() {
        let files = [
            (
                vec![],
                "use lib::r; pub fn pong() -> i32 { return lib::r::ping(); } pub fn main() -> i32 { return pong(); }",
            ),
            (
                vec!["lib", "r"],
                "use root; pub fn ping() -> i32 { return root::pong(); }",
            ),
        ];
        let findings = analysis_findings(&files);
        assert!(
            findings.contains("lib::r:") && findings.contains("[A035]"),
            "cross-file A035 must name the file with the cycle-closing call, got: {findings}"
        );
    }

    /// Two A037 findings at the same line:col in two different imported files
    /// render distinguishably, each named by its own file.
    #[test]
    fn analysis_findings_at_same_location_in_two_files_are_distinguishable() {
        let files = [
            (
                vec![],
                "use lib::a; use lib::b; pub fn main() -> i32 { let x: i32 = lib::a::oob(); let y: i32 = lib::b::oob(); return x; }",
            ),
            (
                vec!["lib", "a"],
                "pub fn oob() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; }",
            ),
            (
                vec!["lib", "b"],
                "pub fn oob() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; }",
            ),
        ];
        let findings = analysis_findings(&files);
        assert!(
            findings.contains("lib::a:") && findings.contains("lib::b:"),
            "same-location findings in two files must each be named, got: {findings}"
        );
    }

    /// A single-file analysis finding is a bare `line:col` with no file prefix.
    #[test]
    fn analysis_finding_in_entry_file_stays_bare() {
        let files = [(
            vec![],
            "pub fn main() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; }",
        )];
        let findings = analysis_findings(&files);
        assert!(
            findings.contains("[A037]"),
            "expected an A037 finding, got: {findings}"
        );
        assert!(
            starts_with_bare_location(&findings),
            "entry-file analysis finding must start with a bare line:col, got: {findings}"
        );
    }

    /// An A037 finding inside a `spec` block in an imported file names the file;
    /// spec bodies are walked carrying their declaring file's module path.
    #[test]
    fn analysis_finding_inside_spec_in_imported_file_names_file() {
        let files = [
            (vec![], "use lib::s; pub fn main() -> i32 { return 0; }"),
            (
                vec!["lib", "s"],
                "spec S { fn check() -> i32 { let a: [i32; 3] = [1,2,3]; return a[5]; } }",
            ),
        ];
        let findings = analysis_findings(&files);
        assert!(
            findings.contains("lib::s:") && findings.contains("[A037]"),
            "A037 inside a spec in an imported file must name `lib::s`, got: {findings}"
        );
    }

    /// A non-fatal finding (a warning) in an imported file is named through the
    /// `AnalysisResult` (Ok) channel — the program type-checks and has no
    /// `Error`-severity finding, so `analyze` returns `Ok`. A011 (empty struct)
    /// is the nameable warning here.
    #[test]
    fn analysis_warning_in_imported_file_names_file_via_ok_channel() {
        let files = [
            (vec![], "use lib::w; pub fn main() -> i32 { return lib::w::helper(); }"),
            (
                vec!["lib", "w"],
                "pub struct Empty {} pub fn helper() -> i32 { return 1; }",
            ),
        ];
        let ctx = try_type_check_multi_file(&files)
            .expect("program was expected to type-check");
        let result = inference_analysis::analyze(&ctx)
            .expect("program has only a warning, so analysis returns Ok");
        let findings = result.to_string();
        assert!(
            findings.contains("warning[A011]") && findings.contains("lib::w:"),
            "an imported-file warning must be named via the Ok channel, got: {findings}"
        );
    }

    /// A warning in the entry file stays a bare `line:col` through the Ok channel,
    /// so single-file warnings are unchanged.
    #[test]
    fn analysis_warning_in_entry_file_stays_bare_via_ok_channel() {
        let files = [(vec![], "pub struct Empty {} pub fn main() -> i32 { return 0; }")];
        let ctx = try_type_check_multi_file(&files)
            .expect("program was expected to type-check");
        let result = inference_analysis::analyze(&ctx)
            .expect("program has only a warning, so analysis returns Ok");
        let findings = result.to_string();
        assert!(
            findings.contains("warning[A011]"),
            "expected an A011 warning, got: {findings}"
        );
        assert!(
            starts_with_bare_location(&findings),
            "entry-file warning must start with a bare line:col, got: {findings}"
        );
    }
}
