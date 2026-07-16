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
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, InlayHintRequest,
    Request as _, Shutdown,
};
use lsp_types::{InitializeParams, PublishDiagnosticsParams, Uri};
use rustc_hash::FxHashMap;

use crate::handlers;

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

/// The analysis host plus per-document bookkeeping. Feature queries and
/// diagnostics are answered against this; the transport is elsewhere.
pub(crate) struct ServerState {
    pub(crate) host: AnalysisHost,
    pub(crate) documents: FxHashMap<Uri, Document>,
    /// Whether the client accepts the hierarchical document-symbol response; when
    /// it does not, symbols are flattened to `SymbolInformation`.
    pub(crate) hierarchical_symbols: bool,
}

impl ServerState {
    pub(crate) fn new(hierarchical_symbols: bool) -> Self {
        Self {
            host: AnalysisHost::default(),
            documents: FxHashMap::default(),
            hierarchical_symbols,
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

/// Runs the message loop until the client exits or the connection closes.
///
/// The shutdown handshake is handled inline rather than delegated to
/// `lsp-server`'s `Connection::handle_shutdown`, which consumes the next message
/// itself and turns anything but `exit` into a fatal protocol error. Instead, a
/// `shutdown` request is answered and flips a `shutting_down` flag; while it is
/// set, every further request is answered with `InvalidRequest` (the spec's
/// behaviour for requests received between `shutdown` and `exit`) and every
/// notification but `exit` is ignored. The `exit` notification ends the loop.
/// Every other request is routed through [`ServerState`], and document
/// notifications may publish diagnostics.
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
pub fn run(connection: Connection, init_params: &InitializeParams) -> anyhow::Result<()> {
    let mut state = ServerState::new(hierarchical_symbol_support(init_params));
    let mut shutting_down = false;

    for message in &connection.receiver {
        match message {
            Message::Request(request) if request.method == Shutdown::METHOD => {
                shutting_down = true;
                send(
                    &connection,
                    Message::Response(Response::new_ok(request.id, ())),
                )?;
            }
            Message::Request(request) if shutting_down => {
                send(
                    &connection,
                    Message::Response(Response::new_err(
                        request.id,
                        ErrorCode::InvalidRequest as i32,
                        "the server is shutting down".to_owned(),
                    )),
                )?;
            }
            Message::Request(request) => {
                let response = state.handle_request_resilient(request);
                send(&connection, Message::Response(response))?;
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
                    send(&connection, Message::Notification(published))?;
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use lsp_server::{Request, RequestId, Response};
    use lsp_types::notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    };
    use lsp_types::request::{HoverRequest, Request as _};
    use lsp_types::Uri;

    use super::{Document, ServerState};

    /// A document that panics the analysis stack: a named constant used as an
    /// array size hits an unimplemented `todo!` deep in the type-checker (#240).
    /// It is the most direct in-tree trigger for the message-loop panic boundary;
    /// if #240 is fixed so this no longer panics, replace it with another
    /// deterministic panic trigger.
    const PANIC_SOURCE: &str = "const N: i32 = 3;\n\
fn main() -> i32 { let arr: [i32; N] = [1, 2, 3]; let i: i32 = 0; return arr[i]; }";

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
    /// for a later query without the staging itself unwinding.
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
        let mut state = ServerState::new(true);
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
    fn malformed_params_are_invalid_params_and_leave_the_server_usable() {
        let mut state = ServerState::new(true);
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
        let mut state = ServerState::new(true);
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
        let mut state = ServerState::new(true);
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
        let mut state = ServerState::new(true);
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
    fn non_file_uri_is_ignored_on_open() {
        let mut state = ServerState::new(true);
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
        let mut state = ServerState::new(true);
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

    #[test]
    fn handle_request_resilient_contains_a_handler_panic() {
        let mut state = ServerState::new(true);
        // Stage both documents without analyzing them (analyzing the panic file
        // would unwind on its own); the requests below are what must be contained.
        // Requests never republish, so staging a healthy sibling this way is safe.
        track(&mut state, "file:///inf-test/panic.inf", PANIC_SOURCE);
        track(
            &mut state,
            "file:///inf-test/ok.inf",
            "fn f() -> i32 { return 1; }",
        );

        let response =
            state.handle_request_resilient(hover_request(1, "file:///inf-test/panic.inf", 1, 25));
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

    #[test]
    fn on_notification_resilient_contains_a_diagnostics_panic_and_recovers() {
        let mut state = ServerState::new(true);
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
            PANIC_SOURCE,
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
}
