//! Module-scope constants at type check: what an initializer may contain, what
//! its value is, and where a compound one is placed in the static data region
//! (#211).
//!
//! The type checker computes every module constant itself, under the program's
//! own arithmetic, so a refusal here is either an initializer that is not a
//! constant expression or one whose operation would trap if the program ran it.
//! Each is reported once, at the expression that fails, against the file the
//! constant is declared in.

use crate::utils::{build_ast, build_multi_file_ast, try_type_check_multi_file};
use inference_type_checker::errors::{ConstEvalFailure, TypeCheckError};
use inference_type_checker::module_consts::ConstValue;
use inference_type_checker::type_info::NumberType;
use inference_type_checker::typed_context::TypedContext;
use inference_type_checker::{TypeCheckDiagnostic, check_with_diagnostics};

fn check(source: &str) -> (TypedContext, Vec<TypeCheckDiagnostic>) {
    let outcome = check_with_diagnostics(build_ast(source.to_string()));
    (outcome.typed_context, outcome.errors)
}

/// The errors of `source`, which must have some.
fn errors(source: &str) -> Vec<TypeCheckError> {
    let (_, errors) = check(source);
    assert!(!errors.is_empty(), "expected a type error for: {source}");
    errors.into_iter().map(|d| d.error).collect()
}

/// The single evaluation failure `source` reports.
fn failure(source: &str) -> ConstEvalFailure {
    let failures: Vec<ConstEvalFailure> = errors(source)
        .into_iter()
        .filter_map(|e| match e {
            TypeCheckError::ConstEvaluationFailed { reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    let [only] = failures.as_slice() else {
        panic!("expected exactly one evaluation failure, got {failures:?}");
    };
    only.clone()
}

/// The value of the module constant named `name` in a program that checks.
fn value_of(source: &str, name: &str) -> ConstValue {
    let (ctx, errors) = check(source);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let def_id = ctx
        .source_files()
        .flat_map(|file| file.defs.iter().copied())
        .find(|&def| ctx.arena().def_name(def) == name)
        .unwrap_or_else(|| panic!("no definition `{name}`"));
    ctx.module_const_value(def_id)
        .unwrap_or_else(|| panic!("`{name}` has no value"))
        .clone()
}

fn int(value: i128, number: NumberType) -> ConstValue {
    ConstValue::Int { value, number }
}

#[test]
fn a_function_call_is_not_a_constant_expression() {
    let errs = errors("fn f() -> i32 { return 1; }\nconst X: i32 = f() + 1;");
    let [
        TypeCheckError::NonConstantInitializer {
            name,
            construct,
            location,
        },
    ] = errs.as_slice()
    else {
        panic!("expected one NonConstantInitializer, got {errs:?}");
    };
    assert_eq!(name, "X");
    assert_eq!(*construct, "a function call");
    assert_eq!((location.start_line, location.start_column), (2, 16));
    let text = errs[0].to_string();
    assert!(
        text.contains("the initializer of `const X` must be a constant expression"),
        "{text}"
    );
}

#[test]
fn a_non_deterministic_value_is_not_a_constant_expression() {
    let errs = errors("const X: i32 = @;");
    assert!(
        errs.iter().any(
            |e| matches!(e, TypeCheckError::NonConstantInitializer { construct, .. }
            if construct.starts_with("`@`"))
        ),
        "{errs:?}"
    );
}

/// A checked operator whose exact result leaves the type has no value: the
/// program would trap. `wrapping` asks for the wrap and gets it.
#[test]
fn a_checked_overflow_is_refused_and_a_wrapping_one_wraps() {
    assert_eq!(
        failure("const X: i8 = 100 + 100;"),
        ConstEvalFailure::Overflow {
            op: "+",
            exact: 200,
            number: NumberType::I8
        }
    );
    assert_eq!(
        failure("const M: i64 = -9223372036854775808;\nconst X: i64 = -M;"),
        ConstEvalFailure::Overflow {
            op: "-",
            exact: 9_223_372_036_854_775_808,
            number: NumberType::I64
        }
    );
    assert_eq!(
        value_of("const X: i8 = wrapping(100 + 100);", "X"),
        int(-56, NumberType::I8)
    );
    let text = errors("const X: u16 = 65535 * 2;")[0].to_string();
    assert!(
        text.contains(
            "`const X` cannot be computed: `*` produces 131070, which does not fit `u16`"
        ),
        "{text}"
    );
    assert!(text.contains("wrapping(...)"), "{text}");
}

/// Division by a constant that is zero is refused at the operation; a literal
/// zero divisor is the type checker's existing error and is not reported twice.
#[test]
fn division_by_zero_is_refused_once() {
    assert_eq!(
        failure("const Z: i32 = 0;\nconst X: i32 = 7 % Z;"),
        ConstEvalFailure::DivisionByZero { op: "%" }
    );
    let errs = errors("const X: i32 = 7 / 0;");
    assert_eq!(errs.len(), 1, "one error for one mistake: {errs:?}");
    assert!(
        matches!(errs[0], TypeCheckError::DivisionByZero { .. }),
        "{errs:?}"
    );
}

#[test]
fn the_smallest_signed_value_divided_by_minus_one_is_refused() {
    assert_eq!(
        failure("const M: i32 = -2147483648;\nconst X: i32 = M / -1;"),
        ConstEvalFailure::DivisionOverflow {
            number: NumberType::I32
        }
    );
    assert_eq!(
        value_of("const M: i32 = -2147483648;\nconst X: i32 = M % -1;", "X"),
        int(0, NumberType::I32),
        "the remainder has a value: zero"
    );
}

#[test]
fn a_shift_by_the_width_or_more_is_refused() {
    assert_eq!(
        failure("const S: u8 = 8;\nconst X: u8 = 1 << S;"),
        ConstEvalFailure::ShiftCountOutOfRange {
            op: "<<",
            count: 8,
            number: NumberType::U8
        }
    );
    assert_eq!(
        failure("const S: i32 = -1;\nconst X: i32 = 8 >> S;"),
        ConstEvalFailure::ShiftCountOutOfRange {
            op: ">>",
            count: -1,
            number: NumberType::I32
        },
        "a negative count is refused too, though the instruction would mask it"
    );
    assert_eq!(
        value_of("const X: u8 = 1 << 7;", "X"),
        int(128, NumberType::U8)
    );
    assert_eq!(
        value_of("const X: i32 = 1 << 31;", "X"),
        int(i128::from(i32::MIN), NumberType::I32),
        "a shift wraps as the instruction does"
    );
}

#[test]
fn an_index_past_the_end_of_a_constant_is_refused() {
    assert_eq!(
        failure("const T: [i32; 2] = [1, 2];\nconst X: i32 = T[2];"),
        ConstEvalFailure::IndexOutOfBounds {
            index: 2,
            length: 2
        }
    );
}

#[test]
fn a_literal_that_does_not_fit_its_type_is_refused() {
    assert_eq!(
        failure("const X: u8 = 256;"),
        ConstEvalFailure::LiteralOutOfRange {
            literal: "256".to_string(),
            number: NumberType::U8
        }
    );
}

/// `&&` and `||` evaluate their right operand only when the left one does not
/// decide, as the program does, so an operand that would trap is not reached.
#[test]
fn short_circuit_operators_skip_an_operand_they_do_not_need() {
    let source = "const Z: i32 = 0;\nconst X: bool = Z != 0 && 10 / Z > 1;";
    assert_eq!(value_of(source, "X"), ConstValue::Bool(false));
}

/// A struct literal's fields are values the cycle check reads, so two struct
/// constants built from each other are a cycle rather than an evaluation that
/// recurses.
#[test]
fn a_value_cycle_through_struct_literals_is_reported() {
    let errs =
        errors("struct P { x: i32; }\nconst A: P = P { x: B.x };\nconst B: P = P { x: A.x };");
    assert!(
        errs.iter()
            .any(|e| matches!(e, TypeCheckError::CircularDefinition { .. })),
        "{errs:?}"
    );
    assert!(
        !errs
            .iter()
            .any(|e| matches!(e, TypeCheckError::ConstEvaluationFailed { .. })),
        "the cycle is reported once, as a cycle: {errs:?}"
    );
}

/// A struct constant's fields are held in declaration order whatever order the
/// literal wrote them in, which is the order the static data lays them out.
#[test]
fn struct_values_hold_their_fields_in_declaration_order() {
    let value = value_of(
        "struct P { a: u8; b: i64; }\nconst X: P = P { b: -1, a: 2 };",
        "X",
    );
    assert_eq!(
        value,
        ConstValue::Struct(vec![
            ("a".to_string(), int(2, NumberType::U8)),
            ("b".to_string(), int(-1, NumberType::I64)),
        ])
    );
}

/// Only compound constants a body reads are placed, in source order, each at
/// its natural alignment; a scalar and a constant only another initializer
/// reads take no bytes.
#[test]
fn the_static_data_region_holds_what_bodies_read_at_natural_alignment() {
    let (ctx, errors) = check(
        "const K: i64 = 3;\n\
         const BYTE: [u8; 3] = [1, 2, 3];\n\
         const UNREAD: [i64; 4] = [1, 2, 3, 4];\n\
         const WIDE: [i64; 2] = [UNREAD[0], K];\n\
         pub fn f() -> i64 { return WIDE[1]; }\n\
         pub fn g() -> u8 { return BYTE[2]; }",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let data = ctx.static_data();
    let placed: Vec<(&str, u64, u64)> = data
        .entries()
        .iter()
        .map(|entry| (entry.name.as_str(), entry.offset, entry.size))
        .collect();
    assert_eq!(placed, [("BYTE", 0, 3), ("WIDE", 8, 16)]);
    assert_eq!(data.size(), 24);
    let image = data.image(&ctx);
    assert_eq!(&image[..3], &[1, 2, 3]);
    assert_eq!(&image[3..8], &[0; 5], "padding is zero");
    assert_eq!(&image[8..16], &1i64.to_le_bytes());
    assert_eq!(&image[16..24], &3i64.to_le_bytes());
}

/// A constant reached through an item import is a `const` like any other: a
/// write to it, or into it, is refused.
#[test]
fn a_write_to_an_imported_constant_is_refused() {
    let entry = "use lib::t::{C};\npub fn f() { C[0] = 1; }";
    let lib = "pub const C: [i32; 2] = [1, 2];";
    let err = try_type_check_multi_file(&[(vec![], entry), (vec!["lib", "t"], lib)])
        .err()
        .expect("the write must be refused")
        .to_string();
    assert!(
        err.contains("cannot assign to immutable variable `C`"),
        "{err}"
    );
}

/// An evaluation failure in another file names that file.
#[test]
fn a_failure_in_another_file_names_that_file() {
    let entry = "use lib::t::{C};\npub fn f() -> i32 { return C; }";
    let lib = "pub const C: i32 = 2147483647 + 1;";
    let outcome = check_with_diagnostics(build_multi_file_ast(&[
        (vec![], entry),
        (vec!["lib", "t"], lib),
    ]));
    let failed = outcome
        .errors
        .iter()
        .find(|d| matches!(d.error, TypeCheckError::ConstEvaluationFailed { .. }))
        .expect("the overflow is reported");
    assert_eq!(failed.file_label.as_deref(), Some("lib::t"));
}
