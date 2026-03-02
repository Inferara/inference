pub mod break_inside_nondet_block;
pub mod break_outside_loop;
pub mod infinite_loop_without_break;
pub mod return_inside_loop;

use break_inside_nondet_block::BreakInsideNondetBlock;
use break_outside_loop::BreakOutsideLoop;
use infinite_loop_without_break::InfiniteLoopWithoutBreak;
use return_inside_loop::ReturnInsideLoop;

/// Returns all registered analysis rules.
///
/// Adding a new rule:
/// 1. Create `rules/new_rule.rs` using the `rule!` macro
/// 2. Add `pub mod new_rule;` above
/// 3. Add `Box::new(NewRule)` to the vec below
#[must_use]
pub fn all_rules() -> Vec<Box<dyn crate::rule::Rule>> {
    vec![
        Box::new(BreakOutsideLoop),
        Box::new(BreakInsideNondetBlock),
        Box::new(ReturnInsideLoop),
        Box::new(InfiniteLoopWithoutBreak),
    ]
}
