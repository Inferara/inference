//! A minimal Language Server Protocol test client.
//!
//! It spawns the built `inference-lsp` binary and speaks the LSP base protocol
//! (framed JSON-RPC) over its stdio. Every read is bounded by a hard timeout, so
//! a server that hangs makes the test *fail* rather than stall the run. Messages
//! are kept as raw [`serde_json::Value`]s and asserted with JSON pointer paths,
//! so the tests exercise the real wire format, not a typed re-encoding of it.
//!
//! [`LspClient::wait_for_response`] and [`LspClient::wait_for_notification`] read
//! past whatever arrives ahead of the message they want and retain it, in arrival
//! order, for the waits that follow — a session is free to interleave
//! notifications with the responses a test awaits. Each of those two bounds its
//! whole wait by a single deadline. The helpers layered on top do not inherit that
//! bound: [`LspClient::wait_for_publish`] loops until a publish matches its URI, so
//! it starts a fresh deadline per non-matching publish and *discards* rather than
//! retains them.
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
/// Generous because the suite runs its server spawns in parallel and shares the
/// machine with whatever else CI is compiling; a hang still fails, just later.
const RECV_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for the child process to exit before killing it and failing.
const EXIT_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// Messages read off the wire but not yet claimed by a wait.
    ///
    /// Invariant: always a subsequence of the arrival sequence, in arrival order.
    /// Every mutation preserves it — reads remove at an index, and appends only ever
    /// come from a wire read, which is by construction later than anything already
    /// here. Code that adds a buffer mutation must keep it.
    ///
    /// Only responses and notifications are ever claimed. A server-to-client
    /// *request* (both `id` and `method`) matches neither [`response_id`] nor
    /// [`notification_method`], so it would accumulate here unclaimed; the server
    /// sends none today.
    pending: Vec<Value>,
    next_id: i64,
    /// The budget each wait gets. Defaults to [`RECV_TIMEOUT`]; the harness's own
    /// tests shorten it so a wait that must give up does so quickly.
    recv_timeout: Duration,
    /// Set once a framing violation is observed; asserted absent at shutdown.
    framing_error: Option<String>,
}

impl LspClient {
    /// Spawns the compiled server binary with piped stdio and starts the reader.
    #[must_use]
    pub fn spawn() -> Self {
        Self::spawn_with_env(&[])
    }

    /// Spawns the server with extra environment variables set on the child process.
    ///
    /// Used by the panic-boundary tests to arm the debug-only analysis-panic seam
    /// (`INFERENCE_LSP_TEST_PANIC_PATH_SUBSTR`) in the spawned server, since the
    /// server runs out of process and cannot be armed in-process.
    #[must_use]
    pub fn spawn_with_env(env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_inference-lsp"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, value) in env {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("spawn inference-lsp");

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
            recv_timeout: RECV_TIMEOUT,
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
    /// prematurely closed / corrupt stream. Already-buffered messages come first.
    pub fn recv_message(&mut self) -> Value {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }
        let deadline = Instant::now() + self.recv_timeout;
        self.recv_from_wire(deadline, "a message")
    }

    /// Reads the next message straight off the wire, ignoring [`Self::pending`] and
    /// giving up at `deadline`.
    ///
    /// The one place a wait turns a reader-thread item into a test failure, so a
    /// clean EOF mid-session, non-protocol bytes, a silent server and a dead reader
    /// all fail the same loud way wherever a wait observes them. `awaited` names what
    /// the caller wanted, so an expiry says which wait gave up. The two drains
    /// ([`Self::drain_publishes`], [`Self::drain_until_eof`]) interpret the same items
    /// separately and end quietly by design — they are collectors, not waits.
    fn recv_from_wire(&mut self, deadline: Instant, awaited: &str) -> Value {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match self.incoming.recv_timeout(remaining) {
            Ok(Incoming::Message(value)) => value,
            Ok(Incoming::Eof) => {
                panic!("server closed its stdout while waiting for {awaited}")
            }
            Ok(Incoming::Framing(error)) => {
                self.framing_error = Some(error.clone());
                panic!("server wrote non-protocol bytes to stdout: {error}");
            }
            Err(RecvTimeoutError::Timeout) => {
                let budget = self.recv_timeout;
                panic!("timed out after {budget:?} waiting for {awaited}")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("the reader thread ended unexpectedly"),
        }
    }

    /// Waits for the response to `id`, retaining every other message in
    /// [`Self::pending`] — in arrival order — so a later `wait_for_notification`,
    /// `wait_for_publish` or `drain_publishes` still finds it.
    ///
    /// The buffer is scanned once, up front; after that every read goes straight to
    /// the wire. Reading through [`Self::recv_message`] instead would hand back the
    /// very messages this loop just buffered and never reach the wire again. The
    /// whole wait — not each read within it — is bounded by [`Self::recv_timeout`],
    /// so a response that never comes fails the test loudly instead of stalling it.
    pub fn wait_for_response(&mut self, id: i64) -> Value {
        if let Some(index) = self
            .pending
            .iter()
            .position(|message| response_id(message) == Some(id))
        {
            return self.pending.remove(index);
        }

        let deadline = Instant::now() + self.recv_timeout;
        let mut stepped_over = 0usize;
        loop {
            let message = self.recv_from_wire(
                deadline,
                &format!("the response to request {id} ({stepped_over} messages stepped over)"),
            );
            if response_id(&message) == Some(id) {
                return message;
            }
            self.pending.push(message);
            stepped_over += 1;
        }
    }

    /// Waits for the next notification with `method`, retaining everything else in
    /// arrival order. Buffer-then-wire and deadline behaviour mirror
    /// [`Self::wait_for_response`].
    pub fn wait_for_notification(&mut self, method: &str) -> Value {
        if let Some(index) = self
            .pending
            .iter()
            .position(|message| notification_method(message) == Some(method))
        {
            return self.pending.remove(index);
        }

        let deadline = Instant::now() + self.recv_timeout;
        let mut stepped_over = 0usize;
        loop {
            let message = self.recv_from_wire(
                deadline,
                &format!("a {method} notification ({stepped_over} messages stepped over)"),
            );
            if notification_method(&message) == Some(method) {
                return message;
            }
            self.pending.push(message);
            stepped_over += 1;
        }
    }

    // --- Convenience -------------------------------------------------------

    /// Sends a request and returns the whole response object (with `result` or
    /// `error`).
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send_request(method, params);
        self.wait_for_response(id)
    }

    /// Sends an `initialize` request with arbitrary params and returns the whole
    /// response (with `result` or `error`), asserting nothing and sending no
    /// `initialized` follow-up. Used to drive the paths where the handshake must
    /// itself fail (malformed params) or be rejected (a repeated initialize).
    pub fn initialize_raw(&mut self, params: Value) -> Value {
        self.request("initialize", params)
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

    /// Sends a `$/cancelRequest` for `id`. Fire-and-forget: the server answers the
    /// cancelled request itself (RequestCanceled), so nothing is awaited here.
    pub fn cancel(&mut self, id: i64) {
        self.send_notification("$/cancelRequest", json!({ "id": id }));
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

/// Contract tests for the waiting primitives themselves.
///
/// A wait for one message must read past whatever stands ahead of it — already
/// buffered or still on the wire — retain that traffic for the waits that follow,
/// and give up at a deadline if its target never comes. A wait that re-took the
/// messages it had itself buffered would instead spin in place: the shape that once
/// made a busy test client look like a hung server.
///
/// Determinism comes from two sources, never from a sleep. Traffic the server would
/// not send is seeded straight into the buffer. Traffic it does send is ordered by
/// the server's own contract — the worker handles jobs in arrival order and publishes
/// for a document write before it dequeues the next job, so a write queued ahead of a
/// request is always published ahead of that request's response.
///
/// A spinning wait never reaches a deadline to expire, so it would *hang* most of
/// these rather than fail them. [`tests::a_wait_makes_progress_instead_of_spinning`]
/// is the one that stays red instead of hanging: it runs the wait on its own thread
/// and bounds it from the test thread.
#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};

    use super::*;

    /// A short budget for the waits that are *meant* to expire, or that must finish
    /// without reading the wire at all. Long enough not to fire spuriously on a loaded
    /// machine, short enough that a regression fails in fractions of a second.
    const SHORT_TIMEOUT: Duration = Duration::from_millis(250);

    /// How long the test thread lets an off-thread wait run before calling it a spin.
    /// Only ever reached on failure — a healthy wait answers in milliseconds — but as
    /// generous as [`RECV_TIMEOUT`], and for the same reason: a loaded machine must
    /// make this slower, never red.
    const SPIN_WATCHDOG: Duration = RECV_TIMEOUT;

    /// The id of a seeded response. `next_id` starts at 0, so no real request is ever
    /// assigned this and no wait can mistake a seeded message for its target.
    const UNCLAIMED_ID: i64 = 9_999;

    const SOURCE: &str = "fn main() -> i32 { return 0; }";
    const SOURCE_V2: &str = "fn main() -> i32 { return 1; }";

    /// A `publishDiagnostics` notification the server never sent.
    fn seeded_publish(uri: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": uri, "diagnostics": [] },
        })
    }

    /// A response the server never sent, answering [`UNCLAIMED_ID`].
    fn seeded_response() -> Value {
        json!({ "jsonrpc": "2.0", "id": UNCLAIMED_ID, "result": Value::Null })
    }

    /// Handshake params for the tests that drive `initialize` by hand rather than
    /// through [`LspClient::initialize_default`], because they assert on the wait
    /// itself and so must send the request and await it in separate steps.
    fn initialize_params() -> Value {
        json!({
            "processId": Value::Null,
            "rootUri": Value::Null,
            "capabilities": {},
        })
    }

    /// Writes a valid single-file fixture and returns its directory and URI. The
    /// directory must outlive the session — it removes itself on drop.
    fn fixture(tag: &str) -> (TempDir, String) {
        let dir = TempDir::new(tag);
        let uri = path_to_uri(&dir.write("main.inf", SOURCE));
        (dir, uri)
    }

    /// Opens a document without awaiting its publish, so the publish is still in
    /// flight when the next message is sent.
    fn open_without_waiting(client: &mut LspClient, uri: &str, version: i64) {
        client.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "inference",
                    "version": version,
                    "text": SOURCE,
                }
            }),
        );
    }

    /// Rewrites a document without awaiting its publish.
    fn change_without_waiting(client: &mut LspClient, uri: &str, version: i64) {
        client.send_notification(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [ { "text": SOURCE_V2 } ],
            }),
        );
    }

    fn document_symbol(client: &mut LspClient, uri: &str) -> i64 {
        client.send_request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    /// The `params.uri` of each buffered message, for order assertions.
    fn buffered_uris(client: &LspClient) -> Vec<String> {
        client
            .pending
            .iter()
            .map(|message| {
                message["params"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    /// The message a caught panic carried, whichever payload shape it used.
    fn panic_text(payload: &(dyn std::any::Any + Send)) -> &str {
        payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("<panic payload was not a string>")
    }

    #[test]
    fn a_response_wait_steps_over_buffered_traffic_and_keeps_its_order() {
        let mut client = LspClient::spawn();
        client.pending.push(seeded_publish("file:///first.inf"));
        client.pending.push(seeded_publish("file:///second.inf"));

        let id = client.send_request("initialize", initialize_params());
        let response = client.wait_for_response(id);

        assert_eq!(
            response["id"],
            json!(id),
            "the awaited response arrives, got {response}"
        );
        assert!(
            response.get("result").is_some(),
            "initialize answers a result, got {response}"
        );
        assert_eq!(
            buffered_uris(&client),
            ["file:///first.inf", "file:///second.inf"],
            "both non-targets are retained, in their original order, got {:?}",
            client.pending
        );

        client.send_notification("initialized", json!({}));
        client.shutdown_exit_ok();
    }

    #[test]
    fn a_notification_wait_steps_over_a_buffered_response() {
        let mut client = LspClient::spawn();
        client.initialize_default(true);
        // Seeded after the handshake to keep this test about the notification wait; the
        // id could not collide with a real request's in any case.
        client.pending.push(seeded_response());

        let (_dir, uri) = fixture("harness-buffered-response");
        open_without_waiting(&mut client, &uri, 1);

        let published = client.wait_for_notification("textDocument/publishDiagnostics");

        assert_eq!(
            published["params"]["uri"],
            json!(uri),
            "the opened document's publish arrives, got {published}"
        );
        assert_eq!(
            client.pending.len(),
            1,
            "the non-target is retained exactly once, got {:?}",
            client.pending
        );
        assert_eq!(
            client.pending[0]["id"],
            json!(UNCLAIMED_ID),
            "and it is the buffered response, got {:?}",
            client.pending
        );

        client.shutdown_exit_ok();
    }

    #[test]
    fn a_response_wait_retains_a_publish_behind_what_was_already_buffered() {
        let mut client = LspClient::spawn();
        client.initialize_default(true);
        client.pending.push(seeded_publish("file:///buffered.inf"));

        let (_dir, uri) = fixture("harness-retain-publish");
        // A write queued ahead of the request, not awaited: the server publishes for it
        // before it dequeues the request, so the wait meets the publish first and has
        // to step over it to reach the response.
        open_without_waiting(&mut client, &uri, 1);
        let id = document_symbol(&mut client, &uri);

        let response = client.wait_for_response(id);
        assert!(
            response.get("error").is_none(),
            "documentSymbol is answered from behind the publish, got {response}"
        );
        assert_eq!(
            buffered_uris(&client),
            ["file:///buffered.inf", uri.as_str()],
            "the publish is retained behind the message already buffered, got {:?}",
            client.pending
        );

        // And a later wait still finds what this one retained.
        let published = client.wait_for_publish(&uri);
        assert!(
            published.diagnostics.is_empty(),
            "a valid document publishes no diagnostics, got {:?}",
            published.diagnostics
        );

        client.shutdown_exit_ok();
    }

    #[test]
    fn a_notification_wait_retains_the_response_that_precedes_it() {
        let mut client = LspClient::spawn();
        client.initialize_default(true);

        let (_dir, uri) = fixture("harness-retain-response");
        client.did_open(&uri, SOURCE, 1);

        // The request is queued ahead of the write, so its response reaches the wire
        // first and the publish wait has to step over it.
        let id = document_symbol(&mut client, &uri);
        change_without_waiting(&mut client, &uri, 2);

        let published = client.wait_for_publish(&uri);
        assert_eq!(
            published.version,
            json!(2),
            "the write's publish arrives, got {:?}",
            published.version
        );
        assert_eq!(
            client.pending.len(),
            1,
            "the response it stepped over is retained, got {:?}",
            client.pending
        );
        assert_eq!(
            client.pending[0]["id"],
            json!(id),
            "and it is the documentSymbol response, got {:?}",
            client.pending
        );

        client.shutdown_exit_ok();
    }

    #[test]
    fn an_already_buffered_target_is_taken_without_reading_the_wire() {
        // Nothing is ever sent to this server, so it answers nothing: any read that
        // reached the wire would burn the whole budget and expire.
        let mut client = LspClient::spawn();
        client.recv_timeout = SHORT_TIMEOUT;
        client.pending.push(seeded_publish("file:///buffered.inf"));
        client.pending.push(seeded_response());

        let first = client.recv_message();
        assert_eq!(
            notification_method(&first),
            Some("textDocument/publishDiagnostics"),
            "the oldest buffered message comes back first, got {first}"
        );

        let response = client.wait_for_response(UNCLAIMED_ID);
        assert_eq!(
            response["id"],
            json!(UNCLAIMED_ID),
            "the buffered response is claimed by the up-front scan, got {response}"
        );
        assert!(
            client.pending.is_empty(),
            "and the buffer is drained, got {:?}",
            client.pending
        );
    }

    #[test]
    fn a_wait_makes_progress_instead_of_spinning() {
        let mut client = LspClient::spawn();
        client.pending.push(seeded_publish("file:///buffered.inf"));
        let id = client.send_request("initialize", initialize_params());

        // The wait runs off-thread so the test thread can outlive it. A wait that
        // re-took its own buffered message would never return and never expire; here
        // that shows up as a failed test rather than a suite that never finishes.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let response = client.wait_for_response(id);
            let _ = tx.send((response, client));
        });

        match rx.recv_timeout(SPIN_WATCHDOG) {
            Ok((response, mut client)) => {
                assert_eq!(
                    response["id"],
                    json!(id),
                    "the awaited response arrives, got {response}"
                );
                assert_eq!(
                    buffered_uris(&client),
                    ["file:///buffered.inf"],
                    "and the non-target is retained, got {:?}",
                    client.pending
                );
                client.send_notification("initialized", json!({}));
                client.shutdown_exit_ok();
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the wait ended without answering")
            }
            Err(RecvTimeoutError::Timeout) => panic!(
                "the wait neither answered nor gave up within {SPIN_WATCHDOG:?} — it spun on its own buffer"
            ),
        }
    }

    #[test]
    fn a_wait_that_never_gets_its_target_gives_up_loudly() {
        // A buffered non-target is the essential ingredient: a silent server expires
        // any implementation, but a wait that re-took its own buffer would never
        // consult the deadline at all.
        let mut client = LspClient::spawn();
        client.recv_timeout = SHORT_TIMEOUT;
        client.pending.push(seeded_publish("file:///buffered.inf"));

        // No request was ever sent, so nothing can answer this id.
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| client.wait_for_response(7)));

        let payload = outcome.expect_err("an unanswerable wait must give up");
        let message = panic_text(&*payload);
        assert!(
            message.contains("timed out"),
            "it gives up on the deadline, got {message:?}"
        );
        assert!(
            message.contains("the response to request 7"),
            "and names what it awaited, got {message:?}"
        );
    }
}
