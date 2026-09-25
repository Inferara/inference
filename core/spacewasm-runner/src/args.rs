//! Arguments written as text, read as the values a function's parameters
//! take, and values written back as text.

use std::num::IntErrorKind;

use spacewasm::{ValType, Value};

use crate::errors::ArgumentError;
use crate::module::ExportedFunction;

/// Reads `raw` as the arguments of `function`, one per parameter it declares.
///
/// Each argument is a decimal integer of its parameter's width, `i32` or
/// `i64`, in the signed range or the unsigned one: a value above the signed
/// maximum is taken as its unsigned bit pattern, so `4294967295` fills an `i32`
/// with `-1`. The parameter types are the ones the module declares, which is
/// why this takes an [`ExportedFunction`] rather than a count: a hidden pointer
/// the lowering introduced for an aggregate return is one of them, and whether
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
            params: function.params.clone(),
            given: raw.to_vec(),
        });
    }
    raw.iter()
        .zip(function.params.iter())
        .enumerate()
        .map(|(index, (text, ty))| coerce(function, index + 1, text, *ty))
        .collect()
}

/// Reads `text`, the argument at `position`, as a value of `ty`.
fn coerce(
    function: &ExportedFunction,
    position: usize,
    text: &str,
    ty: ValType,
) -> Result<Value, ArgumentError> {
    let from_wide: fn(i128) -> Option<Value> = match ty {
        ValType::I32 => |wide| {
            let signed = i32::try_from(wide).ok();
            signed.or_else(|| u32::try_from(wide).ok().map(u32::cast_signed)).map(Value::I32)
        },
        ValType::I64 => |wide| {
            let signed = i64::try_from(wide).ok();
            signed.or_else(|| u64::try_from(wide).ok().map(u64::cast_signed)).map(Value::I64)
        },
        ValType::F32 | ValType::F64 => {
            return Err(ArgumentError::FloatingPoint {
                function: function.name.clone(),
                position,
                ty,
            });
        }
    };
    let out_of_range = || ArgumentError::OutOfRange {
        function: function.name.clone(),
        position,
        ty,
        text: text.to_string(),
    };
    let wide = text.parse::<i128>().map_err(|error| match error.kind() {
        IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => out_of_range(),
        _ => ArgumentError::NotAnInteger {
            function: function.name.clone(),
            position,
            params: function.params.clone(),
            text: text.to_string(),
        },
    })?;
    from_wide(wide).ok_or_else(out_of_range)
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

    fn function(name: &str, params: &[ValType]) -> ExportedFunction {
        ExportedFunction { name: name.to_string(), params: params.to_vec(), result: None }
    }

    fn raw(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    /// The one value `text` reads as for a parameter of `ty`.
    fn read(ty: ValType, text: &str) -> Result<Value, ArgumentError> {
        coerce_arguments(&function("f", &[ty]), &raw(&[text])).map(|mut values| values.remove(0))
    }

    /// Each argument is read at its own parameter's width, and the bounds of
    /// each width's signed range are values of it.
    ///
    /// Fails if a parameter is read at the other width, which would read
    /// `4294967295` for an `i64` as the `i32` bit pattern `-1`, or refuse
    /// `-9223372036854775808` as an `i64`.
    #[test]
    fn each_argument_is_read_at_its_parameters_width() {
        let signature = function("f", &[ValType::I32, ValType::I64, ValType::I32, ValType::I64]);
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
        assert_eq!(read(ValType::I64, "2147483648"), Ok(Value::I64(2_147_483_648)));
        assert_eq!(read(ValType::I64, "4294967295"), Ok(Value::I64(4_294_967_295)));
    }

    /// A value above the signed maximum and within the unsigned one is taken
    /// as its bit pattern, at both widths and at both ends of that stretch.
    ///
    /// Fails if the unsigned half of the range is refused, or read as some
    /// other number than the bit pattern it spells.
    #[test]
    fn a_value_in_the_unsigned_range_is_taken_as_its_bit_pattern() {
        for (ty, text, value) in [
            (ValType::I32, "2147483648", Value::I32(i32::MIN)),
            (ValType::I32, "3000000000", Value::I32(-1_294_967_296)),
            (ValType::I32, "4294967295", Value::I32(-1)),
            (ValType::I64, "9223372036854775808", Value::I64(i64::MIN)),
            (ValType::I64, "18446744073709551615", Value::I64(-1)),
        ] {
            assert_eq!(read(ty, text), Ok(value), "{text} as {}", type_name(ty));
        }
    }

    /// A decimal integer may carry a leading sign, `+` included, and leading
    /// zeros, and reads as the number it spells.
    ///
    /// Fails if either spelling is refused, or read as another number.
    #[test]
    fn a_sign_and_leading_zeros_are_accepted() {
        for (ty, text, value) in [
            (ValType::I32, "+7", Value::I32(7)),
            (ValType::I32, "-0", Value::I32(0)),
            (ValType::I32, "007", Value::I32(7)),
        ] {
            assert_eq!(read(ty, text), Ok(value), "{text} as {}", type_name(ty));
        }
    }

    /// A decimal integer outside both ranges of its width is refused as out of
    /// range, naming the whole range and how its unsigned half is read — one
    /// past each end, and far past it.
    ///
    /// Fails if either end of the range moves, if a number too long for any
    /// integer type is called "not a decimal integer", or if the range text
    /// stops saying what a large value means.
    #[test]
    fn a_value_past_both_ranges_is_refused_as_out_of_range() {
        let huge = "9".repeat(50);
        let huge_negative = format!("-{huge}");
        for (ty, text) in [
            (ValType::I32, "4294967296"),
            (ValType::I32, "-2147483649"),
            (ValType::I64, "18446744073709551616"),
            (ValType::I64, "-9223372036854775809"),
            (ValType::I32, huge.as_str()),
            (ValType::I64, huge_negative.as_str()),
        ] {
            let error = read(ty, text).expect_err("outside both ranges");
            assert_eq!(
                error,
                ArgumentError::OutOfRange {
                    function: "f".to_string(),
                    position: 1,
                    ty,
                    text: text.to_string(),
                }
            );
        }
        assert_eq!(
            read(ValType::I32, "5000000000").map_err(|e| e.to_string()),
            Err("argument 1 for `f` is 5000000000, which does not fit an i32 (-2147483648 to \
                 4294967295; a value above 2147483647 is taken as its unsigned bit pattern)"
                .to_string())
        );
        assert_eq!(
            read(ValType::I64, "-9223372036854775809").map_err(|e| e.to_string()),
            Err("argument 1 for `f` is -9223372036854775809, which does not fit an i64 \
                 (-9223372036854775808 to 18446744073709551615; a value above \
                 9223372036854775807 is taken as its unsigned bit pattern)"
                .to_string())
        );
    }

    /// Text that is no decimal integer is refused naming the argument, what
    /// was written and the whole signature.
    ///
    /// Fails if a refusal names the wrong position, stops quoting the text back
    /// or drops the signature, or if one of these spellings starts being read
    /// as a number.
    #[test]
    fn text_that_is_no_decimal_integer_is_refused_with_the_signature() {
        let add = function("add", &[ValType::I32, ValType::I64]);
        for text in ["4x", "", "1.5", "0x10", " 1", "1 ", "1_000", "-", "+", "--fuel", "１"] {
            let error = coerce_arguments(&add, &raw(&["0", text]))
                .expect_err("the second argument is not a decimal integer");
            assert_eq!(
                error,
                ArgumentError::NotAnInteger {
                    function: "add".to_string(),
                    position: 2,
                    params: vec![ValType::I32, ValType::I64],
                    text: text.to_string(),
                }
            );
            assert_eq!(
                error.to_string(),
                format!(
                    "argument 2 for `add` is `{text}`, which is not a decimal integer; `add` takes \
                     (i32, i64)"
                )
            );
        }
    }

    /// A floating-point parameter is refused whatever its argument says, a
    /// decimal integer included.
    ///
    /// Fails if a float argument starts being parsed, which this reader does
    /// not decide how to spell, or if the refusal names the wrong type.
    #[test]
    fn a_floating_point_parameter_is_refused() {
        for ty in [ValType::F32, ValType::F64] {
            for text in ["1", "0.5", "abc"] {
                let error =
                    coerce_arguments(&function("f", &[ValType::I32, ty]), &raw(&["1", text]))
                        .expect_err("no argument fills a float parameter");
                assert_eq!(
                    error,
                    ArgumentError::FloatingPoint { function: "f".to_string(), position: 2, ty }
                );
                assert_eq!(
                    error.to_string(),
                    format!("`f` takes an {} argument, which the runner cannot pass", type_name(ty))
                );
                assert_eq!(
                    error.clause("`infs run`"),
                    format!("`f` takes an {} argument, which `infs run` cannot pass", type_name(ty))
                );
                assert_eq!(error.clause("the runner"), error.to_string());
            }
        }
    }

    /// Only a floating-point parameter's refusal names the program passing
    /// the arguments, so every other refusal's clause is its text, whichever
    /// name a caller gives.
    ///
    /// Fails if a caller's name leaks into a refusal that names no program,
    /// or a clause drifts from the text it is.
    #[test]
    fn a_refusal_naming_no_program_reads_as_its_text_for_every_caller() {
        let add = function("add", &[ValType::I32, ValType::I32]);
        let refused = |args: &[&str]| {
            coerce_arguments(&add, &raw(args)).expect_err("the arguments are refused")
        };
        let errors = [refused(&["1"]), refused(&["1", "4x"]), refused(&["1", "5000000000"])];
        assert!(
            matches!(
                errors,
                [
                    ArgumentError::Count { .. },
                    ArgumentError::NotAnInteger { .. },
                    ArgumentError::OutOfRange { .. }
                ]
            ),
            "one row per refusal that names no program: {errors:?}"
        );
        for error in errors {
            for embedder in ["`infs run`", "the runner"] {
                assert_eq!(error.clause(embedder), error.to_string(), "{error:?} for {embedder}");
            }
        }
    }

    /// A count that differs is refused with the parameters' types and the
    /// arguments as written, in the grammar each count takes.
    ///
    /// Fails if either list is dropped, or a singular is spelled as a plural.
    #[test]
    fn a_different_count_is_refused_with_the_types_and_the_arguments() {
        let add = function("add", &[ValType::I32, ValType::I32]);
        for (params, args, text) in [
            (
                &add,
                &["2", "40", "7"][..],
                "`add` takes 2 arguments (i32, i32), and 3 were given: 2 40 7",
            ),
            (&add, &["1"][..], "`add` takes 2 arguments (i32, i32), and 1 was given: 1"),
            (&add, &[][..], "`add` takes 2 arguments (i32, i32), and none were given"),
        ] {
            let error = coerce_arguments(params, &raw(args)).expect_err("the counts differ");
            assert_eq!(error.to_string(), text);
        }
        assert_eq!(
            coerce_arguments(&function("main", &[]), &raw(&["5"])).map_err(|e| e.to_string()),
            Err("`main` takes no arguments, and 1 was given: 5".to_string())
        );
        assert_eq!(
            coerce_arguments(&function("wide", &[ValType::I64]), &raw(&["1", "2"])),
            Err(ArgumentError::Count {
                function: "wide".to_string(),
                params: vec![ValType::I64],
                given: raw(&["1", "2"]),
            })
        );
        assert_eq!(
            coerce_arguments(&function("wide", &[ValType::I64]), &raw(&[]))
                .map_err(|e| e.to_string()),
            Err("`wide` takes 1 argument (i64), and none were given".to_string())
        );
        assert_eq!(coerce_arguments(&function("main", &[]), &raw(&[])), Ok(Vec::new()));
    }

    /// What a function takes is one clause, in the grammar each count takes
    /// and with each parameter's type in order, and a count refusal opens
    /// with it.
    ///
    /// Fails if a singular is spelled as a plural, a type is dropped, read at
    /// the other width or moved, or the refusal stops opening with the
    /// clause a caller composes its own sentences from.
    #[test]
    fn a_functions_arity_clause_names_its_parameter_types() {
        for (name, params, clause) in [
            ("main", &[][..], "`main` takes no arguments"),
            ("narrow", &[ValType::I32][..], "`narrow` takes 1 argument (i32)"),
            ("wide", &[ValType::I64][..], "`wide` takes 1 argument (i64)"),
            ("add", &[ValType::I32, ValType::I32][..], "`add` takes 2 arguments (i32, i32)"),
            ("mix", &[ValType::I64, ValType::I32][..], "`mix` takes 2 arguments (i64, i32)"),
        ] {
            let signature = function(name, params);
            assert_eq!(signature.arity_clause(), clause);
            let refusal = coerce_arguments(&signature, &raw(&["1", "2", "3"]))
                .expect_err("three arguments are more than any of these takes");
            assert_eq!(refusal.to_string(), format!("{clause}, and 3 were given: 1 2 3"));
        }
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
