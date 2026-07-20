//! Protocol-level end-to-end tests for the `inference-lsp` binary.
//!
//! Each test spawns the real server over stdio and drives a full LSP session —
//! initialize, feature exchanges, shutdown, exit — asserting on the raw JSON that
//! crosses the wire. The [`harness`] client bounds every read with a timeout, so a
//! regression that hangs the server fails the test instead of stalling the run.
//! All fixtures live in per-test unique temp directories, never at a filesystem
//! root and never in the repo, so the tests are parallel-safe.

mod harness;

use std::time::Duration;

use harness::{LspClient, TempDir, path_to_uri, pos_after, pos_at, pos_at_nth, pos_end};
use serde_json::{Value, json};

// LSP `SymbolKind` numeric values (LSP spec, DocumentSymbol section).
const KIND_METHOD: i64 = 6;
const KIND_FIELD: i64 = 8;
const KIND_INTERFACE: i64 = 11; // A spec maps to Interface.
const KIND_FUNCTION: i64 = 12;
const KIND_STRUCT: i64 = 23;

// LSP `InlayHintKind::Type`.
const INLAY_KIND_TYPE: i64 = 1;

// JSON-RPC error codes.
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
// Only the debug-only panic-boundary test asserts on this code.
#[cfg(debug_assertions)]
const INTERNAL_ERROR: i64 = -32603;

/// The environment variable that arms the server's debug-only analysis-panic seam,
/// and the path marker the panic-boundary tests arm it with. Only the panic-trigger
/// fixture's path carries the marker, so a healthy `main.inf` sibling in the same
/// session analyzes normally. Gated on `debug_assertions` with the tests that use
/// them: the server's seam is a no-op in release builds, so these have no meaning
/// there.
#[cfg(debug_assertions)]
const PANIC_ENV: &str = "INFERENCE_LSP_TEST_PANIC_PATH_SUBSTR";
#[cfg(debug_assertions)]
const PANIC_PATH_MARKER: &str = "panic-trigger";

/// A well-formed document whose *path* (not its contents) makes the armed server
/// seam force a deterministic analysis panic — the trigger the message-loop panic
/// boundary (#241) needs without depending on a specific compiler bug. The former
/// in-tree trigger (a named constant as an array size) became an ordinary
/// diagnostic once #240 was fixed, so the panic is now injected by the seam.
#[cfg(debug_assertions)]
const PANIC_DOC_SOURCE: &str = "fn main() -> i32 { return 0; }";

/// A single-file fixture: an isolated temp dir with `main.inf` written to disk,
/// plus its `file://` URI. The returned [`TempDir`] must be kept alive for the
/// session (it removes the directory on drop). The document is not opened yet, so
/// a test can assert on the diagnostics its own `did_open` returns.
fn fixture(tag: &str, source: &str) -> (TempDir, String) {
    let dir = TempDir::new(tag);
    let path = dir.write("main.inf", source);
    let uri = path_to_uri(&path);
    (dir, uri)
}

/// A fixture whose document path carries [`PANIC_PATH_MARKER`], so an armed server
/// forces a deterministic analysis panic for it (see [`PANIC_DOC_SOURCE`]). The
/// file is named `panic-trigger.inf` rather than `main.inf` so the marker matches
/// only this document, never a healthy sibling in the same temp-dir tag.
#[cfg(debug_assertions)]
fn panic_fixture(tag: &str) -> (TempDir, String) {
    let dir = TempDir::new(tag);
    let path = dir.write("panic-trigger.inf", PANIC_DOC_SOURCE);
    let uri = path_to_uri(&path);
    (dir, uri)
}

fn hover_request(client: &mut LspClient, uri: &str, position: Value) -> Value {
    client.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": position }),
    )
}

fn definition_request(client: &mut LspClient, uri: &str, position: Value) -> Value {
    client.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": position }),
    )
}

fn completion_request(client: &mut LspClient, uri: &str, position: Value) -> Value {
    client.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": position }),
    )
}

/// The labels of a `CompletionResponse::Array`.
fn completion_labels(response: &Value) -> Vec<String> {
    response["result"]
        .as_array()
        .expect("completion result is an array")
        .iter()
        .map(|item| {
            item["label"]
                .as_str()
                .expect("label is a string")
                .to_owned()
        })
        .collect()
}

/// The symbol in `symbols` named `name`, panicking if absent.
fn symbol<'a>(symbols: &'a [Value], name: &str) -> &'a Value {
    symbols
        .iter()
        .find(|symbol| symbol["name"] == json!(name))
        .unwrap_or_else(|| panic!("no symbol named {name:?} in {symbols:?}"))
}

// --- 1. initialize handshake ------------------------------------------------

#[test]
fn initialize_advertises_the_v1_capabilities() {
    let mut client = LspClient::spawn();
    let result = client.initialize_default(true);
    let capabilities = &result["capabilities"];

    assert_eq!(capabilities["textDocumentSync"], json!(1), "full-text sync");
    assert_eq!(capabilities["hoverProvider"], json!(true));
    assert_eq!(capabilities["definitionProvider"], json!(true));
    assert_eq!(capabilities["documentSymbolProvider"], json!(true));
    assert_eq!(capabilities["inlayHintProvider"], json!(true));

    let completion = &capabilities["completionProvider"];
    assert_eq!(completion["resolveProvider"], json!(false));
    let triggers = completion["triggerCharacters"]
        .as_array()
        .expect("trigger characters");
    assert!(triggers.contains(&json!(".")), "`.` is a trigger");
    assert!(triggers.contains(&json!(":")), "`:` is a trigger");

    // No position encoding is negotiated, so the client keeps the UTF-16 default.
    assert!(capabilities.get("positionEncoding").is_none());

    // serverInfo carries the server's name and version, which clients surface in
    // logs and crash reports.
    let server_info = &result["serverInfo"];
    assert_eq!(
        server_info["name"], json!("inference-lsp"),
        "serverInfo names the server, got {result}"
    );
    assert!(
        server_info["version"].as_str().is_some_and(|v| !v.is_empty()),
        "serverInfo carries a non-empty version, got {result}"
    );

    client.shutdown_exit_ok();
}

// --- 2. didOpen clean -> empty diagnostics ----------------------------------

#[test]
fn did_open_clean_file_publishes_empty_diagnostics() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("clean", "fn add(a: i32, b: i32) -> i32 { return a + b; }");
    let published = client.did_open(&uri, "fn add(a: i32, b: i32) -> i32 { return a + b; }", 1);

    assert!(
        published.diagnostics.is_empty(),
        "a clean file has no diagnostics, got {:?}",
        published.diagnostics
    );
    assert_eq!(published.version, json!(1), "the echoed document version");

    client.shutdown_exit_ok();
}

// --- 3. syntax error diagnostic ---------------------------------------------

#[test]
fn did_open_with_syntax_error_publishes_a_syntax_diagnostic() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // A multi-line fixture: the missing expression is on line 1, so a range that
    // regressed to line 0 (or to `0..0`) would fail the exact anchor below rather
    // than pass because `line == 0` happened to hold on a one-line file.
    let source = "fn f() {\n    let x: i32 = ;\n}";
    let (_dir, uri) = fixture("syntax", source);
    let published = client.did_open(&uri, source, 1);

    let diagnostic = published
        .by_code("syntax")
        .expect("a syntax-coded diagnostic");
    assert_eq!(diagnostic["severity"], json!(1), "Error severity");
    assert!(
        diagnostic["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "the message is non-empty"
    );
    // The diagnostic is anchored on the semicolon where an expression was expected.
    assert_eq!(
        diagnostic["range"]["start"],
        pos_at(source, ";"),
        "anchored where the expression is missing"
    );
    assert_eq!(diagnostic["range"]["end"], pos_after(source, ";"));

    client.shutdown_exit_ok();
}

// --- 4. didChange fixes the error -------------------------------------------

#[test]
fn did_change_fixing_the_error_clears_diagnostics() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("fix", "fn f() -> i32 { return x; }");
    let broken = client.did_open(&uri, "fn f() -> i32 { return x; }", 1);
    assert!(
        !broken.diagnostics.is_empty(),
        "the undeclared `x` is reported"
    );

    let fixed = client.did_change(&uri, "fn f() -> i32 { return 1; }", 2);
    assert!(fixed.diagnostics.is_empty(), "the fix clears diagnostics");
    assert_eq!(fixed.version, json!(2), "the new document version");

    client.shutdown_exit_ok();
}

// --- 5. didChange introduces a type error -----------------------------------

#[test]
fn did_change_introducing_a_type_error_surfaces_it_with_location() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("type", "fn add(a: i32, b: i32) -> i32 { return a + b; }");
    let clean = client.did_open(&uri, "fn add(a: i32, b: i32) -> i32 { return a + b; }", 1);
    assert!(clean.diagnostics.is_empty());

    // Returning a `bool` where `i32` is declared is a genuine type mismatch. The
    // return sits on line 1 of a multi-line fixture, so the exact anchor below is
    // not satisfiable by a degenerate `0..0` range.
    let broken_source = "fn f() -> i32 {\n    return true;\n}";
    let broken = client.did_change(&uri, broken_source, 2);
    let diagnostic = broken.by_code("type").expect("a type-coded diagnostic");
    assert_eq!(diagnostic["severity"], json!(1));
    let message = diagnostic["message"].as_str().expect("a message");
    assert!(
        message.contains("mismatch") && message.contains("i32"),
        "the message names the type mismatch, got {message:?}"
    );
    // The diagnostic spans the offending `return true;` statement exactly.
    assert_eq!(
        diagnostic["range"]["start"],
        pos_at(broken_source, "return"),
        "anchored at the return keyword on line 1"
    );
    assert_eq!(
        diagnostic["range"]["end"],
        pos_after(broken_source, "return true;"),
        "range covers the whole return statement"
    );

    client.shutdown_exit_ok();
}

// --- 6. analysis rule finding (A041) ----------------------------------------

#[test]
fn duplicate_local_surfaces_as_an_a041_finding() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "fn f() { let a: i32 = 1; let a: i32 = 2; }";
    let (_dir, uri) = fixture("a041", source);
    let published = client.did_open(&uri, source, 1);

    let finding = published.by_code("A041").expect("an A041 finding");
    assert_eq!(finding["severity"], json!(1), "A041 is an Error");
    assert!(
        finding["message"]
            .as_str()
            .is_some_and(|m| m.contains("already declared")),
        "message explains the duplicate, got {}",
        finding["message"]
    );
    // Anchored on the second declaration.
    assert_eq!(finding["range"]["start"], pos_at_nth(source, "let a", 1));

    client.shutdown_exit_ok();
}

// --- 7. hover ---------------------------------------------------------------

#[test]
fn hover_over_a_local_shows_its_type() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "fn f() -> i32 { let count: i32 = 5; return count; }";
    let (_dir, uri) = fixture("hover-local", source);
    client.did_open(&uri, source, 1);

    let response = hover_request(&mut client, &uri, pos_at_nth(source, "count", 1));
    let contents = &response["result"]["contents"];
    assert_eq!(contents["kind"], json!("markdown"));
    assert!(
        contents["value"]
            .as_str()
            .is_some_and(|v| v.contains("count: i32")),
        "hover renders the local's type, got {}",
        contents["value"]
    );
    assert!(response["result"]["range"].is_object(), "hover has a range");

    client.shutdown_exit_ok();
}

#[test]
fn hover_over_forall_explains_the_nondet_construct() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "fn f() { forall { assert(true); } }";
    let (_dir, uri) = fixture("hover-forall", source);
    client.did_open(&uri, source, 1);

    let response = hover_request(&mut client, &uri, pos_at(source, "forall"));
    let value = response["result"]["contents"]["value"]
        .as_str()
        .expect("markdown hover value");
    assert!(value.contains("forall"), "names the keyword: {value}");
    assert!(
        value.contains("every path must succeed"),
        "carries the non-det explanation: {value}"
    );

    client.shutdown_exit_ok();
}

// --- 8. goto-definition, same file ------------------------------------------

#[test]
fn goto_definition_reaches_a_same_file_function() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "fn helper() -> i32 { return 7; }\nfn use_it() -> i32 { return helper(); }";
    let (_dir, uri) = fixture("goto-local", source);
    client.did_open(&uri, source, 1);

    let response = definition_request(&mut client, &uri, pos_at_nth(source, "helper", 1));
    let location = &response["result"];
    assert_eq!(location["uri"], json!(uri), "same-file target");
    // The focus range points at the definition's name.
    assert_eq!(location["range"]["start"], pos_at(source, "helper"));

    client.shutdown_exit_ok();
}

#[test]
fn goto_definition_at_the_word_end_of_a_call_reaches_the_definition() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The caret is at the exclusive end of the callee name (just before `(`),
    // where a double-click or just-finished keystroke leaves it. The raw offset
    // lands on the call expression, not the identifier, so this exercises the
    // shared one-byte-back fallback over the wire (issue #244).
    let source = "fn caller() -> i32 { return produce(); }\nfn produce() -> i32 { return 7; }";
    let (_dir, uri) = fixture("goto-word-end", source);
    client.did_open(&uri, source, 1);

    let response = definition_request(&mut client, &uri, pos_after(source, "produce"));
    let location = &response["result"];
    assert_eq!(location["uri"], json!(uri), "same-file target");
    // The call's word-end resolves to the callee definition, not the call site.
    assert_eq!(location["range"]["start"], pos_at_nth(source, "produce", 1));

    client.shutdown_exit_ok();
}

// --- 9. cross-file: import a sibling on disk ---------------------------------

#[test]
fn cross_file_entry_is_clean_and_goto_reaches_the_imported_file() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("cross");
    let entry_source = "use lib;\nfn main() -> i32 { return lib::helper(); }";
    let lib_source = "pub fn helper() -> i32 { return 7; }";
    let entry_path = dir.write("main.inf", entry_source);
    let lib_path = dir.write("lib.inf", lib_source);
    let entry_uri = path_to_uri(&entry_path);
    let lib_uri = path_to_uri(&lib_path);

    // Only the entry is opened; the lib is resolved from disk by the project walk.
    let published = client.did_open(&entry_uri, entry_source, 1);
    assert!(
        published.diagnostics.is_empty(),
        "a resolvable on-disk import raises no spurious diagnostics, got {:?}",
        published.diagnostics
    );

    let response = definition_request(&mut client, &entry_uri, pos_at(entry_source, "helper"));
    let location = &response["result"];
    assert_eq!(location["uri"], json!(lib_uri), "target is the lib file");
    assert_eq!(
        location["range"]["start"],
        pos_at(lib_source, "helper"),
        "range is the name in the lib file"
    );

    client.shutdown_exit_ok();
}

// --- 10. missing import -----------------------------------------------------

#[test]
fn missing_import_is_reported_on_the_use_directive() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The `use` directive starts off byte 0 (a header line precedes it) and off
    // line 0, so a degenerate `0..0` or whole-file range would fail the exact
    // anchor assertion below instead of passing vacuously.
    let source = "// entry\nuse libx;\nfn main() -> i32 { return 0; }";
    let (_dir, uri) = fixture("missing-import", source);
    let published = client.did_open(&uri, source, 1);

    let diagnostic = published.by_code("import").expect("an import diagnostic");
    assert!(
        diagnostic["message"]
            .as_str()
            .is_some_and(|m| m.contains("cannot find imported module `libx`")),
        "names the missing module, got {}",
        diagnostic["message"]
    );
    // The range is exactly the `use libx;` directive span, both ends pinned.
    assert_eq!(
        diagnostic["range"]["start"],
        pos_at(source, "use libx;"),
        "range starts at the directive"
    );
    assert_eq!(
        diagnostic["range"]["end"],
        pos_after(source, "use libx;"),
        "range ends at the directive's semicolon"
    );

    client.shutdown_exit_ok();
}

// --- 11. documentSymbol, hierarchical and flat ------------------------------

const SYMBOL_SOURCE: &str = "struct Point { px: i32; fn getx(self) -> i32 { return self.px; } }\n\
spec Laws { fn commutes() {} }\n\
fn entry() { return; }";

#[test]
fn document_symbol_returns_a_hierarchical_tree() {
    let mut client = LspClient::spawn();
    client.initialize_default(true); // hierarchical support declared

    let (_dir, uri) = fixture("symbols-tree", SYMBOL_SOURCE);
    client.did_open(&uri, SYMBOL_SOURCE, 1);

    let response = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = response["result"].as_array().expect("a symbol array");
    assert_eq!(symbols.len(), 3, "three top-level symbols");

    let point = symbol(symbols, "Point");
    assert_eq!(point["kind"], json!(KIND_STRUCT));
    assert_eq!(
        point["selectionRange"]["start"],
        pos_at(SYMBOL_SOURCE, "Point"),
        "selection range is the struct name"
    );
    let point_children = point["children"].as_array().expect("struct children");
    assert_eq!(symbol(point_children, "px")["kind"], json!(KIND_FIELD));
    assert_eq!(symbol(point_children, "getx")["kind"], json!(KIND_METHOD));

    let laws = symbol(symbols, "Laws");
    assert_eq!(
        laws["kind"],
        json!(KIND_INTERFACE),
        "a spec is an interface"
    );
    let laws_children = laws["children"].as_array().expect("spec children");
    assert_eq!(
        symbol(laws_children, "commutes")["kind"],
        json!(KIND_FUNCTION)
    );

    assert_eq!(symbol(symbols, "entry")["kind"], json!(KIND_FUNCTION));

    client.shutdown_exit_ok();
}

#[test]
fn document_symbol_flattens_for_a_non_hierarchical_client() {
    let mut client = LspClient::spawn();
    client.initialize_default(false); // no hierarchical support

    let (_dir, uri) = fixture("symbols-flat", SYMBOL_SOURCE);
    client.did_open(&uri, SYMBOL_SOURCE, 1);

    let response = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = response["result"]
        .as_array()
        .expect("a flat SymbolInformation array");

    // Every symbol, including nested members, appears at the top level.
    for name in ["Point", "px", "getx", "Laws", "commutes", "entry"] {
        let info = symbol(symbols, name);
        assert_eq!(info["location"]["uri"], json!(uri), "{name} location uri");
        assert!(info.get("children").is_none(), "flat symbols do not nest");
    }
    // A member records its enclosing symbol as the container.
    assert_eq!(symbol(symbols, "px")["containerName"], json!("Point"));
    assert_eq!(symbol(symbols, "getx")["containerName"], json!("Point"));
    assert_eq!(symbol(symbols, "commutes")["containerName"], json!("Laws"));

    client.shutdown_exit_ok();
}

// --- 12. completion ---------------------------------------------------------

#[test]
fn completion_at_top_level_offers_keywords_and_defs() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "struct Widget { w: i32; }\nfn compute() -> i32 { return 1; }";
    let (_dir, uri) = fixture("complete-top", source);
    client.did_open(&uri, source, 1);

    let response = completion_request(&mut client, &uri, json!({ "line": 0, "character": 0 }));
    let labels = completion_labels(&response);
    assert!(
        labels.iter().any(|l| l == "fn"),
        "a keyword item: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "forall"),
        "a non-det keyword item"
    );
    assert!(labels.iter().any(|l| l == "Widget"), "the struct");
    assert!(labels.iter().any(|l| l == "compute"), "the function");

    client.shutdown_exit_ok();
}

#[test]
fn completion_after_dot_offers_members_only() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "struct P { x: i32; fn get(self) -> i32 { return self.x; } }\n\
fn m(p: P) -> i32 { return p.; }";
    let (_dir, uri) = fixture("complete-dot", source);
    // The incomplete `p.` produces a syntax diagnostic; consume it.
    client.did_open(&uri, source, 1);

    let response = completion_request(&mut client, &uri, pos_after(source, "p."));
    let labels = completion_labels(&response);
    assert!(labels.iter().any(|l| l == "x"), "the field: {labels:?}");
    assert!(labels.iter().any(|l| l == "get"), "the method: {labels:?}");
    assert!(
        !labels.iter().any(|l| l == "m"),
        "an unrelated top-level fn is excluded: {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l == "fn"),
        "keywords are excluded after a dot: {labels:?}"
    );

    client.shutdown_exit_ok();
}

#[test]
fn completion_after_a_module_qualifier_offers_bare_pub_defs() {
    // The `::` trigger context: after `lib::`, the target module's public defs are
    // offered by their bare name (the form that compiles there), while a private
    // def and the general keyword list are not (issue #246).
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("complete-qualified");
    let entry_source = "use lib;\nfn main() -> i32 { return lib::; }";
    let lib_source = "pub fn helper() -> i32 { return 7; }\nfn secret() -> i32 { return 1; }";
    let entry_path = dir.write("main.inf", entry_source);
    dir.write("lib.inf", lib_source);
    let entry_uri = path_to_uri(&entry_path);

    // The incomplete `lib::` produces a syntax diagnostic; consume it.
    client.did_open(&entry_uri, entry_source, 1);

    let response = completion_request(&mut client, &entry_uri, pos_after(entry_source, "lib::"));
    let labels = completion_labels(&response);
    assert!(
        labels.iter().any(|l| l == "helper"),
        "the module's pub def is offered bare: {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l == "secret"),
        "a private def is not offered after `::`: {labels:?}"
    );
    assert!(
        !labels.iter().any(|l| l == "fn"),
        "keywords are wrong after `::`: {labels:?}"
    );

    client.shutdown_exit_ok();
}

// --- 13. inlay hints on a non-det file --------------------------------------

const NONDET_SOURCE: &str = "fn f() {\n\
    forall { let a: i32 = @; assert(a == a); }\n\
    exists { let b: i32 = @; assert(b == b); }\n\
    unique { assert(true); }\n\
    assume { assert(true); }\n\
}";

#[test]
fn inlay_hints_annotate_every_nondet_construct() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("inlay", NONDET_SOURCE);
    client.did_open(&uri, NONDET_SOURCE, 1);

    let response = client.request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": uri },
            "range": { "start": { "line": 0, "character": 0 }, "end": pos_end(NONDET_SOURCE) },
        }),
    );
    let hints = response["result"].as_array().expect("an inlay-hint array");
    assert_eq!(hints.len(), 6, "four blocks plus two uzumaki: {hints:?}");
    assert!(
        hints.iter().all(|h| h["kind"] == json!(INLAY_KIND_TYPE)),
        "both kinds map to the Type inlay kind"
    );

    // The four block hints, each at the end of its header keyword.
    for (keyword, label) in [
        ("forall", "\u{25B8} every path must succeed"),
        ("exists", "\u{25B8} at least one path must succeed"),
        ("unique", "\u{25B8} exactly one path must succeed"),
        ("assume", "\u{25B8} keeps only paths where this holds"),
    ] {
        let hint = hints
            .iter()
            .find(|h| h["label"] == json!(label))
            .unwrap_or_else(|| panic!("a hint labelled {label:?}"));
        assert_eq!(
            hint["position"],
            pos_after(NONDET_SOURCE, keyword),
            "{keyword} hint sits at the header end"
        );
    }

    // Both `@` bindings, typed `i32`.
    let uzumaki = hints
        .iter()
        .filter(|h| h["label"] == json!("\u{25B8} ranges over every value of its type (i32)"))
        .count();
    assert_eq!(uzumaki, 2, "one uzumaki hint per `@`");

    client.shutdown_exit_ok();
}

// --- 14. UTF-16 positions ----------------------------------------------------

#[test]
fn utf16_positions_resolve_past_a_multibyte_string_literal() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The emoji is two astral characters (four UTF-16 units, eight UTF-8 bytes)
    // before `count` on the same line, so a byte-vs-UTF-16 confusion would land
    // the position on the wrong token.
    let source =
        "fn f() -> i32 { let s: i32 = \"\u{1F600}\u{1F680}\"; let count: i32 = 5; return count; }";
    let (_dir, uri) = fixture("utf16", source);
    client.did_open(&uri, source, 1);

    // Hover at the *use* of `count` resolves to its type.
    let hover = hover_request(&mut client, &uri, pos_at_nth(source, "count", 1));
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|v| v.contains("count: i32")),
        "hover resolved across the emoji, got {}",
        hover["result"]["contents"]["value"]
    );

    // Definition at the same position reaches the `let count` binding name.
    let definition = definition_request(&mut client, &uri, pos_at_nth(source, "count", 1));
    assert_eq!(definition["result"]["uri"], json!(uri));
    assert_eq!(
        definition["result"]["range"]["start"],
        pos_at(source, "count"),
        "definition reaches the binding name"
    );

    client.shutdown_exit_ok();
}

// --- 15. didClose clears diagnostics ----------------------------------------

#[test]
fn did_close_clears_the_documents_diagnostics() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("close", "fn f() -> i32 { return x; }");
    let opened = client.did_open(&uri, "fn f() -> i32 { return x; }", 1);
    assert!(
        !opened.diagnostics.is_empty(),
        "the broken doc reports errors"
    );

    let closed = client.did_close(&uri);
    assert!(closed.diagnostics.is_empty(), "close clears diagnostics");
    assert_eq!(
        closed.version,
        Value::Null,
        "a cleared publish carries no version"
    );

    client.shutdown_exit_ok();
}

// --- 16. robustness ---------------------------------------------------------

#[test]
fn unknown_method_errors_and_the_server_stays_alive() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    let source = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("robust-unknown", source);
    client.did_open(&uri, source, 1);

    let response = client.request("textDocument/rename", json!({ "unsupported": true }));
    assert_eq!(
        response["error"]["code"],
        json!(METHOD_NOT_FOUND),
        "an unknown method is MethodNotFound"
    );

    // The server still answers a well-formed request afterwards.
    let hover = hover_request(&mut client, &uri, pos_at(source, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server is still responsive"
    );

    client.shutdown_exit_ok();
}

#[test]
fn malformed_params_error_and_the_server_stays_alive() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    let source = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("robust-malformed", source);
    client.did_open(&uri, source, 1);

    // A hover request whose params are not a `HoverParams` (no position).
    let bad = client.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(
        bad["error"]["code"],
        json!(INVALID_PARAMS),
        "missing position is InvalidParams"
    );

    // A well-formed hover still succeeds.
    let good = hover_request(&mut client, &uri, pos_at(source, "f("));
    assert!(good.get("error").is_none(), "the server recovered");

    client.shutdown_exit_ok();
}

#[test]
fn non_file_uri_is_ignored_without_crashing() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    let source = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("robust-untitled", source);
    client.did_open(&uri, source, 1);

    // An untitled buffer is not a file the server can analyze; it publishes
    // nothing and must not crash. Send it raw, then confirm no publish names it —
    // asserting the *absence* of a publish for the untitled URI, the pattern the
    // sibling query/fragment test uses, not merely that the server survived.
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "untitled:Untitled-1",
                "languageId": "inference",
                "version": 1,
                "text": "fn g() {}"
            }
        }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        !published
            .iter()
            .any(|(published_uri, _)| published_uri.contains("Untitled-1")),
        "a non-file URI is not analyzed or published, got {published:?}"
    );

    // A subsequent request against the real document still works.
    let hover = hover_request(&mut client, &uri, pos_at(source, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server survived the untitled open"
    );

    client.shutdown_exit_ok();
}

// --- 17. shutdown / exit ----------------------------------------------------

#[test]
fn shutdown_then_exit_exits_cleanly() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    // Open nothing; just run the shutdown/exit handshake.
    client.shutdown_exit_ok();
}

#[test]
fn exit_without_shutdown_exits_zero_on_this_server() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The LSP spec asks a server to exit with code 1 when `exit` arrives with no
    // preceding `shutdown`. This server does not track that: lsp-server's reader
    // stops after forwarding `exit`, the message loop then ends, and `main`
    // returns `Ok`, so the process exits 0 in both cases. Asserting the actual
    // behavior documents the deviation rather than hiding it.
    client.exit();
    let status = client.wait_for_exit();
    assert_eq!(
        status.code(),
        Some(0),
        "this server exits 0 even without a prior shutdown (spec would be 1)"
    );
}

// --- 18. stdout carries only framed protocol --------------------------------

#[test]
fn a_full_session_writes_only_framed_protocol_to_stdout() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // Exercise every request and notification kind in one session; the harness
    // reader parses the whole stdout stream as framed messages and records any
    // byte that is not, so a stray `println!` or half-written frame would make
    // `wait_for_exit` fail. This asserts the property by construction.
    let source = "struct P { x: i32; fn get(self) -> i32 { return self.x; } }\n\
fn use_it(p: P) -> i32 { forall { let n: i32 = @; assert(n == n); } return p.get(); }";
    let (_dir, uri) = fixture("framing", source);
    client.did_open(&uri, source, 1);

    hover_request(&mut client, &uri, pos_at(source, "get"));
    definition_request(&mut client, &uri, pos_at_nth(source, "get", 1));
    completion_request(&mut client, &uri, json!({ "line": 0, "character": 0 }));
    client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    client.request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": uri },
            "range": { "start": { "line": 0, "character": 0 }, "end": pos_end(source) },
        }),
    );
    client.did_change(&uri, "fn f() -> i32 { return 1; }", 2);
    client.did_close(&uri);

    // `shutdown_exit_ok` calls `wait_for_exit`, which asserts no framing violation
    // and a clean end of stream — i.e. every stdout byte was a valid framed message.
    client.shutdown_exit_ok();
}

// --- 19. editing an imported file republishes dependent documents -----------

#[test]
fn editing_an_imported_file_republishes_open_dependents() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("stale-cross-file");
    let main_source = "use lib;\nfn main() -> i32 { return lib::helper(); }";
    let lib_ok = "pub fn helper() -> i32 { return 7; }";
    let lib_broken = "pub fn other() -> i32 { return 8; }";
    let main_path = dir.write("main.inf", main_source);
    let lib_path = dir.write("lib.inf", lib_ok);
    let main_uri = path_to_uri(&main_path);
    let lib_uri = path_to_uri(&lib_path);

    // Both documents open clean.
    let opened_main = client.did_open(&main_uri, main_source, 1);
    assert!(
        opened_main.diagnostics.is_empty(),
        "main opens clean, got {:?}",
        opened_main.diagnostics
    );
    let opened_lib = client.did_open(&lib_uri, lib_ok, 1);
    assert!(opened_lib.diagnostics.is_empty(), "lib opens clean");

    // Opening lib invalidated the still-open dependent main, which the loop
    // republishes deterministically once it goes idle (issue #247). Consume that
    // exact republish by waiting for main's publish — a protocol barrier keyed on
    // message ordering — rather than a fixed wall-clock drain: a straggling clean
    // republish under CI load can no longer be mistaken for the post-edit publish
    // asserted below, because it is drained here explicitly.
    let main_after_lib_open = client.wait_for_publish(&main_uri);
    assert!(
        main_after_lib_open.diagnostics.is_empty(),
        "main is still clean after lib opens, got {:?}",
        main_after_lib_open.diagnostics
    );

    // Break lib: main now calls a function that no longer exists. The change to
    // lib must produce a fresh publish for the dependent main, without touching
    // main itself — otherwise the editor keeps rendering main as clean. The
    // dependent's republish is the next publish naming main's URI; `wait_for_publish`
    // skips the eager lib publish (which names lib) and blocks until main arrives,
    // so no timer bounds the wait.
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": lib_uri, "version": 2 },
            "contentChanges": [ { "text": lib_broken } ],
        }),
    );
    let main_broken = client.wait_for_publish(&main_uri);
    assert!(
        !main_broken.diagnostics.is_empty(),
        "the dependent main.inf is republished with errors"
    );

    // Fix lib: main's now-stale errors must clear via another fresh republish.
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": lib_uri, "version": 3 },
            "contentChanges": [ { "text": lib_ok } ],
        }),
    );
    let main_fixed = client.wait_for_publish(&main_uri);
    assert!(
        main_fixed.diagnostics.is_empty(),
        "fixing lib clears main's stale errors, got {:?}",
        main_fixed.diagnostics
    );

    client.shutdown_exit_ok();
}

// --- 20. a request after shutdown is answered InvalidRequest ------------------

#[test]
fn request_after_shutdown_is_answered_invalid_request() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    client.shutdown();

    // Per the LSP spec, a request received between `shutdown` and `exit` is
    // answered with InvalidRequest (-32600) while the server keeps waiting for
    // `exit` — it must not tear the connection down with no response.
    let response = client.request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": "file:///inf-test/x.inf" },
            "position": { "line": 0, "character": 0 }
        }),
    );
    assert_eq!(
        response["error"]["code"],
        json!(-32600),
        "a post-shutdown request is InvalidRequest, got {response}"
    );

    // The server is still alive and exits cleanly on the exit notification.
    client.exit();
    let status = client.wait_for_exit();
    assert_eq!(
        status.code(),
        Some(0),
        "the server still exits cleanly after answering the post-shutdown request"
    );
}

// --- 21. a URI carrying a query or fragment is ignored, not analyzed ---------

#[test]
fn query_or_fragment_uri_is_ignored_without_crashing() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    let source = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("uri-query", source);
    client.did_open(&uri, source, 1);

    // A `file://` URI carrying a query is not a document this server can serve. It
    // must be ignored — never interned as the garbage path `main.inf?ver=1` and
    // published under it. Send it raw, then confirm no publish names it.
    let query_uri = format!("{uri}?ver=1");
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": query_uri,
                "languageId": "inference",
                "version": 1,
                "text": "fn g() {}"
            }
        }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        !published
            .iter()
            .any(|(published_uri, _)| published_uri.contains("?ver=1")),
        "a query-bearing URI is not analyzed or published, got {published:?}"
    );

    // A subsequent request against the real document still works.
    let hover = hover_request(&mut client, &uri, pos_at(source, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server survived the query-bearing open"
    );

    client.shutdown_exit_ok();
}

// --- 22. an analysis panic is contained, not fatal to the session (#241) ------
//
// These three tests drive the server's debug-only analysis-panic seam, which is a
// no-op in release builds; they are compiled and run only under `debug_assertions`
// (the standard `cargo test` runs debug).

#[cfg(debug_assertions)]
#[test]
fn a_request_whose_analysis_panics_is_answered_internal_error() {
    let mut client = LspClient::spawn_with_env(&[(PANIC_ENV, PANIC_PATH_MARKER)]);
    client.initialize_default(true);

    // The panic file lives on disk but is never opened; a request against it reads
    // it from disk, analyzes, and unwinds (the armed seam fires on its path). The
    // message-loop boundary must turn that into a failed request carrying its own
    // id, not a dead process.
    let (_dir, panic_uri) = panic_fixture("panic-request");
    let healthy = "fn f() -> i32 { return 1; }";
    let (_healthy_dir, healthy_uri) = fixture("panic-request-healthy", healthy);
    client.did_open(&healthy_uri, healthy, 1);

    let response = hover_request(&mut client, &panic_uri, pos_at(PANIC_DOC_SOURCE, "main"));
    assert_eq!(
        response["error"]["code"],
        json!(INTERNAL_ERROR),
        "a request whose analysis panics is answered InternalError, got {response}"
    );

    // The server still answers a well-formed request against a healthy document.
    let hover = hover_request(&mut client, &healthy_uri, pos_at(healthy, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server stays responsive after containing the panic, got {hover}"
    );

    client.shutdown_exit_ok();
}

#[cfg(debug_assertions)]
#[test]
fn a_didopen_whose_diagnostics_panic_does_not_kill_the_server() {
    let mut client = LspClient::spawn_with_env(&[(PANIC_ENV, PANIC_PATH_MARKER)]);
    client.initialize_default(true);

    let healthy = "fn f() -> i32 { return 1; }";
    let (_healthy_dir, healthy_uri) = fixture("panic-didopen-healthy", healthy);
    client.did_open(&healthy_uri, healthy, 1);

    // Opening the panic file computes its diagnostics on the loop thread, which
    // unwinds (the armed seam fires on its path). The notification boundary contains
    // it: nothing is published for the file (so we must not wait on a publish that
    // never comes), and the session lives on. Sent raw for that reason.
    let (_dir, panic_uri) = panic_fixture("panic-didopen");
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": panic_uri,
                "languageId": "inference",
                "version": 1,
                "text": PANIC_DOC_SOURCE,
            }
        }),
    );

    // The healthy document is still served after the panicking open.
    let hover = hover_request(&mut client, &healthy_uri, pos_at(healthy, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server survived the panicking didOpen and still answers, got {hover}"
    );

    client.shutdown_exit_ok();
}

#[cfg(debug_assertions)]
#[test]
fn repeated_didopen_of_a_panicking_document_never_kills_the_server() {
    let mut client = LspClient::spawn_with_env(&[(PANIC_ENV, PANIC_PATH_MARKER)]);
    client.initialize_default(true);

    let healthy = "fn f() -> i32 { return 1; }";
    let (_healthy_dir, healthy_uri) = fixture("panic-loop-healthy", healthy);
    client.did_open(&healthy_uri, healthy, 1);

    // The amplification the issue describes: a client that crashes and auto-restarts
    // re-sends didOpen for the same bad file. Each didOpen unwinds during diagnostics;
    // every one must be contained, and the healthy document stay answerable across all
    // of them — otherwise one bad file becomes a permanent LSP outage.
    let (_dir, panic_uri) = panic_fixture("panic-loop");
    for version in 1..=5 {
        client.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": panic_uri,
                    "languageId": "inference",
                    "version": version,
                    "text": PANIC_DOC_SOURCE,
                }
            }),
        );
        let hover = hover_request(&mut client, &healthy_uri, pos_at(healthy, "f("));
        assert!(
            hover.get("error").is_none(),
            "the server is still alive after panicking didOpen #{version}, got {hover}"
        );
    }

    client.shutdown_exit_ok();
}

#[test]
fn named_constant_array_size_publishes_a_diagnostic_over_the_wire() {
    // The #240 fix seen end-to-end: the source that used to `todo!`-panic the
    // analysis (a named constant as an array size) now type-checks into an ordinary
    // diagnostic. The server is spawned *without* the panic seam armed, so opening
    // the file must publish a normal diagnostic instead of crashing the session.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "const N: i32 = 3;\n\
fn main() -> i32 { let arr: [i32; N] = [1, 2, 3]; return arr[0]; }";
    let (_dir, uri) = fixture("const-array-size", source);
    let published = client.did_open(&uri, source, 1);
    assert!(
        published
            .diagnostics
            .iter()
            .filter_map(|d| d["message"].as_str())
            .any(|message| message.contains("array size must be an integer literal")),
        "the named-constant array size is published as a diagnostic, got {:?}",
        published.diagnostics
    );

    client.shutdown_exit_ok();
}

// --- 23. a second shutdown is answered InvalidRequest (#249 item 1) -----------

#[test]
fn a_second_shutdown_is_answered_invalid_request() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The first shutdown succeeds with a null result.
    client.shutdown();

    // A second shutdown is a request received after `shutdown`, so the spec says it
    // errors with InvalidRequest rather than being answered a second null success.
    let response = client.request("shutdown", Value::Null);
    assert_eq!(
        response["error"]["code"],
        json!(INVALID_REQUEST),
        "a repeated shutdown is InvalidRequest, got {response}"
    );

    // The server still exits cleanly on `exit`.
    client.exit();
    let status = client.wait_for_exit();
    assert_eq!(
        status.code(),
        Some(0),
        "the server exits cleanly after rejecting the second shutdown"
    );
}

// --- 24. malformed initialize params fail the initialize request (#249 item 2) -

#[test]
fn malformed_initialize_params_fail_the_initialize_request() {
    let mut client = LspClient::spawn();

    // `processId` is declared `Option<u32>`; a fractional value cannot deserialize.
    // The failure must fail the initialize *request* with an error response, not
    // deserialize after the handshake and abort the process with no answer.
    let response = client.initialize_raw(json!({
        "processId": 1.5,
        "rootUri": Value::Null,
        "capabilities": {},
    }));
    assert_eq!(
        response["error"]["code"],
        json!(INVALID_PARAMS),
        "malformed initialize params fail the initialize request, got {response}"
    );
    assert!(
        response.get("result").is_none(),
        "a failed initialize carries no result, got {response}"
    );

    // The server tears the session down cleanly once the client sends `exit`.
    client.exit();
    let status = client.wait_for_exit();
    assert!(
        status.success(),
        "the server exits cleanly after failing initialize, got {status:?}"
    );
}

// --- 25. a mid-session initialize is InvalidRequest, not MethodNotFound (item 6) -

#[test]
fn a_repeated_initialize_is_answered_invalid_request() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // `initialize` may be sent only once. A second one mid-session is a protocol
    // error answered InvalidRequest — not the misleading MethodNotFound that a
    // generic unknown-method fallthrough would report.
    let response = client.initialize_raw(json!({ "capabilities": {} }));
    assert_eq!(
        response["error"]["code"],
        json!(INVALID_REQUEST),
        "a repeated initialize is InvalidRequest, got {response}"
    );
    assert_ne!(
        response["error"]["code"],
        json!(METHOD_NOT_FOUND),
        "specifically not MethodNotFound"
    );

    client.shutdown_exit_ok();
}

// --- 26. a plaintext-only client gets plaintext hover (#249 item 3) -----------

#[test]
fn a_plaintext_only_client_receives_plaintext_hover() {
    let mut client = LspClient::spawn();
    // The client advertises only plaintext hover content — no markdown.
    client.initialize(json!({
        "textDocument": { "hover": { "contentFormat": ["plaintext"] } }
    }));

    let source = "fn f() -> i32 { let count: i32 = 5; return count; }";
    let (_dir, uri) = fixture("hover-plaintext", source);
    client.did_open(&uri, source, 1);

    let response = hover_request(&mut client, &uri, pos_at_nth(source, "count", 1));
    let contents = &response["result"]["contents"];
    assert_eq!(
        contents["kind"],
        json!("plaintext"),
        "a plaintext-only client gets plaintext hover, got {contents}"
    );
    let value = contents["value"].as_str().expect("a hover value");
    assert!(
        value.contains("count: i32"),
        "the type still renders, got {value:?}"
    );
    assert!(
        !value.contains('`'),
        "no backticks are rendered literally, got {value:?}"
    );

    client.shutdown_exit_ok();
}

// --- 27. an inlay range with an out-of-range end is clamped, not disabled (item 4) -

#[test]
fn inlay_hint_range_with_an_out_of_range_end_is_clamped_to_the_window() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("inlay-clamp", NONDET_SOURCE);
    client.did_open(&uri, NONDET_SOURCE, 1);

    // The window starts at the `exists` line (line 2) and ends far past EOF. An
    // out-of-range end must *clamp* to the file end, keeping the valid start — so
    // the window is honored and the `forall` hints on line 1 stay excluded. The
    // bug returned every hint by disabling the clip entirely.
    let response = client.request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 2, "character": 0 },
                "end": { "line": 999, "character": 0 },
            },
        }),
    );
    let hints = response["result"].as_array().expect("an inlay-hint array");

    assert!(
        hints
            .iter()
            .all(|h| h["position"]["line"].as_i64().is_some_and(|line| line >= 2)),
        "every returned hint is inside the clamped window (line >= 2), got {hints:?}"
    );
    assert!(
        !hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} every path must succeed")),
        "the forall hints on line 1 are excluded, got {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} at least one path must succeed")),
        "the exists hint inside the window is present, got {hints:?}"
    );
    // exists + unique + assume block hints, plus the exists `@` uzumaki hint.
    assert_eq!(hints.len(), 4, "only the windowed hints, got {hints:?}");

    client.shutdown_exit_ok();
}

// --- 28. didClose of an unmappable URI publishes nothing (#249 item 5) --------

#[test]
fn did_close_of_an_unmappable_uri_publishes_nothing() {
    let mut client = LspClient::spawn();
    client.initialize_default(true);
    let source = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("close-unmappable", source);
    client.did_open(&uri, source, 1);

    // Closing a URI this server cannot map to a file must publish nothing — no
    // empty diagnostics set under the garbage URI, and no dependents sweep that
    // would republish the real open document. Send it raw and confirm no publish
    // names it (the assert-no-publish pattern).
    client.send_notification(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": "untitled:Untitled-1" } }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        !published
            .iter()
            .any(|(published_uri, _)| published_uri.contains("Untitled-1")),
        "an unmappable close publishes nothing under its URI, got {published:?}"
    );
    assert!(
        published.is_empty(),
        "an unmappable close triggers no dependents republish either, got {published:?}"
    );

    // A subsequent request against the real document still works.
    let hover = hover_request(&mut client, &uri, pos_at(source, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server survived the unmappable close"
    );

    client.shutdown_exit_ok();
}

// --- 29. a typing burst is coalesced over the real stdio transport (#247 item 1) -

#[test]
fn a_typing_burst_is_coalesced_into_far_fewer_recomputes() {
    // Regression for the coalescer being a no-op over the production transport.
    // lsp-server's stdio uses a zero-capacity rendezvous channel, so before the
    // transport pump a burst never accumulated where the coalescer could see it —
    // every keystroke recomputed and published once (a burst of N produced exactly N
    // publishes). Sending a dense burst of raw didChanges — without waiting for a
    // publish between them — lets a backlog build in the pump's buffer while the
    // server is still analyzing an earlier change, so the rest collapse to their
    // final text. The burst must therefore yield strictly fewer publishes than the
    // number of changes sent.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // A chunky source so a debug-build recompute comfortably outlasts the microseconds
    // it takes to write and buffer the next frame, guaranteeing a backlog forms.
    let mut base = String::new();
    for i in 0..60 {
        base.push_str(&format!("fn f{i}() -> i32 {{ return {i}; }}\n"));
    }
    let (_dir, uri) = fixture("coalesce-burst", &base);
    client.did_open(&uri, &base, 1);

    // A dense burst of full-text changes, each still clean, sent raw so they queue
    // behind the first recompute rather than being drained one at a time.
    const BURST: i64 = 25;
    for version in 2..=BURST + 1 {
        let clean = format!("{base}fn extra{version}() -> i32 {{ return {version}; }}\n");
        client.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [ { "text": clean } ],
            }),
        );
    }
    // The final change (highest version) breaks the file, so the definitive last state
    // is distinguishable from every clean intermediate — proving the coalescer keeps
    // the final text, not a stale earlier one.
    let final_version = BURST + 2;
    let broken = format!("{base}fn broken() -> i32 {{ return undeclared_x; }}\n");
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": final_version },
            "contentChanges": [ { "text": broken } ],
        }),
    );

    let publishes = client.drain_publishes(Duration::from_secs(3));
    let for_uri: Vec<_> = publishes.iter().filter(|(u, _)| u == &uri).collect();
    let changes_sent = final_version - 1; // versions 2..=final_version

    assert!(!for_uri.is_empty(), "the burst must publish at least once");
    assert!(
        (for_uri.len() as i64) < changes_sent,
        "a coalesced burst publishes fewer than the {changes_sent} changes sent, got {} \
         (before the transport pump this was exactly {changes_sent})",
        for_uri.len()
    );
    // The last publish for the file carries the final broken text's diagnostic.
    let last = for_uri.last().expect("at least one publish");
    assert!(
        !last.1.is_empty(),
        "the last publish reflects the final broken text, got {:?}",
        last.1
    );

    client.shutdown_exit_ok();
}

// --- 30. requests after didClose (issue #254) -------------------------------

#[test]
fn requests_after_did_close_fall_back_to_disk_content() {
    // VS Code fires hover/definition on a preview-tab close race: a request can
    // arrive just after didClose. A document backed by a file on disk must answer
    // against the *disk* text once its overlay is dropped. Disk and buffer diverge
    // here (different function names) so the fallback is unambiguous.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("request-after-close");
    let disk_src = "fn disk_fn() -> i32 { let val: i32 = 5; return val; }";
    let overlay_src = "fn overlay_fn() -> i32 { let ov: i32 = 9; return ov; }";
    let path = dir.write("main.inf", disk_src);
    let uri = path_to_uri(&path);

    // While open, documentSymbol reflects the overlay text.
    client.did_open(&uri, overlay_src, 1);
    let open_symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(
        open_symbols["result"][0]["name"],
        json!("overlay_fn"),
        "while open, symbols come from the overlay, got {open_symbols}"
    );

    client.did_close(&uri);

    // documentSymbol now reflects the disk text (the overlay is gone).
    let closed_symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = closed_symbols["result"]
        .as_array()
        .expect("a symbol array from disk");
    assert_eq!(symbols.len(), 1, "one disk symbol, got {symbols:?}");
    assert_eq!(
        symbols[0]["name"],
        json!("disk_fn"),
        "symbols fall back to the disk text after close"
    );

    // hover resolves against the disk text: the buffer's `ov` is gone, but `val`
    // exists on disk.
    let hover = hover_request(&mut client, &uri, pos_at_nth(disk_src, "val", 1));
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|v| v.contains("val: i32")),
        "hover after close resolves against disk content, got {}",
        hover["result"]["contents"]["value"]
    );

    // definition against the disk text reaches the disk binding.
    let def = definition_request(&mut client, &uri, pos_at_nth(disk_src, "val", 1));
    assert_eq!(def["result"]["uri"], json!(uri), "same-file disk target");
    assert_eq!(
        def["result"]["range"]["start"],
        pos_at(disk_src, "val"),
        "definition reaches the disk binding name"
    );

    client.shutdown_exit_ok();
}

#[test]
fn requests_after_did_close_of_a_never_on_disk_document_answer_null() {
    // The other half: a buffer that never existed on disk (a scratch file, or one
    // deleted while open). After didClose there is neither an overlay nor a disk
    // file, so a stale request must answer null — no crash, no fabricated result —
    // and the server stays alive.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("request-after-close-null");
    let source = "fn scratch() -> i32 { let val: i32 = 1; return val; }";
    // A path under the temp dir that is never written to disk.
    let path = dir.path.join("ghost.inf");
    let uri = path_to_uri(&path);

    client.did_open(&uri, source, 1);
    client.did_close(&uri);

    let position = pos_at_nth(source, "val", 1);
    let hover = hover_request(&mut client, &uri, position.clone());
    assert_eq!(
        hover["result"],
        Value::Null,
        "hover after close of a never-on-disk doc is null, got {hover}"
    );
    let def = definition_request(&mut client, &uri, position);
    assert_eq!(
        def["result"],
        Value::Null,
        "definition after close of a never-on-disk doc is null, got {def}"
    );
    let symbols = client.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert_eq!(
        symbols["result"],
        Value::Null,
        "documentSymbol after close of a never-on-disk doc is null, got {symbols}"
    );

    // A subsequent request against a real document still works, proving the server
    // stayed alive through the stale requests.
    let live_source = "fn g() -> i32 { return 1; }";
    let (_dir2, live_uri) = fixture("after-close-null-live", live_source);
    client.did_open(&live_uri, live_source, 1);
    let live = hover_request(&mut client, &live_uri, pos_at(live_source, "g("));
    assert!(
        live.get("error").is_none(),
        "the server survived the stale post-close requests"
    );

    client.shutdown_exit_ok();
}

// --- 31. didChange before didOpen is dropped, not adopted (#275) -------------

#[test]
fn did_change_before_did_open_is_dropped_then_a_later_did_open_is_adopted() {
    // A `didChange` for a URI the client never sent `didOpen` for is a protocol
    // violation (LSP 3.17 sends `didChange` only for an open document). The server
    // drops it — no interning, no tracking, no publish — rather than silently
    // adopting a never-opened document and enrolling it in future dependents
    // republishes (#275), matching the URI layer's treat-unmappable-input-as-absent
    // philosophy. A later proper `didOpen` of the same URI is then adopted normally.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // The file exists on disk with clean text; the stray change carries broken
    // text. Were the change adopted, its broken overlay would publish a diagnostic
    // under this URI — the absence of any such publish is the assertion.
    let disk_src = "fn f() -> i32 { return 1; }";
    let (_dir, uri) = fixture("change-before-open", disk_src);
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 3 },
            "contentChanges": [ { "text": "fn f() -> i32 { return missing_x; }" } ],
        }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        !published
            .iter()
            .any(|(published_uri, _)| published_uri == &uri),
        "a didChange for a never-opened document publishes nothing, got {published:?}"
    );

    // The server stays alive and answers a request against the untracked URI: with
    // no overlay installed it reads the clean disk text, so hover over `f` resolves
    // without error — the dropped change was never applied.
    let hover = hover_request(&mut client, &uri, pos_at(disk_src, "f("));
    assert!(
        hover.get("error").is_none(),
        "the server survived the dropped change and still answers, got {hover}"
    );

    // A later proper `didOpen` of the same URI is adopted normally: it publishes a
    // fresh diagnostic set for the opened (broken) text and echoes its version,
    // completely unaffected by the earlier dropped change.
    let opened = client.did_open(&uri, "fn f() -> i32 { return also_missing; }", 5);
    assert_eq!(
        opened.version,
        json!(5),
        "the later open's version is echoed back"
    );
    assert!(
        !opened.diagnostics.is_empty(),
        "the later didOpen is adopted and its broken text analyzed, got {:?}",
        opened.diagnostics
    );

    client.shutdown_exit_ok();
}

#[test]
fn a_did_change_after_did_close_is_dropped_and_leaves_a_dependent_untouched() {
    // The close-race half of #275: after `didClose` the document leaves the tracked
    // set, so a late `didChange` (VS Code can emit one on a preview-tab close race)
    // is the same protocol violation and is dropped — it does not resurrect
    // tracking, publishes nothing, and leaves a still-open dependent untouched.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("change-after-close");
    let main_source = "use lib;\nfn main() -> i32 { return lib::helper(); }";
    let lib_source = "pub fn helper() -> i32 { return 7; }";
    let main_path = dir.write("main.inf", main_source);
    let lib_path = dir.write("lib.inf", lib_source);
    let main_uri = path_to_uri(&main_path);
    let lib_uri = path_to_uri(&lib_path);

    // Both open clean. Opening lib invalidates the open dependent main, whose
    // deferred republish is consumed here so a later drain sees a clean slate.
    let opened_main = client.did_open(&main_uri, main_source, 1);
    assert!(opened_main.diagnostics.is_empty(), "main opens clean");
    let opened_lib = client.did_open(&lib_uri, lib_source, 1);
    assert!(opened_lib.diagnostics.is_empty(), "lib opens clean");
    let main_after_lib_open = client.wait_for_publish(&main_uri);
    assert!(
        main_after_lib_open.diagnostics.is_empty(),
        "main stays clean after lib opens, got {:?}",
        main_after_lib_open.diagnostics
    );

    // Close lib: its overlay drops and main is republished from the on-disk lib
    // (still clean). Consume that republish before probing the dropped change.
    let closed = client.did_close(&lib_uri);
    assert!(
        closed.diagnostics.is_empty(),
        "closing lib clears its diagnostics"
    );
    let main_after_lib_close = client.wait_for_publish(&main_uri);
    assert!(
        main_after_lib_close.diagnostics.is_empty(),
        "main re-reads lib from disk and stays clean after close, got {:?}",
        main_after_lib_close.diagnostics
    );

    // A late `didChange` for the now-closed lib, carrying text that would break
    // main's `lib::helper()` call, is dropped: nothing is published — not for lib,
    // not for the still-open dependent main.
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": lib_uri, "version": 2 },
            "contentChanges": [ { "text": "pub fn other() -> i32 { return 8; }" } ],
        }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        published.is_empty(),
        "a didChange after close publishes nothing at all, got {published:?}"
    );

    // The still-open main is untouched: it answers goto and still resolves the
    // cross-file call into the on-disk lib, proving the dropped change never
    // installed a broken lib overlay.
    let response = definition_request(&mut client, &main_uri, pos_at(main_source, "helper"));
    assert_eq!(
        response["result"]["uri"],
        json!(lib_uri),
        "main still resolves helper into the on-disk lib, got {response}"
    );

    client.shutdown_exit_ok();
}

#[test]
fn a_dropped_did_change_does_not_perturb_an_unrelated_open_document() {
    // A dropped `didChange` for a never-opened URI must be a complete no-op for
    // every other document: it publishes nothing and leaves an unrelated open
    // document's tracking and analysis intact (#275).
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    // An unrelated document, opened clean and tracked.
    let bystander_src = "fn a() -> i32 { return 1; }";
    let (_dir_a, bystander_uri) = fixture("dropped-change-bystander", bystander_src);
    let opened = client.did_open(&bystander_uri, bystander_src, 1);
    assert!(opened.diagnostics.is_empty(), "the bystander opens clean");

    // A stray change for a different, never-opened URI is dropped.
    let (_dir_b, ghost_uri) = fixture("dropped-change-ghost", "fn b() -> i32 { return 2; }");
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": ghost_uri, "version": 9 },
            "contentChanges": [ { "text": "fn b() -> i32 { return missing; }" } ],
        }),
    );
    let published = client.drain_publishes(Duration::from_secs(1));
    assert!(
        published.is_empty(),
        "a dropped change publishes nothing for anyone, got {published:?}"
    );

    // The bystander is still tracked and analyzed against its own overlay: a real
    // change to it publishes fresh, correct diagnostics and advances its version.
    let changed = client.did_change(&bystander_uri, "fn a() -> i32 { return still_missing; }", 2);
    assert_eq!(
        changed.version,
        json!(2),
        "the bystander's version advances"
    );
    assert!(
        !changed.diagnostics.is_empty(),
        "the bystander still analyzes its overlay after the unrelated dropped change, got {:?}",
        changed.diagnostics
    );

    client.shutdown_exit_ok();
}

// --- 32. percent-encoded URIs round-trip end to end (#254) -------------------

#[test]
fn percent_encoded_uris_round_trip_through_a_space_and_non_ascii_directory() {
    // Every fixture path in the suite is otherwise plain ASCII, so the full
    // percent-encode/decode round-trip is never exercised end to end. Here the
    // project lives in a directory whose name has a space AND a non-ASCII
    // character, so the URIs carry `%20` and `%C3%AF`. Three URIs must round-trip:
    // the didOpen target the client sends, the publishDiagnostics URI the server
    // echoes, and a cross-file goto target URI the server emits from a path.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let dir = TempDir::new("uri-encoding");
    let entry_source = "use lib;\nfn main() -> i32 { return lib::helper(); }";
    let lib_source = "pub fn helper() -> i32 { return 7; }";
    // A subdirectory with a space and `ï`, shared by both files.
    let entry_path = dir.write("na\u{ef}ve dir/main.inf", entry_source);
    let lib_path = dir.write("na\u{ef}ve dir/lib.inf", lib_source);
    let entry_uri = path_to_uri(&entry_path);
    let lib_uri = path_to_uri(&lib_path);

    // The encoded URI really does carry the escapes, so the round-trip is genuine.
    assert!(
        entry_uri.contains("%20") && entry_uri.contains("%C3%AF"),
        "the entry URI is percent-encoded, got {entry_uri}"
    );

    // didOpen with the percent-encoded URI resolves the on-disk import cleanly (the
    // server decodes the URI to a path, resolves `use lib;` in the same directory),
    // and its publishDiagnostics echoes the same encoded URI back — `did_open`
    // waits for a publish whose `uri` equals `entry_uri` exactly.
    let published = client.did_open(&entry_uri, entry_source, 1);
    assert!(
        published.diagnostics.is_empty(),
        "the import resolves under the encoded directory, got {:?}",
        published.diagnostics
    );

    // A cross-file goto: the server builds the lib target's URI from its path, which
    // must round-trip back to the same percent-encoded spelling the client uses.
    let response = definition_request(&mut client, &entry_uri, pos_at(entry_source, "helper"));
    let location = &response["result"];
    assert_eq!(
        location["uri"],
        json!(lib_uri),
        "the cross-file target URI round-trips through the encoded directory, got {location}"
    );
    assert_eq!(
        location["range"]["start"],
        pos_at(lib_source, "helper"),
        "the target range is the name in the lib file"
    );

    client.shutdown_exit_ok();
}

// --- 33. inlayHint honors a bounded sub-document range (#254) ----------------

#[test]
fn inlay_hint_honors_a_bounded_sub_document_range() {
    // The #249 clamp test pins only the start side of the window (its end is past
    // EOF, which clamps to the file end). This pins that the handler honors
    // `params.range.end` too: a window strictly inside the document must exclude
    // hints both before its start and at/after its end. A regression that ignored
    // the end would leak the later blocks' hints.
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let (_dir, uri) = fixture("inlay-subrange", NONDET_SOURCE);
    client.did_open(&uri, NONDET_SOURCE, 1);

    // Window: the `exists` line (line 2) up to the start of the `unique` line
    // (line 3). Only the two line-2 hints — the `exists` block hint and its `@`
    // uzumaki — fall in the half-open [start, end).
    let response = client.request(
        "textDocument/inlayHint",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 2, "character": 0 },
                "end": { "line": 3, "character": 0 },
            },
        }),
    );
    let hints = response["result"].as_array().expect("an inlay-hint array");

    assert!(
        hints.iter().all(|h| h["position"]["line"] == json!(2)),
        "every returned hint is on the windowed line 2, got {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} at least one path must succeed")),
        "the exists block hint inside the window is present, got {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} ranges over every value of its type (i32)")),
        "the exists-line uzumaki hint inside the window is present, got {hints:?}"
    );
    assert!(
        !hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} every path must succeed")),
        "the forall hint before the window start is excluded, got {hints:?}"
    );
    assert!(
        !hints
            .iter()
            .any(|h| h["label"] == json!("\u{25B8} exactly one path must succeed")),
        "the unique hint at/after the window end is excluded, got {hints:?}"
    );
    assert_eq!(hints.len(), 2, "only the two windowed line-2 hints, got {hints:?}");

    client.shutdown_exit_ok();
}

// --- 34. a position past EOF answers null at the wire level (#254) -----------

#[test]
fn a_position_past_the_last_line_answers_null_for_every_position_request() {
    // A stale position from rapid typing can name a line beyond the document. The
    // wire-level contract is a null result — never a crash, never a wrong token:
    // the offset conversion returns None for an out-of-range line, so hover,
    // definition, and completion each answer null. (A past-end *column* on a valid
    // line clamps instead; only a line past the last one is unmappable, which the
    // ide-layer unit tests cover but no e2e pinned.)
    let mut client = LspClient::spawn();
    client.initialize_default(true);

    let source = "fn f() -> i32 { let count: i32 = 5; return count; }";
    let (_dir, uri) = fixture("past-eof", source);
    client.did_open(&uri, source, 1);

    let far_past_eof = json!({ "line": 999, "character": 0 });

    let hover = hover_request(&mut client, &uri, far_past_eof.clone());
    assert_eq!(
        hover["result"],
        Value::Null,
        "hover past the last line is null, got {hover}"
    );
    let def = definition_request(&mut client, &uri, far_past_eof.clone());
    assert_eq!(
        def["result"],
        Value::Null,
        "definition past the last line is null, got {def}"
    );
    let completion = completion_request(&mut client, &uri, far_past_eof);
    assert_eq!(
        completion["result"],
        Value::Null,
        "completion past the last line is null, got {completion}"
    );

    // The server is still responsive to an in-range request.
    let good = hover_request(&mut client, &uri, pos_at_nth(source, "count", 1));
    assert!(
        good.get("error").is_none(),
        "the server survived the stale out-of-range positions"
    );

    client.shutdown_exit_ok();
}
