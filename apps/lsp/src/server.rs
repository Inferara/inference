//! The server state and the single-threaded message loop.
//!
//! [`ServerState`] holds the analysis host and the set of open documents; it turns
//! one request into one [`Response`] and one notification into the diagnostics to
//! publish, with no I/O of its own — which is what makes it directly testable.
//! [`run`] owns the transport: it reads messages, handles the shutdown/exit
//! handshake inline, routes everything else through the state, and writes the
//! results back. Nothing here prints to stdout; that stream is the protocol
//! channel.

use std::path::PathBuf;

use inference_ide::AnalysisHost;
use lsp_server::{Connection, ErrorCode, ExtractError, Message, Notification, Request, Response};
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
                let response = state.handle_request(request);
                send(&connection, Message::Response(response))?;
            }
            Message::Notification(notification) if notification.method == Exit::METHOD => {
                return Ok(());
            }
            // A stray notification after `shutdown` (other than `exit`) is dropped.
            Message::Notification(_) if shutting_down => {}
            Message::Notification(notification) => {
                for params in state.on_notification(notification) {
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

#[cfg(test)]
mod tests {
    use lsp_server::{Request, RequestId, Response};
    use lsp_types::notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    };
    use lsp_types::request::{HoverRequest, Request as _};

    use super::ServerState;

    fn open(state: &mut ServerState, uri: &str, text: &str) {
        let params = serde_json::json!({
            "textDocument": { "uri": uri, "languageId": "inference", "version": 1, "text": text }
        });
        let notification =
            lsp_server::Notification::new(DidOpenTextDocument::METHOD.to_owned(), params);
        state.on_notification(notification);
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
}
