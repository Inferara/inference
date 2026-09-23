//! The per-export source-type descriptor published on `CodegenOutput`.
//!
//! A consumer that reads only the emitted module sees `i32` where the source
//! wrote `bool`, `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, an enum tag, a struct
//! pointer or an array pointer. Anything that has to choose a foreign calling
//! convention for an exported function — a contract ABI, a generated binding —
//! needs to know which of those it is holding, and the descriptor is where that
//! is written down.
//!
//! Two properties are load-bearing and are asserted here rather than assumed.
//! An entry describes only what the source declared: the hidden pointer of a
//! compound return is not one of its parameters, and the return records the
//! convention instead. And exactly the functions the export section carries are
//! described — a private function, a method, a `pub fn` in an imported file and
//! a spec-inner function are all absent, and the rest follow the order of the
//! function exports in the export section, which also carries `memory` and
//! `__stack_pointer` for a module with memory.
//!
//! Each parameter also carries the name the source gave it, since a contract
//! spec, for one, calls an input by its name. The module's `name` custom section
//! records a named parameter only as optional debug data, keyed by function and
//! local index, which an optimizer may strip and which records nothing for `_`;
//! the descriptor is the record a consumer reads. A parameter written `_` is
//! recorded as having none, not given one.

#[cfg(test)]
mod export_abi_descriptor_tests {
    use crate::utils::{
        codegen_output, codegen_output_multi_file_no_analysis,
        codegen_with_full_config_no_analysis,
    };
    use inf_wasmparser::{ExternalKind, Parser, Payload};
    use inference_wasm_codegen::{
        AbiParam, AbiReturn, AbiType, CodegenOutput, CompilationMode, ExportSignature, Target,
    };

    /// The exported function names the emitted module carries, in export-section
    /// order, skipping the `memory` and `__stack_pointer` exports that share the
    /// section.
    ///
    /// The descriptor claims to be in that order and to cover exactly those
    /// names, and both halves of the claim are only checkable against the bytes.
    fn exported_function_names(wasm: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            if let Payload::ExportSection(reader) = payload.expect("valid wasm payload") {
                for export in reader {
                    let export = export.expect("valid export");
                    if export.kind == ExternalKind::Func {
                        names.push(export.name.to_string());
                    }
                }
            }
        }
        names
    }

    /// The declared parameter and result counts of the exported function `name`.
    ///
    /// The one cross-check that ties the descriptor to the module: a wrapper
    /// generated from an entry has to call the function the entry describes, so
    /// a descriptor with an arity the emitted function does not have would
    /// produce a module that fails to validate.
    fn exported_function_arity(wasm: &[u8], name: &str) -> (usize, usize) {
        let mut func_types: Vec<(usize, usize)> = Vec::new();
        let mut function_section: Vec<u32> = Vec::new();
        let mut exports: Vec<(String, u32)> = Vec::new();
        for payload in Parser::new(0).parse_all(wasm) {
            match payload.expect("valid wasm payload") {
                Payload::TypeSection(reader) => {
                    for func_type in reader.into_iter_err_on_gc_types() {
                        let func_type = func_type.expect("function type");
                        func_types.push((func_type.params().len(), func_type.results().len()));
                    }
                }
                Payload::FunctionSection(reader) => {
                    for type_idx in reader {
                        function_section.push(type_idx.expect("valid function type index"));
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("valid export");
                        if export.kind == ExternalKind::Func {
                            exports.push((export.name.to_string(), export.index));
                        }
                    }
                }
                _ => {}
            }
        }
        let (_, func_idx) = exports
            .iter()
            .find(|(export_name, _)| export_name == name)
            .unwrap_or_else(|| panic!("no exported function named `{name}`"));
        let type_idx = function_section[*func_idx as usize];
        func_types[type_idx as usize]
    }

    fn names_of(output: &CodegenOutput) -> Vec<&str> {
        output
            .export_signatures()
            .iter()
            .map(|signature| signature.name.as_str())
            .collect()
    }

    fn signature<'a>(output: &'a CodegenOutput, name: &str) -> &'a ExportSignature {
        output
            .export_signatures()
            .iter()
            .find(|signature| signature.name == name)
            .unwrap_or_else(|| panic!("no export signature named `{name}`"))
    }

    /// The four shapes the descriptor exists to tell apart, in one module: three
    /// scalars that all lower to `i32`, a function with neither parameter nor
    /// result, a struct parameter and an array return that share the `i32`
    /// pointer lowering while meaning entirely different things, and the one
    /// width that lowers to `i64`.
    #[test]
    fn four_exports_are_described_in_export_section_order() {
        let source = "\
struct Point { x: i32; y: i32; }

pub fn f(a: u32, b: i32, c: bool) -> u32 {
    if c {
        return a;
    }
    return 0;
}

pub fn g() {
}

pub fn h(s: Point) -> [i32; 4] {
    return [s.x, s.y, s.x, s.y];
}

pub fn k(n: u64) -> u64 {
    return n;
}
";
        let output = codegen_output(source);
        let signatures = output.export_signatures();

        assert_eq!(
            names_of(&output),
            vec!["f", "g", "h", "k"],
            "one entry per exported function, in declaration order"
        );
        assert_eq!(
            names_of(&output),
            exported_function_names(output.wasm()),
            "the descriptor follows the order of the function exports and names \
             exactly the functions the export section carries"
        );
        assert_eq!(signatures.len(), 4);

        assert_eq!(
            signatures[0].params,
            vec![
                AbiParam::named("a", AbiType::U32),
                AbiParam::named("b", AbiType::I32),
                AbiParam::named("c", AbiType::Bool),
            ],
            "three parameters that all lower to `i32` keep their source types and names"
        );
        assert_eq!(signatures[0].ret, AbiReturn::Scalar(AbiType::U32));

        assert!(signatures[1].params.is_empty());
        assert_eq!(signatures[1].ret, AbiReturn::Unit);

        assert_eq!(
            signatures[2].params,
            vec![AbiParam::named(
                "s",
                AbiType::Struct {
                    name: "Point".to_string()
                }
            )],
            "the sret pointer is not a declared parameter, so `h` has exactly one"
        );
        assert_eq!(
            signatures[2].ret,
            AbiReturn::Sret(AbiType::Array {
                elem: Box::new(AbiType::I32),
                len: 4,
            }),
        );

        assert_eq!(signatures[3].params, vec![AbiParam::named("n", AbiType::U64)]);
        assert_eq!(signatures[3].ret, AbiReturn::Scalar(AbiType::U64));
    }

    /// The descriptor's arity is the emitted function's arity, once the sret
    /// pointer the return already records is accounted for. A rewriter builds a
    /// wrapper that calls the described function, so a mismatch here is a module
    /// that does not validate.
    #[test]
    fn declared_arity_matches_the_emitted_function() {
        let source = "\
struct Point { x: i32; y: i32; }

pub fn scalars(a: u32, b: bool) -> i32 {
    if b {
        return 1;
    }
    return 0;
}

pub fn compound(s: Point) -> [i32; 2] {
    return [s.x, s.y];
}

pub fn nothing() {
}
";
        let output = codegen_output(source);

        assert_eq!(exported_function_arity(output.wasm(), "scalars"), (2, 1));
        assert_eq!(signature(&output, "scalars").params.len(), 2);

        // One declared parameter plus the hidden pointer, and no result: the
        // shape `AbiReturn::Sret` names.
        assert_eq!(exported_function_arity(output.wasm(), "compound"), (2, 0));
        assert_eq!(signature(&output, "compound").params.len(), 1);
        assert!(matches!(
            signature(&output, "compound").ret,
            AbiReturn::Sret(_)
        ));

        assert_eq!(exported_function_arity(output.wasm(), "nothing"), (0, 0));
        assert!(signature(&output, "nothing").params.is_empty());
    }

    /// `-> ()` and no arrow at all are two spellings of one declaration, and the
    /// descriptor must not distinguish them: they produce the same empty result
    /// list in the module. There is no third: `unit` is a reserved word the
    /// parser refuses.
    #[test]
    fn every_spelling_of_a_void_return_is_unit() {
        let source = "\
pub fn implicit() {
}

pub fn parens() -> () {
}
";
        let output = codegen_output(source);

        assert_eq!(names_of(&output), vec!["implicit", "parens"]);
        for name in ["implicit", "parens"] {
            assert_eq!(
                signature(&output, name).ret,
                AbiReturn::Unit,
                "`{name}` returns nothing"
            );
        }
    }

    /// Every integer width, signed and unsigned, at both positions. Six of these
    /// eight lower to the same `i32`, which is the whole reason the descriptor
    /// is published.
    #[test]
    fn every_integer_width_keeps_its_signedness_and_size() {
        let source = "\
pub fn narrow(a: u8, b: i8, c: u16, d: i16) -> u8 {
    return a;
}

pub fn wide(a: u32, b: i32, c: u64, d: i64) -> i64 {
    return d;
}
";
        let output = codegen_output(source);

        assert_eq!(
            signature(&output, "narrow").params,
            vec![
                AbiParam::named("a", AbiType::U8),
                AbiParam::named("b", AbiType::I8),
                AbiParam::named("c", AbiType::U16),
                AbiParam::named("d", AbiType::I16),
            ],
        );
        assert_eq!(
            signature(&output, "narrow").ret,
            AbiReturn::Scalar(AbiType::U8)
        );
        assert_eq!(
            signature(&output, "wide").params,
            vec![
                AbiParam::named("a", AbiType::U32),
                AbiParam::named("b", AbiType::I32),
                AbiParam::named("c", AbiType::U64),
                AbiParam::named("d", AbiType::I64),
            ],
        );
        assert_eq!(
            signature(&output, "wide").ret,
            AbiReturn::Scalar(AbiType::I64)
        );
    }

    /// An enum is a bare tag, not a pointer, and it is the one nominal type that
    /// can be returned by value. Nothing in the emitted signature separates it
    /// from an `i32` parameter.
    #[test]
    fn an_enum_is_described_as_an_enum_at_both_positions() {
        let source = "\
enum Color { Red, Green, Blue }

pub fn pick(c: Color, n: i32) -> Color {
    if n > 0 {
        return c;
    }
    return Color::Red;
}
";
        let output = codegen_output(source);
        let pick = signature(&output, "pick");

        assert_eq!(
            pick.params,
            vec![
                AbiParam::named(
                    "c",
                    AbiType::Enum {
                        name: "Color".to_string()
                    }
                ),
                AbiParam::named("n", AbiType::I32),
            ],
        );
        assert_eq!(
            pick.ret,
            AbiReturn::Scalar(AbiType::Enum {
                name: "Color".to_string()
            }),
            "an enum returns its tag by value, so it is a scalar return"
        );
        assert_eq!(
            exported_function_arity(output.wasm(), "pick"),
            (2, 1),
            "an enum return is a result, not a hidden pointer"
        );
    }

    /// A struct return takes the same hidden-pointer convention an array return
    /// takes, and the two are distinguishable only here.
    #[test]
    fn a_struct_return_is_an_sret_struct() {
        let source = "\
struct Point { x: i32; y: i32; }

pub fn origin() -> Point {
    return Point { x: 0, y: 0 };
}
";
        let output = codegen_output(source);

        assert_eq!(
            signature(&output, "origin").ret,
            AbiReturn::Sret(AbiType::Struct {
                name: "Point".to_string()
            }),
        );
        assert!(
            signature(&output, "origin").params.is_empty(),
            "the caller-owned pointer is a lowering artifact, not a declared parameter"
        );
        assert_eq!(exported_function_arity(output.wasm(), "origin"), (1, 0));
    }

    /// A nested array nests, element type and all: `[[i32; 2]; 3]` is three rows
    /// of two, and a consumer laying out the region needs both numbers. One
    /// `i32` pointer arrives either way, so the nesting exists only here.
    #[test]
    fn a_nested_array_parameter_nests() {
        let source = "\
pub fn first(g: [[i32; 2]; 3]) -> i32 {
    return g[0][0];
}
";
        let output = codegen_output(source);

        assert_eq!(
            signature(&output, "first").params,
            vec![AbiParam::named(
                "g",
                AbiType::Array {
                    elem: Box::new(AbiType::Array {
                        elem: Box::new(AbiType::I32),
                        len: 2,
                    }),
                    len: 3,
                }
            )],
        );
        assert_eq!(exported_function_arity(output.wasm(), "first"), (1, 1));
    }

    /// A parameter written `_: T` binds no name but still occupies an ABI slot:
    /// the caller pushes an argument for it. Dropping it from the descriptor
    /// would shift every later parameter, and so would giving the later ones
    /// its position in the naming.
    #[test]
    fn an_unnamed_parameter_occupies_a_described_slot() {
        let source = "\
pub fn ignore_first(_: u32, b: bool) -> i32 {
    if b {
        return 1;
    }
    return 0;
}
";
        let output = codegen_output(source);

        assert_eq!(
            signature(&output, "ignore_first").params,
            vec![
                AbiParam::unnamed(AbiType::U32),
                AbiParam::named("b", AbiType::Bool),
            ],
        );
        assert_eq!(exported_function_arity(output.wasm(), "ignore_first"), (2, 1));
    }

    /// Every way a function with a body can write a parameter, and the name
    /// each one records: `_` records none, and a named parameter records its
    /// spelling exactly — a leading underscore kept, a `mut` dropped, a name
    /// longer than a Stellar contract spec admits kept whole, and the same name
    /// in two exports recorded in each. The descriptor states what the source
    /// wrote at every target; what a name is allowed to be is a target's
    /// question.
    #[test]
    fn each_parameter_records_the_name_its_declaration_spells() {
        let source = "\
pub fn f(_: u32, b: i32) -> u32 {
    return 0;
}

pub fn g(_: bool, _: u32, _kept: i32, mut counted: u32) -> u32 {
    counted = counted + 1;
    return counted;
}

pub fn h(forty_byte_parameter_name_recorded_whole: u32, b: bool) -> u32 {
    return forty_byte_parameter_name_recorded_whole;
}
";
        let output = codegen_output(source);

        assert_eq!(
            signature(&output, "f").params,
            vec![
                AbiParam::unnamed(AbiType::U32),
                AbiParam::named("b", AbiType::I32),
            ],
        );
        assert_eq!(
            signature(&output, "g").params,
            vec![
                AbiParam::unnamed(AbiType::Bool),
                AbiParam::unnamed(AbiType::U32),
                AbiParam::named("_kept", AbiType::I32),
                AbiParam::named("counted", AbiType::U32),
            ],
            "two `_` parameters are two unnamed slots, `_kept` is a name, and \
             `mut` is not part of one"
        );
        let long = "forty_byte_parameter_name_recorded_whole";
        assert_eq!(long.len(), 40, "the fixture's name is the length it claims");
        assert_eq!(
            signature(&output, "h").params,
            vec![
                AbiParam::named(long, AbiType::U32),
                AbiParam::named("b", AbiType::Bool),
            ],
            "a name is recorded whole, and `b` is recorded again in its own export"
        );
        for name in ["f", "g", "h"] {
            assert_eq!(
                exported_function_arity(output.wasm(), name).0,
                signature(&output, name).params.len(),
                "`{name}`: a named parameter and an unnamed one each occupy one slot"
            );
        }
    }

    /// `main` is exported like any other entry-file `pub fn`, so it is described
    /// like any other.
    #[test]
    fn main_is_described_like_any_other_export() {
        let source = "\
pub fn main() -> i32 {
    return 7;
}
";
        let output = codegen_output(source);

        assert!(output.has_main());
        assert_eq!(names_of(&output), vec!["main"]);
        assert_eq!(
            signature(&output, "main").ret,
            AbiReturn::Scalar(AbiType::I32)
        );
    }

    /// Only an entry-file, top-level `pub fn` is exported, and only what is
    /// exported is described. A private function, a method — public or not — and
    /// a spec-inner function are all absent, because none of them is reachable
    /// from outside the module.
    #[test]
    fn only_exported_functions_are_described() {
        let source = "\
struct Counter {
    n: i32;

    pub fn get(self) -> i32 {
        return self.n;
    }
}

fn helper(a: i32) -> i32 {
    return a + 1;
}

pub fn visible(a: i32) -> i32 {
    return helper(a);
}
";
        let output = codegen_output(source);

        assert_eq!(
            names_of(&output),
            vec!["visible"],
            "a private function and a public method are not exports"
        );
        assert_eq!(
            names_of(&output),
            exported_function_names(output.wasm()),
            "the descriptor and the export section agree about what is exported"
        );
    }

    /// A `pub fn` in an imported file is intra-project visibility, not an export,
    /// so it is described nowhere — while the entry file's own exports still are.
    #[test]
    fn a_public_function_in_an_imported_file_is_not_described() {
        let entry = "\
use lib::math::{double};

pub fn entry(a: i32) -> i32 {
    return double(a);
}
";
        let math = "\
pub fn double(n: i32) -> i32 {
    return n * 2;
}
";
        let output = codegen_output_multi_file_no_analysis(&[
            (vec![], entry),
            (vec!["lib", "math"], math),
        ]);

        assert_eq!(names_of(&output), vec!["entry"]);
        assert_eq!(
            names_of(&output),
            exported_function_names(output.wasm()),
            "the descriptor and the export section agree across files"
        );
        assert_eq!(
            signature(&output, "entry").params,
            vec![AbiParam::named("a", AbiType::I32)],
            "the entry export's name comes from its own declaration"
        );
    }

    /// A nominal type reaches code generation as one of two carriers: a bare name
    /// when it was item-imported, and a `::`-joined path when it was reached
    /// through a namespace. Both must resolve, and each records the spelling the
    /// annotation used.
    #[test]
    fn imported_and_qualified_nominal_types_both_resolve() {
        let entry = "\
use lib::shapes::{Level};
use lib::geom;

pub fn by_bare_name(l: Level) -> i32 {
    return 0;
}

pub fn by_qualified_path(p: geom::Point) -> i32 {
    return p.x;
}
";
        let shapes = "pub enum Level { Low, Mid, High }\n";
        let geom = "pub struct Point { x: i32; y: i32; }\n";
        let output = codegen_output_multi_file_no_analysis(&[
            (vec![], entry),
            (vec!["lib", "shapes"], shapes),
            (vec!["lib", "geom"], geom),
        ]);

        assert_eq!(
            signature(&output, "by_bare_name").params,
            vec![AbiParam::named(
                "l",
                AbiType::Enum {
                    name: "Level".to_string()
                }
            )],
        );
        assert_eq!(
            signature(&output, "by_qualified_path").params,
            vec![AbiParam::named(
                "p",
                AbiType::Struct {
                    name: "geom::Point".to_string()
                }
            )],
            "a qualified annotation records the path it was written with"
        );
    }

    /// A module with nothing public exports nothing and describes nothing. The
    /// empty descriptor is a statement — there is no contract surface here — and
    /// a consumer that requires one has to say so itself.
    #[test]
    fn a_module_with_no_exports_describes_nothing() {
        let source = "\
fn private(a: i32) -> i32 {
    return a;
}
";
        let output = codegen_output(source);

        assert!(output.export_signatures().is_empty());
        assert!(exported_function_names(output.wasm()).is_empty());
    }

    /// The whole corpus, not one fixture at a time: for every module the
    /// compiler produces, the descriptor names exactly the functions the export
    /// section carries, in the same order.
    ///
    /// The per-shape tests above each pin a construct somebody thought to write
    /// down. This one holds the invariant open over every construct the suite
    /// exercises, which is what makes it a net rather than a sample: a nominal
    /// form the descriptor walk learns to refuse while the value-type walk keeps
    /// accepting it produces an export with no entry, and that shows up here
    /// even when it appears in a fixture written for some unrelated reason.
    ///
    /// Analysis is skipped so that fixtures written to exercise a construct
    /// analysis rejects still reach code generation, and a fixture that fails to
    /// compile at all is another test's business.
    #[test]
    fn every_corpus_fixture_describes_exactly_its_exported_functions() {
        let sources = crate::corpus::single_file_corpus_sources();
        assert!(
            sources.len() >= 100,
            "expected at least 100 single-file fixtures, found {}; a collector \
             that silently found nothing would pass this test vacuously",
            sources.len()
        );

        let mut compiled = 0usize;
        let mut with_exports = 0usize;
        for (label, source) in &sources {
            let Ok(output) = codegen_with_full_config_no_analysis(
                source,
                Target::Wasm32,
                CompilationMode::Compile,
                Target::Wasm32.default_opt_level(),
            ) else {
                continue;
            };
            compiled += 1;
            let described: Vec<&str> = names_of(&output);
            let exported = exported_function_names(output.wasm());
            assert_eq!(
                described, exported,
                "{label}: the descriptor and the export section disagree"
            );
            if !exported.is_empty() {
                with_exports += 1;
            }
        }

        assert!(
            compiled >= 100,
            "expected at least 100 fixtures to reach code generation, got {compiled}"
        );
        assert!(
            with_exports >= 50,
            "expected at least 50 compiled fixtures to export a function, got \
             {with_exports}; comparing two empty lists proves nothing"
        );
    }
}
