pub mod array_index_64bit;
pub mod array_uzumaki_as_argument;
pub mod compound_literal_as_argument;
pub mod break_inside_nondet_block;
pub mod break_outside_loop;
pub mod compound_literal_position;
pub mod compound_return_call_assignment;
pub mod compound_return_call_position;
pub mod dead_code;
pub mod empty_enum_definition;
pub mod empty_struct_definition;
pub mod extern_function_call;
pub mod infinite_loop_without_break;
pub mod literal_out_of_range;
pub mod method_call_chain_compound;
pub mod method_never_accesses_self;
pub mod missing_return;
pub mod return_inside_loop;
pub mod return_inside_nondet_block;
pub mod standalone_uzumaki;
pub mod uninitialized_variable;
pub mod uzumaki_in_reassignment;
pub mod uzumaki_outside_nondet_block;

use array_index_64bit::ArrayIndex64Bit;
use array_uzumaki_as_argument::ArrayUzumakiAsArgument;
use compound_literal_as_argument::CompoundLiteralAsArgument;
use break_inside_nondet_block::BreakInsideNonDetBlock;
use break_outside_loop::BreakOutsideLoop;
use compound_literal_position::CompoundLiteralPosition;
use compound_return_call_assignment::CompoundReturnCallAssignment;
use compound_return_call_position::CompoundReturnCallPosition;
use dead_code::DeadCode;
use empty_enum_definition::EmptyEnumDefinition;
use empty_struct_definition::EmptyStructDefinition;
use extern_function_call::ExternFunctionCall;
use infinite_loop_without_break::InfiniteLoopWithoutBreak;
use literal_out_of_range::LiteralOutOfRange;
use method_call_chain_compound::MethodCallChainCompound;
use method_never_accesses_self::MethodNeverAccessesSelf;
use missing_return::MissingReturn;
use return_inside_loop::ReturnInsideLoop;
use return_inside_nondet_block::ReturnInsideNonDetBlock;
use standalone_uzumaki::StandaloneUzumaki;
use uninitialized_variable::UninitializedVariable;
use uzumaki_in_reassignment::UzumakiInReassignment;
use uzumaki_outside_nondet_block::UzumakiOutsideNonDetBlock;

/// Returns all registered analysis rules.
///
/// Adding a new rule:
/// 1. Create `rules/new_rule.rs` using the `rule!` macro
/// 2. Add `pub mod new_rule;` above
/// 3. Add `&NewRule` to the slice below
#[must_use = "returns all registered analysis rules"]
pub fn all_rules() -> &'static [&'static dyn crate::rule::Rule] {
    &[
        &BreakOutsideLoop,
        &BreakInsideNonDetBlock,
        &ReturnInsideLoop,
        &InfiniteLoopWithoutBreak,
        &ReturnInsideNonDetBlock,
        &UzumakiOutsideNonDetBlock,
        &MissingReturn,
        &StandaloneUzumaki,
        &EmptyEnumDefinition,
        &MethodNeverAccessesSelf,
        &EmptyStructDefinition,
        &CompoundLiteralAsArgument,
        &ArrayUzumakiAsArgument,
        &CompoundLiteralPosition,
        &CompoundReturnCallPosition,
        &CompoundReturnCallAssignment,
        &MethodCallChainCompound,
        &ArrayIndex64Bit,
        &DeadCode,
        &LiteralOutOfRange,
        &UzumakiInReassignment,
        &ExternFunctionCall,
        &UninitializedVariable,
    ]
}
