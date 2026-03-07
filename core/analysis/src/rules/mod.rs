pub mod break_inside_nondet_block;
pub mod break_outside_loop;
pub mod infinite_loop_without_break;
pub mod return_inside_loop;
pub mod return_inside_nondet_block;

use break_inside_nondet_block::BreakInsideNonDetBlock;
use break_outside_loop::BreakOutsideLoop;
use infinite_loop_without_break::InfiniteLoopWithoutBreak;
use return_inside_loop::ReturnInsideLoop;
use return_inside_nondet_block::ReturnInsideNonDetBlock;

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
    ]
}
