//! How a diagnostic spells the types it quotes.
//!
//! A reader who copies a type out of a message and writes it back into the
//! program has to get a type the language accepts, so every type a diagnostic
//! quotes is rendered in its source spelling: `()` for the unit type, `bool`,
//! `string`, the integer names, and a user type by name. These tests pin the
//! rendering from both ends: the message a mismatch produces, and that each
//! rendering, written back as a type annotation, checks to the type it came
//! from.

use crate::utils::build_ast;
use inference_ast::ids::NodeId;
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::type_info::{NumberType, TypeInfo, TypeInfoKind};

/// Type-checks `source` and renders every diagnostic.
fn diagnostics(source: &str) -> Vec<String> {
    check_with_diagnostics(build_ast(source.to_string()))
        .errors
        .into_iter()
        .map(|d| d.error.to_string())
        .collect()
}

fn ty(kind: TypeInfoKind) -> TypeInfo {
    TypeInfo {
        kind,
        type_params: vec![],
    }
}

fn array(element: TypeInfo, length: u32) -> TypeInfo {
    ty(TypeInfoKind::Array(Box::new(element), length))
}

#[test]
fn a_unit_call_returned_as_an_integer_names_the_unit_type_as_written() {
    let errors = diagnostics("fn g() { } fn f() -> i32 { return g(); }");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("type mismatch in return statement: expected `i32`, found `()`"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_bool_array_quotes_its_element_as_written() {
    let errors = diagnostics("fn f() { let a: [i32; 2] = [true, false]; }");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("expected `[i32; 2]`, found `[bool; 2]`")),
        "{errors:?}"
    );
}

/// Every rendering, written back as a parameter's annotation, is accepted, and
/// the parameter's type renders exactly as the annotation was written.
#[test]
fn every_rendered_type_is_accepted_as_written() {
    let point = ty(TypeInfoKind::Struct(
        "Point".to_string(),
        "Point".to_string(),
    ));
    let level = ty(TypeInfoKind::Enum("Level".to_string(), "Level".to_string()));
    let mut types = vec![
        TypeInfo::default(),
        TypeInfo::boolean(),
        TypeInfo::string(),
        point.clone(),
        level,
        array(TypeInfo::boolean(), 2),
        array(array(TypeInfo::default(), 2), 3),
        array(point, 4),
    ];
    types.extend(
        NumberType::ALL
            .iter()
            .map(|number| ty(TypeInfoKind::Number(*number))),
    );

    for ty in types {
        let rendered = ty.to_string();
        let source = format!(
            "struct Point {{ x: i32; }} enum Level {{ Low, High }} fn f(probe: {rendered}) {{ }}"
        );
        let outcome = check_with_diagnostics(build_ast(source.clone()));
        assert!(
            outcome.errors.is_empty(),
            "`{rendered}` is not accepted as a type: {:?}",
            outcome
                .errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        let ctx = &outcome.typed_context;
        let (param, _) = ctx
            .arena()
            .idents
            .iter()
            .find(|(_, ident)| ident.name == "probe")
            .expect("the parameter is in the arena");
        let recorded = ctx
            .get_node_typeinfo(NodeId::Ident(param))
            .expect("the parameter is typed");
        assert_eq!(
            recorded.to_string(),
            rendered,
            "`{rendered}` reads back as a different type in {source}"
        );
    }
}
