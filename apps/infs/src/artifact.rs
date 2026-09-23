//! One pass over a built artifact's bytes, answering the questions a post-build
//! step has to ask before it acts on the file.
//!
//! A finished `.wasm` is all `infs` has to go on. Whether a verification-only
//! opcode leaked into an ordinary function, whether the module carries bulk
//! memory, whether it records which of its functions trap on arithmetic
//! overflow, which functions it imports from its environment — none of that is
//! recorded anywhere but in the bytes, so each answer costs a parse. Asking them
//! together costs one, and it keeps the answers consistent with one another: no
//! caller can pair a bulk-memory verdict taken from one traversal with a guard
//! record taken from another.
//!
//! The questions are about the artifact, not about any one thing done to it, so
//! the scan sits beside the commands rather than inside one. A command that
//! needs a different subset of the same facts reads them from the module that
//! owns the parse, instead of reaching into a sibling command for them or
//! opening a second pass of its own.

use std::path::Path;

use anyhow::Result;
use inf_wasmparser::{Operator, Parser, Payload, TypeRef};

/// The custom section recording which functions of a module trap on arithmetic
/// overflow.
///
/// A hand-synchronised copy of the name code generation emits and the linker
/// reads, kept here rather than pulled in so the CLI does not depend on a
/// compiler crate for one string. It is held to the wire format by a unit test
/// below, which spells the string out; each of the other copies is pinned the
/// same way where it lives, which is what makes a drift in any one of them fail
/// somewhere.
pub(crate) const CHECKED_SECTION_NAME: &str = "inference.checked";

/// Where a verification construct belongs, which every refusal of an
/// [`ArtifactScan::VerificationConstruct`] artifact tells its reader: one clause,
/// so the commands that refuse one — `infs run`, and `[build.wasm-opt]` before
/// the optimizer — send the reader to the same place in the same words.
pub(crate) const VERIFICATION_CONSTRUCTS_BELONG_IN_SPECS: &str = "Verification constructs \
     (forall/exists/assume/unique and `@`/uzumaki) belong in `spec` blocks, which compile-mode \
     builds strip";

/// What one scan of an artifact's bytes found.
///
/// The two states are mutually exclusive by construction, so a caller can never
/// read a bulk-memory verdict off an artifact the scan rejected outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactScan {
    /// A verification-only construct leaked into an ordinary function; the
    /// payload is its source spelling (e.g. `"forall"`, `"i32.uzumaki"`).
    VerificationConstruct(&'static str),
    /// An ordinary executable artifact: whether it carries any bulk-memory
    /// operator, whether it records which of its functions trap on arithmetic
    /// overflow, and the `(module, field)` of every function it imports, in
    /// import-section order.
    ///
    /// Only function imports are listed, because a function is the only kind of
    /// import an Inference artifact carries: code generation emits no other
    /// kind, and the linker refuses a main module that imports any other kind.
    Executable {
        uses_bulk_memory: bool,
        records_overflow_guards: bool,
        function_imports: Vec<(String, String)>,
    },
}

/// Scans `wasm_bytes` once for the four facts a caller needs up front:
/// whether a verification-only construct leaked into the artifact, whether the
/// artifact carries bulk memory, whether it records which of its functions trap
/// on arithmetic overflow, and which functions it imports.
///
/// Compile-mode builds strip `spec` blocks, so a well-formed executable artifact
/// carries no verification construct. Finding one means it leaked into an
/// ordinary function — `wasm-opt` would reject the unknown `0xfc` opcode with an
/// opaque error, so the scan stops there and lets the caller surface it with
/// remediation instead.
///
/// `artifact` is the file `wasm_bytes` were read from, and every failure below
/// names it. It is a parameter rather than a spelling baked in here because the
/// scan belongs to the artifact and not to any one command, and the commands do
/// not all act on the same file: a project build writes `<root>/out/main.wasm`,
/// while a single-file build writes `out/<source stem>.wasm`. A message naming
/// a file the caller did not scan sends its reader to the wrong artifact.
///
/// # Errors
///
/// Errors if the artifact cannot be parsed as WebAssembly.
pub(crate) fn scan_artifact(wasm_bytes: &[u8], artifact: &Path) -> Result<ArtifactScan> {
    let mut uses_bulk_memory = false;
    let mut records_overflow_guards = false;
    let mut function_imports = Vec::new();
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload
            .map_err(|err| anyhow::anyhow!("failed to scan {}: {err}", artifact.display()))?;
        if let Payload::CustomSection(reader) = &payload
            && reader.name() == CHECKED_SECTION_NAME
        {
            records_overflow_guards = true;
            continue;
        }
        if let Payload::ImportSection(reader) = payload {
            for import in reader {
                let import = import.map_err(|err| {
                    anyhow::anyhow!(
                        "failed to read an import while scanning {}: {err}",
                        artifact.display()
                    )
                })?;
                match import.ty {
                    TypeRef::Func(_) => {
                        function_imports.push((import.module.to_string(), import.name.to_string()));
                    }
                    // Code generation emits function imports only, and the
                    // linker refuses a main module that imports any other kind,
                    // so no Inference artifact reaches this arm.
                    TypeRef::Table(_)
                    | TypeRef::Memory(_)
                    | TypeRef::Global(_)
                    | TypeRef::Tag(_) => {}
                }
            }
            continue;
        }
        let Payload::CodeSectionEntry(body) = payload else {
            continue;
        };
        let operators = body.get_operators_reader().map_err(|err| {
            anyhow::anyhow!(
                "failed to read a function body while scanning {}: {err}",
                artifact.display()
            )
        })?;
        for op in operators {
            let op = op.map_err(|err| {
                anyhow::anyhow!(
                    "failed to decode an operator while scanning {}: {err}",
                    artifact.display()
                )
            })?;
            if let Some(name) = verification_construct_name(&op) {
                return Ok(ArtifactScan::VerificationConstruct(name));
            }
            uses_bulk_memory |= is_bulk_memory(&op);
        }
    }
    Ok(ArtifactScan::Executable {
        uses_bulk_memory,
        records_overflow_guards,
        function_imports,
    })
}

/// Whether `op` belongs to the bulk-memory proposal.
///
/// Two sanctioned sources put one of these in a built artifact: a project that
/// opts in with `[build] wasm-features = ["bulk-memory"]`, in which case codegen
/// emits `memory.copy`/`memory.fill` directly; and a statically merged external
/// module, which the linker's supported-feature envelope admits regardless of
/// what the project requested. Neither is distinguishable here, and neither needs
/// to be — the predicate answers what the bytes contain. The segment-indexed
/// forms are included even though the merge rejects them today and codegen never
/// emits them, so that a widened linker or codegen cannot silently produce an
/// artifact Binaryen is not told to parse.
fn is_bulk_memory(op: &Operator) -> bool {
    use Operator::{DataDrop, MemoryCopy, MemoryFill, MemoryInit};
    matches!(
        op,
        MemoryFill { .. } | MemoryCopy { .. } | MemoryInit { .. } | DataDrop { .. }
    )
}

/// The source spelling of a verification-only operator, or `None` for an
/// ordinary executable one.
///
/// This is the local mirror of `is_verification_only` in
/// `core/wasm-linker/src/safety.rs` — the linker's fail-closed predicate over
/// the same six opcodes. Both consume the same `inf-wasmparser` fork, so a new
/// verification opcode requires touching that fork (where the mirrored-predicate
/// note lives); a wasm-linker dependency for six match arms is not worth the
/// coupling.
fn verification_construct_name(op: &Operator) -> Option<&'static str> {
    use Operator::{Assume, Exists, Forall, I32Uzumaki, I64Uzumaki, Unique};
    match op {
        Forall { .. } => Some("forall"),
        Exists { .. } => Some("exists"),
        Assume { .. } => Some("assume"),
        Unique { .. } => Some("unique"),
        I32Uzumaki { .. } => Some("i32.uzumaki"),
        I64Uzumaki { .. } => Some("i64.uzumaki"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        MEMORY_COPY_BODY, MEMORY_FILL_BODY, module_with_custom_section, module_with_imports,
        module_with_raw_body,
    };

    /// Scans a module built in memory. Nothing below reads the reported
    /// artifact name, since none of these modules is ever written to a file, so
    /// the scans share one stand-in.
    fn scan(module: &[u8]) -> ArtifactScan {
        scan_artifact(module, Path::new("scanned.wasm")).expect("the module parses")
    }

    #[test]
    fn the_guard_record_section_is_named_as_the_wire_format_has_it() {
        assert_eq!(CHECKED_SECTION_NAME, "inference.checked");
    }

    #[test]
    fn scan_artifact_detects_each_nondet_block() {
        for (sub_opcode, name) in [
            (0x3a_u8, "forall"),
            (0x3b, "exists"),
            (0x3c, "assume"),
            (0x3d, "unique"),
        ] {
            // `<nondet> (empty blocktype) end; end`.
            let body = [0x00, 0xfc, sub_opcode, 0x40, 0x0b, 0x0b];
            let module = module_with_raw_body(&body);
            assert_eq!(
                scan(&module),
                ArtifactScan::VerificationConstruct(name),
                "sub-opcode {sub_opcode:#x} must be reported as `{name}`"
            );
        }
    }

    #[test]
    fn scan_artifact_detects_uzumaki() {
        // `<uzumaki> drop; end`, for both the i32 and i64 forms.
        let i32_body = [0x00, 0xfc, 0x31, 0x1a, 0x0b];
        assert_eq!(
            scan(&module_with_raw_body(&i32_body)),
            ArtifactScan::VerificationConstruct("i32.uzumaki")
        );
        let i64_body = [0x00, 0xfc, 0x32, 0x1a, 0x0b];
        assert_eq!(
            scan(&module_with_raw_body(&i64_body)),
            ArtifactScan::VerificationConstruct("i64.uzumaki")
        );
    }

    #[test]
    fn scan_artifact_reports_an_artifact_that_records_overflow_guards() {
        let module = module_with_custom_section(&[0x0b], CHECKED_SECTION_NAME, &[1, 1, 0]);
        assert_eq!(
            scan(&module),
            ArtifactScan::Executable {
                uses_bulk_memory: false,
                records_overflow_guards: true,
                function_imports: Vec::new(),
            }
        );
    }

    #[test]
    fn scan_artifact_reads_no_guard_record_from_another_custom_section() {
        // The verdict keys on the section's name, not on there being a custom
        // section at all -- a `name` or `producers` section says nothing about
        // overflow, and marking an artifact that records no guards would put a
        // claim in it its producer never made.
        let module = module_with_custom_section(&[0x0b], "producers", &[0]);
        assert_eq!(
            scan(&module),
            ArtifactScan::Executable {
                uses_bulk_memory: false,
                records_overflow_guards: false,
                function_imports: Vec::new(),
            }
        );
    }

    #[test]
    fn scan_artifact_reports_a_plain_body_as_bulk_free() {
        // An ordinary executable body (just `end`) carries neither a
        // verification-only opcode nor bulk memory.
        let module = module_with_raw_body(&[0x0b]);
        assert_eq!(
            scan(&module),
            ArtifactScan::Executable {
                uses_bulk_memory: false,
                records_overflow_guards: false,
                function_imports: Vec::new(),
            }
        );
    }

    #[test]
    fn scan_artifact_detects_each_bulk_memory_operator() {
        // The four bulk-memory operators, each in an otherwise ordinary body.
        // `memory.init 0 0` and `data.drop 0` decode without their segments;
        // the scan reads operators, it does not validate.
        let memory_init: &[u8] = &[
            0x41, 0x00, 0x41, 0x00, 0x41, 0x00, 0xfc, 0x08, 0x00, 0x00, 0x0b,
        ];
        let data_drop: &[u8] = &[0xfc, 0x09, 0x00, 0x0b];
        for (body, name) in [
            (MEMORY_FILL_BODY, "memory.fill"),
            (MEMORY_COPY_BODY, "memory.copy"),
            (memory_init, "memory.init"),
            (data_drop, "data.drop"),
        ] {
            let module = module_with_raw_body(body);
            assert_eq!(
                scan(&module),
                ArtifactScan::Executable {
                    uses_bulk_memory: true,
                    records_overflow_guards: false,
                    function_imports: Vec::new(),
                },
                "{name} must be reported as bulk memory"
            );
        }
    }

    #[test]
    fn scan_artifact_reports_verification_construct_ahead_of_bulk_memory() {
        // A leaked construct is a hard error, so it wins over the bulk verdict
        // even when both are present — the caller never has to choose.
        let mut body = vec![0xfc, 0x31, 0x1a];
        body.extend_from_slice(MEMORY_FILL_BODY);
        assert_eq!(
            scan(&module_with_raw_body(&body)),
            ArtifactScan::VerificationConstruct("i32.uzumaki")
        );
    }

    /// Every function import is listed, in import-section order and unsorted, and
    /// no other kind of import is: the scan skips the memory import between the
    /// two functions, and the functions on either side of it keep their order.
    #[test]
    fn scan_artifact_lists_function_imports_in_import_order() {
        use wasm_encoder::{EntityType, MemoryType};

        let memory = EntityType::Memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        let module = module_with_imports(&[
            ("fprime_core", "telemetry", EntityType::Function(0)),
            ("env", "memory", memory),
            ("env", "clock_ms", EntityType::Function(0)),
        ]);
        assert_eq!(
            scan(&module),
            ArtifactScan::Executable {
                uses_bulk_memory: false,
                records_overflow_guards: false,
                function_imports: vec![
                    ("fprime_core".to_string(), "telemetry".to_string()),
                    ("env".to_string(), "clock_ms".to_string()),
                ],
            }
        );
    }

    /// A module that imports nothing lists nothing — including one whose import
    /// section holds only non-function imports.
    #[test]
    fn scan_artifact_lists_no_function_import_where_there_is_none() {
        use wasm_encoder::{EntityType, MemoryType};

        let memory_only = module_with_imports(&[(
            "env",
            "memory",
            EntityType::Memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        )]);
        for module in [module_with_raw_body(&[0x0b]), memory_only] {
            let ArtifactScan::Executable {
                function_imports, ..
            } = scan(&module)
            else {
                panic!("an ordinary module is an executable artifact");
            };
            assert!(
                function_imports.is_empty(),
                "no function import was declared, got: {function_imports:?}"
            );
        }
    }
}
