//! Arguments written as text, read as the values a function's parameters
//! take, and values written back as text.

use spacewasm::{ValType, Value};

use crate::errors::ArgumentError;
use crate::module::ExportedFunction;

/// Reads `raw` as the arguments of `function`, one per parameter it declares.
///
/// Each argument is a decimal integer of its parameter's width, `i32` or
/// `i64`. The parameter types are the ones the module declares, which is why
/// this takes an [`ExportedFunction`] rather than a count: a hidden pointer the
/// lowering introduced for an aggregate return is one of them, and whether
/// `-1` is 32 or 64 bits wide is the module's to say.
///
/// # Errors
///
/// Returns an [`ArgumentError`] naming the first argument that is not a value
/// of its parameter, or the two counts when they differ. A floating-point
/// parameter is refused whatever its argument says.
pub fn coerce_arguments(
    function: &ExportedFunction,
    raw: &[String],
) -> Result<Vec<Value>, ArgumentError> {
    if raw.len() != function.params.len() {
        return Err(ArgumentError::Count {
            function: function.name.clone(),
            expected: function.params.len(),
            given: raw.len(),
        });
    }
    raw.iter()
        .zip(function.params.iter())
        .enumerate()
        .map(|(index, (text, ty))| {
            let not_an_integer = || ArgumentError::NotAnInteger {
                function: function.name.clone(),
                position: index + 1,
                ty: *ty,
                text: text.clone(),
            };
            match ty {
                ValType::I32 => text.parse::<i32>().map(Value::I32).map_err(|_| not_an_integer()),
                ValType::I64 => text.parse::<i64>().map(Value::I64).map_err(|_| not_an_integer()),
                ValType::F32 | ValType::F64 => Err(ArgumentError::FloatingPoint {
                    function: function.name.clone(),
                    position: index + 1,
                    ty: *ty,
                }),
            }
        })
        .collect()
}

/// A value as a command line prints it: an integer in signed decimal, a float
/// as Rust writes it.
#[must_use]
pub fn render(value: Value) -> String {
    match value {
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
    }
}

/// The WebAssembly spelling of a value type.
#[must_use]
pub fn type_name(ty: ValType) -> &'static str {
    match ty {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
    }
}

/// "1 argument" or "3 arguments".
pub(crate) fn arguments_phrase(count: usize) -> String {
    if count == 1 {
        "1 argument".to_string()
    } else {
        format!("{count} arguments")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(params: &[ValType]) -> ExportedFunction {
        ExportedFunction { name: "f".to_string(), params: params.to_vec(), result: None }
    }

    fn raw(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    /// Each argument is read at its own parameter's width, and the bounds of
    /// each width are values of it.
    ///
    /// Fails if a parameter is read at the other width, which would take
    /// `2147483648` as an `i32` or refuse `-9223372036854775808` as an `i64`.
    #[test]
    fn each_argument_is_read_at_its_parameters_width() {
        let signature = function(&[ValType::I32, ValType::I64, ValType::I32, ValType::I64]);
        assert_eq!(
            coerce_arguments(
                &signature,
                &raw(&["-2147483648", "-9223372036854775808", "2147483647", "9223372036854775807"])
            ),
            Ok(vec![
                Value::I32(i32::MIN),
                Value::I64(i64::MIN),
                Value::I32(i32::MAX),
                Value::I64(i64::MAX),
            ])
        );
        assert_eq!(
            coerce_arguments(&function(&[ValType::I64]), &raw(&["2147483648"])),
            Ok(vec![Value::I64(2_147_483_648)])
        );
    }

    /// A value past its parameter's width, and text that is no integer, are
    /// both refused naming the argument, its type and what was written.
    ///
    /// Fails if a refusal names the wrong position or type, or if the text
    /// stops being quoted back.
    #[test]
    fn an_argument_that_is_not_an_integer_of_its_width_is_refused() {
        for (ty, text) in [
            (ValType::I32, "2147483648"),
            (ValType::I32, "-2147483649"),
            (ValType::I64, "9223372036854775808"),
            (ValType::I32, "4x"),
            (ValType::I32, ""),
            (ValType::I64, "1.5"),
        ] {
            let error = coerce_arguments(&function(&[ValType::I32, ty]), &raw(&["0", text]))
                .expect_err("the second argument is not a value of its parameter");
            assert_eq!(
                error.to_string(),
                format!("argument 2 of `f` is `{}`, and `{text}` is not one", type_name(ty))
            );
        }
    }

    /// A floating-point parameter is refused whatever its argument says.
    ///
    /// Fails if a float argument starts being parsed, which this reader does
    /// not decide how to spell.
    #[test]
    fn a_floating_point_parameter_is_refused() {
        for ty in [ValType::F32, ValType::F64] {
            let error = coerce_arguments(&function(&[ty]), &raw(&["1"]))
                .expect_err("no argument fills a float parameter");
            assert_eq!(
                error,
                ArgumentError::FloatingPoint { function: "f".to_string(), position: 1, ty }
            );
            assert!(error.to_string().starts_with(&format!(
                "argument 1 of `f` is `{}`, and this harness passes decimal integers only: ",
                type_name(ty)
            )));
        }
    }

    /// A count that differs is refused with both counts, in words.
    ///
    /// Fails if either count is dropped or the singular is spelled as a plural.
    #[test]
    fn a_different_count_is_refused_with_both_counts() {
        let two = function(&[ValType::I32, ValType::I32]);
        assert_eq!(
            coerce_arguments(&two, &raw(&["1"])).map_err(|e| e.to_string()),
            Err("`f` takes 2 arguments; 1 argument given".to_string())
        );
        assert_eq!(
            coerce_arguments(&function(&[ValType::I32]), &raw(&[])).map_err(|e| e.to_string()),
            Err("`f` takes 1 argument; 0 arguments given".to_string())
        );
        assert_eq!(coerce_arguments(&function(&[]), &raw(&[])), Ok(Vec::new()));
    }

    /// Every value type renders in decimal and is named in WebAssembly's
    /// spelling.
    #[test]
    fn values_render_in_decimal_and_types_in_webassemblys_spelling() {
        assert_eq!(render(Value::I32(-7)), "-7");
        assert_eq!(render(Value::I64(i64::MIN)), "-9223372036854775808");
        assert_eq!(render(Value::F32(1.5)), "1.5");
        assert_eq!(render(Value::F64(-0.25)), "-0.25");
        assert_eq!(
            [ValType::I32, ValType::I64, ValType::F32, ValType::F64].map(type_name),
            ["i32", "i64", "f32", "f64"]
        );
    }
}
