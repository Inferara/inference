/// Integration tests for analysis rule A058, and for how A036 measures a stack
/// that shares its memory with constant data (#211).
///
/// - A058: StaticDataExceedsMemory — the shadow stack and the constant data
///   placed above it must fit the pages the memory is guaranteed. A stack the
///   build did not size gives up what the data needs, so the rule fires only
///   when that is not enough: data that leaves no frame at all, or a requested
///   `stack-size` that, with the data beside it, needs more than the pages.
///
/// Each finding is checked for what makes it actionable — the constants named,
/// the anchor at the first constant that does not fit, the overflow — and the
/// help it gives is followed: the layout it names is resolved and the program
/// analyzed again, which must pass.
#[cfg(test)]
mod rules_a058_tests {
    use crate::utils::{build_ast, build_multi_file_ast};
    use inference_analysis::errors::{AnalysisDiagnostic, LabeledDiagnostic};
    use inference_analysis::{AnalysisOptions, MemoryLayout, MemoryRequest};
    use inference_type_checker::typed_context::TypedContext;
    use inference_wasm_codegen::MemoryLayoutSource;

    fn type_check(source: &str) -> TypedContext {
        inference_type_checker::TypeCheckerBuilder::build_typed_context(build_ast(
            source.to_string(),
        ))
        .expect("type checking should succeed")
        .typed_context()
    }

    fn layout(pages: Option<u32>, stack_size: Option<u32>) -> MemoryLayout {
        MemoryLayout::resolve(
            MemoryRequest {
                pages,
                max_pages: None,
                stack_size,
            },
            MemoryLayoutSource::Flag,
        )
        .expect("a layout a build can request")
    }

    /// The labeled findings rule `rule` reports for `ctx` under `layout`, run
    /// the way the language server runs each rule.
    fn findings(ctx: &TypedContext, layout: MemoryLayout, rule: &str) -> Vec<LabeledDiagnostic> {
        let options = AnalysisOptions {
            layout,
            ..AnalysisOptions::default()
        };
        inference_analysis::rules::all_rules()
            .iter()
            .find(|r| r.id() == rule)
            .unwrap_or_else(|| panic!("no rule {rule}"))
            .check(ctx, options)
    }

    /// The layout a finding's help asks for, applied to `layout`.
    fn follow(layout: MemoryLayout, request: MemoryRequest) -> MemoryLayout {
        MemoryLayout::resolve(
            MemoryRequest {
                pages: request.pages.or(Some(layout.pages())),
                // A fixed memory's maximum follows its size, as the build's own
                // request leaves it; only a growable one keeps its cap.
                max_pages: request
                    .max_pages
                    .or(layout.is_growable().then_some(layout.max_pages())),
                stack_size: request
                    .stack_size
                    .or(layout.is_stack_requested().then_some(layout.stack_size())),
            },
            MemoryLayoutSource::Flag,
        )
        .expect("the help names a layout a build accepts")
    }

    const TABLES: &str = r#"
        const SMALL: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        const BIG: [i32; 1000] = [7; 1000];
        pub fn f() -> i32 { return BIG[999]; }
        pub fn g() -> u8 { return SMALL[7]; }
    "#;

    /// A full-page requested stack leaves no room: the finding names both
    /// constants, largest first, is anchored at the first one that does not
    /// fit, and its first help — two pages — clears it.
    #[test]
    fn a_requested_stack_and_constant_data_that_need_more_than_the_pages() {
        let ctx = type_check(TABLES);
        let requested = layout(None, Some(65_536));
        let found = findings(&ctx, requested, "A058");
        let [finding] = found.as_slice() else {
            panic!("expected one A058, got {found:?}");
        };
        let AnalysisDiagnostic::StaticDataExceedsMemory {
            data,
            refusal,
            location,
            ..
        } = &finding.diagnostic
        else {
            unreachable!()
        };
        assert_eq!(data.items[0].name, "BIG");
        assert_eq!(data.items[1].name, "SMALL");
        assert_eq!(data.total, 8 + 4_000, "SMALL at 0, BIG at 8: no padding");
        assert_eq!(refusal.overflow_bytes(), 4_008);
        assert_eq!(
            location.start_line, 2,
            "anchored at SMALL, the first that does not fit"
        );
        let text = finding.diagnostic.to_string();
        assert!(
            text.contains(
                "note: the constant data is 4000 bytes for `BIG` and 8 bytes for `SMALL`"
            ),
            "{text}"
        );
        assert!(text.contains("help: raise the memory to 2 pages"), "{text}");
        assert!(
            text.contains("lower the stack to at most 61520 bytes"),
            "{text}"
        );

        let more_pages = follow(
            requested,
            MemoryRequest {
                pages: Some(2),
                ..Default::default()
            },
        );
        assert!(findings(&ctx, more_pages, "A058").is_empty());
        let smaller_stack = layout(None, Some(61_520));
        assert!(findings(&ctx, smaller_stack, "A058").is_empty());
        assert!(findings(&ctx, smaller_stack, "A036").is_empty());
    }

    /// The default stack gives the data the room it needs, so a program with
    /// constant tables builds in the default page without any `[memory]` table.
    #[test]
    fn a_default_stack_makes_room_for_constant_data() {
        let ctx = type_check(TABLES);
        assert!(findings(&ctx, MemoryLayout::default(), "A058").is_empty());
        assert!(findings(&ctx, MemoryLayout::default(), "A036").is_empty());
    }

    /// Data that fills the page leaves a default stack no frame: the finding
    /// says so and offers only more pages.
    #[test]
    fn constant_data_that_leaves_no_frame_for_a_default_stack() {
        let ctx = type_check(
            "const FULL: [u8; 65536] = [0; 65536];\npub fn f() -> u8 { return FULL[0]; }",
        );
        let found = findings(&ctx, MemoryLayout::default(), "A058");
        let [finding] = found.as_slice() else {
            panic!("expected one A058, got {found:?}");
        };
        let text = finding.diagnostic.to_string();
        assert!(
            text.contains("65536 bytes of static data leave no room for the shadow stack"),
            "{text}"
        );
        assert!(text.contains("help: raise the memory to 2 pages"), "{text}");
        assert!(!text.contains("lower the stack"), "{text}");
        assert!(findings(&ctx, layout(Some(2), None), "A058").is_empty());
    }

    /// The deepest call chain is measured against the stack the data leaves,
    /// and the finding explains why that stack is below the default and that
    /// one more page gives it back.
    #[test]
    fn a036_measures_against_the_stack_constant_data_leaves() {
        const SOURCE: &str = r#"
            const T: [u8; 2048] = [1; 2048];
            pub fn deep() -> u8 {
                let a: [u8; 64000] = [2; 64000];
                return a[0] + T[0];
            }
        "#;
        let ctx = type_check(SOURCE);
        let found = findings(&ctx, MemoryLayout::default(), "A036");
        let [finding] = found.as_slice() else {
            panic!("a 64000-byte frame does not fit the 63488 bytes the data leaves: {found:?}");
        };
        let text = finding.diagnostic.to_string();
        assert!(text.contains("exceeding the 63488-byte stack"), "{text}");
        assert!(
            text.contains("less what the 2048 bytes of constant data above it take"),
            "{text}"
        );
        assert!(text.contains("`pages = 2` under `[memory]`"), "{text}");
        assert!(findings(&ctx, layout(Some(2), None), "A036").is_empty());
    }

    /// A constant in another file is named by its path, and the finding is
    /// filed against that file.
    #[test]
    fn a_constant_in_another_file_is_named_and_anchored_there() {
        let entry = "use lib::t::{BIG};\npub fn f() -> i32 { return BIG[0]; }";
        let lib = "pub const BIG: [i32; 100] = [1; 100];";
        let ctx = inference_type_checker::TypeCheckerBuilder::build_typed_context(
            build_multi_file_ast(&[(vec![], entry), (vec!["lib", "t"], lib)]),
        )
        .expect("the program type-checks")
        .typed_context();
        let found = findings(&ctx, layout(None, Some(65_536)), "A058");
        let [finding] = found.as_slice() else {
            panic!("expected one A058, got {found:?}");
        };
        assert_eq!(finding.module_path, ["lib", "t"]);
        assert!(
            finding
                .diagnostic
                .to_string()
                .contains("400 bytes for `lib::t::BIG`")
        );
    }
}
