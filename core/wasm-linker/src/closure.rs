//! Transitive closure of a satisfied import.
//!
//! Given the function an external module exports to satisfy a main-module
//! import, this computes the set of *everything that function transitively
//! depends on* inside its own module: the functions it calls (directly or
//! indirectly), and — recorded for tier classification — whether any body in
//! the closure touches memory, globals, tables, or data/element segments.
//!
//! Only locally-defined functions enter the closure. If a closure body calls
//! one of the external module's *own* imports, that is surfaced so the linker
//! can reject it: a static merge cannot satisfy a transitive host import.

use std::collections::{BTreeSet, VecDeque};

use inf_wasmparser::{BinaryReader, FunctionBody, Operator};

use crate::parse::ParsedModule;
use crate::safety::{check_operator, opens_control_frame, MAX_CONTROL_DEPTH};
use crate::LinkError;

/// What a closure's bodies touch, used by tier classification.
#[derive(Debug, Default, Clone)]
pub(crate) struct ClosureEffects {
    /// Any body reads or writes linear memory (load/store/copy/fill/size/grow).
    pub uses_memory: bool,
    /// Any body grows linear memory (`memory.grow`). Tracked separately so the
    /// merge can reconcile growth against the reconciled output memory maximum.
    pub uses_memory_grow: bool,
    /// Any body reads or writes a global.
    pub uses_globals: bool,
    /// Any body refers to a data segment (`memory.init` / `data.drop`).
    pub uses_data_segments: bool,
    /// Any body performs an indirect call or otherwise touches the table /
    /// element space (`call_indirect`, `table.*`, `ref.func`, `elem.drop`).
    pub uses_tables: bool,
}

/// The result of closing over an exported function.
#[derive(Debug, Clone)]
pub(crate) struct Closure {
    /// Local function indices to copy, in ascending order (deterministic).
    pub local_func_indices: Vec<u32>,
    pub effects: ClosureEffects,
}

/// Computes the transitive closure of the function at `root_func_idx` inside
/// `module`.
///
/// # Errors
///
/// Returns [`LinkError::TransitiveHostImport`] if the closure reaches one of
/// the module's own imported functions — a static merge has no body to copy
/// for it.
pub(crate) fn compute(
    module: &ParsedModule,
    root_func_idx: u32,
) -> Result<Closure, LinkError> {
    let import_count = module.local_func_base();
    let mut visited: BTreeSet<u32> = BTreeSet::new();
    let mut queue: VecDeque<u32> = VecDeque::new();
    let mut effects = ClosureEffects::default();

    queue.push_back(root_func_idx);

    while let Some(func_idx) = queue.pop_front() {
        if func_idx < import_count {
            // The root export is guaranteed local by the caller; reaching an
            // import here means a body inside the closure called one.
            let import = &module.imported_funcs[func_idx as usize];
            return Err(LinkError::TransitiveHostImport {
                module: import.module.clone(),
                field: import.field.clone(),
            });
        }
        if !visited.insert(func_idx) {
            continue;
        }

        let local = module
            .local_funcs
            .get((func_idx - import_count) as usize)
            .ok_or_else(|| {
                LinkError::Parse(format!(
                    "function body references function index {func_idx}, which is out of range"
                ))
            })?;
        scan_body(&local.body, &mut effects, |callee| {
            queue.push_back(callee);
        })?;
    }

    Ok(Closure {
        local_func_indices: visited.into_iter().collect(),
        effects,
    })
}

/// Walks a function body's operators, recording effects and reporting every
/// directly-called function index through `on_call`.
///
/// Every operator is gated through the fail-closed allow-list
/// ([`check_operator`]): an operator the static merge does not model — an
/// atomic, a SIMD op, an exception-handling instruction, a typed reference, a
/// multi-memory access — is rejected here, before its closure is committed,
/// rather than copied verbatim into a structurally-invalid output.
fn scan_body(
    body: &[u8],
    effects: &mut ClosureEffects,
    mut on_call: impl FnMut(u32),
) -> Result<(), LinkError> {
    let reader = BinaryReader::new(body, 0);
    let func_body = FunctionBody::new(reader);
    let ops = func_body
        .get_operators_reader()
        .map_err(|e| LinkError::Parse(e.to_string()))?;

    let mut control_depth: usize = 0;
    for op in ops {
        let op = op.map_err(|e| LinkError::Parse(e.to_string()))?;

        // Bound structured-control-flow nesting so the downstream wasm-to-v
        // translator (which recurses one frame per level) cannot be driven to
        // stack exhaustion by an adversarially deep external body. An `End`
        // closes the innermost frame; a `block`/`loop`/`if`/non-det op opens a
        // new one. This scan gates external bodies; the main module's body is
        // bounded by the matching cap in `crate::rewrite::reencode_body`, so an
        // over-nested body is kept out of the merged module whatever its origin.
        if opens_control_frame(&op) {
            control_depth += 1;
            // Reject at `>=` so a body nested exactly `MAX_CONTROL_DEPTH` deep is
            // rejected by *both* this scan and the wasm-to-v translator, which
            // itself rejects at `depth >= 256`. With a strict `>` the two caps
            // disagreed: a body at exactly the cap would link here but then abort
            // the `-v` translator that admits only `depth < 256`.
            if control_depth >= MAX_CONTROL_DEPTH {
                return Err(LinkError::UnsupportedConstruct(format!(
                    "external function body nests structured control flow at least {MAX_CONTROL_DEPTH} levels deep"
                )));
            }
        } else if matches!(op, Operator::End) {
            control_depth = control_depth.saturating_sub(1);
        }

        let effect = check_operator(&op)?;
        effects.uses_memory |= effect.uses_memory;
        effects.uses_memory_grow |= effect.uses_memory_grow;
        effects.uses_globals |= effect.uses_globals;
        effects.uses_data_segments |= effect.uses_data_segments;
        effects.uses_tables |= effect.uses_tables;

        // Calls drag their target into the closure. `ref.func` also references a
        // function (and marks table use, surfaced by `check_operator`).
        match op {
            Operator::Call { function_index }
            | Operator::ReturnCall { function_index }
            | Operator::RefFunc { function_index } => {
                on_call(function_index);
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! Unit tests for closure effect scanning over hand-built modules.
    //!
    //! Effects from table/element operators and `ref.func` mark a closure as
    //! touching the table space (Tier C). The `link` API rejects such modules at
    //! tier classification, so these scan-level effects are asserted directly by
    //! computing the closure of a module that uses them.

    use super::*;
    use crate::parse::ParsedModule;

    fn parse(wat: &str) -> ParsedModule {
        let bytes = wat::parse_str(wat).expect("valid WAT");
        ParsedModule::parse(&bytes).expect("parse")
    }

    #[test]
    fn ref_func_marks_table_use_and_enqueues_target() {
        // `root` takes a reference to internal `target` via `ref.func`. The scan
        // must mark table use *and* drag `target` into the closure.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                ref.func 1
                drop)
              (func (;1;) (type 0))
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let cl = compute(&module, root).expect("closure computes");
        assert!(cl.effects.uses_tables, "ref.func must mark table use");
        assert_eq!(
            cl.local_func_indices,
            vec![0, 1],
            "ref.func target must be pulled into the closure"
        );
    }

    #[test]
    fn call_indirect_marks_table_use() {
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (table (;0;) 1 funcref)
              (func (;0;) (type 0)
                i32.const 0
                call_indirect (type 0))
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let cl = compute(&module, root).expect("closure computes");
        assert!(cl.effects.uses_tables, "call_indirect must mark table use");
    }

    #[test]
    fn table_size_marks_table_use() {
        let module = parse(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (table (;0;) 1 funcref)
              (func (;0;) (type 0) (result i32)
                table.size 0)
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let cl = compute(&module, root).expect("closure computes");
        assert!(cl.effects.uses_tables, "table.size must mark table use");
    }

    #[test]
    fn global_access_marks_global_use() {
        let module = parse(
            r#"
            (module
              (type (;0;) (func (result i32)))
              (global (;0;) i32 (i32.const 3))
              (func (;0;) (type 0) (result i32)
                global.get 0)
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let cl = compute(&module, root).expect("closure computes");
        assert!(cl.effects.uses_globals, "global.get must mark global use");
    }

    #[test]
    fn out_of_range_call_index_is_a_clean_error() {
        // A body that calls a function index past the module's function count
        // must yield a `LinkError::Parse`, never index `local_funcs` out of
        // bounds and panic. `wat` assembles a numeric `call N` without resolving
        // it, so the out-of-range index reaches the closure walk.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                call 99)
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let err = compute(&module, root).expect_err("out-of-range call must error");
        assert!(
            matches!(err, LinkError::Parse(_)),
            "expected Parse, got {err:?}"
        );
    }

    /// Builds a single-function module whose body nests `depth` empty `block`s,
    /// exported as `root`, for the depth-cap boundary tests.
    fn module_nested(depth: usize) -> ParsedModule {
        let mut body = String::new();
        for _ in 0..depth {
            body.push_str("block ");
        }
        for _ in 0..depth {
            body.push_str("end ");
        }
        parse(&format!(
            r#"(module (func (;0;) (export "root") {body}))"#
        ))
    }

    #[test]
    fn nesting_exactly_at_the_cap_is_the_first_rejected_depth() {
        // D1: the closure scan rejects at `control_depth >= MAX_CONTROL_DEPTH`, so
        // a body nested exactly `MAX_CONTROL_DEPTH` deep is rejected — matching the
        // wasm-to-v translator, which itself rejects at `depth >= 256`. One level
        // shallower must still merge, so the cap is exact, not off-by-one.
        let at_cap = module_nested(MAX_CONTROL_DEPTH);
        let root = at_cap.exported_func_index("root").unwrap();
        let err = compute(&at_cap, root)
            .expect_err("a body nested exactly at the cap must be rejected");
        assert!(
            matches!(&err, LinkError::UnsupportedConstruct(msg) if msg.contains("nests structured control flow")),
            "expected an UnsupportedConstruct naming the nesting limit, got {err:?}"
        );

        let below_cap = module_nested(MAX_CONTROL_DEPTH - 1);
        let root = below_cap.exported_func_index("root").unwrap();
        assert!(
            compute(&below_cap, root).is_ok(),
            "a body nested one level below the cap must still merge"
        );
    }

    #[test]
    fn shared_callee_is_visited_once() {
        // `root` calls `shared` twice; the re-visit guard (`visited.insert`) must
        // keep the closure to two distinct functions, not loop or duplicate.
        let module = parse(
            r#"
            (module
              (type (;0;) (func))
              (func (;0;) (type 0)
                call 1
                call 1)
              (func (;1;) (type 0))
              (export "root" (func 0)))
            "#,
        );
        let root = module.exported_func_index("root").unwrap();
        let cl = compute(&module, root).expect("closure computes");
        assert_eq!(
            cl.local_func_indices,
            vec![0, 1],
            "a doubly-called callee must appear exactly once"
        );
    }
}
