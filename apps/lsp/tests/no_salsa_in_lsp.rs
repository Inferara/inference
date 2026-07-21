//! Guard: the protocol layer must not depend on Salsa.
//!
//! `apps/lsp` talks to the semantic layer only through `inference-ide`'s
//! path-addressed feature API; the Salsa database lives behind `ide-db` and must
//! never leak up here. This walks this crate's own `src/` and fails if any `.rs`
//! line mentions `salsa`, naming the offending `file:line` so a regression points
//! straight at the leak (#157).

use std::fs;
use std::path::Path;

#[test]
fn no_salsa_symbol_in_lsp_source() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    collect(&src, &mut offenders);
    assert!(
        offenders.is_empty(),
        "salsa must not appear in apps/lsp source; offending lines: {offenders:?}"
    );
}

/// Recursively collects `file:line` for every `.rs` line containing `salsa`,
/// visiting entries in a stable order so the failure message is deterministic.
fn collect(dir: &Path, offenders: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable source dir")
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, offenders);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path).expect("readable .rs file");
            for (i, line) in text.lines().enumerate() {
                if line.to_lowercase().contains("salsa") {
                    offenders.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
    }
}
