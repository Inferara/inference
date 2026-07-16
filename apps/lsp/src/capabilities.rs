//! The capability set this server advertises during the initialize handshake.

use lsp_types::{
    CompletionOptions, HoverProviderCapability, OneOf, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

/// The server's advertised capabilities: full-text sync plus hover, definition,
/// completion (triggered on `.` and `:`), document symbols, and inlay hints.
///
/// `position_encoding` is left unset, so the client uses the LSP default of
/// UTF-16 — the only encoding this server converts to (see the `convert` module).
#[must_use = "capabilities are only useful when sent in the initialize result"]
pub(crate) fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![".".to_owned(), ":".to_owned()]),
            ..CompletionOptions::default()
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        inlay_hint_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::server_capabilities;

    #[test]
    fn advertises_the_v1_feature_set_as_json() {
        let value = serde_json::to_value(server_capabilities()).expect("capabilities serialize");

        // Full-text sync is advertised as the numeric kind 1.
        assert_eq!(value["textDocumentSync"], serde_json::json!(1));
        assert_eq!(value["hoverProvider"], serde_json::json!(true));
        assert_eq!(value["definitionProvider"], serde_json::json!(true));
        assert_eq!(value["documentSymbolProvider"], serde_json::json!(true));
        assert_eq!(value["inlayHintProvider"], serde_json::json!(true));

        let completion = &value["completionProvider"];
        assert_eq!(completion["resolveProvider"], serde_json::json!(false));
        assert_eq!(
            completion["triggerCharacters"],
            serde_json::json!([".", ":"])
        );

        // No position-encoding negotiation: the client falls back to UTF-16.
        assert!(value.get("positionEncoding").is_none());
    }
}
