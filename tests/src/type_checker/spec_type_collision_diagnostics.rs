//! A spec-inner type refused registration still reports its own mistakes.
//!
//! A `struct` or `enum` declared inside a `spec` whose name collides with one
//! already registered in the same file is refused registration: cross-spec type
//! mangling does not exist, so the two would collapse to one canonical key and
//! one layout. Refusing the registration is correct, but the declaration's own
//! errors — a repeated field, a repeated variant, a receiver that is not the
//! first parameter — belong to the text the user wrote, not to the registration,
//! and were being discarded with it. The collision is fatal, so the second
//! mistake was never shown, not even after renaming.
//!
//! These tests pin those diagnostics as reachable through the collided
//! declaration, agreeing across both source orders, and pin that a declaration
//! which does register reports each of them exactly once.

use crate::utils::{build_ast, try_type_check_multi_file};
use inference_type_checker::check_with_diagnostics;
use inference_type_checker::errors::{RegistrationKind, TypeCheckError};

fn diagnostics(source: &str) -> Vec<TypeCheckError> {
    let arena = build_ast(source.to_string());
    check_with_diagnostics(arena)
        .errors
        .into_iter()
        .map(|d| d.error)
        .collect()
}

/// Asserts the same-file spec collision was reported for a type of `kind` named
/// `name`, which is the diagnostic these declarations are refused registration by.
fn assert_collision_reported(errors: &[TypeCheckError], kind: RegistrationKind, name: &str) {
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeCheckError::RegistrationFailed { kind: got, name: got_name, reason: Some(reason), .. }
                if *got == kind
                    && got_name == name
                    && reason.contains("duplicate definition within a file's spec scopes")
        )),
        "the collision itself is still reported: {errors:?}"
    );
}

fn duplicate_fields(errors: &[TypeCheckError]) -> Vec<(&str, &str)> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::DuplicateStructFieldDefinition {
                struct_name,
                field_name,
                ..
            } => Some((struct_name.as_str(), field_name.as_str())),
            _ => None,
        })
        .collect()
}

fn duplicate_variants(errors: &[TypeCheckError]) -> Vec<(&str, &str)> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::DuplicateEnumVariant {
                enum_name,
                variant_name,
                ..
            } => Some((enum_name.as_str(), variant_name.as_str())),
            _ => None,
        })
        .collect()
}

fn misplaced_receivers(errors: &[TypeCheckError]) -> Vec<&str> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeCheckError::SelfReferenceNotFirstParameter { function_name, .. } => {
                Some(function_name.as_str())
            }
            _ => None,
        })
        .collect()
}

mod collided_declaration {
    use super::*;

    #[test]
    fn duplicate_field_is_reported_beside_the_collision() {
        let source = r#"struct Helper { value: i32; } spec Sp { struct Helper { value: i32; value: i32; } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Struct, "Helper");
        assert_eq!(
            duplicate_fields(&errors),
            vec![("Helper", "value")],
            "the collided struct's repeated field is reported once: {errors:?}"
        );
    }

    #[test]
    fn duplicate_variant_is_reported_beside_the_collision() {
        let source = r#"enum E { A, B } spec Sp { enum E { A, A } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Enum, "E");
        assert_eq!(
            duplicate_variants(&errors),
            vec![("E", "A")],
            "the collided enum's repeated variant is reported once: {errors:?}"
        );
    }

    #[test]
    fn misplaced_receiver_is_reported_beside_the_collision() {
        // The receiver rule is what the collided declaration hid most damagingly: a
        // method whose receiver is not first mis-binds an argument, and the user was
        // shown only the collision.
        let source = r#"struct Helper { value: i32; } spec Sp { struct Helper { value: i32; fn m(k: i32, self) -> i32 { return self.value + k; } } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Struct, "Helper");
        assert_eq!(
            misplaced_receivers(&errors),
            vec!["Helper::m"],
            "the collided method's misplaced receiver is reported once: {errors:?}"
        );
    }

    #[test]
    fn every_hidden_diagnostic_surfaces_together() {
        let source = r#"struct Helper { value: i32; } enum E { A, B } spec Sp { struct Helper { value: i32; value: i32; fn m(k: i32, self) -> i32 { return k; } } enum E { A, A } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Struct, "Helper");
        assert_collision_reported(&errors, RegistrationKind::Enum, "E");
        assert_eq!(
            duplicate_fields(&errors),
            vec![("Helper", "value")],
            "repeated field surfaces: {errors:?}"
        );
        assert_eq!(
            duplicate_variants(&errors),
            vec![("E", "A")],
            "repeated variant surfaces: {errors:?}"
        );
        assert_eq!(
            misplaced_receivers(&errors),
            vec!["Helper::m"],
            "misplaced receiver surfaces: {errors:?}"
        );
    }

    /// Asserts the collided `Helper` declaration's own errors surfaced beside the
    /// collision that refused it registration: registration is partitioned so that
    /// specs go last, so the spec copy loses wherever it is written, and both
    /// orders must therefore report the same set.
    fn assert_helper_diagnostics_surfaced(errors: &[TypeCheckError]) {
        assert_collision_reported(errors, RegistrationKind::Struct, "Helper");
        assert_eq!(
            duplicate_fields(errors),
            vec![("Helper", "value")],
            "repeated field surfaces: {errors:?}"
        );
        assert_eq!(
            misplaced_receivers(errors),
            vec!["Helper::m"],
            "misplaced receiver surfaces: {errors:?}"
        );
    }

    #[test]
    fn spec_written_after_the_top_level_type_surfaces_the_hidden_diagnostics() {
        let source = r#"struct Helper { value: i32; } spec Sp { struct Helper { value: i32; value: i32; fn m(k: i32, self) -> i32 { return k; } } }"#;
        assert_helper_diagnostics_surfaced(&diagnostics(source));
    }

    #[test]
    fn spec_written_before_the_top_level_type_surfaces_the_hidden_diagnostics() {
        // The same file reordered: a user cannot make the spec declaration's errors
        // appear or vanish by moving the top-level twin past it.
        let source = r#"spec Sp { struct Helper { value: i32; value: i32; fn m(k: i32, self) -> i32 { return k; } } } struct Helper { value: i32; }"#;
        assert_helper_diagnostics_surfaced(&diagnostics(source));
    }

    /// Two specs in one file collide with each other the same way a spec collides
    /// with a top-level type, so the second one's declaration errors surface too.
    #[test]
    fn spec_against_spec_collision_reports_the_hidden_diagnostics() {
        let source =
            r#"spec A { struct Helper { x: i32; } } spec B { struct Helper { y: i32; y: i32; } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Struct, "Helper");
        assert_eq!(
            duplicate_fields(&errors),
            vec![("Helper", "y")],
            "the second spec's repeated field is reported: {errors:?}"
        );
    }

    /// A cycle through a collided declaration's own field legitimately stays
    /// unreported. The cycle check resolves each field type through the symbol
    /// table, and the collided declaration is deliberately never registered there,
    /// so there is nothing to walk. Only what the declaration carries
    /// syntactically — its fields, its variants, its receiver position — is
    /// recoverable from the refused path, and this pins that boundary rather than
    /// claiming more.
    #[test]
    fn recursive_field_of_a_collided_struct_stays_unreported() {
        let source =
            r#"struct Helper { other: i32; } spec Sp { struct Helper { inner: Helper; } }"#;
        let errors = diagnostics(source);
        assert_collision_reported(&errors, RegistrationKind::Struct, "Helper");
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, TypeCheckError::RecursiveStructDefinition { .. })),
            "a collided declaration never registers, so its cycle is not walked: {errors:?}"
        );
        assert_eq!(
            errors.len(),
            1,
            "the collision is the whole report for this declaration: {errors:?}"
        );
    }

    #[test]
    fn collided_spec_struct_in_imported_file_surfaces_with_the_file_label() {
        // Registration walks the merged multi-file arena, where source locations are
        // per-file-local, so a recovered declaration diagnostic must carry the
        // defining file's label exactly as the collision beside it does.
        let files = [
            (
                vec![],
                "use lib::helper::{Helper}; pub fn main() -> i32 { let h: Helper = Helper { value: 1 }; return h.value; }",
            ),
            (
                vec!["lib", "helper"],
                "pub struct Helper { value: i32; } spec Sp { struct Helper { value: i32; value: i32; } }",
            ),
        ];
        let Err(err) = try_type_check_multi_file(&files) else {
            panic!("a colliding spec struct in an imported file must be rejected");
        };
        let message = err.to_string();
        assert!(
            message.contains(
                "lib::helper:1:45: error registering struct `Helper`: duplicate definition within a file's spec scopes"
            ),
            "the collision is attributed to the defining file: {message}"
        );
        assert!(
            message.contains(
                "lib::helper:1:73: duplicate field `value` in struct definition `Helper`"
            ),
            "the recovered declaration diagnostic carries the same label: {message}"
        );
    }
}

mod registered_declaration {
    use super::*;

    /// Making the checks reachable from the rejected path must not report them
    /// twice on the path that does register, which is the same code walked in the
    /// registration arm.
    #[test]
    fn spec_inner_struct_without_a_collision_reports_a_repeated_field_once() {
        let source = r#"spec Sp { struct Helper { value: i32; value: i32; } }"#;
        let errors = diagnostics(source);
        assert_eq!(
            duplicate_fields(&errors),
            vec![("Helper", "value")],
            "one repeated field, one report: {errors:?}"
        );
    }

    #[test]
    fn top_level_struct_reports_a_repeated_field_once() {
        let source = r#"struct Helper { value: i32; value: i32; }"#;
        let errors = diagnostics(source);
        assert_eq!(
            duplicate_fields(&errors),
            vec![("Helper", "value")],
            "one repeated field, one report: {errors:?}"
        );
    }

    #[test]
    fn top_level_enum_reports_a_repeated_variant_once() {
        let source = r#"enum E { A, A }"#;
        let errors = diagnostics(source);
        assert_eq!(
            duplicate_variants(&errors),
            vec![("E", "A")],
            "one repeated variant, one report: {errors:?}"
        );
    }

    #[test]
    fn spec_inner_enum_without_a_collision_reports_a_repeated_variant_once() {
        let source = r#"spec Sp { enum E { A, A } }"#;
        let errors = diagnostics(source);
        assert_eq!(
            duplicate_variants(&errors),
            vec![("E", "A")],
            "one repeated variant, one report: {errors:?}"
        );
    }

    #[test]
    fn spec_inner_struct_without_a_collision_reports_a_misplaced_receiver_once() {
        let source = r#"spec Sp { struct Helper { value: i32; fn m(k: i32, self) -> i32 { return self.value + k; } } }"#;
        let errors = diagnostics(source);
        assert_eq!(
            errors.len(),
            1,
            "a registered spec struct reports the misplacement once: {errors:?}"
        );
        assert_eq!(misplaced_receivers(&errors), vec!["Helper::m"]);
    }

    /// Two mistakes in one registered declaration, which is where a double report
    /// would show up as a count rather than as a wrong name: the whole aggregate is
    /// two diagnostics, one per mistake.
    #[test]
    fn spec_inner_struct_with_two_mistakes_reports_each_once() {
        let source = r#"spec Sp { struct Helper { value: i32; value: i32; fn m(k: i32, self) -> i32 { return k; } } }"#;
        let errors = diagnostics(source);
        assert_eq!(
            errors.len(),
            2,
            "one repeated field and one misplaced receiver, reported once each: {errors:?}"
        );
        assert_eq!(duplicate_fields(&errors), vec![("Helper", "value")]);
        assert_eq!(misplaced_receivers(&errors), vec!["Helper::m"]);
    }

    /// The contrast to [`super::collided_declaration::recursive_field_of_a_collided_struct_stays_unreported`]:
    /// the cycle diagnostic does exist for a spec-inner struct, and is lost only
    /// through the refused-registration path.
    #[test]
    fn recursive_field_of_a_registered_spec_struct_is_reported() {
        let source = r#"spec Sp { struct Helper { inner: Helper; } }"#;
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                TypeCheckError::RecursiveStructDefinition { struct_name, field_name, .. }
                    if struct_name == "Helper" && field_name == "inner"
            )),
            "a registered spec struct has its cycle walked: {errors:?}"
        );
    }
}
