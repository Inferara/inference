//! The conformance checker over the committed corpus, without a decoder.
//!
//! The sibling tier at `tests/tests/spacewasm/` runs the real interpreter and
//! therefore lives in an integration binary. Everything here asks the same
//! questions of `inference_target_conformance::spacewasm::check` alone, which is
//! what lets it run inside the library beside the code generation tests that
//! produce the artifacts.
//!
//! Three claims:
//!
//! 1. **The corpus is conformant.** Every committed golden this compiler emits
//!    for the default partition is a module a `SpaceWasm` embedder could load,
//!    except the proof-mode ones, which carry the custom `0xfc` verification
//!    operators and are correctly refused — they are not artifacts anything
//!    flies.
//! 2. **The numbers are reviewed.** Each golden's exact control depth, operand
//!    height and operand width are committed in [`CORPUS`], so a code generation
//!    change that deepens nesting or widens a frame shows up as a diff on a pull
//!    request rather than as a load failure on hardware. A bound in the shape of
//!    "no worse than before" would have permitted exactly the wrong direction.
//! 3. **Nothing escapes by being added.** A companion gate requires the table to
//!    name every artifact under the corpus root, and the sweep asserts a floor on
//!    how many it actually visited, so neither a new fixture nor a collector that
//!    found nothing can pass quietly.
//!
//! # Where this disagrees with the decode tier, deliberately
//!
//! The decode tier excludes two classes of golden: those carrying a verification
//! operator, and those declaring an import. Only the first is a refusal here. An
//! import section is ordinary WebAssembly 1.0 and this crate is asked whether a
//! module *conforms*, while the decoder is asked whether it *instantiates* —
//! and an import instantiates only against a host module the embedder
//! registered, which that tier deliberately registers none of. What this checker
//! does hold an import to are the caps an embedder could never satisfy: a name
//! no `HostName` can carry, an arity no host list can hold. The difference is
//! stated by a test of its own rather than left for a reader to infer from two
//! lists that do not match.

#[cfg(test)]
mod spacewasm_conformance_tests {
    use crate::corpus::{
        carries_verification_operator, codegen_for_target_no_analysis, golden_wasm_artifacts,
        has_import_section, relative_to_test_data, single_file_corpus_sources,
    };
    use inference_target_conformance::spacewasm::{LIMITS_FROM, Violation, check};
    use inference_wasm_codegen::Target;
    use wasm_encoder::{
        BlockType, CodeSection, ConstExpr, CustomSection, EntityType, ExportKind, ExportSection,
        Function, FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction, Module,
        RefType, TableSection, TableType, TypeSection, ValType,
    };

    /// What `check` must say about one committed artifact.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Verdict {
        /// Refused. The only artifacts in the corpus that earn this are the
        /// proof-mode ones, whose bodies carry the custom `0xfc` verification
        /// operators; the assertion is on the reason, not merely on the refusal.
        Refused,
        /// Accepted, with exactly these module-wide maxima: control frames
        /// including the function-body frame, operand-stack height in values,
        /// and operand-stack width in words.
        ///
        /// Three numbers rather than a pass mark because they are what an
        /// embedder is sized from, and because each of them can move for a
        /// different reason: nesting is a lowering's shape, the value height is
        /// how much an expression keeps live, and the word width is that
        /// weighted by type. A change to any one of them is a reviewed diff.
        Maxima(u32, u32, u32),
    }

    use Verdict::{Maxima, Refused};

    /// Fewer goldens than this and the walk found something other than the
    /// corpus, so the table comparison below would be comparing two short lists
    /// that happen to agree.
    const CORPUS_FLOOR: usize = 150;

    /// Fewer fixtures than this compiling for the target means a gate widened,
    /// and the single-file sweep stops covering the pipeline it exists to
    /// cover.
    const SINGLE_FILE_FLOOR: usize = 100;

    /// How many default-partition goldens declare an import.
    ///
    /// An exact count rather than a floor: this is the one class where this
    /// tier and the decode tier answer differently — conformant here, refused
    /// there for an unresolved import — so a golden joining or leaving it is a
    /// reviewer's question, the same standard [`CORPUS`] itself is held to. A
    /// floor would let the class shrink toward empty without a diff. The sibling
    /// tier commits the names; the count is what a sweep in a different binary
    /// can hold them to.
    const IMPORT_BEARING_COUNT: usize = 17;

    /// Every golden `.wasm` of the default partition, with what `check` says
    /// about it.
    ///
    /// Generated once and maintained by hand: a row that has to move is a
    /// reviewer's question about why the module changed, which is the whole
    /// value of committing the numbers.
    ///
    /// The rows are held at the `const`'s own column rather than indented
    /// inside it: a handful of corpus paths pass the column limit unindented
    /// already, and a deeper indent would push dozens more over it, wrapping
    /// rows a reviewer reads as one line each.
    const CORPUS: &[(&str, Verdict)] = &[
    ("codegen/wasm/algo_array/algo_array.wasm", Maxima(7, 6, 8)),
    ("codegen/wasm/algo_bitwise/algo_bitwise.wasm", Maxima(4, 5, 5)),
    ("codegen/wasm/algo_converge/algo_converge.wasm", Refused),
    ("codegen/wasm/algo_i64_mixed/algo_i64_mixed.wasm", Maxima(7, 3, 5)),
    ("codegen/wasm/algo_iter/algo_iter.wasm", Maxima(7, 3, 5)),
    ("codegen/wasm/arith_overflow/arith_overflow.wasm", Maxima(4, 3, 5)),
    ("codegen/wasm/base/array_assign/array_assign.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/base/array_index/array_index.wasm", Maxima(2, 4, 4)),
    ("codegen/wasm/base/array_literal/array_literal.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/array_nondet/array_nondet.wasm", Refused),
    ("codegen/wasm/base/array_of_structs/array_of_structs.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/array_params/array_params.wasm", Maxima(3, 3, 3)),
    ("codegen/wasm/base/array_self_ref_reassign/array_self_ref_reassign.wasm", Maxima(4, 3, 5)),
    ("codegen/wasm/base/array_zero_literal/array_zero_literal.wasm", Maxima(1, 2, 3)),
    ("codegen/wasm/base/assert/assert.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/base/assign/assign.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/assign_nondet/assign_nondet.wasm", Refused),
    ("codegen/wasm/base/binary_ops/binary_ops.wasm", Maxima(4, 3, 5)),
    ("codegen/wasm/base/bool_const/bool_const.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/bool_literal/bool_literal.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/const/const.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/const_array/const_array.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/const_array_sum/const_array_sum.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/const_compound_copy/const_compound_copy.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/const_compound_mixed/const_compound_mixed.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/const_in_forall/const_in_forall.wasm", Refused),
    ("codegen/wasm/base/const_sret_call/const_sret_call.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/const_struct/const_struct.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/enum_array/enum_array.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/enum_assign/enum_assign.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/base/enum_compare/enum_compare.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/base/enum_in_struct/enum_in_struct.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/enum_multi/enum_multi.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/enum_params/enum_params.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/base/enum_uzumaki_domain/enum_uzumaki_domain.wasm", Refused),
    ("codegen/wasm/base/enum_variant/enum_variant.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/fn_calls/fn_calls.wasm", Maxima(1, 2, 2)),
    ("codegen/wasm/base/fn_params/fn_params.wasm", Maxima(1, 1, 2)),
    ("codegen/wasm/base/i64_uzumaki/i64_uzumaki.wasm", Refused),
    ("codegen/wasm/base/if_bool_exprs/if_bool_exprs.wasm", Maxima(3, 2, 2)),
    ("codegen/wasm/base/if_else/if_else.wasm", Maxima(3, 2, 2)),
    ("codegen/wasm/base/if_else_compound_overlap/if_else_compound_overlap.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/if_nondet/if_nondet.wasm", Refused),
    ("codegen/wasm/base/local_variables/local_variables.wasm", Refused),
    ("codegen/wasm/base/local_variables_exec/local_variables_exec.wasm", Maxima(1, 1, 2)),
    ("codegen/wasm/base/method_array_return/method_array_return.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/method_assoc/method_assoc.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/method_cross_call/method_cross_call.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/base/method_i64_fields/method_i64_fields.wasm", Maxima(2, 3, 5)),
    ("codegen/wasm/base/method_instance/method_instance.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/method_multi_struct/method_multi_struct.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/method_return_struct/method_return_struct.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/base/method_self_mutate/method_self_mutate.wasm", Maxima(2, 4, 4)),
    ("codegen/wasm/base/method_three_fields/method_three_fields.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/mixed_visibility/mixed_visibility.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/multidim_array_literal/multidim_array_literal.wasm", Maxima(1, 3, 4)),
    ("codegen/wasm/base/multidim_array_uzumaki/multidim_array_uzumaki.wasm", Refused),
    ("codegen/wasm/base/narrow_uzumaki/narrow_uzumaki.wasm", Refused),
    ("codegen/wasm/base/nested_array_of_structs/nested_array_of_structs.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/nested_struct/nested_struct.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/nested_struct_with_array/nested_struct_with_array.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/nondet/nondet.wasm", Refused),
    ("codegen/wasm/base/numeric_literals/numeric_literals.wasm", Maxima(1, 1, 2)),
    ("codegen/wasm/base/struct_access/struct_access.wasm", Maxima(2, 3, 4)),
    ("codegen/wasm/base/struct_array_field_nondet/struct_array_field_nondet.wasm", Refused),
    ("codegen/wasm/base/struct_assign/struct_assign.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/base/struct_copy/struct_copy.wasm", Maxima(2, 3, 4)),
    ("codegen/wasm/base/struct_literal/struct_literal.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/base/struct_nondet/struct_nondet.wasm", Refused),
    ("codegen/wasm/base/struct_params/struct_params.wasm", Maxima(2, 3, 4)),
    ("codegen/wasm/base/struct_return/struct_return.wasm", Maxima(1, 3, 4)),
    ("codegen/wasm/base/struct_self_ref_reassign/struct_self_ref_reassign.wasm", Maxima(2, 4, 6)),
    ("codegen/wasm/base/struct_with_array/struct_with_array.wasm", Maxima(2, 4, 4)),
    (
        "codegen/wasm/base/struct_with_array_of_structs/struct_with_array_of_structs.wasm",
        Maxima(2, 4, 4),
    ),
    ("codegen/wasm/base/struct_with_nested_array/struct_with_nested_array.wasm", Maxima(2, 4, 4)),
    ("codegen/wasm/base/trivial/trivial.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/base/u32_uzumaki/u32_uzumaki.wasm", Refused),
    ("codegen/wasm/binops_bool/binops_bool.wasm", Maxima(3, 3, 3)),
    ("codegen/wasm/binops_compound/binops_compound.wasm", Maxima(4, 4, 7)),
    ("codegen/wasm/binops_i64_arith/binops_i64_arith.wasm", Maxima(4, 3, 5)),
    ("codegen/wasm/binops_i64_bitwise/binops_i64_bitwise.wasm", Maxima(1, 2, 4)),
    ("codegen/wasm/binops_i64_cmp/binops_i64_cmp.wasm", Maxima(1, 3, 5)),
    ("codegen/wasm/binops_narrow_shift/binops_narrow_shift.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/binops_paren/binops_paren.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/binops_shift_edge/binops_shift_edge.wasm", Maxima(1, 2, 4)),
    ("codegen/wasm/binops_sub_i32/binops_sub_i32.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/binops_u32/binops_u32.wasm", Maxima(3, 2, 2)),
    ("codegen/wasm/binops_unary_combos/binops_unary_combos.wasm", Maxima(4, 3, 5)),
    ("codegen/wasm/bulk_free/array_copy_loop/array_copy_loop.wasm", Maxima(4, 4, 5)),
    ("codegen/wasm/bulk_free/array_copy_tail/array_copy_tail.wasm", Maxima(2, 4, 4)),
    ("codegen/wasm/bulk_free/frame_fill/frame_fill.wasm", Maxima(4, 4, 6)),
    ("codegen/wasm/bulk_free/overlap_corners/overlap_corners.wasm", Maxima(4, 5, 5)),
    ("codegen/wasm/bulk_free/sret_big/sret_big.wasm", Maxima(4, 4, 6)),
    ("codegen/wasm/bulk_free/struct_copy_loop/struct_copy_loop.wasm", Maxima(4, 4, 6)),
    ("codegen/wasm/checked_arith/checked_arith.wasm", Maxima(6, 4, 7)),
    ("codegen/wasm/export_narrow_params/export_narrow_params.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/expr_deep_nesting/expr_deep_nesting.wasm", Maxima(4, 5, 5)),
    ("codegen/wasm/extern_import/host_import/host_import.wasm", Maxima(1, 1, 2)),
    ("codegen/wasm/extern_import/host_import_fprime/host_import_fprime.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/extern_import/import_dedup/import_dedup.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/extern_import/import_with_locals/import_with_locals.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/extern_import/multi_import/multi_import.wasm", Maxima(1, 2, 2)),
    ("codegen/wasm/extern_import/single_import/single_import.wasm", Maxima(1, 2, 2)),
    ("codegen/wasm/literal_ctx_array_elements/literal_ctx_array_elements.wasm", Maxima(4, 4, 7)),
    ("codegen/wasm/literal_ctx_i64/literal_ctx_i64.wasm", Maxima(4, 4, 8)),
    ("codegen/wasm/literal_ctx_nested_array/literal_ctx_nested_array.wasm", Maxima(2, 4, 6)),
    ("codegen/wasm/loops/break_nested_if/break_nested_if.wasm", Maxima(5, 3, 3)),
    ("codegen/wasm/loops/infinite_loop_break/infinite_loop_break.wasm", Maxima(6, 3, 3)),
    ("codegen/wasm/loops/loop_accumulator/loop_accumulator.wasm", Maxima(6, 3, 3)),
    ("codegen/wasm/loops/loop_break_early/loop_break_early.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/loops/loop_in_nondet/loop_in_nondet.wasm", Refused),
    ("codegen/wasm/loops/loop_return_array/loop_return_array.wasm", Maxima(5, 4, 4)),
    ("codegen/wasm/loops/loop_with_array/loop_with_array.wasm", Maxima(6, 5, 5)),
    ("codegen/wasm/loops/loop_with_if/loop_with_if.wasm", Maxima(5, 3, 3)),
    ("codegen/wasm/loops/loop_zero_init/loop_zero_init.wasm", Maxima(6, 4, 6)),
    ("codegen/wasm/loops/loop_zero_iters/loop_zero_iters.wasm", Maxima(3, 1, 1)),
    ("codegen/wasm/loops/nested_loop/nested_loop.wasm", Maxima(6, 3, 3)),
    ("codegen/wasm/loops/nondet_then_break/nondet_then_break.wasm", Refused),
    ("codegen/wasm/loops/simple_loop/simple_loop.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/loops/void_loop/void_loop.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/multi_file_golden/cross_file_method/cross_file_method.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/multi_file_golden/cross_file_struct/cross_file_struct.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/multi_file_golden/dup_struct/dup_struct.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/multi_file_golden/item_import/item_import.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/multi_file_golden/method_mangling/method_mangling.wasm", Maxima(1, 3, 3)),
    ("codegen/wasm/multi_file_golden/proof_exists/proof_exists.wasm", Maxima(2, 3, 4)),
    ("codegen/wasm/multi_file_golden/proof_specs/proof_specs.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/multi_file_golden/proof_unique/proof_unique.wasm", Maxima(2, 2, 2)),
    ("codegen/wasm/multi_file_golden/re_export_chain/re_export_chain.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/multi_file_golden/root_only_export/root_only_export.wasm", Maxima(2, 3, 3)),
    ("codegen/wasm/multi_file_golden/single_via_project/single_via_project.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/multi_file_golden/two_file/two_file.wasm", Maxima(1, 1, 1)),
    ("codegen/wasm/param_by_ref/alias_receiver/alias_receiver.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/param_by_ref/alias_same_argument/alias_same_argument.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/param_by_ref/alias_sret/alias_sret.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/param_by_ref/alias_whole_and_part/alias_whole_and_part.wasm", Maxima(4, 5, 5)),
    ("codegen/wasm/param_by_ref/extern_forward/extern_forward.wasm", Maxima(4, 3, 3)),
    (
        "codegen/wasm/param_by_ref/extern_forward_read_only/extern_forward_read_only.wasm",
        Maxima(4, 3, 3),
    ),
    ("codegen/wasm/param_by_ref/mut_never_written/mut_never_written.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/param_by_ref/read_only_params/read_only_params.wasm", Maxima(2, 4, 6)),
    ("codegen/wasm/param_by_ref/written_param/written_param.wasm", Maxima(4, 4, 6)),
    (
        "codegen/wasm/self_extern_escape/escape_nested_block/escape_nested_block.wasm",
        Maxima(4, 3, 3),
    ),
    ("codegen/wasm/self_extern_escape/escape_nested_expr/escape_nested_expr.wasm", Maxima(4, 3, 3)),
    (
        "codegen/wasm/self_extern_escape/escape_scalar_projection/escape_scalar_projection.wasm",
        Maxima(4, 3, 3),
    ),
    ("codegen/wasm/self_extern_escape/escape_sub_object/escape_sub_object.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/self_extern_escape/escape_whole_self/escape_whole_self.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/self_extern_escape/escape_with_param/escape_with_param.wasm", Maxima(4, 3, 3)),
    ("codegen/wasm/self_extern_escape/mut_self_extern/mut_self_extern.wasm", Maxima(4, 4, 4)),
    ("codegen/wasm/self_extern_escape/no_escape_self/no_escape_self.wasm", Maxima(4, 3, 3)),
    (
        "codegen/wasm/self_extern_escape/read_only_extern_self/read_only_extern_self.wasm",
        Maxima(4, 3, 3),
    ),
    ("codegen/wasm/short_circuit/short_circuit.wasm", Maxima(5, 5, 5)),
    (
        "codegen/wasm/unnamed_params/exported_ignored_enum_parameter/exported_ignored_enum_parameter.wasm",
        Maxima(2, 2, 2),
    ),
    (
        "codegen/wasm/unnamed_params/exported_ignored_narrow_parameter/exported_ignored_narrow_parameter.wasm",
        Maxima(1, 2, 2),
    ),
    (
        "codegen/wasm/unnamed_params/ignored_compound_parameter/ignored_compound_parameter.wasm",
        Maxima(1, 1, 1),
    ),
    ("codegen/wasm/unnamed_params/ignored_parameter/ignored_parameter.wasm", Maxima(2, 3, 3)),
    (
        "codegen/wasm/unnamed_params/ignored_parameter_after_receiver/ignored_parameter_after_receiver.wasm",
        Maxima(4, 3, 3),
    ),
    ("codegen/wasm/void_forms/explicit_void_return/explicit_void_return.wasm", Maxima(1, 1, 1)),
    (
        "codegen/wasm/void_forms/unit_expression_statement/unit_expression_statement.wasm",
        Maxima(1, 1, 1),
    ),
    (
        "codegen/wasm/void_forms/unit_return_type_spelled_unit/unit_return_type_spelled_unit.wasm",
        Maxima(1, 1, 1),
    ),
    ("codegen/wasm/void_forms/void_return_inside_if/void_return_inside_if.wasm", Maxima(2, 2, 3)),
    ];

    /// Reads a golden, panicking with the path rather than with an io error
    /// alone.
    fn read(path: &std::path::Path) -> Vec<u8> {
        std::fs::read(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    /// Every artifact of the default partition is named in [`CORPUS`].
    ///
    /// Without this the sweep below would be a statement about the rows that
    /// happen to be listed, and a fixture added tomorrow would be covered by
    /// nothing at all. Compared as sorted lists so the failure names the
    /// difference rather than a count.
    ///
    /// The walk is `golden_wasm_artifacts`, which is every golden except the
    /// opt-in `bulk_memory_golden` family, and that family is out of scope by
    /// construction rather than by omission: a module built with a post-1.0
    /// feature requested is outside WebAssembly 1.0 and so can never be
    /// conformant, which its own gates assert. A golden added there is fenced
    /// by those and named by no row here.
    ///
    /// Fails when a golden of the default partition is added, removed or
    /// renamed without the table moving with it.
    #[test]
    fn every_golden_in_the_corpus_is_listed() {
        let mut walked: Vec<String> = golden_wasm_artifacts()
            .iter()
            .map(|path| relative_to_test_data(path))
            .collect();
        walked.sort();
        let mut listed: Vec<String> = CORPUS.iter().map(|(name, _)| (*name).to_string()).collect();
        listed.sort();
        assert_eq!(
            walked, listed,
            "the table must name exactly the artifacts under the corpus root, or a new \
             fixture escapes the conformance sweep by being added"
        );
    }

    /// Every golden gets exactly the verdict the table records for it.
    ///
    /// The accepted rows carry their three maxima, so this is also the gate that
    /// makes a code generation change visible: a lowering that nests one level
    /// deeper, or that keeps one more value live across a call, moves a number
    /// here and shows up on the pull request.
    ///
    /// Fails when a golden's shape changes, when one starts or stops carrying a
    /// verification operator, or when the corpus shrinks below the floor.
    #[test]
    fn every_golden_matches_its_committed_verdict() {
        let artifacts = golden_wasm_artifacts();
        assert!(
            artifacts.len() >= CORPUS_FLOOR,
            "expected at least {CORPUS_FLOOR} goldens, found {}; a collector that found \
             nothing would otherwise pass this vacuously",
            artifacts.len()
        );

        let mut refused = 0;
        for path in &artifacts {
            let name = relative_to_test_data(path);
            let expected = CORPUS
                .iter()
                .find(|(listed, _)| *listed == name)
                .map(|(_, verdict)| *verdict)
                .unwrap_or_else(|| panic!("{name} is not in the table"));
            let wasm = read(path);

            match (expected, check(&wasm)) {
                (Maxima(depth, values, words), Ok(report)) => assert_eq!(
                    (report.deepest().1, report.tallest().1, report.widest().1),
                    (depth, values, words),
                    "{name}: the maxima moved. Deepest is `{}`, tallest `{}`, widest `{}`",
                    report.deepest().0,
                    report.tallest().0,
                    report.widest().0
                ),
                (Maxima(..), Err(violations)) => {
                    panic!("{name} is listed as conformant and was refused:\n{violations}")
                }
                (Refused, Ok(_)) => panic!(
                    "{name} is listed as refused and passed; a proof-mode artifact that \
                     stopped carrying its verification operators would land here"
                ),
                (Refused, Err(violations)) => {
                    assert!(
                        violations
                            .as_slice()
                            .iter()
                            .any(|violation| matches!(violation, Violation::OutsideWasm1 { .. })),
                        "{name} must be refused for carrying an instruction outside \
                         WebAssembly 1.0, not for a size limit:\n{violations}"
                    );
                    assert!(
                        carries_verification_operator(&wasm),
                        "{name} is refused, and the only artifacts in this corpus that may be \
                         are the ones carrying a verification operator"
                    );
                    refused += 1;
                }
            }
        }
        println!(
            "spacewasm conformance sweep: {} goldens, {refused} refused for carrying a \
             verification operator",
            artifacts.len()
        );
        assert!(
            refused > 0,
            "no golden was refused; the proof-mode half of this table would then be \
             asserting nothing"
        );
    }

    /// An import-bearing golden is conformant, and the decode tier still refuses
    /// it.
    ///
    /// This is the one place the two tiers answer differently, and it is a
    /// difference in the question rather than a disagreement: conformance is a
    /// property of the bytes, instantiation needs a host module the sibling tier
    /// deliberately registers none of. Stating it here means a reader comparing
    /// the two exclusion lists finds the answer instead of assuming one of them
    /// is wrong — and that a future change making `check` refuse imports
    /// outright fails a test that says why it must not.
    ///
    /// Fails if `check` starts refusing a module for declaring an import whose
    /// names and arity an embedder could satisfy.
    #[test]
    fn an_import_bearing_golden_is_conformant_even_though_it_cannot_instantiate_alone() {
        let with_imports: Vec<_> = golden_wasm_artifacts()
            .into_iter()
            .filter(|path| has_import_section(&read(path)))
            .collect();
        assert_eq!(
            with_imports.len(),
            IMPORT_BEARING_COUNT,
            "the import-bearing class has changed size; confirm the new membership is \
             intended and move the count, and the sibling tier's committed list with it"
        );
        for path in &with_imports {
            check(&read(path)).unwrap_or_else(|violations| {
                panic!(
                    "{} declares an import and is otherwise ordinary WebAssembly 1.0, so it \
                     must be conformant:\n{violations}",
                    path.display()
                )
            });
        }
        println!(
            "spacewasm conformance: {} import-bearing goldens are conformant",
            with_imports.len()
        );
    }

    /// Every single-file fixture code generation accepts for this target lands
    /// on the same side of `check` as its committed golden would.
    ///
    /// The goldens above are bytes someone regenerated; this is the live
    /// pipeline, so a lowering that starts emitting something outside the
    /// envelope is caught before any golden is refreshed for it.
    ///
    /// Compiled through the no-analysis entry point, which is what the corpus
    /// sweeps use: a good number of these fixtures exercise a construct an
    /// analysis rule legitimately rejects, and they still have to reach code
    /// generation or the sweep silently stops covering the shapes it was written
    /// for. That makes the set a superset of what a real build produces, and
    /// nothing is skipped inside it, because a skip is where coverage goes to
    /// die.
    ///
    /// Every module the target accepts is held to two things, and the first is
    /// the stronger claim: it carries no verification operator at all. The
    /// target's own non-determinism gate is total over a body, so a module here
    /// carrying one would mean the gate has a hole and a `0xfc` instruction no
    /// WebAssembly decoder reads had been written for a runtime that is a
    /// WebAssembly decoder — which is a fault to fail on rather than a fixture
    /// to tolerate. It used to be tolerated, when the gate read only the
    /// statement kinds one walk enumerated and a `forall` in a loop body reached
    /// emission; the surplus that arm existed for is empty now.
    ///
    /// Fails if code generation starts emitting, for this target, a module its
    /// runtime could not load — whether because it carries an operator no
    /// standard defines or because it exceeds one of the interpreter's maxima.
    #[test]
    fn every_single_file_fixture_compiled_for_spacewasm_is_conformant() {
        let sources = single_file_corpus_sources();
        let mut conformant = 0;
        for (path, source) in &sources {
            let Ok(output) = codegen_for_target_no_analysis(source, Target::SpaceWasm) else {
                continue;
            };
            assert!(
                !carries_verification_operator(output.wasm()),
                "{path} reached emission for the spacewasm target carrying a verification \
                 operator; the target's non-determinism gate is total over a body, so this \
                 is a hole in it and not a fixture to tolerate"
            );
            check(output.wasm()).unwrap_or_else(|violations| {
                panic!("{path} compiles for spacewasm but is not conformant:\n{violations}")
            });
            conformant += 1;
        }
        println!(
            "spacewasm conformance: {conformant} of {} single-file fixtures compiled and \
             passed, none carrying a verification operator",
            sources.len()
        );
        assert!(
            conformant >= SINGLE_FILE_FLOOR,
            "expected at least {SINGLE_FILE_FLOOR} fixtures to compile for spacewasm and \
             pass, only {conformant} did; a gate that started refusing everything would \
             otherwise pass this vacuously"
        );
    }

    /// The sentence the target's non-determinism gate refuses with.
    ///
    /// Matched rather than "code generation returned an error", because every
    /// other refusal code generation can make would otherwise read as agreement
    /// with the rules below.
    const NON_DET_REFUSAL: &str = "does not support non-deterministic operations";

    /// Which of A006 and A042 analysis reports for `source`.
    ///
    /// Both are errors, so a source either fails analysis carrying one of them or
    /// does not report them at all.
    fn non_det_rules_reported(source: &str) -> Vec<&'static str> {
        let arena = crate::utils::build_ast(source.to_string());
        let typed_context = inference_type_checker::TypeCheckerBuilder::build_typed_context(arena)
            .expect("every corpus fixture type-checks")
            .typed_context();
        match inference_analysis::analyze(&typed_context) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .errors()
                .iter()
                .map(|diagnostic| diagnostic.rule_id())
                .filter(|id| matches!(*id, "A006" | "A042"))
                .collect(),
        }
    }

    /// Over the whole corpus, the target's non-determinism gate refuses exactly
    /// the fixtures analysis rejects with A006 or A042.
    ///
    /// The gate is a second *total* reading of a question `core/analysis` already
    /// answers, and two total readings of one question that are never compared
    /// drift apart. The direction that reaches a user is the gate being the
    /// stricter of the two: a real build runs analysis first, so a program the
    /// rules accept and the gate refuses fails at code generation with a message
    /// that names a function and carries no source location at all.
    ///
    /// An equivalence rather than an inclusion, because both directions are
    /// faults — a gate wider than the rules is the one above, and a gate narrower
    /// than them is a program reaching emission with an instruction the runtime
    /// cannot decode. The gate's verdict is read off its own sentence rather than
    /// off "code generation failed", so a fixture refused for some other reason
    /// cannot pass as agreement.
    ///
    /// Fails if either reading moves without the other.
    #[test]
    fn the_non_determinism_gate_refuses_exactly_what_a006_and_a042_reject() {
        let sources = single_file_corpus_sources();
        let mut refused = 0;
        for (path, source) in &sources {
            let gate_refused = match codegen_for_target_no_analysis(source, Target::SpaceWasm) {
                Ok(_) => false,
                Err(error) => error.to_string().contains(NON_DET_REFUSAL),
            };
            let reported = non_det_rules_reported(source);
            assert_eq!(
                gate_refused,
                !reported.is_empty(),
                "{path}: the spacewasm gate {} it while analysis reported {}",
                if gate_refused {
                    "refused"
                } else {
                    "did not refuse"
                },
                if reported.is_empty() {
                    String::from("neither A006 nor A042")
                } else {
                    reported.join(" and ")
                }
            );
            if gate_refused {
                refused += 1;
            }
        }
        println!(
            "spacewasm conformance: the gate and A006/A042 agree on all {} fixtures, {refused} \
             of them refused",
            sources.len()
        );
        assert!(
            refused > 0,
            "no fixture exercised the refusing side, so the equivalence held vacuously"
        );
    }

    // -----------------------------------------------------------------------
    // What a refusal says
    // -----------------------------------------------------------------------

    /// Compiles WAT, panicking with the text rather than with a parse error
    /// alone.
    fn wat(text: &str) -> Vec<u8> {
        wat::parse_str(text).unwrap_or_else(|e| panic!("the fixture is not valid WAT: {e}"))
    }

    /// A refusal carries the finding, both numbers, the authority and one thing
    /// to change.
    ///
    /// The four parts are the whole design of the message: a finding with no
    /// second number cannot be acted on without a hex editor, and one with no
    /// authority sends a reader to shorten a name to the wrong length. The
    /// caller's two sentences — what was checked and what happened to it — are
    /// asserted too, since nothing else in the library can supply them.
    ///
    /// Fails if a rendering refactor drops a part, which no assertion on the
    /// finding alone would notice.
    #[test]
    fn a_refusal_names_both_numbers_its_authority_and_a_remedy() {
        let over = wat(&format!("(module (func (param {})))", "i32 ".repeat(256)));
        let violations = check(&over).expect_err("256 parameter words are refused");
        let rendered = violations.render("out/main.wasm", "No file was written.");

        for fragment in [
            "out/main.wasm",
            "No file was written.",
            "declares 256 parameter words",
            "at most 255",
            "single byte",
            "collect them into a struct",
        ] {
            assert!(
                rendered.contains(fragment),
                "the refusal must carry `{fragment}`, got:\n{rendered}"
            );
        }
    }

    /// A module whose single function declares `groups` as run-length locals.
    ///
    /// Built with the encoder because the counts reach tens of thousands and a
    /// locals group is a run-length pair the encoder writes directly, while WAT
    /// would need one declaration per local.
    fn module_with_locals(groups: &[(u32, ValType)]) -> Vec<u8> {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut code = CodeSection::new();
        let mut function = Function::new(groups.iter().copied());
        function.instruction(&Instruction::End);
        code.function(&function);
        module.section(&code);
        module.finish()
    }

    /// A module declaring `types_count` `[] -> []` types and one function whose
    /// `call_indirect` names type `type_index`.
    ///
    /// A table is declared because `call_indirect` validates against one, and no
    /// element segment because nothing here runs the module — this asks what
    /// `check` says, and the sibling oracle in `tests/tests/spacewasm/` asks the
    /// real decoder about the same shape, where the segment's encoding does
    /// matter.
    fn module_with_call_indirect_against_type(types_count: u32, type_index: u32) -> Vec<u8> {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        for _ in 0..types_count {
            types.ty().function([], []);
        }
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: 1,
            maximum: Some(1),
            shared: false,
        });
        module.section(&tables);
        let mut code = CodeSection::new();
        let mut function = Function::new([]);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::CallIndirect {
            type_index,
            table_index: 0,
        });
        function.instruction(&Instruction::End);
        code.function(&function);
        module.section(&code);
        module.finish()
    }

    /// A module whose single function runs `body` inside a block, with the
    /// block's `end` and the function's appended.
    fn module_with_block_body(body: &[Instruction<'_>]) -> Vec<u8> {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([], []);
        module.section(&types);
        let mut functions = FunctionSection::new();
        functions.function(0);
        module.section(&functions);
        let mut code = CodeSection::new();
        let mut function = Function::new([]);
        function.instruction(&Instruction::Block(BlockType::Empty));
        for instruction in body {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        code.function(&function);
        module.section(&code);
        module.finish()
    }

    /// A module declaring `count` `i32` globals and exporting global `index`.
    ///
    /// No function and no body: the export descriptor is enough to reach the
    /// narrowing accessor, and a module carrying an instruction as well would
    /// not say which of the two earned the refusal.
    fn module_exporting_global(count: u32, index: u32) -> Vec<u8> {
        let mut module = Module::new();
        let mut globals = GlobalSection::new();
        for _ in 0..count {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: false,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
        }
        module.section(&globals);
        let mut exports = ExportSection::new();
        exports.export("g", ExportKind::Global, index);
        module.section(&exports);
        module.finish()
    }

    /// Every refusal carries a finding, an authority and a remedy of its own.
    ///
    /// The two tests above prove the three-part shape on two variants. This
    /// proves it on all of them, which matters because the authority and the
    /// remedy are per-variant prose: a reader told the wrong authority shortens
    /// a name to the decoder's 32 and meets the registration wall at 31, and a
    /// reader given a source-level remedy for a shape no source can express
    /// goes looking for a declaration that is not there. Neither mistake is
    /// visible in a test that matches on the variant.
    ///
    /// The rows assert *presence*, not exclusivity: several of these shapes are
    /// also outside WebAssembly 1.0, and the scan is deliberately not
    /// short-circuited, so a module may carry more findings than the one it was
    /// built for.
    ///
    /// Fails if any arm's authority or remedy is emptied, swapped with another
    /// arm's, or folded into a shared sentence.
    #[test]
    fn every_refusal_carries_its_own_authority_and_remedy() {
        let long_name = "n".repeat(32);
        let over_long_module_name = wat(&format!("(module (import \"{long_name}\" \"f\" (func)))"));
        let over_long_field_name = wat(&format!("(module (import \"h\" \"{long_name}\" (func)))"));
        let mut over_deep_branch = vec![Instruction::I32Const(1); 256];
        over_deep_branch.push(Instruction::Br(0));
        let rows: [(&str, Vec<u8>, &str, &str, &str); 16] = [
            (
                "a post-1.0 instruction",
                wat("(module (func i32.const 0 i32.extend8_s drop))"),
                "not WebAssembly 1.0",
                "decodes WebAssembly 1.0 plus mutable globals",
                "Rebuild the linked module against the WebAssembly 1.0 baseline",
            ),
            (
                "256 parameter words",
                wat(&format!("(module (func (param {})))", "i32 ".repeat(256))),
                "parameter words exceeded",
                "parameter size in a single byte",
                "collect them into a struct",
            ),
            (
                "65536 local words",
                module_with_locals(&[(32_768, ValType::I64)]),
                "local words exceeded",
                "local size in a 16-bit field",
                "Split the function into smaller functions",
            ),
            (
                "65534 local words under the frame bound",
                module_with_locals(&[(32_767, ValType::I64)]),
                "call frame too wide",
                "bounds the whole call frame",
                "Split the function into smaller functions",
            ),
            (
                "a 65536-local group",
                module_with_locals(&[(65_536, ValType::I32)]),
                "locals group too large",
                "run-length groups",
                "not producible by this compiler",
            ),
            (
                "a call_indirect against type 65536",
                module_with_call_indirect_against_type(65_537, 65_536),
                "the type index of a `call_indirect` is 65536",
                "one 16-bit immediate",
                "fewer function types",
            ),
            (
                "a br_table of 65536 targets",
                module_with_block_body(&[
                    Instruction::I32Const(0),
                    Instruction::BrTable(vec![0u32; 65_536].as_slice().into(), 0),
                ]),
                "the target count of a `br_table` is 65536",
                "one 16-bit immediate",
                "narrower jump table",
            ),
            // The one refusal this crate makes that the decoder does not, so
            // its authority sentence has to say so where the other fifteen say
            // what upstream refuses. A reader told only "SpaceWasm rejects it"
            // would go looking for a decode failure that never happens.
            (
                "an export of global 65536",
                module_exporting_global(65_537, 65_536),
                "defined index truncated: the export `g` names global 65536",
                "one refusal this crate makes that the decoder does not",
                "split that module",
            ),
            (
                "a branch unwinding 256 words",
                module_with_block_body(&over_deep_branch),
                "discards 256 operand words",
                "single byte of its jump target word",
                "bind them to locals",
            ),
            (
                "a 32-byte import module name",
                over_long_module_name.clone(),
                "import module name",
                "registration limit, not a decode limit",
                "Shorten the module name",
            ),
            (
                "a 32-byte import field name",
                over_long_field_name.clone(),
                "import field name",
                "registration limit, not a decode limit",
                "Rename the `external fn`",
            ),
            (
                "a 10-parameter import",
                wat(&format!(
                    "(module (import \"h\" \"f\" (func (param {}))))",
                    "i32 ".repeat(10)
                )),
                "host function takes too many parameters",
                "fixed-size list of 9",
                "Declare fewer parameters on the `external fn`",
            ),
            (
                "a two-result import",
                wat("(module (import \"h\" \"f\" (func (result i32 i32))))"),
                "host function returns more than one value",
                "returns at most one value",
                "not producible by this compiler",
            ),
            (
                "a 33-byte custom section name",
                wat(&format!("(module (@custom \"{}\" \"x\"))", "c".repeat(33))),
                "custom section name too long",
                "fixed 32-byte buffer",
                "not producible by this compiler",
            ),
            (
                "65537 memory pages",
                wat("(module (memory 65537))"),
                "linear memory too large",
                "whole 32-bit address space",
                "not producible by this compiler",
            ),
            // A memory, table or global import carries no signature to hold to
            // the host caps, and the scan records it anyway so its names still
            // meet the registration cap. Nothing but a non-function import
            // proves that: every other import row above is a function, and a
            // scan that dropped the others entirely would leave them all green.
            (
                "a 32-byte name on a memory import",
                wat(&format!("(module (import \"{long_name}\" \"mem\" (memory 1)))")),
                "import module name",
                "registration limit, not a decode limit",
                "Shorten the module name",
            ),
        ];

        for (label, module, finding, authority, remedy) in &rows {
            let rendered = check(module)
                .err()
                .unwrap_or_else(|| panic!("{label}: the checker accepted a refused shape"))
                .to_string();
            for fragment in [finding, authority, remedy] {
                assert!(
                    rendered.contains(*fragment),
                    "{label}: the refusal must carry `{fragment}`, got:\n{rendered}"
                );
            }
        }

        // The two halves of an import's name meet the same cap for the same
        // reason and are shortened by different edits, so their remedies are
        // the pair most likely to be collapsed into one sentence.
        let module_name = check(&over_long_module_name)
            .expect_err("a 32-byte module name is refused")
            .to_string();
        let field_name = check(&over_long_field_name)
            .expect_err("a 32-byte field name is refused")
            .to_string();
        assert!(
            !module_name.contains("Rename the `external fn`"),
            "the module half must not advise renaming the extern: {module_name}"
        );
        assert!(
            !field_name.contains("Shorten the module name"),
            "the field half must not advise shortening the module name: {field_name}"
        );
        for rendered in [&module_name, &field_name] {
            assert!(
                rendered.contains("is 32 bytes") && rendered.contains("at most 31"),
                "the block must carry both caps, or a reader shortens to the decoder's 32 \
                 and meets the registration wall at 31: {rendered}"
            );
        }
    }

    /// The findings arrive in the order the documentation promises.
    ///
    /// A refusal with several findings is read top to bottom, and the order is
    /// the only thing that makes it readable: the WebAssembly 1.0 verdict first
    /// because it can explain the rest, then the functions in index order, then
    /// the imports, then what the sections outside a body name — the exports,
    /// the `start` section and the element segments — then the module's own
    /// shape. Nothing else in this tier looks at the order — every other
    /// assertion searches the slice — so a reordering of the scan would
    /// otherwise be invisible.
    ///
    /// Fails if a category moves, or if the order becomes the order the scan
    /// happens to discover findings in.
    #[test]
    fn the_findings_arrive_in_the_documented_order() {
        let module = module_with_one_finding_per_group();
        let violations = check(&module).expect_err("five findings are five refusals");
        let order: Vec<&str> = violations
            .as_slice()
            .iter()
            .map(|violation| match violation {
                Violation::OutsideWasm1 { .. } => "wasm1",
                Violation::ParamWordsExceeded { .. } => "function",
                Violation::ImportArityExceeded { .. } => "import",
                Violation::IndexTruncated { .. } => "reference",
                Violation::CustomSectionNameTooLong { .. } => "module",
                other => panic!("this module earns no other finding, got {other:?}"),
            })
            .collect();
        assert_eq!(
            order,
            ["wasm1", "function", "import", "reference", "module"],
            "the rendered refusal reads top to bottom, and the documented order is what \
             makes it read"
        );
    }

    /// A module earning exactly one finding from each of the five groups
    /// `check` documents an order for.
    ///
    /// Built with the encoder rather than from WAT because one of the five
    /// needs 65,537 globals, which WAT would spell one declaration at a time.
    /// The five faults are deliberately unrelated to each other: a
    /// sign-extension instruction, a 256-word parameter list, a ten-parameter
    /// import, an export naming a global past the interpreter's 16-bit word,
    /// and a 33-byte custom section name.
    fn module_with_one_finding_per_group() -> Vec<u8> {
        let mut module = Module::new();
        module.section(&CustomSection {
            name: "c".repeat(33).into(),
            data: b"x"[..].into(),
        });
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32; 10], []);
        types.ty().function([ValType::I32; 256], []);
        module.section(&types);
        let mut imports = ImportSection::new();
        imports.import("h", "f", EntityType::Function(0));
        module.section(&imports);
        let mut functions = FunctionSection::new();
        functions.function(1);
        module.section(&functions);
        let mut globals = GlobalSection::new();
        for _ in 0..65_537 {
            globals.global(
                GlobalType {
                    val_type: ValType::I32,
                    mutable: false,
                    shared: false,
                },
                &ConstExpr::i32_const(0),
            );
        }
        module.section(&globals);
        let mut exports = ExportSection::new();
        exports.export("g", ExportKind::Global, 65_536);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut function = Function::new([]);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Extend8S);
        function.instruction(&Instruction::Drop);
        function.instruction(&Instruction::End);
        code.function(&function);
        module.section(&code);
        module.finish()
    }

    /// A shape this compiler cannot emit is blamed on provenance, not on the
    /// source.
    ///
    /// "Shorten the custom section name" is advice about a file the user did not
    /// write: nothing in the language names a custom section. The remedy for the
    /// six such shapes therefore splits — report a compiler bug, or rebuild the
    /// external — because only the person holding the build knows which half
    /// applies.
    ///
    /// Both halves of the split are asserted, and on every shape that spells
    /// them itself: the four that share one sentence are represented by the
    /// custom-section name, `IndexTooLarge`'s two kinds each say the same thing
    /// in their own words while still naming the edit, and `IndexTruncated`
    /// names a third edit again — split the module that carries the definitions.
    /// A row for each, because the sentences are written out separately and a
    /// single row would let the others lose the provenance half unnoticed.
    ///
    /// Fails if one of those arms is given a source-level remedy that does not
    /// exist, or drops the provenance question and names only the edit.
    #[test]
    fn a_shape_this_compiler_cannot_produce_splits_its_remedy_on_provenance() {
        let named = wat(&format!("(module (@custom \"{}\" \"x\"))", "c".repeat(33)));
        let indexed = module_with_call_indirect_against_type(65_537, 65_536);
        let tabled = module_with_block_body(&[
            Instruction::I32Const(0),
            Instruction::BrTable(vec![0u32; 65_536].as_slice().into(), 0),
        ]);
        let truncated = module_exporting_global(65_537, 65_536);
        let rows: [(&str, Vec<u8>, Vec<&str>); 4] = [
            (
                "a 33-byte custom section name",
                named,
                vec![
                    "is 33 bytes",
                    "at most 32",
                    "not producible by this compiler",
                    "report a compiler bug",
                    "rebuild the external",
                ],
            ),
            (
                "a call_indirect against type 65536",
                indexed,
                vec![
                    "emits no `call_indirect`",
                    "came from a linked module or a post-build step",
                    "fewer function types",
                    "report a compiler bug",
                ],
            ),
            (
                "a br_table of 65536 targets",
                tabled,
                vec![
                    "emits no `br_table`",
                    "came from a linked module or a post-build step",
                    "narrower jump table",
                    "report a compiler bug",
                ],
            ),
            (
                "an export of global 65536",
                truncated,
                vec![
                    "emits one global and nowhere near 65,536 functions",
                    "came from a linked module or a post-build step",
                    "split that module",
                    "report a compiler bug",
                ],
            ),
        ];
        for (label, wasm, fragments) in rows {
            let rendered = check(&wasm)
                .err()
                .unwrap_or_else(|| panic!("{label} must be refused"))
                .to_string();
            for fragment in fragments {
                assert!(
                    rendered.contains(fragment),
                    "{label}: the refusal must carry `{fragment}`, got:\n{rendered}"
                );
            }
        }
    }

    /// An operand the validator stopped tracking ends the word sum.
    ///
    /// After `unreachable` the stack is polymorphic and a slot's type is
    /// genuinely unknown. The interpreter's verifier stops accumulating there
    /// and counts nothing above it, and this crate has to agree with it rather
    /// than round the unknown up: the figure feeds the frame bound, so a
    /// checker that counted an untracked slot at two words would refuse
    /// functions the decoder loads. The value count is unaffected — an unknown
    /// operand is still an operand — so the two units part company here, which
    /// is the clearest statement that they are different quantities.
    ///
    /// The decoder witnesses this rule directly in the oracle tier, where a
    /// function whose locals sit on the frame boundary is accepted by both
    /// sides only because the untracked operand above them costs nothing.
    ///
    /// Fails if the unknown slot is given any width, or if the walk stops
    /// counting the known operands below it.
    #[test]
    fn an_operand_the_validator_stopped_tracking_ends_the_word_sum() {
        // The function returns nothing so that the sample taken after `end`,
        // where the results are back on the stack as known types, cannot be the
        // one the maxima come from.
        let module = wat("(module (func unreachable select drop))");
        let report = check(&module).expect("the module is conformant");
        assert_eq!(
            (report.tallest().1, report.widest().1),
            (1, 0),
            "one live operand of unknown type is one value and no words"
        );

        // The `block` is what keeps the `i64` alive: `unreachable` truncates
        // the stack to the enclosing frame's height, so an untracked slot can
        // only sit above a known one inside a nested block.
        let below = wat("(module (func i64.const 1 block unreachable select drop end drop))");
        let report = check(&below).expect("the module is conformant");
        assert_eq!(
            (report.tallest().1, report.widest().1),
            (2, 2),
            "the i64 below the untracked slot is still counted at its own width"
        );
    }

    /// A function the `name` section does not name is named by its index.
    ///
    /// Every module this compiler writes carries a name section, but a linked
    /// external need not, and a refusal that named the offending function `` is
    /// no refusal at all.
    ///
    /// Fails if the fallback is dropped or the index is taken from the wrong
    /// index space.
    #[test]
    fn a_function_with_no_name_is_named_by_its_index() {
        let module = wat(&format!(
            "(module (import \"h\" \"f\" (func)) (func (param {})))",
            "i32 ".repeat(256)
        ));
        let report = check(&module);
        let rendered = report.expect_err("256 parameter words are refused").to_string();
        assert!(
            rendered.contains("`func[1]`"),
            "the unnamed function must be named by its position in the function index space, \
             which starts after the imports, got:\n{rendered}"
        );
    }

    /// A module defining no function has no budget to report.
    ///
    /// Reachable from a source that declares only types, and the arm exists so
    /// the summary line does not offer an embedder a maximum attained by a
    /// function that is not there.
    ///
    /// Fails if the empty case starts reporting a function or panicking, or if
    /// the reported module size stops being the size of the bytes checked.
    #[test]
    fn a_module_with_no_function_reports_no_budget() {
        let module = wat("(module)");
        let report = check(&module).expect("an empty module is conformant");
        assert!(report.functions.is_empty());
        assert_eq!(report.deepest(), (String::new(), 0));
        assert_eq!(report.tallest(), (String::new(), 0));
        assert_eq!(report.widest(), (String::new(), 0));
        assert_eq!(report.wasm_size, module.len());
        assert_eq!(report.limits_from, LIMITS_FROM);
    }
}
