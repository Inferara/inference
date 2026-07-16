//! Editor-facing rendering of a checked type to a source-like string.

use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};

/// Renders `ty` as the source-like type spelling shown in hovers and inlays.
///
/// It differs from the compiler's own `Display` in two ways chosen for a reader:
/// - the built-in scalars use their lowercase source spellings (`unit` / `bool` /
///   `string`), not the capitalized debug forms the checker prints;
/// - a struct or enum is named by its canonical key, which is the bare type name
///   for a same-file type and the `::`-qualified module path for a cross-module
///   one (`lib::geom::Point`). The key is exactly the module-qualified bare name,
///   never an internal mangled form, so it reads as source.
#[must_use]
pub(crate) fn render_type(ty: &TypeInfo) -> String {
    let mut out = render_kind(&ty.kind);
    // A bare generic reference is a type parameter, spelled with a trailing prime
    // (`T'`). A generic *application* names its base unprimed and primes each
    // argument instead (`Array u32'`); the checker lowers both to
    // `Generic`, distinguished only by whether type params are present.
    if matches!(ty.kind, TypeInfoKind::Generic(_)) && ty.type_params.is_empty() {
        out.push('\'');
    }
    for type_param in &ty.type_params {
        out.push(' ');
        out.push_str(type_param);
        out.push('\'');
    }
    out
}

fn render_kind(kind: &TypeInfoKind) -> String {
    match kind {
        TypeInfoKind::Unit => "unit".to_string(),
        TypeInfoKind::Bool => "bool".to_string(),
        TypeInfoKind::String => "string".to_string(),
        TypeInfoKind::Number(number) => number.as_str().to_string(),
        TypeInfoKind::Array(element, length) => format!("[{}; {}]", render_type(element), length),
        // The canonical key is the bare name for an entry-file type and the
        // `::`-joined module path otherwise, so it is already the module-qualified
        // bare name the editor wants.
        TypeInfoKind::Struct(_, key) | TypeInfoKind::Enum(_, key) => key.clone(),
        // `Generic` renders as its bare name here; the prime that tells a
        // reference (`T'`) apart from an application base (`Array u32'`) is added
        // in `render_type`, where the type params needed to decide it are known.
        TypeInfoKind::Custom(name)
        | TypeInfoKind::Qualified(name)
        | TypeInfoKind::QualifiedName(name)
        | TypeInfoKind::Function(name)
        | TypeInfoKind::Spec(name)
        | TypeInfoKind::Generic(name) => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_type_checker::type_info::NumberType;

    fn number(n: NumberType) -> TypeInfo {
        TypeInfo {
            kind: TypeInfoKind::Number(n),
            type_params: vec![],
        }
    }

    #[test]
    fn scalars_use_lowercase_source_spellings() {
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Bool,
                type_params: vec![]
            }),
            "bool"
        );
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Unit,
                type_params: vec![]
            }),
            "unit"
        );
        assert_eq!(render_type(&number(NumberType::I32)), "i32");
        assert_eq!(render_type(&number(NumberType::U64)), "u64");
    }

    #[test]
    fn struct_renders_by_canonical_key() {
        // Entry-file struct: key equals the bare name.
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Struct("Point".into(), "Point".into()),
                type_params: vec![],
            }),
            "Point"
        );
        // Cross-module struct: key is the `::`-qualified module path.
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Struct("Point".into(), "lib::geom::Point".into()),
                type_params: vec![],
            }),
            "lib::geom::Point"
        );
    }

    #[test]
    fn array_renders_element_and_length() {
        let ty = TypeInfo {
            kind: TypeInfoKind::Array(Box::new(number(NumberType::I32)), 4),
            type_params: vec![],
        };
        assert_eq!(render_type(&ty), "[i32; 4]");
    }

    #[test]
    fn string_and_enum_render_by_source_and_key() {
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::String,
                type_params: vec![],
            }),
            "string"
        );
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Enum("Color".into(), "lib::Color".into()),
                type_params: vec![],
            }),
            "lib::Color"
        );
    }

    #[test]
    fn opaque_string_kinds_render_verbatim() {
        let cases = [
            (TypeInfoKind::Custom("Widget".into()), "Widget"),
            (
                TypeInfoKind::Qualified("lib::geom::Point".into()),
                "lib::geom::Point",
            ),
            (
                TypeInfoKind::QualifiedName("geo::Level".into()),
                "geo::Level",
            ),
            (TypeInfoKind::Function("adder".into()), "adder"),
            (TypeInfoKind::Spec("Laws".into()), "Laws"),
        ];
        for (kind, expected) in cases {
            assert_eq!(
                render_type(&TypeInfo {
                    kind,
                    type_params: vec![],
                }),
                expected
            );
        }
    }

    #[test]
    fn generic_gets_a_prime_and_type_params_are_appended() {
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Generic("T".into()),
                type_params: vec![],
            }),
            "T'"
        );
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Custom("Vec".into()),
                type_params: vec!["T".into()],
            }),
            "Vec T'"
        );
    }

    #[test]
    fn generic_application_base_is_not_primed() {
        // `TypeInfo::from_type_id` lowers a generic application (`Array u32'`) to
        // Generic("Array") with type params ["u32"]. The prime belongs on the
        // argument, not the base: the source spelling is `Array u32'`, never
        // `Array' u32'`.
        assert_eq!(
            render_type(&TypeInfo {
                kind: TypeInfoKind::Generic("Array".into()),
                type_params: vec!["u32".into()],
            }),
            "Array u32'"
        );
    }
}
