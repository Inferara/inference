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
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

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

    // No position encoding is negotiated, so the client keeps the UTF-16 default;
    // lsp-server 0.8 attaches no serverInfo.
    assert!(capabilities.get("positionEncoding").is_none());
    assert!(result.get("serverInfo").is_none(), "no serverInfo declared");

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
    // nothing and must not crash. Send it raw (there is no publish to wait for).
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
    // Drain the republish that opening lib triggered for the still-clean main.
    client.drain_publishes(Duration::from_millis(500));

    // Break lib: main now calls a function that no longer exists. The change to
    // lib must produce a fresh publish for the dependent main, without touching
    // main itself — otherwise the editor keeps rendering main as clean.
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": lib_uri, "version": 2 },
            "contentChanges": [ { "text": lib_broken } ],
        }),
    );
    let after_break = client.drain_publishes(Duration::from_secs(5));
    let main_broken = after_break
        .iter()
        .find(|(uri, _)| uri == &main_uri)
        .unwrap_or_else(|| {
            panic!("main.inf was not republished after lib changed: {after_break:?}")
        });
    assert!(
        !main_broken.1.is_empty(),
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
    let after_fix = client.drain_publishes(Duration::from_secs(5));
    let main_fixed = after_fix
        .iter()
        .find(|(uri, _)| uri == &main_uri)
        .unwrap_or_else(|| {
            panic!("main.inf was not republished after lib was fixed: {after_fix:?}")
        });
    assert!(
        main_fixed.1.is_empty(),
        "fixing lib clears main's stale errors, got {:?}",
        main_fixed.1
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
