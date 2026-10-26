//! One pass over a built artifact's bytes, answering the questions a post-build
//! step has to ask before it acts on the file.
//!
//! A finished `.wasm` is all `infs` has to go on. Whether a verification-only
//! opcode leaked into an ordinary function, whether the module carries bulk
//! memory, whether it records which of its functions trap on arithmetic
//! overflow, which functions and which memory it imports from its environment —
//! none of that is recorded anywhere but in the bytes, so each answer costs a
//! parse. Asking them together costs one, and it keeps the answers consistent
//! with one another: no caller can pair a bulk-memory verdict taken from one
//! traversal with a guard record taken from another.
//!
//! The questions are about the artifact, not about any one thing done to it, so
//! the scan sits beside the commands rather than inside one. A command that
//! needs a different subset of the same facts reads them from the module that
//! owns the parse, instead of reaching into a sibling command for them or
//! opening a second pass of its own.
//!
//! One question is asked on its own pass, [`exported_function_signature`]: the
//! type of one export, which only `infs run` needs, of one artifact it is about
//! to invoke, and which no answer of the scan has to agree with.

use std::path::Path;

use anyhow::Result;
use inf_wasmparser::{ExternalKind, FuncType, Operator, Parser, Payload, TypeRef};

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

/// The custom section recording which functions of a module hold an array
/// access emitted without its runtime bounds guard.
///
/// A hand-synchronised copy, pinned by a unit test below, for the reason
/// [`CHECKED_SECTION_NAME`] is.
pub(crate) const BOUNDS_ELIDED_SECTION_NAME: &str = "inference.bounds_elided";

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
    /// overflow, whether it records which of its functions omit a bounds guard,
    /// the `(module, field)` of every function it imports, in import-section
    /// order, and the memory it imports, if it imports one.
    ///
    /// A function and a memory are the only kinds of import an Inference
    /// artifact carries. Code generation emits a memory import only for a
    /// layout that asks to import its memory, and `infc`'s link step holds the
    /// artifact to exactly that layout's `env.memory` and refuses a main module
    /// that imports any other kind: a table, a global, a tag, or a second
    /// memory.
    Executable {
        uses_bulk_memory: bool,
        records_overflow_guards: bool,
        records_elided_bounds: bool,
        function_imports: Vec<(String, String)>,
        memory_import: Option<ScannedMemoryImport>,
    },
}

/// A memory import as an artifact's import section spells it: the two-level
/// name and the limits, in 64 KiB pages.
///
/// Read from the bytes rather than taken from the layout the build asked for,
/// so a caller that has to supply the memory supplies the one the artifact it
/// is about to run declares — after linking and, in a project, after
/// `[build.wasm-opt]` — rather than the one a manifest describes. The limits
/// are `u64`s because that is how `inf_wasmparser` represents them, wide enough
/// for a 64-bit memory; a 32-bit memory, the only kind an Inference artifact
/// imports, encodes each as a `u32` LEB128. `inf_wasmparser` calls the minimum
/// `initial`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScannedMemoryImport {
    /// The import's module name, the first half of its two-level name.
    pub module: String,
    /// The import's field name, the second half of its two-level name.
    pub field: String,
    /// The fewest pages the supplied memory may have.
    pub minimum: u64,
    /// The most pages the supplied memory may have, or `None` when the import
    /// sets no maximum.
    pub maximum: Option<u64>,
}

/// Scans `wasm_bytes` once for the six facts a caller needs up front:
/// whether a verification-only construct leaked into the artifact, whether the
/// artifact carries bulk memory, whether it records which of its functions trap
/// on arithmetic overflow, whether it records which omit a bounds guard, which
/// functions it imports, and which memory it imports.
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
    let mut records_elided_bounds = false;
    let mut function_imports = Vec::new();
    let mut memory_import = None;
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload
            .map_err(|err| anyhow::anyhow!("failed to scan {}: {err}", artifact.display()))?;
        if let Payload::CustomSection(reader) = &payload
            && reader.name() == CHECKED_SECTION_NAME
        {
            records_overflow_guards = true;
            continue;
        }
        if let Payload::CustomSection(reader) = &payload
            && reader.name() == BOUNDS_ELIDED_SECTION_NAME
        {
            records_elided_bounds = true;
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
                    // Memory 0, the one every memory instruction of an
                    // Inference artifact addresses. `infc`'s link step admits
                    // no second memory import, so a later one is left to the
                    // arm below.
                    TypeRef::Memory(ty) if memory_import.is_none() => {
                        memory_import = Some(ScannedMemoryImport {
                            module: import.module.to_string(),
                            field: import.name.to_string(),
                            minimum: ty.initial,
                            maximum: ty.maximum,
                        });
                    }
                    // Code generation emits no other import, and the link step
                    // refuses a main module that imports a table, a global, a
                    // tag or a second memory, so no Inference artifact reaches
                    // this arm.
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
        records_elided_bounds,
        function_imports,
        memory_import,
    })
}

/// The WebAssembly type of the function `wasm_bytes` exports as `name`, or
/// `None` when it exports no function by that name.
///
/// The type is the compiled function's, which is not always the one its source
/// declares: a struct or array parameter is an `i32` address, and a function
/// returning a struct or array takes the address to write its result to as a
/// hidden first parameter and returns nothing. An exported import is typed by
/// its import, since imported functions come first in the function index space.
///
/// `artifact` is the file `wasm_bytes` were read from, and names it in a
/// failure, as it does for [`scan_artifact`].
///
/// # Errors
///
/// Errors if the artifact's type, import, function or export section cannot be
/// read.
pub(crate) fn exported_function_signature(
    wasm_bytes: &[u8],
    name: &str,
    artifact: &Path,
) -> Result<Option<FuncType>> {
    let unreadable = |err: inf_wasmparser::BinaryReaderError| {
        anyhow::anyhow!("failed to read {}: {err}", artifact.display())
    };
    let mut types = Vec::new();
    let mut function_types = Vec::new();
    let mut exported = None;
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        match payload.map_err(unreadable)? {
            Payload::TypeSection(reader) => {
                for ty in reader.into_iter_err_on_gc_types() {
                    types.push(ty.map_err(unreadable)?);
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    if let TypeRef::Func(ty) = import.map_err(unreadable)?.ty {
                        function_types.push(ty);
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for ty in reader {
                    function_types.push(ty.map_err(unreadable)?);
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(unreadable)?;
                    if export.name == name && export.kind == ExternalKind::Func {
                        exported = Some(export.index);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(exported
        .and_then(|function| function_types.get(function as usize))
        .and_then(|ty| types.get(*ty as usize))
        .cloned())
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
    fn the_elided_bounds_record_section_is_named_as_the_wire_format_has_it() {
        assert_eq!(BOUNDS_ELIDED_SECTION_NAME, "inference.bounds_elided");
    }

    #[test]
    fn scan_artifact_reports_an_artifact_that_records_elided_bounds() {
        let module = module_with_custom_section(&[0x0b], BOUNDS_ELIDED_SECTION_NAME, &[1, 1, 0]);
        assert_eq!(
            scan(&module),
            ArtifactScan::Executable {
                uses_bulk_memory: false,
                records_overflow_guards: false,
                records_elided_bounds: true,
                function_imports: Vec::new(),
                memory_import: None,
            }
        );
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
                records_elided_bounds: false,
                function_imports: Vec::new(),
                memory_import: None,
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
                records_elided_bounds: false,
                function_imports: Vec::new(),
                memory_import: None,
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
                records_elided_bounds: false,
                function_imports: Vec::new(),
                memory_import: None,
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
                    records_elided_bounds: false,
                    function_imports: Vec::new(),
                    memory_import: None,
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
    /// no other kind of import is: the memory import between the two functions
    /// is reported on its own, and the functions on either side of it keep their
    /// order.
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
                records_elided_bounds: false,
                function_imports: vec![
                    ("fprime_core".to_string(), "telemetry".to_string()),
                    ("env".to_string(), "clock_ms".to_string()),
                ],
                memory_import: Some(ScannedMemoryImport {
                    module: "env".to_string(),
                    field: "memory".to_string(),
                    minimum: 1,
                    maximum: None,
                }),
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

    /// A memory import of `minimum` pages, at most `maximum`, unshared and
    /// 32-bit, as an Inference artifact declares one.
    fn imported_memory(minimum: u64, maximum: Option<u64>) -> wasm_encoder::EntityType {
        wasm_encoder::EntityType::Memory(wasm_encoder::MemoryType {
            minimum,
            maximum,
            memory64: false,
            shared: false,
            page_size_log2: None,
        })
    }

    /// What the scan reports `module` imports as its memory.
    fn memory_import_of(module: &[u8]) -> Option<ScannedMemoryImport> {
        let ArtifactScan::Executable { memory_import, .. } = scan(module) else {
            panic!("an ordinary module is an executable artifact");
        };
        memory_import
    }

    /// The memory import as written: its two names and both limits, copied
    /// rather than assumed. A fixed memory, a growable one, one with no maximum,
    /// a zero minimum beside the largest 32-bit maximum, and one under a name
    /// that is not `env.memory` each read back exactly, so `infs run` can supply
    /// the memory the artifact declares whatever the layout that built it.
    ///
    /// Fails if a limit is dropped, swapped or defaulted, or if the names are
    /// taken from anywhere but the import.
    #[test]
    fn scan_artifact_reports_a_memory_imports_names_and_limits() {
        let rows: [(&str, &str, u64, Option<u64>); 5] = [
            ("env", "memory", 1, Some(1)),
            ("env", "memory", 2, Some(8)),
            ("env", "memory", 1, None),
            ("env", "memory", 0, Some(65_536)),
            ("host", "heap", 3, Some(5)),
        ];
        for (module_name, field, minimum, maximum) in rows {
            let module =
                module_with_imports(&[(module_name, field, imported_memory(minimum, maximum))]);
            assert_eq!(
                scan(&module),
                ArtifactScan::Executable {
                    uses_bulk_memory: false,
                    records_overflow_guards: false,
                    records_elided_bounds: false,
                    function_imports: Vec::new(),
                    memory_import: Some(ScannedMemoryImport {
                        module: module_name.to_string(),
                        field: field.to_string(),
                        minimum,
                        maximum,
                    }),
                },
                "{module_name}.{field} ({minimum}, {maximum:?})"
            );
        }
    }

    /// A module that defines its memory, or has none, imports no memory, and
    /// one that imports only functions, a table or a global does not either:
    /// only a memory import is reported as one.
    #[test]
    fn scan_artifact_reports_no_memory_import_for_a_module_that_imports_none() {
        use crate::testing::module_with_memory_and_raw_body;
        use wasm_encoder::{EntityType, GlobalType, RefType, TableType, ValType};

        let table = EntityType::Table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: 1,
            maximum: None,
            shared: false,
        });
        let global = EntityType::Global(GlobalType {
            val_type: ValType::I32,
            mutable: false,
            shared: false,
        });
        for module in [
            module_with_raw_body(&[0x0b]),
            module_with_memory_and_raw_body(&[0x0b]),
            module_with_imports(&[("env", "clock_ms", EntityType::Function(0))]),
            module_with_imports(&[("env", "table", table), ("env", "global", global)]),
        ] {
            assert_eq!(memory_import_of(&module), None);
        }
    }

    /// With two memory imports only the first is reported — memory 0, the one
    /// an Inference artifact's memory instructions address — and the function
    /// imports on either side of both keep their order. A table import ahead of
    /// the memory does not displace it.
    ///
    /// Fails if a later memory import overwrites the first, or if reporting the
    /// memory stops the function imports being listed.
    #[test]
    fn scan_artifact_reports_the_first_memory_import_beside_the_functions() {
        use wasm_encoder::{EntityType, RefType, TableType};

        let table = EntityType::Table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: 1,
            maximum: None,
            shared: false,
        });
        let module = module_with_imports(&[
            ("env", "table", table),
            ("env", "memory", imported_memory(2, Some(8))),
            ("env", "clock_ms", EntityType::Function(0)),
            ("env", "second", imported_memory(4, Some(4))),
            ("fprime_core", "telemetry", EntityType::Function(0)),
        ]);
        let ArtifactScan::Executable {
            function_imports,
            memory_import,
            ..
        } = scan(&module)
        else {
            panic!("an ordinary module is an executable artifact");
        };
        assert_eq!(
            memory_import,
            Some(ScannedMemoryImport {
                module: "env".to_string(),
                field: "memory".to_string(),
                minimum: 2,
                maximum: Some(8),
            })
        );
        assert_eq!(
            function_imports,
            [
                ("env".to_string(), "clock_ms".to_string()),
                ("fprime_core".to_string(), "telemetry".to_string()),
            ]
        );
    }

    /// The type of the function a module exports under a name, read in the
    /// function index space: a local function by its own type, an exported
    /// import by its import's, and nothing for a name that is not a function's.
    ///
    /// Fails if the index space stops counting imports first, which reads a
    /// local export's type off the wrong entry once a module imports anything.
    #[test]
    fn an_exports_signature_is_read_in_the_function_index_space() {
        use crate::testing::module_exporting;
        use inf_wasmparser::ValType as Read;
        use wasm_encoder::{
            CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
            ImportSection, Instruction, MemorySection, MemoryType, Module, TypeSection,
            ValType::{I32, I64},
        };

        let signature = |module: &[u8], name: &str| {
            exported_function_signature(module, name, Path::new("scanned.wasm"))
                .expect("the module parses")
                .map(|ty| (ty.params().to_vec(), ty.results().to_vec()))
        };

        let plain = module_exporting(&[("add", &[I32, I32], &[I32]), ("main", &[I64], &[])]);
        assert_eq!(signature(&plain, "main"), Some((vec![Read::I64], vec![])));
        assert_eq!(
            signature(&plain, "add"),
            Some((vec![Read::I32, Read::I32], vec![Read::I32]))
        );
        assert_eq!(signature(&plain, "missing"), None);

        // Type 0 is the import's `(i64) -> i32`, type 1 the local `main`'s
        // `(i32) -> ()`; `main` is function 1, after the one import.
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([I64], [I32]);
        types.ty().function([I32], []);
        module.section(&types);
        let mut imports = ImportSection::new();
        imports.import("env", "f", EntityType::Function(0));
        module.section(&imports);
        let mut funcs = FunctionSection::new();
        funcs.function(1);
        module.section(&funcs);
        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memories);
        let mut exports = ExportSection::new();
        exports.export("main", ExportKind::Func, 1);
        exports.export("reexported", ExportKind::Func, 0);
        exports.export("memory", ExportKind::Memory, 0);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        body.instruction(&Instruction::End);
        code.function(&body);
        module.section(&code);
        let module = module.finish();

        assert_eq!(signature(&module, "main"), Some((vec![Read::I32], vec![])));
        assert_eq!(
            signature(&module, "reexported"),
            Some((vec![Read::I64], vec![Read::I32]))
        );
        assert_eq!(
            signature(&module, "memory"),
            None,
            "a memory is not a function, whatever its name"
        );
        assert!(
            exported_function_signature(b"\0asm\x01\0\0\0\x07", "main", Path::new("cut.wasm"))
                .expect_err("a truncated export section is an error")
                .to_string()
                .contains("cut.wasm"),
            "a failure names the artifact"
        );
    }
}
