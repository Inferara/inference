//! A minimal Language Server Protocol test client.
//!
//! It spawns the built `inference-lsp` binary and speaks the LSP base protocol
//! (framed JSON-RPC) over its stdio. Every read is bounded by a hard timeout, so
//! a server that hangs makes the test *fail* rather than stall the run. Messages
//! are kept as raw [`serde_json::Value`]s and asserted with JSON pointer paths,
//! so the tests exercise the real wire format, not a typed re-encoding of it.
//!
//! A background reader thread parses the child's stdout into framed messages. If
//! any byte of that stream is not valid framing, the reader reports it as a
//! [`Incoming::Framing`] error instead of a message — which is what lets a test
//! assert the server never writes non-protocol bytes to stdout.

#![allow(dead_code)] // A shared test-support module; not every helper is used by every test.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long any single message read waits before the test gives up on the server.
const RECV_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for the child process to exit before killing it and failing.
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// One item the reader thread pulls off the child's stdout.
enum Incoming {
    /// A well-framed JSON-RPC message.
    Message(Value),
    /// The stream ended cleanly between messages.
    Eof,
    /// A stdout byte sequence that was not valid framed protocol. The server must
    /// never produce this; a test treats it as a failure.
    Framing(String),
}

/// A spawned `inference-lsp` process with a framed-JSON-RPC channel to it.
pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Incoming>,
    reader: Option<JoinHandle<()>>,
    /// Messages received while waiting for a different, specific one.
    pending: Vec<Value>,
    next_id: i64,
    /// Set once a framing violation is observed; asserted absent at shutdown.
    framing_error: Option<String>,
}

impl LspClient {
    /// Spawns the compiled server binary with piped stdio and starts the reader.
    #[must_use]
    pub fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_inference-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn inference-lsp");

        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || read_loop(stdout, &tx));

        LspClient {
            child,
            stdin,
            incoming: rx,
            reader: Some(reader),
            pending: Vec::new(),
            next_id: 0,
            framing_error: None,
        }
    }

    // --- Low-level protocol ------------------------------------------------

    /// Sends a request and returns the id it was assigned.
    pub fn send_request(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        id
    }

    /// Sends a notification (no id, no response expected).
    pub fn send_notification(&mut self, method: &str, params: Value) {
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Receives the next message in arrival order, failing on timeout or a
    /// prematurely closed / corrupt stream.
    pub fn recv_message(&mut self) -> Value {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        match self.incoming.recv_timeout(RECV_TIMEOUT) {
            Ok(Incoming::Message(value)) => value,
            Ok(Incoming::Eof) => panic!("server closed its stdout while a message was expected"),
            Ok(Incoming::Framing(error)) => {
                self.framing_error = Some(error.clone());
                panic!("server wrote non-protocol bytes to stdout: {error}");
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for a message from the server")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("the reader thread ended unexpectedly"),
        }
    }

    /// Waits for the response to `id`, buffering any notifications that arrive
    /// first so a later `wait_for_notification` can still find them.
    pub fn wait_for_response(&mut self, id: i64) -> Value {
        loop {
            if let Some(index) = self
                .pending
                .iter()
                .position(|message| response_id(message) == Some(id))
            {
                return self.pending.remove(index);
            }
            let message = self.recv_message();
            if response_id(&message) == Some(id) {
                return message;
            }
            self.pending.push(message);
        }
    }

    /// Waits for the next notification with `method`, buffering everything else.
    pub fn wait_for_notification(&mut self, method: &str) -> Value {
        loop {
            if let Some(index) = self
                .pending
                .iter()
                .position(|message| notification_method(message) == Some(method))
            {
                return self.pending.remove(index);
            }
            let message = self.recv_message();
            if notification_method(&message) == Some(method) {
                return message;
            }
            self.pending.push(message);
        }
    }

    // --- Convenience -------------------------------------------------------

    /// Sends a request and returns the whole response object (with `result` or
    /// `error`).
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send_request(method, params);
        self.wait_for_response(id)
    }

    /// Runs the initialize handshake with the given client capabilities and
    /// returns the `InitializeResult` value, then sends `initialized`.
    pub fn initialize(&mut self, capabilities: Value) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "processId": Value::Null,
                "rootUri": Value::Null,
                "capabilities": capabilities,
            }),
        );
        assert!(
            response.get("error").is_none(),
            "initialize failed: {response}"
        );
        let result = response
            .get("result")
            .cloned()
            .expect("initialize returns a result");
        self.send_notification("initialized", json!({}));
        result
    }

    /// The common case: initialize declaring (or not) hierarchical document-symbol
    /// support, returning the `InitializeResult`.
    pub fn initialize_default(&mut self, hierarchical_symbols: bool) -> Value {
        self.initialize(json!({
            "textDocument": {
                "documentSymbol": {
                    "hierarchicalDocumentSymbolSupport": hierarchical_symbols,
                }
            }
        }))
    }

    /// Opens a document and returns the diagnostics that its `publishDiagnostics`
    /// carries.
    pub fn did_open(&mut self, uri: &str, text: &str, version: i64) -> PublishedDiagnostics {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "inference",
                    "version": version,
                    "text": text,
                }
            }),
        );
        self.wait_for_publish(uri)
    }

    /// Replaces a document's whole text (full-sync) and returns the new
    /// diagnostics.
    pub fn did_change(&mut self, uri: &str, text: &str, version: i64) -> PublishedDiagnostics {
        self.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [ { "text": text } ],
            }),
        );
        self.wait_for_publish(uri)
    }

    /// Closes a document and returns the (expected empty) cleared diagnostics.
    pub fn did_close(&mut self, uri: &str) -> PublishedDiagnostics {
        self.send_notification(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
        self.wait_for_publish(uri)
    }

    /// Collects every `publishDiagnostics` that arrives within `window`, as
    /// `(uri, diagnostics)` pairs in arrival order.
    ///
    /// One document notification legitimately triggers several publishes (the
    /// notified document plus every other open one), so a test that observes the
    /// cross-document republish drains the whole burst rather than waiting on a
    /// single URI. Already-buffered publishes are taken first; non-publish
    /// messages stay buffered for later `wait_for_*` calls.
    pub fn drain_publishes(&mut self, window: Duration) -> Vec<(String, Vec<Value>)> {
        let mut collected = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            if notification_method(&self.pending[index]) == Some("textDocument/publishDiagnostics")
            {
                collected.push(publish_pair(&self.pending.remove(index)));
            } else {
                index += 1;
            }
        }

        let deadline = Instant::now() + window;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return collected;
            }
            match self.incoming.recv_timeout(remaining) {
                Ok(Incoming::Message(value)) => {
                    if notification_method(&value) == Some("textDocument/publishDiagnostics") {
                        collected.push(publish_pair(&value));
                    } else {
                        self.pending.push(value);
                    }
                }
                Ok(Incoming::Eof) => return collected,
                Ok(Incoming::Framing(error)) => {
                    self.framing_error = Some(error);
                    return collected;
                }
                Err(RecvTimeoutError::Timeout) => return collected,
                Err(RecvTimeoutError::Disconnected) => return collected,
            }
        }
    }

    /// Waits for a `publishDiagnostics` notification for `uri`.
    pub fn wait_for_publish(&mut self, uri: &str) -> PublishedDiagnostics {
        loop {
            let message = self.wait_for_notification("textDocument/publishDiagnostics");
            let params = &message["params"];
            if params["uri"] == json!(uri) {
                return PublishedDiagnostics {
                    version: params.get("version").cloned().unwrap_or(Value::Null),
                    diagnostics: params["diagnostics"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                };
            }
        }
    }

    // --- Lifecycle ---------------------------------------------------------

    /// Sends the shutdown request and asserts its result is JSON `null`.
    pub fn shutdown(&mut self) {
        let response = self.request("shutdown", Value::Null);
        assert!(
            response.get("error").is_none(),
            "shutdown returned an error: {response}"
        );
        assert_eq!(
            response.get("result"),
            Some(&Value::Null),
            "shutdown result must be null, got {response}"
        );
    }

    /// Sends the exit notification.
    pub fn exit(&mut self) {
        self.send_notification("exit", Value::Null);
    }

    /// Drains any trailing messages, asserts the stream ended cleanly with no
    /// framing violation, and returns the process exit status (killing and
    /// failing if it does not exit in time).
    pub fn wait_for_exit(&mut self) -> ExitStatus {
        self.drain_until_eof();
        assert!(
            self.framing_error.is_none(),
            "server wrote non-protocol bytes to stdout: {:?}",
            self.framing_error
        );

        let deadline = Instant::now() + EXIT_TIMEOUT;
        loop {
            match self.child.try_wait().expect("poll child") {
                Some(status) => return status,
                None if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    panic!("server did not exit within {EXIT_TIMEOUT:?}");
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    /// The full happy-path lifecycle tail: shutdown, exit, and assert exit code 0.
    pub fn shutdown_exit_ok(&mut self) {
        self.shutdown();
        self.exit();
        let status = self.wait_for_exit();
        assert!(
            status.success(),
            "clean shutdown must exit 0, got {status:?}"
        );
    }

    /// Reads until the stream ends, keeping notifications in `pending` and
    /// recording any framing violation. Used before waiting on process exit.
    fn drain_until_eof(&mut self) {
        loop {
            match self.incoming.recv_timeout(RECV_TIMEOUT) {
                Ok(Incoming::Message(value)) => self.pending.push(value),
                Ok(Incoming::Eof) => return,
                Ok(Incoming::Framing(error)) => {
                    self.framing_error = Some(error);
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {
                    panic!("server did not close its stdout within {RECV_TIMEOUT:?}")
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn write_message(&mut self, message: &Value) {
        let body = serde_json::to_vec(message).expect("serialize message");
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin
            .write_all(header.as_bytes())
            .expect("write header");
        self.stdin.write_all(&body).expect("write body");
        self.stdin.flush().expect("flush");
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // A panicking test must never leak a live server process.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// The payload of a `textDocument/publishDiagnostics` notification.
pub struct PublishedDiagnostics {
    pub version: Value,
    pub diagnostics: Vec<Value>,
}

impl PublishedDiagnostics {
    /// The first diagnostic whose `code` equals `code`, if any.
    #[must_use]
    pub fn by_code(&self, code: &str) -> Option<&Value> {
        self.diagnostics.iter().find(|d| d["code"] == json!(code))
    }
}

/// The reader thread: parse framed messages off `stdout` until EOF or a violation.
fn read_loop(stdout: std::process::ChildStdout, tx: &Sender<Incoming>) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_one(&mut reader) {
            Ok(Some(value)) => {
                if tx.send(Incoming::Message(value)).is_err() {
                    return;
                }
            }
            Ok(None) => {
                let _ = tx.send(Incoming::Eof);
                return;
            }
            Err(error) => {
                let _ = tx.send(Incoming::Framing(error));
                return;
            }
        }
    }
}

/// Reads one framed message: `Content-Length` headers, a blank line, then exactly
/// that many body bytes. `Ok(None)` is a clean end of stream between messages.
fn read_one(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if read == 0 {
            return if saw_header {
                Err("stream ended in the middle of a message header".to_owned())
            } else {
                Ok(None)
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        saw_header = true;
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| format!("malformed header line: {line:?}"))?;
        if key.trim().eq_ignore_ascii_case("content-length") {
            let parsed = value
                .trim()
                .parse()
                .map_err(|_| format!("invalid content-length: {value:?}"))?;
            content_length = Some(parsed);
        }
    }

    let length = content_length.ok_or("message had no content-length header")?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|e| format!("message body was not valid JSON: {e}"))
}

/// The request id a response message answers, if it is a response.
fn response_id(message: &Value) -> Option<i64> {
    if message.get("method").is_some() {
        return None; // A request/notification, not a response.
    }
    message.get("id").and_then(Value::as_i64)
}

/// The method of a notification (a message with a method but no id).
fn notification_method(message: &Value) -> Option<&str> {
    if message.get("id").is_some() {
        return None;
    }
    message.get("method").and_then(Value::as_str)
}

/// The `(uri, diagnostics)` a `publishDiagnostics` notification carries.
fn publish_pair(message: &Value) -> (String, Vec<Value>) {
    let params = &message["params"];
    let uri = params["uri"].as_str().unwrap_or_default().to_owned();
    let diagnostics = params["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    (uri, diagnostics)
}

// --- Position and URI helpers ----------------------------------------------

/// The 0-based line / UTF-16 character LSP position of `byte_offset` in `source`.
fn lsp_position(source: &str, byte_offset: usize) -> Value {
    let mut line = 0u32;
    let mut character = 0u32;
    for (index, ch) in source.char_indices() {
        if index >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    json!({ "line": line, "character": character })
}

/// The LSP position at the start of the first occurrence of `needle`.
#[must_use]
pub fn pos_at(source: &str, needle: &str) -> Value {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in source"));
    lsp_position(source, offset)
}

/// The LSP position at the start of the `n`-th (0-based) occurrence of `needle`.
#[must_use]
pub fn pos_at_nth(source: &str, needle: &str, n: usize) -> Value {
    let mut start = 0;
    for _ in 0..n {
        let found = source[start..]
            .find(needle)
            .unwrap_or_else(|| panic!("fewer than {} occurrences of {needle:?}", n + 1));
        start += found + needle.len();
    }
    let found = source[start..]
        .find(needle)
        .unwrap_or_else(|| panic!("fewer than {} occurrences of {needle:?}", n + 1));
    lsp_position(source, start + found)
}

/// The LSP position just past the end of the first occurrence of `needle`.
#[must_use]
pub fn pos_after(source: &str, needle: &str) -> Value {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in source"));
    lsp_position(source, offset + needle.len())
}

/// The LSP position at the very end of `source` (one past its last byte).
#[must_use]
pub fn pos_end(source: &str) -> Value {
    lsp_position(source, source.len())
}

/// A throwaway directory under the system temp dir, removed on drop. Fixtures for
/// the e2e tests live here — never at a filesystem root, never in the repo.
pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    #[must_use]
    pub fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inference-lsp-e2e-{tag}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        TempDir { path }
    }

    /// Writes `contents` to `<dir>/<relative>`, creating parents, and returns the
    /// absolute path.
    pub fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let dest = self.path.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent dir");
        }
        std::fs::write(&dest, contents).expect("write fixture");
        dest
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The `file://` URI naming `path`, mirroring the server's own URI encoding
/// (forward slashes, a leading slash before a drive letter, percent-encoded
/// non-path bytes) so a POSIX and a Windows host each round-trip through the same
/// spelling the server expects.
#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_str().expect("fixture path is valid UTF-8");
    let normalized = text.replace('\\', "/");
    let absolute = if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    };
    format!("file://{}", percent_encode(&absolute))
}

fn percent_encode(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for &byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}
