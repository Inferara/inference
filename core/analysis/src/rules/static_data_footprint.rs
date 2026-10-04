//! A058: The program's static footprint must fit the memory it declares.
//!
//! An Inference program allocates nothing at run time. Its linear memory holds
//! exactly two things, both sized before it runs: the shadow stack at
//! `[0, stack-size)`, and directly above it the static data region, the bytes of
//! every array and struct module constant a function reads. Both have to fit the
//! `pages` the module declares — the memory it is guaranteed at instantiation,
//! when the data segment is written, before any growth.
//!
//! The type checker lays the region out (`StaticData`) and the build's layout
//! places it (`MemoryLayout::with_static_data`): a stack the build did not size
//! gives up the part of its default 64 KiB the data needs, so this rule fires
//! only when that is not enough — data that leaves no room for a stack at all,
//! or a `stack-size` the build asked for that, with the data beside it, needs
//! more than the pages hold. A036 then measures the deepest call chain against
//! the stack that is left. The two rules together prove the program can never
//! need more memory than it declares: every chain fits the stack, and the stack
//! and the data fit the pages (Power of 10, Rule 3).
//!
//! The finding is anchored at the first constant that does not fit, and names
//! the largest constants, the bytes the footprint is over by, and each fix that
//! holds the program: more pages, or — when the build asked for a stack larger
//! than its deepest call chain needs — a stack small enough to leave the data
//! its room.

use inference_type_checker::module_consts::StaticDataEntry;

use crate::errors::{AnalysisDiagnostic, ConstantData, ConstantDataItem, LabeledDiagnostic};
use crate::rules::stack_depth::deepest_chain;

crate::rule! {
    /// The shadow stack and the constant data above it must fit the memory's
    /// pages.
    #[id = "A058"]
    #[name = "Static data exceeds memory"]
    #[severity = error]
    pub struct StaticDataExceedsMemory;
    fn check(ctx: &TypedContext, options: AnalysisOptions) -> Vec<LabeledDiagnostic> {
        let static_data = ctx.static_data();
        if static_data.is_empty() {
            return Vec::new();
        }
        let Err(refusal) = options.layout.with_static_data(static_data.size()) else {
            return Vec::new();
        };
        // The first constant that ends past what the memory leaves above the
        // stack is the one the region no longer holds; the region is never
        // empty here, so there is always a last constant to fall back on.
        let room = refusal
            .memory_bytes()
            .saturating_sub(u64::from(refusal.stack_bytes));
        let entries = static_data.entries();
        let Some(anchor) = entries
            .iter()
            .find(|entry| entry.offset + entry.size > room)
            .or_else(|| entries.last())
        else {
            return Vec::new();
        };
        let mut items: Vec<ConstantDataItem> = entries
            .iter()
            .map(|entry| ConstantDataItem {
                name: qualified_name(entry),
                bytes: entry.size,
            })
            .collect();
        items.sort_by_key(|item| std::cmp::Reverse(item.bytes));
        let chain_bytes = deepest_chain(ctx).map_or(0, |chain| chain.total());
        vec![LabeledDiagnostic::new(
            anchor.module_path.clone(),
            AnalysisDiagnostic::StaticDataExceedsMemory {
                data: ConstantData {
                    items,
                    total: static_data.size(),
                },
                refusal,
                chain_bytes,
                layout: options.layout,
                location: anchor.location,
            },
        )]
    }
}

/// The constant as a reader in the entry file names it: bare for one of its own,
/// path-qualified for one in another file.
fn qualified_name(entry: &StaticDataEntry) -> String {
    if entry.module_path.is_empty() {
        entry.name.clone()
    } else {
        format!("{}::{}", entry.module_path.join("::"), entry.name)
    }
}
