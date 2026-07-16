//! The server state and the single-threaded message loop.
//!
//! [`ServerState`] holds the analysis host and the set of open documents; it turns
//! one request into one [`Response`] and one notification into the diagnostics to
//! publish, with no I/O of its own — which is what makes it directly testable.
//! [`run`] owns the transport: it reads messages, handles the shutdown/exit
//! handshake inline, routes everything else through the state, and writes the
//! results back. Nothing here prints to stdout; that stream is the protocol
//! channel.

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;

use inference_ide::AnalysisHost;
use lsp_server::{
    Connection, ErrorCode, ExtractError, Message, Notification, Request, RequestId, Response,
};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, Initialize, InlayHintRequest,
    Request as _, Shutdown,
};
use lsp_types::{InitializeParams, MarkupKind, PublishDiagnosticsParams, Uri};
use rustc_hash::FxHashMap;

use crate::{capabilities, handlers};

/// The server name reported in the initialize result's `serverInfo` (the crate
/// name), which clients surface in their logs and crash reports.
const SERVER_NAME: &str = env!("CARGO_PKG_NAME");
/// The server version reported in the initialize result's `serverInfo` (the crate
/// version).
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A document the editor has opened, with the path the analysis host knows it by
/// and the last version the editor reported (echoed back in published
/// diagnostics so the client can correlate them).
pub(crate) struct Document {
    pub(crate) path: PathBuf,
    pub(crate) version: i32,
    /// The document's last-seen text. Retained so [`ServerState`] can rebuild the
    /// analysis host from the tracked documents after a contained panic, without
    /// re-reading anything from disk (the editor's overlay may never have been
    /// saved).
    pub(crate) text: Arc<str>,
}

/// The client capabilities this server negotiates once during initialize and then
/// consults while answering requests.
#[derive(Clone, Copy)]
pub(crate) struct NegotiatedCapabilities {
    /// The client accepts the hierarchical document-symbol response; when it does
    /// not, symbols are flattened to `SymbolInformation`.
    pub(crate) hierarchical_symbols: bool,
    /// The client accepts Markdown hover content; when it does not, hover contents
    /// are emitted as plain text (no code fences or backticks).
    pub(crate) hover_markdown: bool,
}

impl NegotiatedCapabilities {
    /// Reads the negotiated bits out of the client's `initialize` capabilities,
    /// defaulting each to the LSP fallback when the client is silent.
    fn from_init_params(init_params: &InitializeParams) -> Self {
        Self {
            hierarchical_symbols: hierarchical_symbol_support(init_params),
            hover_markdown: hover_markdown_support(init_params),
        }
    }

    /// The markup kind to render hover contents in for this client.
    pub(crate) fn hover_format(self) -> MarkupKind {
        if self.hover_markdown {
            MarkupKind::Markdown
        } else {
            MarkupKind::PlainText
        }
    }
}

/// The analysis host plus per-document bookkeeping. Feature queries and
/// diagnostics are answered against this; the transport is elsewhere.
pub(crate) struct ServerState {
    pub(crate) host: AnalysisHost,
    pub(crate) documents: FxHashMap<Uri, Document>,
    pub(crate) capabilities: NegotiatedCapabilities,
}

impl ServerState {
    pub(crate) fn new(capabilities: NegotiatedCapabilities) -> Self {
        Self {
            host: AnalysisHost::default(),
            documents: FxHashMap::default(),
            capabilities,
        }
    }

    /// Routes a request through [`handle_request`](Self::handle_request),
    /// containing any panic in the analysis stack.
    ///
    /// A `todo!`/`unwrap` deep in the type-checker or analysis passes (the class
    /// tracked in #240) unwinds; left unguarded it would tear down the whole
    /// session. Caught here, the offending request is answered with an
    /// `InternalError` carrying its original id — so the client can correlate the
    /// failure — and every other document keeps working. A stack overflow aborts
    /// the process on its own and cannot be caught; that is intentionally left to
    /// abort.
    pub(crate) fn handle_request_resilient(&mut self, request: Request) -> Response {
        let id = request.id.clone();
        catch(|| self.handle_request(request)).unwrap_or_else(|| panic_response(id))
    }

    /// Routes a request to its handler, producing the response to send back. An
    /// unknown method is a `MethodNotFound` error; malformed params are
    /// `InvalidParams`; neither disturbs the server.
    pub(crate) fn handle_request(&mut self, request: Request) -> Response {
        if request.method == HoverRequest::METHOD {
            self.dispatch::<HoverRequest>(request, handlers::hover)
        } else if request.method == GotoDefinition::METHOD {
            self.dispatch::<GotoDefinition>(request, handlers::goto_definition)
        } else if request.method == Completion::METHOD {
            self.dispatch::<Completion>(request, handlers::completion)
        } else if request.method == DocumentSymbolRequest::METHOD {
            self.dispatch::<DocumentSymbolRequest>(request, handlers::document_symbol)
        } else if request.method == InlayHintRequest::METHOD {
            self.dispatch::<InlayHintRequest>(request, handlers::inlay_hint)
        } else if request.method == Initialize::METHOD {
            // `initialize` is a once-per-session lifecycle request; the handshake
            // already consumed the only valid one, so a second is InvalidRequest —
            // not the misleading MethodNotFound the generic arm below would give.
            Response::new_err(
                request.id,
                ErrorCode::InvalidRequest as i32,
                "the server is already initialized".to_owned(),
            )
        } else {
            Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported request: {}", request.method),
            )
        }
    }

    /// Deserializes the request's params for `R`, runs `handler`, and wraps the
    /// result — turning a params-deserialization failure into `InvalidParams`.
    fn dispatch<R>(
        &mut self,
        request: Request,
        handler: fn(&mut ServerState, R::Params) -> R::Result,
    ) -> Response
    where
        R: lsp_types::request::Request,
    {
        let id = request.id.clone();
        match request.extract::<R::Params>(R::METHOD) {
            Ok((id, params)) => Response::new_ok(id, handler(self, params)),
            Err(ExtractError::JsonError { error, .. }) => {
                Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string())
            }
            Err(ExtractError::MethodMismatch(request)) => Response::new_err(
                id,
                ErrorCode::InvalidRequest as i32,
                format!(
                    "method mismatch: expected {}, got {}",
                    R::METHOD,
                    request.method
                ),
            ),
        }
    }

    /// Applies a document notification through
    /// [`on_notification`](Self::on_notification), containing any panic in the
    /// analysis stack.
    ///
    /// Diagnostics are computed on the message loop thread, so a panic while
    /// analyzing a just-opened or just-changed document would otherwise abort the
    /// session — and, because the client re-sends the same `didOpen` on restart,
    /// crash-loop until it gives up (#241). Caught here, the failed notification
    /// publishes nothing, and the analysis host — which the unwinding computation
    /// may have left with half-updated state — is rebuilt from the tracked open
    /// documents so later queries start from a clean, consistent host.
    pub(crate) fn on_notification_resilient(
        &mut self,
        notification: Notification,
    ) -> Vec<PublishDiagnosticsParams> {
        match catch(|| self.on_notification(notification)) {
            Some(publishes) => publishes,
            None => {
                self.rebuild_host();
                Vec::new()
            }
        }
    }

    /// Applies a document notification, returning the diagnostics to publish for
    /// the notified document *and* every other open document (see
    /// [`publishes_with_dependents`](Self::publishes_with_dependents)). An unknown
    /// or unparsable notification publishes nothing.
    pub(crate) fn on_notification(
        &mut self,
        notification: Notification,
    ) -> Vec<PublishDiagnosticsParams> {
        let primary = if notification.method == DidOpenTextDocument::METHOD {
            match notification.extract(DidOpenTextDocument::METHOD) {
                Ok(params) => handlers::did_open(self, params),
                Err(_) => return Vec::new(),
            }
        } else if notification.method == DidChangeTextDocument::METHOD {
            match notification.extract(DidChangeTextDocument::METHOD) {
                Ok(params) => handlers::did_change(self, params),
                Err(_) => return Vec::new(),
            }
        } else if notification.method == DidCloseTextDocument::METHOD {
            match notification.extract(DidCloseTextDocument::METHOD) {
                Ok(params) => handlers::did_close(self, params),
                Err(_) => return Vec::new(),
            }
        } else {
            return Vec::new();
        };
        self.publishes_with_dependents(primary)
    }

    /// Extends a notified document's `primary` publish with a fresh republish for
    /// every other open document.
    ///
    /// A change to one file can invalidate another open document whose import
    /// closure includes it — `ide-db` drops exactly those analyses — but the
    /// notified document is the only one the client is told about unless we
    /// republish the rest. Republishing every open document is the simple correct
    /// choice: an unaffected document's analysis is still memoized, so its
    /// republish recomputes nothing, and editors keep only a handful of files
    /// open, so the cost is bounded by that count, not the project size.
    fn publishes_with_dependents(
        &mut self,
        primary: Option<PublishDiagnosticsParams>,
    ) -> Vec<PublishDiagnosticsParams> {
        let Some(primary) = primary else {
            return Vec::new();
        };
        let dependents: Vec<Uri> = self
            .documents
            .keys()
            .filter(|uri| **uri != primary.uri)
            .cloned()
            .collect();
        let mut publishes = Vec::with_capacity(dependents.len() + 1);
        publishes.push(primary);
        for uri in dependents {
            publishes.push(handlers::publish_diagnostics_params(self, &uri));
        }
        publishes
    }

    /// Replaces the analysis host with a fresh one carrying only the tracked open
    /// documents' last-seen text.
    ///
    /// Called after a contained analysis panic: rather than reason about which
    /// cached state the unwinding computation may have corrupted, the host is
    /// discarded and rebuilt from the [`documents`](Self::documents) the server
    /// still considers open. The first query after this recomputes every analysis
    /// from scratch. Documents are the sole source of truth here, so anything the
    /// editor had not opened is simply gone — which is correct.
    fn rebuild_host(&mut self) {
        let mut host = AnalysisHost::default();
        for document in self.documents.values() {
            host.open_document(&document.path, Arc::clone(&document.text));
        }
        self.host = host;
    }
}

/// Runs the initialize handshake, answering the `initialize` request directly so a
/// malformed `InitializeParams` fails *that request* instead of aborting the
/// process after the handshake has already completed.
///
/// `lsp-server`'s convenience `Connection::initialize` returns the raw params and
/// hard-codes the result body, so the caller can only deserialize *after* the
/// handshake and cannot attach `serverInfo`. This drives `initialize_start` /
/// `initialize_finish` instead: the params are validated first, and only a valid
/// `initialize` completes the handshake — with `serverInfo` (name and version)
/// attached. A malformed one is answered with an `InvalidParams` error and the
/// session ends without ever entering the message loop.
///
/// Returns the negotiated params on success, or `None` when initialize failed (the
/// error response has already been sent, and the client is expected to `exit`).
///
/// # Errors
///
/// Returns an error if the handshake cannot be driven to completion — the
/// connection dropped, or a message could not be written.
pub fn initialize(connection: &Connection) -> anyhow::Result<Option<InitializeParams>> {
    let (id, params) = connection.initialize_start()?;
    let init_params: InitializeParams = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(error) => {
            send(
                connection,
                Message::Response(Response::new_err(
                    id,
                    ErrorCode::InvalidParams as i32,
                    format!("invalid initialize params: {error}"),
                )),
            )?;
            drain_until_exit(connection);
            return Ok(None);
        }
    };

    let result = serde_json::json!({
        "capabilities": capabilities::server_capabilities(),
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
    });
    connection.initialize_finish(id, result)?;
    Ok(Some(init_params))
}

/// Drains messages until the client sends `exit` or closes the connection.
///
/// Used after a failed `initialize`: it keeps the process alive long enough for
/// the error response to be written, and keeps a receiver on the channel so the
/// transport's reader thread is never left blocked forwarding into it.
fn drain_until_exit(connection: &Connection) {
    for message in &connection.receiver {
        if matches!(&message, Message::Notification(n) if n.method == Exit::METHOD) {
            return;
        }
    }
}

/// Runs the message loop until the client exits or the connection closes.
///
/// The shutdown handshake is handled inline rather than delegated to
/// `lsp-server`'s `Connection::handle_shutdown`, which consumes the next message
/// itself and turns anything but `exit` into a fatal protocol error. Instead, a
/// `shutdown` request is answered and flips a `shutting_down` flag; while it is
/// set, every further request — including a repeated `shutdown` — is answered with
/// `InvalidRequest` (the spec's behaviour for requests received between `shutdown`
/// and `exit`) and every notification but `exit` is ignored. The `exit`
/// notification ends the loop. Every other request is routed through
/// [`ServerState`], and document notifications may publish diagnostics.
///
/// The `shutting_down` guard precedes the `shutdown` arm so a *second* `shutdown`
/// is rejected like any other post-shutdown request rather than answered a second
/// `null` success.
///
/// Requests and notifications are dispatched through the resilient wrappers
/// ([`handle_request_resilient`](ServerState::handle_request_resilient),
/// [`on_notification_resilient`](ServerState::on_notification_resilient)), so an
/// unwinding panic in the analysis stack fails a single request or publish
/// instead of the whole session.
///
/// # Errors
///
/// Returns an error if a message cannot be written to the transport.
pub fn run(connection: &Connection, init_params: &InitializeParams) -> anyhow::Result<()> {
    let mut state = ServerState::new(NegotiatedCapabilities::from_init_params(init_params));
    let mut shutting_down = false;

    for message in &connection.receiver {
        match message {
            Message::Request(request) if shutting_down => {
                send(
                    connection,
                    Message::Response(Response::new_err(
                        request.id,
                        ErrorCode::InvalidRequest as i32,
                        "the server is shutting down".to_owned(),
                    )),
                )?;
            }
            Message::Request(request) if request.method == Shutdown::METHOD => {
                shutting_down = true;
                send(
                    connection,
                    Message::Response(Response::new_ok(request.id, ())),
                )?;
            }
            Message::Request(request) => {
                let response = state.handle_request_resilient(request);
                send(connection, Message::Response(response))?;
            }
            Message::Notification(notification) if notification.method == Exit::METHOD => {
                return Ok(());
            }
            // A stray notification after `shutdown` (other than `exit`) is dropped.
            Message::Notification(_) if shutting_down => {}
            Message::Notification(notification) => {
                for params in state.on_notification_resilient(notification) {
                    let published =
                        Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
                    send(connection, Message::Notification(published))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Whether the client advertised support for the hierarchical document-symbol
/// response; absent capabilities mean the flat form.
fn hierarchical_symbol_support(init_params: &InitializeParams) -> bool {
    init_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.document_symbol.as_ref())
        .and_then(|document_symbol| document_symbol.hierarchical_document_symbol_support)
        .unwrap_or(false)
}

/// Whether the client accepts Markdown hover content.
///
/// The client's `textDocument.hover.contentFormat` lists the formats it supports,
/// most-preferred first. A client that lists formats but omits Markdown wants
/// plain text; a client that advertises no `contentFormat` at all keeps the
/// historical Markdown default.
fn hover_markdown_support(init_params: &InitializeParams) -> bool {
    init_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.hover.as_ref())
        .and_then(|hover| hover.content_format.as_ref())
        .is_none_or(|formats| formats.contains(&MarkupKind::Markdown))
}

fn send(connection: &Connection, message: Message) -> anyhow::Result<()> {
    connection
        .sender
        .send(message)
        .map_err(|error| anyhow::anyhow!("failed to send message: {error}"))
}

/// Runs `f`, containing an unwinding panic and returning `None` in its place.
///
/// The process-wide panic hook still runs first, so the panic's message and
/// backtrace are written to stderr as usual — only the unwind is swallowed, and
/// only stderr (never stdout, the protocol channel) is touched. `f` borrows the
/// server state mutably, which is not `UnwindSafe`; asserting it is safe is sound
/// here because the sole recovery — [`ServerState::rebuild_host`] — discards the
/// state a panic could have left inconsistent rather than reading it back.
fn catch<R>(f: impl FnOnce() -> R) -> Option<R> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).ok()
}

/// The response to a request whose handler panicked: the analysis stack unwound
/// and was contained, so this request fails but the session lives on. The
/// original request id is echoed so the client can match the failure to its call.
fn panic_response(id: RequestId) -> Response {
    Response::new_err(
        id,
        ErrorCode::InternalError as i32,
        "the request handler panicked and was contained; the server is still running".to_owned(),
    )
}

/// The environment variable that arms the analysis-panic test seam out of process.
///
/// The panic-boundary tests (#241) need a document whose analysis deterministically
/// unwinds. Rather than couple that guarantee to a specific compiler bug, the seam
/// is deliberate and self-contained: any document whose path contains the substring
/// named here panics when it is analyzed. The out-of-process e2e harness sets this
/// on the server it spawns; the in-process unit tests use the thread-local arm
/// ([`arm_analysis_panic`]) instead, which does not race across parallel tests.
#[cfg(debug_assertions)]
const TEST_PANIC_ENV: &str = "INFERENCE_LSP_TEST_PANIC_PATH_SUBSTR";

#[cfg(debug_assertions)]
thread_local! {
    /// In-process arm for [`analysis_panic_seam`]. `None` unless a unit test set it;
    /// see [`TEST_PANIC_ENV`] for the out-of-process arm.
    static ANALYSIS_PANIC_SUBSTR: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Test-only seam (compiled only in debug builds) that forces a deterministic panic
/// while analyzing a marked document, so the message-loop panic boundary (#241) can
/// be exercised without depending on a specific compiler bug as its trigger.
///
/// A document panics when its path contains the armed substring — set either by the
/// [`TEST_PANIC_ENV`] environment variable (the out-of-process e2e harness) or by
/// the thread-local [`arm_analysis_panic`] (in-process unit tests). Every other
/// document is analyzed normally. In release builds this compiles to nothing.
#[cfg(debug_assertions)]
pub(crate) fn analysis_panic_seam(path: &std::path::Path) {
    let display = path.to_string_lossy();
    let matches = |substr: &str| !substr.is_empty() && display.contains(substr);
    let armed_in_process = ANALYSIS_PANIC_SUBSTR.with(|cell| {
        cell.borrow().as_deref().is_some_and(matches)
    });
    let armed_by_env = std::env::var(TEST_PANIC_ENV).is_ok_and(|substr| matches(&substr));
    assert!(
        !(armed_in_process || armed_by_env),
        "deliberate LSP analysis panic for {display}: exercising the #241 panic boundary"
    );
}

/// Release builds carry no test seam; the call sites optimize away entirely.
#[cfg(not(debug_assertions))]
#[inline]
pub(crate) fn analysis_panic_seam(_path: &std::path::Path) {}

/// Arms [`analysis_panic_seam`] in the current thread for documents whose path
/// contains `substr`, disarming when the returned guard drops. Used by the unit
/// tests, where an environment variable would race across the parallel test
/// threads sharing this process. Gated on `debug_assertions` alongside the seam it
/// arms, so a release test build (where the seam is a no-op) neither references the
/// absent thread-local nor compiles a dead arming helper.
#[cfg(all(test, debug_assertions))]
pub(crate) fn arm_analysis_panic(substr: &str) -> AnalysisPanicArm {
    ANALYSIS_PANIC_SUBSTR.with(|cell| *cell.borrow_mut() = Some(substr.to_owned()));
    AnalysisPanicArm
}

/// Drop guard returned by [`arm_analysis_panic`]; clears the thread-local arm.
#[cfg(all(test, debug_assertions))]
pub(crate) struct AnalysisPanicArm;

#[cfg(all(test, debug_assertions))]
impl Drop for AnalysisPanicArm {
    fn drop(&mut self) {
        ANALYSIS_PANIC_SUBSTR.with(|cell| *cell.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    // `Arc` and the `Document` constructor are only reached through `track`, which is
    // debug-only; gating the imports keeps a release test build warning-free.
    #[cfg(debug_assertions)]
    use std::sync::Arc;

    use lsp_server::{Request, RequestId, Response};
    use lsp_types::notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    };
    use lsp_types::request::{HoverRequest, Initialize, Request as _};
    use lsp_types::Uri;

    use super::{NegotiatedCapabilities, ServerState};
    #[cfg(debug_assertions)]
    use super::{arm_analysis_panic, Document};

    /// A client that supports everything this server negotiates. The individual
    /// tests here do not depend on the negotiated bits (those are exercised by the
    /// `convert`/`handlers` and e2e tests), so they share one fully-capable client.
    fn full_client() -> NegotiatedCapabilities {
        NegotiatedCapabilities {
            hierarchical_symbols: true,
            hover_markdown: true,
        }
    }

    /// The path substring the panic-boundary tests arm the analysis-panic seam with;
    /// only the documents deliberately named with it (`.../panic.inf`) unwind, while
    /// the healthy siblings analyze normally.
    #[cfg(debug_assertions)]
    const PANIC_MARKER: &str = "panic";

    /// A well-formed document used by the panic-boundary tests. Its analysis panics
    /// not because of its contents — it type-checks cleanly — but because its path
    /// carries [`PANIC_MARKER`] and the armed [`super::analysis_panic_seam`] forces
    /// a deterministic unwind for it (see #241 for the boundary it exercises).
    #[cfg(debug_assertions)]
    const PANIC_DOC_SOURCE: &str = "fn main() -> i32 { return 0; }";

    fn open(state: &mut ServerState, uri: &str, text: &str) {
        state.on_notification(did_open_notification(uri, text));
    }

    fn did_open_notification(uri: &str, text: &str) -> lsp_server::Notification {
        let params = serde_json::json!({
            "textDocument": { "uri": uri, "languageId": "inference", "version": 1, "text": text }
        });
        lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params)
    }

    /// Installs `text` as the overlay for `uri` and tracks the document, without
    /// computing diagnostics — so a document whose *analysis* panics can be staged
    /// for a later query without the staging itself unwinding. Only the debug-only
    /// panic-boundary test uses it.
    #[cfg(debug_assertions)]
    fn track(state: &mut ServerState, uri: &str, text: &str) {
        let uri = Uri::from_str(uri).expect("a valid uri");
        let path = crate::uri::to_path(&uri).expect("a file uri");
        let text: Arc<str> = text.into();
        state.host.open_document(&path, Arc::clone(&text));
        state.documents.insert(
            uri,
            Document {
                path,
                version: 1,
                text,
            },
        );
    }

    fn diagnostics_for(state: &mut ServerState, uri: &str) -> Vec<lsp_types::Diagnostic> {
        let uri = Uri::from_str(uri).expect("a valid uri");
        crate::handlers::publish_diagnostics_params(state, &uri).diagnostics
    }

    #[cfg(debug_assertions)]
    fn hover_request(id: i32, uri: &str, line: u32, character: u32) -> Request {
        Request::new(
            RequestId::from(id),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
    }

    fn error_code(response: &Response) -> i32 {
        response.error.as_ref().expect("an error response").code
    }

    #[test]
    fn unknown_request_is_method_not_found() {
        let mut state = ServerState::new(full_client());
        let request = Request::new(
            RequestId::from(1),
            "textDocument/rename".to_owned(),
            serde_json::json!({}),
        );
        let response = state.handle_request(request);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::MethodNotFound as i32
        );
    }

    #[test]
    fn a_repeated_initialize_request_is_invalid_request() {
        // The handshake consumed the one valid `initialize`; a second one arriving
        // mid-session is a protocol error (InvalidRequest), not the misleading
        // MethodNotFound the generic unknown-method arm would report.
        let mut state = ServerState::new(full_client());
        let request = Request::new(
            RequestId::from(1),
            Initialize::METHOD.to_owned(),
            serde_json::json!({ "capabilities": {} }),
        );
        let response = state.handle_request(request);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InvalidRequest as i32,
            "a second initialize is InvalidRequest"
        );
    }

    #[test]
    fn malformed_params_are_invalid_params_and_leave_the_server_usable() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );

        // A hover request whose params are not a `HoverParams`.
        let bad = Request::new(
            RequestId::from(2),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({ "unexpected": true }),
        );
        let response = state.handle_request(bad);
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InvalidParams as i32
        );

        // The server still answers a well-formed request afterwards.
        let good = Request::new(
            RequestId::from(3),
            HoverRequest::METHOD.to_owned(),
            serde_json::json!({
                "textDocument": { "uri": "file:///inf-test/main.inf" },
                "position": { "line": 0, "character": 3 }
            }),
        );
        let response = state.handle_request(good);
        assert!(response.error.is_none(), "a valid request still succeeds");
        assert!(
            response.result.is_some_and(|result| !result.is_null()),
            "hover over the function name returns a result"
        );
    }

    #[test]
    fn did_open_broken_source_publishes_a_diagnostic() {
        let mut state = ServerState::new(full_client());
        let params = serde_json::json!({
            "textDocument": {
                "uri": "file:///inf-test/broken.inf",
                "languageId": "inference",
                "version": 7,
                "text": "fn f() -> i32 { return x; }"
            }
        });
        let notification =
            lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params);
        let mut publishes = state.on_notification(notification);
        assert_eq!(publishes.len(), 1, "the only open document publishes once");
        let published = publishes.remove(0);
        assert_eq!(published.uri.as_str(), "file:///inf-test/broken.inf");
        assert_eq!(published.version, Some(7));
        assert!(
            !published.diagnostics.is_empty(),
            "an undeclared variable is reported"
        );
    }

    #[test]
    fn a_notification_republishes_every_open_document() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/a.inf",
            "fn a() -> i32 { return 1; }",
        );
        open(
            &mut state,
            "file:///inf-test/b.inf",
            "fn b() -> i32 { return 2; }",
        );

        // A change to a.inf publishes for a.inf *and* republishes b.inf, so a
        // client can never keep stale diagnostics on a document a cross-file edit
        // invalidated.
        let params = serde_json::json!({
            "textDocument": { "uri": "file:///inf-test/a.inf", "version": 2 },
            "contentChanges": [ { "text": "fn a() -> i32 { return 11; }" } ]
        });
        let notification =
            lsp_server::Notification::new(DidChangeTextDocument::METHOD.to_owned(), params);
        let publishes = state.on_notification(notification);
        let uris: Vec<&str> = publishes.iter().map(|p| p.uri.as_str()).collect();
        assert!(
            uris.contains(&"file:///inf-test/a.inf"),
            "the changed document"
        );
        assert!(
            uris.contains(&"file:///inf-test/b.inf"),
            "the other open document is republished too, got {uris:?}"
        );
    }

    #[test]
    fn did_close_publishes_an_empty_set() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );
        let params = serde_json::json!({ "textDocument": { "uri": "file:///inf-test/main.inf" } });
        let notification =
            lsp_server::Notification::new(DidCloseTextDocument::METHOD.to_owned(), params);
        let mut publishes = state.on_notification(notification);
        assert_eq!(
            publishes.len(),
            1,
            "closing the only document clears it once"
        );
        let published = publishes.remove(0);
        assert!(published.diagnostics.is_empty());
        assert_eq!(published.version, None);
    }

    #[test]
    fn did_close_of_an_unmappable_uri_publishes_nothing() {
        // A URI this server cannot map to a file was never opened or tracked, so
        // closing it must publish nothing — not an empty diagnostics set under a
        // garbage URI, and no dependents republish — mirroring `did_open`, which
        // already ignores such URIs. An open document is present to prove the sweep
        // over other documents is *not* triggered by the unmappable close.
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/main.inf",
            "fn f() -> i32 { return 1; }",
        );

        let params = serde_json::json!({ "textDocument": { "uri": "untitled:Untitled-1" } });
        let notification =
            lsp_server::Notification::new(DidCloseTextDocument::METHOD.to_owned(), params);
        assert!(
            state.on_notification(notification).is_empty(),
            "closing an unmappable URI publishes nothing at all"
        );
    }

    #[test]
    fn non_file_uri_is_ignored_on_open() {
        let mut state = ServerState::new(full_client());
        let params = serde_json::json!({
            "textDocument": {
                "uri": "untitled:Untitled-1",
                "languageId": "inference",
                "version": 1,
                "text": "fn f() {}"
            }
        });
        let notification =
            lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params);
        assert!(
            state.on_notification(notification).is_empty(),
            "an untitled buffer is not analyzed and publishes nothing"
        );
    }

    #[test]
    fn rebuild_host_reconstructs_tracked_documents() {
        let mut state = ServerState::new(full_client());
        open(
            &mut state,
            "file:///inf-test/clean.inf",
            "fn f() -> i32 { return 1; }",
        );
        open(
            &mut state,
            "file:///inf-test/broken.inf",
            "fn g() -> i32 { return x; }",
        );

        state.rebuild_host();

        // The rebuilt host carries each tracked document's last-seen text, so the
        // clean file is still clean and the broken one still reports its undeclared
        // variable — proving the text, not just the path, survives the rebuild.
        assert!(
            diagnostics_for(&mut state, "file:///inf-test/clean.inf").is_empty(),
            "the clean document stays clean after a rebuild"
        );
        assert!(
            !diagnostics_for(&mut state, "file:///inf-test/broken.inf").is_empty(),
            "the broken document still reports after a rebuild"
        );
    }

    // Gated on `debug_assertions`: the analysis-panic seam these two tests rely on is
    // a no-op in release builds, so the tests are compiled and run only where it is
    // active (the standard `cargo test` runs debug).
    #[cfg(debug_assertions)]
    #[test]
    fn handle_request_resilient_contains_a_handler_panic() {
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        // Stage both documents (tracking never analyzes, so it never unwinds); the
        // hover request against the panic file is what the seam makes unwind, and
        // what the resilient wrapper must contain. Requests never republish, so
        // staging a healthy sibling this way is safe.
        track(&mut state, "file:///inf-test/panic.inf", PANIC_DOC_SOURCE);
        track(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn f() -> i32 { return 1; }",
        );

        let response =
            state.handle_request_resilient(hover_request(1, "file:///inf-test/panic.inf", 0, 3));
        assert_eq!(
            error_code(&response),
            lsp_server::ErrorCode::InternalError as i32,
            "a panicking handler answers InternalError"
        );
        assert_eq!(
            response.id,
            RequestId::from(1),
            "the failed request's own id is echoed back"
        );

        // A healthy request against another document still succeeds afterward.
        let good =
            state.handle_request_resilient(hover_request(2, "file:///inf-test/ok.inf", 0, 3));
        assert!(
            good.error.is_none(),
            "the server still answers after containing a panic"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn on_notification_resilient_contains_a_diagnostics_panic_and_recovers() {
        let _arm = arm_analysis_panic(PANIC_MARKER);
        let mut state = ServerState::new(full_client());
        // A healthy document opened before the bad one; the bad open's panic must
        // not disturb it. (An undeclared variable keeps its diagnostics non-empty.)
        open(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn g() -> i32 { return x; }",
        );

        // Opening a document whose diagnostics computation panics publishes nothing
        // and rebuilds the host rather than tearing down the session.
        let publishes = state.on_notification_resilient(did_open_notification(
            "file:///inf-test/panic.inf",
            PANIC_DOC_SOURCE,
        ));
        assert!(
            publishes.is_empty(),
            "a panic during diagnostics publishes nothing"
        );

        // The host was rebuilt from the tracked documents, so the healthy document
        // is still analyzable and still reports its undeclared variable.
        assert!(
            !diagnostics_for(&mut state, "file:///inf-test/ok.inf").is_empty(),
            "the healthy document survives the recovery and is still analyzed"
        );
    }

    #[test]
    fn named_constant_array_size_publishes_a_diagnostic_not_a_panic() {
        // The #240 fix at the LSP boundary: the source that used to `todo!`-panic the
        // analysis now type-checks into an ordinary diagnostic, so opening it
        // publishes a normal diagnostic set instead of unwinding the session. The
        // seam is not armed here — the panic must be gone on its own.
        let mut state = ServerState::new(full_client());
        let source = "const N: i32 = 3;\n\
fn main() -> i32 { let arr: [i32; N] = [1, 2, 3]; return arr[0]; }";
        let mut publishes = state.on_notification(did_open_notification(
            "file:///inf-test/const-size.inf",
            source,
        ));
        assert_eq!(publishes.len(), 1, "the only open document publishes once");
        let published = publishes.remove(0);
        assert!(
            published
                .diagnostics
                .iter()
                .any(|d| d.message.contains("array size must be an integer literal")),
            "the named-constant array size is reported as a diagnostic, got {:?}",
            published.diagnostics
        );
    }
}
