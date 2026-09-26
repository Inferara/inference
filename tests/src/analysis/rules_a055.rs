/// Integration tests for analysis rule A055.
///
/// - A055: `ParamWordsExceeded` — at the `spacewasm` target, the signature code
///   generation emits for a function may declare at most 255 four-byte
///   parameter words: two for `i64` and `u64`, one for every other parameter
///   type, one for a `self` receiver, and one for the hidden pointer a struct or
///   array result is written through.
///
/// Three groups. The verdicts: every lowering shape at, just under and just over
/// the limit, and where the finding is reported. The gating: which targets and
/// which functions the rule measures at all. And the differential test that
/// holds the rule's count to the words code generation actually emits, over the
/// corpus and a purpose-built matrix, against the conformance checker's reading
/// of every emitted module — the rule mirrors code generation's lowering rather
/// than calling it, and this is what keeps the mirror honest.
#[cfg(test)]
mod analysis_rules_tests {
    use crate::utils::{build_ast, try_type_check_multi_file};
    use inference_analysis::errors::{AnalysisDiagnostic, ParamWords, ParamWordsGroup};
    use inference_analysis::{AnalysisOptions, TargetName};
    use inference_ast::nodes::Location;
    use inference_type_checker::typed_context::TypedContext;

    fn type_check(source: &str) -> TypedContext {
        let arena = build_ast(source.to_string());
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("type checking should succeed for analysis test input")
            .typed_context()
    }

    fn options(target: TargetName) -> AnalysisOptions {
        AnalysisOptions {
            target,
            ..AnalysisOptions::default()
        }
    }

    /// Every A055 finding `ctx` produces when analyzed for `target`, in report
    /// order.
    fn a055_at(ctx: &TypedContext, target: TargetName) -> Vec<AnalysisDiagnostic> {
        match inference_analysis::analyze_with_options(ctx, options(target)) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .filter(|e| matches!(e, AnalysisDiagnostic::ParamWordsExceeded { .. }))
                .cloned()
                .collect(),
        }
    }

    fn spacewasm_findings(source: &str) -> Vec<AnalysisDiagnostic> {
        a055_at(&type_check(source), TargetName::SpaceWasm)
    }

    /// The function and the itemized words of the one A055 finding `source`
    /// produces at the `spacewasm` target.
    fn only_finding(source: &str) -> (String, ParamWords) {
        let findings = spacewasm_findings(source);
        assert_eq!(findings.len(), 1, "expected exactly one A055 finding: {findings:?}");
        match findings.into_iter().next() {
            Some(AnalysisDiagnostic::ParamWordsExceeded {
                function, words, ..
            }) => (function, words),
            other => panic!("not an A055 finding: {other:?}"),
        }
    }

    /// `count` parameters of type `ty`, named `{prefix}0`, `{prefix}1`, and so
    /// on, as a parameter list.
    pub(super) fn params(prefix: &str, ty: &str, count: usize) -> String {
        (0..count)
            .map(|index| format!("{prefix}{index}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn group(ty: &str, count: u32, words_each: u32) -> ParamWordsGroup {
        ParamWordsGroup {
            ty: ty.to_string(),
            count,
            words_each,
        }
    }

    fn words(params: Vec<ParamWordsGroup>) -> ParamWords {
        ParamWords {
            receiver: false,
            params,
            result_pointer: None,
        }
    }

    const POINT: &str = "pub struct Point { x: i32; y: i32; }\n";
    const PAIR: &str = "pub struct Pair { a: i32; b: i32; }\n";
    const COLOR: &str = "pub enum Color { Red, Green, Blue }\n";

    // ---------------------------------------------------------------------
    // Every lowering shape, at, just under and just over the limit
    // ---------------------------------------------------------------------

    /// Every scalar narrower than 64 bits, and `bool`, lowers to an `i32` and
    /// costs one word, so 255 of them is the most a function may take.
    #[test]
    fn a055_one_word_scalars_at_under_and_over_the_limit() {
        for ty in ["i8", "u8", "i16", "u16", "i32", "u32", "bool"] {
            for fits in [254, 255] {
                let source = format!("pub fn f({}) {{ }}", params("p", ty, fits));
                assert!(
                    spacewasm_findings(&source).is_empty(),
                    "{fits} `{ty}` parameters are {fits} words and fit"
                );
            }
            let over = format!("pub fn f({}) {{ }}", params("p", ty, 256));
            let (function, words_found) = only_finding(&over);
            assert_eq!(function, "f");
            assert_eq!(words_found, words(vec![group(ty, 256, 1)]));
            assert_eq!(words_found.total(), 256);
        }
    }

    /// A 64-bit scalar lowers to an `i64`, which the interpreter counts as two
    /// words: 127 of them and one `u32` is exactly the limit, 128 of them is
    /// one over, while 128 `i32` would be half of it.
    #[test]
    fn a055_64_bit_scalars_are_two_words() {
        for ty in ["i64", "u64"] {
            let under = format!("pub fn f({}) {{ }}", params("p", ty, 127));
            assert!(spacewasm_findings(&under).is_empty(), "127 `{ty}` are 254 words");
            let at = format!("pub fn f({}, last: u32) {{ }}", params("p", ty, 127));
            assert!(spacewasm_findings(&at).is_empty(), "127 `{ty}` and a `u32` are 255");
            let over = format!("pub fn f({}) {{ }}", params("p", ty, 128));
            let (_, words_found) = only_finding(&over);
            assert_eq!(words_found, words(vec![group(ty, 128, 2)]));
            assert_eq!(words_found.total(), 256);
        }
    }

    /// A struct, an array at any nesting and of any element width, and an enum
    /// are each one word: the first two travel as an address, the third as its
    /// tag. A `[i64; 4]` parameter is one word, not two and not eight.
    #[test]
    fn a055_compound_and_enum_parameters_are_one_word() {
        for ty in [
            "Point",
            "[i64; 4]",
            "[[i32; 2]; 3]",
            "[Point; 2]",
            "Color",
            "[Color; 3]",
        ] {
            for fits in [254, 255] {
                let source = format!("{POINT}{COLOR}pub fn f({}) {{ }}", params("p", ty, fits));
                assert!(
                    spacewasm_findings(&source).is_empty(),
                    "{fits} `{ty}` parameters are {fits} words and fit"
                );
            }
            let over = format!("{POINT}{COLOR}pub fn f({}) {{ }}", params("p", ty, 256));
            let (_, words_found) = only_finding(&over);
            assert_eq!(words_found, words(vec![group(ty, 256, 1)]));
        }
    }

    /// The same boundary written with the hidden pointer: 127 `i64` and a `u32`
    /// are exactly 255 words until the function returns a struct, which the
    /// caller receives through a pointer passed ahead of every parameter.
    #[test]
    fn a055_the_hidden_result_pointer_is_one_word() {
        let list = format!("{}, last: u32", params("p", "i64", 127));
        let scalar = format!("pub fn f({list}) -> i32 {{ return 1; }}");
        assert!(spacewasm_findings(&scalar).is_empty(), "a scalar result costs nothing");

        let returns_struct = format!(
            "{PAIR}pub fn f({list}) -> Pair {{\n    let r: Pair = Pair {{ a: 1, b: 2 }};\n    \
             return r;\n}}"
        );
        let (function, words_found) = only_finding(&returns_struct);
        assert_eq!(function, "f");
        assert_eq!(
            words_found,
            ParamWords {
                receiver: false,
                params: vec![group("i64", 127, 2), group("u32", 1, 1)],
                result_pointer: Some("Pair".to_string()),
            }
        );
        assert_eq!(words_found.total(), 256);
    }

    /// An array result takes the hidden pointer too. An enum result is its tag,
    /// returned as a value, and a 64-bit or unit result is a result: none of
    /// the three costs a parameter word.
    #[test]
    fn a055_only_a_struct_or_array_result_takes_a_pointer() {
        let list = params("p", "u32", 255);
        let array = format!(
            "pub fn f({list}) -> [i32; 2] {{\n    let a: [i32; 2] = [1, 2];\n    return a;\n}}"
        );
        let (_, words_found) = only_finding(&array);
        assert_eq!(words_found.result_pointer.as_deref(), Some("[i32; 2]"));
        assert_eq!(words_found.total(), 256);

        for (result, body) in [
            ("Color", "return Color::Red;"),
            ("u64", "return 1;"),
            ("i64", "return 1;"),
            ("()", ""),
        ] {
            let source = format!("{COLOR}pub fn f({list}) -> {result} {{ {body} }}");
            assert!(
                spacewasm_findings(&source).is_empty(),
                "a `{result}` result costs no parameter word"
            );
        }
    }

    /// A method's `self` is the address of its struct: one word, whether or not
    /// it is `mut`. An associated function has no receiver to count.
    #[test]
    fn a055_a_method_receiver_is_one_word() {
        for receiver in ["self", "mut self"] {
            let fits = format!(
                "pub struct S {{\n    x: i32;\n    pub fn f({receiver}, {}) -> i32 {{ \
                 return self.x; }}\n}}",
                params("p", "u32", 254)
            );
            assert!(spacewasm_findings(&fits).is_empty(), "`{receiver}` and 254 words fit");

            let over = format!(
                "pub struct S {{\n    x: i32;\n    pub fn f({receiver}, {}) -> i32 {{ \
                 return self.x; }}\n}}",
                params("p", "u32", 255)
            );
            let (function, words_found) = only_finding(&over);
            assert_eq!(function, "S::f", "a method is named the way a call spells it");
            assert_eq!(
                words_found,
                ParamWords {
                    receiver: true,
                    params: vec![group("u32", 255, 1)],
                    result_pointer: None,
                }
            );
        }

        let associated = format!(
            "pub struct S {{\n    x: i32;\n    pub fn g({}) -> i32 {{ return 1; }}\n}}",
            params("p", "u32", 256)
        );
        let (function, words_found) = only_finding(&associated);
        assert_eq!(function, "S::g");
        assert!(!words_found.receiver, "an associated function takes no receiver");
    }

    /// A method returning its own struct pays for both hidden words: the
    /// receiver and the result pointer.
    #[test]
    fn a055_a_method_can_pay_for_a_receiver_and_a_result_pointer() {
        let source = format!(
            "pub struct S {{\n    x: i32;\n    pub fn f(self, {}) -> S {{\n        \
             let s: S = S {{ x: self.x }};\n        return s;\n    }}\n}}",
            params("p", "u32", 254)
        );
        let (function, words_found) = only_finding(&source);
        assert_eq!(function, "S::f");
        assert_eq!(
            words_found,
            ParamWords {
                receiver: true,
                params: vec![group("u32", 254, 1)],
                result_pointer: Some("S".to_string()),
            }
        );
        assert_eq!(words_found.total(), 256);
    }

    /// `_: T` binds nothing, but it still occupies a slot and every call passes
    /// an argument for it, so it costs what a named parameter of `T` costs and
    /// groups with one.
    #[test]
    fn a055_an_ignored_parameter_costs_its_type() {
        let ignored = (0..256).map(|_| "_: u32").collect::<Vec<_>>().join(", ");
        let (_, words_found) = only_finding(&format!("pub fn f({ignored}) {{ }}"));
        assert_eq!(words_found, words(vec![group("u32", 256, 1)]));

        let half = (0..128).map(|_| "_: i64").collect::<Vec<_>>().join(", ");
        let mixed = format!("pub fn f({half}, {}) {{ }}", params("p", "i64", 1));
        let (_, words_found) = only_finding(&mixed);
        assert_eq!(words_found, words(vec![group("i64", 129, 2)]));
    }

    /// A bare positional parameter is A050's finding, and it is counted as the
    /// `_: T` that repairs it: the words stay what they will be once it is
    /// fixed.
    #[test]
    fn a055_a_bare_positional_parameter_is_counted_as_its_repair() {
        let bare = |ty: &str, count: usize| vec![ty; count].join(", ");
        let ctx = type_check(&format!("pub fn f({}) {{ }}", bare("u32", 256)));
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("A050 and A055 both reject it");
        assert!(errors.errors().iter().any(|e| e.rule_id() == "A050"));
        let (_, words_found) = only_finding(&format!("pub fn f({}) {{ }}", bare("u32", 256)));
        assert_eq!(words_found, words(vec![group("u32", 256, 1)]));

        let (_, words_found) = only_finding(&format!("pub fn f({}) {{ }}", bare("i64", 128)));
        assert_eq!(words_found, words(vec![group("i64", 128, 2)]));
        assert!(
            spacewasm_findings(&format!("pub fn f({}) {{ }}", bare("i64", 127))).is_empty(),
            "127 bare `i64` are 254 words"
        );
    }

    /// A unit parameter has no argument slot, so it adds nothing: it is A049's
    /// finding, and a function that fits without it still fits.
    #[test]
    fn a055_a_unit_parameter_adds_nothing() {
        let fits = format!("pub fn f({}, u: ()) {{ }}", params("p", "u32", 255));
        let ctx = type_check(&fits);
        assert!(a055_at(&ctx, TargetName::SpaceWasm).is_empty());
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("A049 rejects the unit parameter");
        assert!(errors.errors().iter().any(|e| e.rule_id() == "A049"));

        let over = format!("pub fn f({}, u: ()) {{ }}", params("p", "u32", 256));
        let (_, words_found) = only_finding(&over);
        assert_eq!(words_found, words(vec![group("u32", 256, 1)]));
    }

    /// Every parameter type with no lowering adds nothing, whichever rule or
    /// phase refuses it: `string` (A048), an array of `()` (A049), and a
    /// function type or a `spec`'s name, which pass analysis and code
    /// generation refuses. At 255 other words the function fits, and at 256 the
    /// itemization leaves the parameter out.
    #[test]
    fn a055_a_parameter_with_no_lowering_adds_nothing() {
        for (extra, owner) in [
            ("s: string", Some("A048")),
            ("a: [(); 2]", Some("A049")),
            ("f: fn(i32) -> i32", None),
            ("t: S", None),
        ] {
            let spec = "spec S { fn prop() { } }\n";
            let fits = format!("{spec}pub fn f({}, {extra}) {{ }}", params("p", "u32", 255));
            let ctx = type_check(&fits);
            assert!(a055_at(&ctx, TargetName::SpaceWasm).is_empty(), "`{extra}` adds nothing");
            let verdict =
                inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm));
            match owner {
                Some(rule) => assert!(
                    verdict.is_err_and(|e| e.errors().iter().any(|d| d.rule_id() == rule)),
                    "`{extra}` is {rule}'s finding"
                ),
                None => assert!(verdict.is_ok(), "`{extra}` passes analysis"),
            }

            let over = format!("{spec}pub fn f({}, {extra}) {{ }}", params("p", "u32", 256));
            let (_, words_found) = only_finding(&over);
            assert_eq!(words_found, words(vec![group("u32", 256, 1)]), "`{extra}`");
        }
    }

    /// A type parameter has no lowering either: a generic function is A051's
    /// finding, and however many parameters of the type it declares, A055
    /// counts none of them.
    #[test]
    fn a055_a_type_parameter_adds_nothing() {
        let source = format!("pub fn f T'({}) -> u32 {{ return 1; }}", params("p", "T", 300));
        let ctx = type_check(&source);
        assert!(a055_at(&ctx, TargetName::SpaceWasm).is_empty());
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("A051 rejects the declaration");
        assert!(errors.errors().iter().any(|e| e.rule_id() == "A051"));
    }

    /// The breakdown groups parameters by type in the order each type first
    /// appears, so the itemization follows the declaration.
    #[test]
    fn a055_groups_follow_the_declaration_order() {
        let source = format!(
            "{POINT}pub fn f(a: u8, {}, b: u8, {}, c: Point) {{ }}",
            params("w", "i64", 100),
            params("n", "u32", 100)
        );
        let (_, words_found) = only_finding(&source);
        assert_eq!(
            words_found,
            words(vec![
                group("u8", 2, 1),
                group("i64", 100, 2),
                group("u32", 100, 1),
                group("Point", 1, 1),
            ])
        );
        assert_eq!(words_found.total(), 303);
    }

    /// The finding sits on the declaration and spans its parameter list, from
    /// the first parameter to the end of the last, across however many lines
    /// the list is written on.
    #[test]
    fn a055_is_reported_on_the_parameter_list() {
        let list = (0..256)
            .map(|index| format!("    p{index}: u32"))
            .collect::<Vec<_>>()
            .join(",\n");
        let source = format!("pub fn wide(\n{list}\n) {{ }}\n");
        let findings = spacewasm_findings(&source);
        assert_eq!(findings.len(), 1);
        let location: Location = *findings[0].location();
        let start = source.find("p0: u32").expect("first parameter");
        let end = source.find("p255: u32").expect("last parameter") + "p255: u32".len();
        assert_eq!(location.offset_start as usize, start);
        assert_eq!(location.offset_end as usize, end);
        assert_eq!((location.start_line, location.start_column), (2, 5));
        assert_eq!((location.end_line, location.end_column), (257, 14));
    }

    /// A method's list opens on its receiver and an unnamed parameter's on the
    /// `_`, so the span starts there.
    #[test]
    fn a055_the_span_starts_at_a_receiver_or_an_unnamed_parameter() {
        let list = |count: usize| {
            (0..count)
                .map(|index| format!("        p{index}: u32"))
                .collect::<Vec<_>>()
                .join(",\n")
        };
        for first in ["self", "mut self"] {
            let source = format!(
                "pub struct S {{\n    x: i32;\n    pub fn f(\n        {first},\n{}\n    ) \
                 -> i32 {{ return self.x; }}\n}}\n",
                list(255)
            );
            let findings = spacewasm_findings(&source);
            assert_eq!(findings.len(), 1, "`{first}` and 255 words is 256");
            let location = *findings[0].location();
            let start = source.find(first).expect("the receiver");
            let end = source.find("p254: u32").expect("last parameter") + "p254: u32".len();
            assert_eq!(location.offset_start as usize, start, "`{first}`");
            assert_eq!(location.offset_end as usize, end, "`{first}`");
            assert_eq!((location.start_line, location.start_column), (4, 9), "`{first}`");
        }

        let source = format!("pub fn f(_: u32, {}) {{ }}", params("p", "u32", 255));
        let findings = spacewasm_findings(&source);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].location().offset_start as usize,
            source.find("_: u32").expect("the unnamed parameter")
        );
    }

    /// Every function over the limit is its own finding, and one under it is
    /// none.
    #[test]
    fn a055_reports_each_function_over_the_limit() {
        let source = format!(
            "pub fn a({}) {{ }}\npub fn b({}) {{ }}\npub fn c({}) {{ }}\npub struct S {{\n    \
             x: i32;\n    pub fn m(self, {}) -> i32 {{ return self.x; }}\n    pub fn n(self, \
             {}) -> i32 {{ return self.x; }}\n}}\n",
            params("p", "u32", 256),
            params("p", "u32", 255),
            params("p", "i64", 200),
            params("p", "u32", 255),
            params("p", "u32", 254),
        );
        let reported: Vec<(String, u32)> = spacewasm_findings(&source)
            .into_iter()
            .map(|finding| match finding {
                AnalysisDiagnostic::ParamWordsExceeded {
                    function, words, ..
                } => (function, words.total()),
                other => panic!("not an A055 finding: {other:?}"),
            })
            .collect();
        let expected = [("a", 256), ("c", 400), ("S::m", 256)]
            .map(|(function, total)| (function.to_string(), total));
        assert_eq!(reported, expected);
    }

    /// The whole rendered finding, as `infc` prints it.
    #[test]
    fn a055_renders_with_its_rule_id_and_location() {
        let source = format!("pub fn wide({}) -> u32 {{ return p0; }}", params("p", "u32", 256));
        let ctx = type_check(&source);
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("256 words is over the limit");
        let rendered = errors.to_string();
        assert!(
            rendered.contains(
                "1:13: error[A055]: `wide` declares 256 parameter words, and SpaceWasm accepts \
                 at most 255 in one function: 256 for 256 `u32` parameters; SpaceWasm counts"
            ),
            "got: {rendered}"
        );
    }

    // ---------------------------------------------------------------------
    // Which targets and which functions
    // ---------------------------------------------------------------------

    /// The limit is SpaceWasm's alone. The same program analyzed for the other
    /// targets, and under the default settings, has no A055 finding.
    #[test]
    fn a055_is_silent_for_every_other_target() {
        let source = format!("pub fn wide({}) -> u32 {{ return p0; }}", params("p", "u32", 300));
        let ctx = type_check(&source);
        assert_eq!(a055_at(&ctx, TargetName::SpaceWasm).len(), 1);
        for target in [TargetName::Wasm32, TargetName::Stellar] {
            assert!(
                a055_at(&ctx, target).is_empty(),
                "`{}` sets no parameter-word limit",
                target.as_str()
            );
        }
        assert!(
            inference_analysis::analyze(&ctx).is_ok(),
            "the default settings are the default target's"
        );
        assert_eq!(AnalysisOptions::default().target, TargetName::DEFAULT);
    }

    /// A `spec` is stripped from a compile-mode build, and a SpaceWasm build is
    /// never anything else, so its functions are not measured — nor counted
    /// among the functions the artifact defines.
    #[test]
    fn a055_does_not_measure_a_spec() {
        let source = format!(
            "spec S {{\n    fn wide({}) {{ }}\n}}\npub fn main() -> i32 {{ return 0; }}",
            params("p", "u32", 256)
        );
        let ctx = type_check(&source);
        assert!(a055_at(&ctx, TargetName::SpaceWasm).is_empty());
        let counted: Vec<String> = inference_analysis::param_words(&ctx)
            .keys()
            .map(|key| key.name_section_symbol())
            .collect();
        assert_eq!(counted, vec!["main".to_string()]);
    }

    /// An `external fn` is an import, not a definition: its caps are the host
    /// import check's, and a linked body is measured by the post-link check.
    #[test]
    fn a055_does_not_measure_an_external_function() {
        let source = format!(
            "external fn wide({}) -> u32;\nuse {{ wide }} from lib;\npub fn main() -> i32 \
             {{ return 0; }}",
            params("p", "u32", 256)
        );
        let ctx = type_check(&source);
        assert!(a055_at(&ctx, TargetName::SpaceWasm).is_empty());
        assert_eq!(inference_analysis::param_words(&ctx).len(), 1, "only `main` is defined");
    }

    /// A function in an imported file is measured like one in the entry file,
    /// and its finding names the file it is in.
    #[test]
    fn a055_measures_an_imported_file_and_names_it() {
        let lib = format!("pub fn wide({}) -> u64 {{ return p0; }}", params("p", "u64", 128));
        let ctx = try_type_check_multi_file(&[
            (vec![], "use lib::big;\npub fn main() -> i32 { return 0; }"),
            (vec!["lib", "big"], lib.as_str()),
        ])
        .expect("the program type-checks");
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("the imported function is over the limit");
        let rendered = errors.to_string();
        assert!(
            rendered.contains("lib::big:1:13: error[A055]: `wide` declares 256 parameter words"),
            "got: {rendered}"
        );
    }

    /// A method in an imported file is measured and named like one in the entry
    /// file, with the file's label in front.
    #[test]
    fn a055_measures_a_method_in_an_imported_file() {
        let lib = format!(
            "pub struct Point {{\n    x: i32;\n    pub fn scaled(self, {}) -> Point {{\n        \
             let p: Point = Point {{ x: self.x }};\n        return p;\n    }}\n}}",
            params("p", "u32", 254)
        );
        let ctx = try_type_check_multi_file(&[
            (vec![], "use lib::geom;\npub fn main() -> i32 { return 0; }"),
            (vec!["lib", "geom"], lib.as_str()),
        ])
        .expect("the program type-checks");
        let errors = inference_analysis::analyze_with_options(&ctx, options(TargetName::SpaceWasm))
            .expect_err("the method is over the limit");
        let rendered = errors.to_string();
        assert!(
            rendered.contains(
                "lib::geom:3:19: error[A055]: `Point::scaled` declares 256 parameter words"
            ),
            "got: {rendered}"
        );
        assert!(
            rendered.contains(
                "1 for the `self` receiver, 254 for 254 `u32` parameters and 1 for the hidden \
                 pointer a `Point` result is written through"
            ),
            "got: {rendered}"
        );
    }

    /// A bare struct or enum name is resolved in the file that writes it. Two
    /// files declaring one name as a struct and as an enum each count their own:
    /// a result of the file's enum is a value, a result of its struct takes the
    /// hidden pointer, whatever the other file declares under the name.
    #[test]
    fn a055_resolves_a_bare_name_in_the_file_that_writes_it() {
        let enum_result = format!(
            "pub enum Thing {{ A, B }}\npub fn g({}, t: Thing) -> Thing {{ return t; }}",
            params("p", "u32", 254)
        );
        let struct_result = format!(
            "pub struct Thing {{ x: i32; }}\npub fn g({}, t: Thing) -> Thing {{\n    \
             let r: Thing = Thing {{ x: 1 }};\n    return r;\n}}",
            params("p", "u32", 254)
        );
        let entry_struct = "use lib::other;\npub struct Thing { x: i32; }\npub fn main() -> i32 \
                            { return 0; }";
        let entry_enum = "use lib::other;\npub enum Thing { A, B }\npub fn main() -> i32 \
                          { return 0; }";

        let ctx = try_type_check_multi_file(&[
            (vec![], entry_struct),
            (vec!["lib", "other"], enum_result.as_str()),
        ])
        .expect("the program type-checks");
        assert!(
            a055_at(&ctx, TargetName::SpaceWasm).is_empty(),
            "`lib::other`'s `Thing` is its enum: 254 words, a tag, and a value result"
        );

        let ctx = try_type_check_multi_file(&[
            (vec![], entry_enum),
            (vec!["lib", "other"], struct_result.as_str()),
        ])
        .expect("the program type-checks");
        let findings = a055_at(&ctx, TargetName::SpaceWasm);
        let [AnalysisDiagnostic::ParamWordsExceeded { words: found, .. }] = findings.as_slice()
        else {
            panic!("expected one A055 finding: {findings:?}");
        };
        assert_eq!(
            found,
            &ParamWords {
                receiver: false,
                params: vec![group("u32", 254, 1), group("Thing", 1, 1)],
                result_pointer: Some("Thing".to_string()),
            },
            "`lib::other`'s `Thing` is its struct: an address, and a result through a pointer, \
             each named as the file spells it"
        );
    }

    /// A `::`-qualified struct or enum is one word as a parameter, like the bare
    /// name, and a qualified struct result takes the hidden pointer.
    #[test]
    fn a055_counts_qualified_types_like_bare_ones() {
        let entry = format!(
            "use lib::geom;\npub fn f({}, l: lib::geom::Level) -> lib::geom::Point {{\n    \
             return lib::geom::Point {{ x: 1, y: 2 }};\n}}",
            params("p", "lib::geom::Point", 254)
        );
        let ctx = try_type_check_multi_file(&[
            (vec![], entry.as_str()),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; }\npub enum Level { Low, High }",
            ),
        ])
        .expect("the program type-checks");
        let findings = a055_at(&ctx, TargetName::SpaceWasm);
        let [AnalysisDiagnostic::ParamWordsExceeded { words, .. }] = findings.as_slice() else {
            panic!("expected one A055 finding: {findings:?}");
        };
        assert_eq!(
            words,
            &ParamWords {
                receiver: false,
                params: vec![
                    group("lib::geom::Point", 254, 1),
                    group("lib::geom::Level", 1, 1),
                ],
                result_pointer: Some("lib::geom::Point".to_string()),
            }
        );

        let enum_result = format!(
            "use lib::geom;\npub fn f({}, l: lib::geom::Level) -> lib::geom::Level {{\n    \
             return l;\n}}",
            params("p", "lib::geom::Point", 254)
        );
        let ctx = try_type_check_multi_file(&[
            (vec![], enum_result.as_str()),
            (
                vec!["lib", "geom"],
                "pub struct Point { x: i32; y: i32; }\npub enum Level { Low, High }",
            ),
        ])
        .expect("the program type-checks");
        assert!(
            a055_at(&ctx, TargetName::SpaceWasm).is_empty(),
            "a qualified enum result is a value: 255 words fit"
        );
    }

    /// Every in-process pipeline analyzes a program for the target its module
    /// is built for, through one mapping from the emission target to the name.
    /// A wrong arm would leave A055 silent for every in-process SpaceWasm build.
    #[test]
    fn a055_in_process_pipelines_analyze_for_their_own_target() {
        for target in inference_wasm_codegen::Target::ALL {
            assert_eq!(crate::utils::target_name(target).as_str(), target.as_str());
            let options = inference_wasm_codegen::CodegenOptions {
                target,
                ..Default::default()
            };
            let analysis = crate::utils::analysis_options(&options);
            assert_eq!(analysis.target.as_str(), target.as_str());
            assert_eq!(analysis.stack_budget_bytes, options.layout.stack_size());
        }
    }

    /// The rule's limit is a transcription of the conformance checker's, which
    /// carries the upstream source it was read from. The two must not drift.
    #[test]
    fn a055_limit_is_the_conformance_checkers() {
        assert_eq!(
            inference_analysis::rules::param_words_exceeded::SPACEWASM_MAX_PARAM_WORDS,
            inference_target_conformance::spacewasm::MAX_PARAM_WORDS
        );
    }
}

/// The differential test: the words A055 counts are the words code generation
/// emits.
///
/// Every module here is built at `Target::SpaceWasm` in compile mode, and the
/// conformance checker reads each defined function's parameter words off the
/// module's own type section. The analysis side keys its count by the
/// function's name-section symbol, so each function is paired with itself, and
/// the two *sets* must be equal as well as the numbers: a function the rule
/// measures that the module does not define, or the reverse, is as much a
/// mirror defect as a wrong count.
///
/// Analysis is skipped on the way to code generation. The count does not depend
/// on whether a program passes analysis, and many corpus fixtures exercise a
/// construct some rule legitimately rejects, so skipping it is what lets the
/// sweep compare every signature the corpus has.
#[cfg(test)]
mod param_words_differential_tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use super::analysis_rules_tests::params;
    use crate::corpus::{single_file_corpus_sources, test_data_path};
    use crate::utils::{build_multi_file_ast, try_build_ast};
    use inference_analysis::errors::AnalysisDiagnostic;
    use inference_analysis::{AnalysisOptions, TargetName};
    use inference_ast::arena::AstArena;
    use inference_ast::ids::TypeId;
    use inference_ast::nodes::{ArgKind, Def, SimpleTypeKind, TypeNode};
    use inference_target_conformance::spacewasm::{Violation, check};
    use inference_type_checker::typed_context::TypedContext;
    use inference_wasm_codegen::{CodegenOptions, CompilationMode, Target};

    /// Fewer corpus programs than this building for the target means a gate
    /// widened or a walk found the wrong directory, and the sweep has stopped
    /// covering the corpus it claims to. 195 built when the sweep was written.
    const CORPUS_PROGRAM_FLOOR: usize = 190;

    /// Fewer functions than this compared across the corpus means the same.
    /// 1,009 were compared when the sweep was written.
    const CORPUS_FUNCTION_FLOOR: usize = 1_000;

    fn type_check(arena: AstArena) -> Option<TypedContext> {
        inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .ok()
            .map(|built| built.typed_context())
    }

    /// The module a `spacewasm` build of `ctx` emits, or `None` when code
    /// generation refuses the program.
    fn spacewasm_module(ctx: &TypedContext) -> Option<Vec<u8>> {
        let target = Target::SpaceWasm;
        let options = CodegenOptions {
            target,
            mode: CompilationMode::Compile,
            opt_level: target.default_opt_level(),
            ..Default::default()
        };
        inference_wasm_codegen::codegen(ctx, "differential", options)
            .ok()
            .map(|output| output.wasm().to_vec())
    }

    /// The analysis side: every function the rule measures, by name-section
    /// symbol.
    fn counted(ctx: &TypedContext) -> BTreeMap<String, u32> {
        let by_key = inference_analysis::param_words(ctx);
        let by_symbol: BTreeMap<String, u32> = by_key
            .iter()
            .map(|(key, words)| (key.name_section_symbol(), *words))
            .collect();
        assert_eq!(by_symbol.len(), by_key.len(), "two functions share a symbol");
        by_symbol
    }

    /// The module side, for a module inside every limit: each defined
    /// function's parameter words, by the name its `name` section gives it.
    fn emitted(label: &str, wasm: &[u8]) -> BTreeMap<String, u32> {
        let report = check(wasm).unwrap_or_else(|refusal| {
            panic!("{label}: the module must conform to be compared: {refusal}")
        });
        let names: BTreeMap<String, u32> = report
            .functions
            .iter()
            .map(|function| {
                let name = function
                    .name
                    .clone()
                    .unwrap_or_else(|| panic!("{label}: function {} is unnamed", function.index));
                (name, function.param_words)
            })
            .collect();
        assert_eq!(names.len(), report.functions.len(), "{label}: two functions share a name");
        names
    }

    /// Compares one conformant program's two readings and returns how many
    /// functions were compared.
    fn assert_agrees(label: &str, ctx: &TypedContext, wasm: &[u8]) -> usize {
        let counted = counted(ctx);
        let emitted = emitted(label, wasm);
        assert_eq!(
            counted, emitted,
            "{label}: A055's parameter words must be the words the module declares"
        );
        emitted.len()
    }

    /// Whether a bare or `::`-qualified type written in `module_path` names a
    /// struct or an enum there, or `None` for any other type.
    fn nominal(ctx: &TypedContext, ty: TypeId, module_path: &[String]) -> Option<&'static str> {
        let arena = ctx.arena();
        match &arena[ty].kind {
            TypeNode::Custom(name) => {
                let name = &arena[*name].name;
                if ctx.lookup_struct_in(name, module_path).is_some() {
                    Some("struct")
                } else if ctx.lookup_enum_in(name, module_path).is_some() {
                    Some("enum")
                } else {
                    None
                }
            }
            TypeNode::Qualified { .. } => {
                let path = arena[ty].kind.qualified_segments(arena)?;
                if ctx.lookup_struct_by_qualified_path(&path, module_path).is_some() {
                    Some("struct")
                } else if ctx.lookup_enum_by_qualified_path(&path, module_path).is_some() {
                    Some("enum")
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The census name of a type written in `module_path`, as a parameter or
    /// a result: its lowering class, whether it is `::`-qualified, and whether
    /// its file is an imported one, since each is a different input to the
    /// count's resolution.
    fn census_name(ctx: &TypedContext, ty: TypeId, module_path: &[String]) -> String {
        let arena = ctx.arena();
        let imported = if module_path.is_empty() { "" } else { " in an imported file" };
        match &arena[ty].kind {
            TypeNode::Simple(SimpleTypeKind::I64 | SimpleTypeKind::U64) => "64-bit".to_string(),
            TypeNode::Simple(SimpleTypeKind::Unit) => "unit".to_string(),
            TypeNode::Simple(_) => "32-bit or narrower".to_string(),
            TypeNode::Array { .. } => {
                let mut element = ty;
                while let TypeNode::Array { element: inner, .. } = &arena[element].kind {
                    element = *inner;
                }
                format!("array of {}", census_name(ctx, element, module_path))
            }
            TypeNode::Qualified { .. } => format!(
                "qualified {}{imported}",
                nominal(ctx, ty, module_path).unwrap_or("unresolved")
            ),
            _ => format!("{}{imported}", nominal(ctx, ty, module_path).unwrap_or("other")),
        }
    }

    /// The signature shapes one program's defined functions use, for the census
    /// below: every parameter and result by [`census_name`], and the receiver and
    /// `_` forms by name.
    fn shapes(ctx: &TypedContext) -> BTreeSet<String> {
        let arena = ctx.arena();
        let mut found = BTreeSet::new();
        for source_file in ctx.source_files() {
            let module_path = source_file.module_path.as_slice();
            for &def_id in &source_file.defs {
                let functions: Vec<_> = match &arena[def_id].kind {
                    Def::Function { .. } => vec![def_id],
                    Def::Struct { methods, .. } => methods.clone(),
                    _ => Vec::new(),
                };
                for function in functions {
                    let Def::Function { args, returns, .. } = &arena[function].kind else {
                        continue;
                    };
                    for arg in args {
                        let ty = match &arg.kind {
                            ArgKind::SelfRef { .. } => {
                                found.insert("a `self` receiver".to_string());
                                continue;
                            }
                            ArgKind::Ignored { ty } => {
                                found.insert("a `_` parameter".to_string());
                                *ty
                            }
                            ArgKind::Named { ty, .. } | ArgKind::TypeOnly(ty) => *ty,
                        };
                        if matches!(arena[ty].kind, TypeNode::Simple(SimpleTypeKind::Bool)) {
                            found.insert("bool parameter".to_string());
                        }
                        found.insert(format!("{} parameter", census_name(ctx, ty, module_path)));
                    }
                    if let Some(ty) = returns {
                        found.insert(format!("{} result", census_name(ctx, *ty, module_path)));
                    }
                }
            }
        }
        found
    }

    /// Every program of the committed corpus that type-checks: the single-file
    /// code generation fixtures, the `panic_free` shapes, the multi-file golden
    /// trees, and the `inf` programs, which also reach through `use` into other
    /// files. Which of them a SpaceWasm build accepts is the caller's question.
    fn corpus_programs() -> Vec<(String, TypedContext)> {
        let mut programs = Vec::new();
        let mut sources = single_file_corpus_sources();
        for fixture in read_dir_sorted(&test_data_path().join("panic_free")) {
            if fixture.extension().is_some_and(|ext| ext == "inf") {
                let source = std::fs::read_to_string(&fixture)
                    .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture.display()));
                sources.push((fixture.display().to_string(), source));
            }
        }
        for (path, source) in sources {
            if let Some(ctx) = try_build_ast(source).ok().and_then(type_check) {
                programs.push((path, ctx));
            }
        }
        let trees = test_data_path().join("codegen/wasm/multi_file_golden");
        let mut entries = Vec::new();
        for tree in read_dir_sorted(&trees) {
            entries.push(tree.join("src").join("main.inf"));
        }
        for program in read_dir_sorted(&test_data_path().join("inf")) {
            if program.extension().is_some_and(|ext| ext == "inf") {
                entries.push(program);
            }
        }
        for entry in entries {
            let Ok(project) = inference::parse_project(&entry) else {
                continue;
            };
            if let Some(ctx) = type_check(project.arena) {
                programs.push((entry.display().to_string(), ctx));
            }
        }
        programs
    }

    fn read_dir_sorted(dir: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()))
            .map(|entry| entry.expect("a directory entry").path())
            .collect();
        paths.sort();
        paths
    }

    /// A program for each shape the corpus is thin on, every one of which must
    /// build: the census below is only a claim about shapes that were compared.
    fn matrix() -> Vec<(&'static str, TypedContext)> {
        let single = r"
pub struct Point {
    x: i32;
    y: i32;

    pub fn norm(self) -> i32 {
        return self.x;
    }

    pub fn shift(mut self, dx: i32) -> i32 {
        self.x = self.x + dx;
        return self.x;
    }

    pub fn origin() -> Point {
        let p: Point = Point { x: 0, y: 0 };
        return p;
    }

    pub fn pair(self, other: Point, k: u64) -> [i32; 2] {
        let a: [i32; 2] = [self.x, other.y];
        return a;
    }
}

pub enum Color { Red, Green, Blue }

pub fn scalars(a: i8, b: u8, c: i16, d: u16, e: i32, f: u32, g: bool) -> i32 {
    return e;
}

pub fn wide(a: i64, b: u64, c: i32) -> i64 {
    return a;
}

pub fn compound(p: Point, xs: [i32; 3], grid: [[i64; 2]; 2], c: Color, ps: [Point; 2]) -> i32 {
    return p.x;
}

pub fn ignored(_: i64, _: Point, _: u8, n: i32) -> i32 {
    return n;
}

pub fn make_array(n: i32) -> [i64; 4] {
    let a: [i64; 4] = [1, 2, 3, 4];
    return a;
}

pub fn make_point(x: i32, y: i32) -> Point {
    let p: Point = Point { x: x, y: y };
    return p;
}

pub fn pick(c: Color) -> Color {
    return c;
}

pub fn nothing() {
}

pub fn wide_result(a: u64) -> u64 {
    return a;
}
";
        let entry = r"
use lib::geom;

pub fn take(p: lib::geom::Point, q: lib::geom::Point, n: u64) -> i32 {
    return p.x;
}

pub fn give(l: lib::geom::Level) -> lib::geom::Point {
    return lib::geom::Point { x: 1, y: 7 };
}

pub fn level(l: lib::geom::Level, n: i64) -> lib::geom::Level {
    return l;
}

pub fn rows(ps: [lib::geom::Point; 2], ls: [lib::geom::Level; 2], c: [Color; 3]) -> i32 {
    return 0;
}

pub enum Color { Red, Green }
";
        let geom = r"
pub struct Point {
    x: i32;
    y: i32;

    pub fn scaled(self, k: i64) -> Point {
        let p: Point = Point { x: self.x, y: self.y };
        return p;
    }
}

pub enum Level { Low, High }

pub fn far(a: u64, b: i64) -> i64 {
    return b;
}

pub fn local(p: Point, l: Level, ls: [Level; 2], n: u64) -> Level {
    return l;
}

pub fn own(p: Point) -> Point {
    return p;
}
";
        // One name, a struct in one file and an enum in the other, each used
        // bare in its own file: a count resolved in the wrong file gives the
        // enum result a hidden pointer, or takes the struct result's away.
        let collision_entry = r"
use lib::other;

pub struct Thing {
    x: i32;
}

pub fn here(t: Thing, n: i64) -> Thing {
    return t;
}
";
        let collision_lib = r"
pub enum Thing { A, B }

pub fn there(t: Thing, n: i64) -> Thing {
    return t;
}
";
        let single = type_check(try_build_ast(single.to_string()).expect("the matrix parses"))
            .expect("the single-file matrix type-checks");
        let multi = type_check(build_multi_file_ast(&[
            (vec![], entry),
            (vec!["lib", "geom"], geom),
        ]))
        .expect("the multi-file matrix type-checks");
        let collision = type_check(build_multi_file_ast(&[
            (vec![], collision_entry),
            (vec!["lib", "other"], collision_lib),
        ]))
        .expect("the collision matrix type-checks");
        let reversed = type_check(build_multi_file_ast(&[
            (vec![], collision_lib),
            (vec!["lib", "other"], collision_entry.replace("use lib::other;", "").as_str()),
        ]))
        .expect("the reversed collision matrix type-checks");
        vec![
            ("the single-file matrix", single),
            ("the multi-file matrix", multi),
            ("the collision matrix", collision),
            ("the reversed collision matrix", reversed),
        ]
    }

    /// Every function of every corpus program a SpaceWasm build accepts, and
    /// of the matrix: A055's count and the module's are the same set of
    /// functions with the same numbers, and together they cover every shape a
    /// signature can take.
    #[test]
    fn a055_counts_the_words_code_generation_emits() {
        let mut programs = 0;
        let mut functions = 0;
        let mut census = BTreeSet::new();
        for (label, ctx) in corpus_programs() {
            // The rule runs on every program that type-checks — in the editor,
            // on every keystroke — whether or not it goes on to build.
            let _ = inference_analysis::param_words(&ctx);
            let Some(wasm) = spacewasm_module(&ctx) else {
                continue;
            };
            functions += assert_agrees(&label, &ctx, &wasm);
            programs += 1;
            census.extend(shapes(&ctx));
        }
        assert!(
            programs >= CORPUS_PROGRAM_FLOOR,
            "only {programs} corpus programs built for the target"
        );
        assert!(
            functions >= CORPUS_FUNCTION_FLOOR,
            "only {functions} corpus functions were compared"
        );

        for (label, ctx) in matrix() {
            let wasm = spacewasm_module(&ctx)
                .unwrap_or_else(|| panic!("{label} must build for the SpaceWasm target"));
            assert_agrees(label, &ctx, &wasm);
            census.extend(shapes(&ctx));
        }

        for shape in [
            "64-bit parameter",
            "32-bit or narrower parameter",
            "bool parameter",
            "struct parameter",
            "enum parameter",
            "struct in an imported file parameter",
            "enum in an imported file parameter",
            "qualified struct parameter",
            "qualified enum parameter",
            "array of 32-bit or narrower parameter",
            "array of 64-bit parameter",
            "array of struct parameter",
            "array of enum parameter",
            "array of enum in an imported file parameter",
            "array of qualified struct parameter",
            "array of qualified enum parameter",
            "a `_` parameter",
            "a `self` receiver",
            "32-bit or narrower result",
            "64-bit result",
            "struct result",
            "enum result",
            "struct in an imported file result",
            "enum in an imported file result",
            "qualified struct result",
            "qualified enum result",
            "array of 32-bit or narrower result",
            "array of 64-bit result",
        ] {
            assert!(census.contains(shape), "no compared signature has a {shape}: {census:?}");
        }
    }

    /// Over the limit the checker refuses the module instead of measuring it,
    /// and each refusal carries the function's words. They must be A055's count,
    /// for the same functions, at every shape that reaches the boundary.
    #[test]
    fn a055_counts_the_words_the_checker_refuses() {
        let ignored = (0..255).map(|_| "_: u32").collect::<Vec<_>>().join(", ");
        let sources = [
            format!("pub fn f({}) {{ }}", params("p", "i64", 128)),
            format!(
                "pub struct Pair {{ a: i32; b: i32; }}\npub fn f({}, last: u32) -> Pair {{\n    \
                 let r: Pair = Pair {{ a: 1, b: 2 }};\n    return r;\n}}",
                params("p", "i64", 127)
            ),
            format!(
                "pub struct S {{\n    x: i32;\n    pub fn f(self, {}) -> i32 {{ return self.x; \
                 }}\n}}",
                params("p", "u32", 255)
            ),
            format!("pub struct P {{ x: i32; }}\npub fn f({}) {{ }}", params("p", "P", 256)),
            format!("pub fn f({}) {{ }}", params("p", "[i64; 4]", 256)),
            format!(
                "pub fn f({ignored}) -> [i32; 2] {{\n    let a: [i32; 2] = [1, 2];\n    \
                 return a;\n}}"
            ),
            format!("pub enum E {{ A, B }}\npub fn f({}) {{ }}", params("p", "E", 300)),
        ];
        for source in &sources {
            let arena = try_build_ast(source.clone()).expect("the shape parses");
            let ctx = type_check(arena).expect("the shape type-checks");
            let wasm = spacewasm_module(&ctx).expect("code generation does not check the limit");
            let refused: BTreeMap<String, u64> = match check(&wasm) {
                Ok(_) => panic!("a module over the limit must be refused:\n{source}"),
                Err(violations) => violations
                    .as_slice()
                    .iter()
                    .filter_map(|violation| match violation {
                        Violation::ParamWordsExceeded { function, words } => {
                            Some((function.clone(), *words))
                        }
                        _ => None,
                    })
                    .collect(),
            };
            let over: BTreeMap<String, u64> = counted(&ctx)
                .into_iter()
                .filter(|(_, words)| *words > 255)
                .map(|(name, words)| (name, u64::from(words)))
                .collect();
            assert!(!over.is_empty(), "each shape reaches the boundary:\n{source}");
            assert_eq!(over, refused, "A055 and the checker disagree about:\n{source}");

            let options = AnalysisOptions {
                target: TargetName::SpaceWasm,
                ..AnalysisOptions::default()
            };
            let findings: Vec<u64> = inference_analysis::analyze_with_options(&ctx, options)
                .expect_err("A055 refuses it before code generation")
                .errors()
                .iter()
                .filter_map(|finding| match finding {
                    AnalysisDiagnostic::ParamWordsExceeded { words, .. } => {
                        Some(u64::from(words.total()))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                findings,
                refused.values().copied().collect::<Vec<_>>(),
                "the rule's finding reports the export's count:\n{source}"
            );
        }
    }
}
