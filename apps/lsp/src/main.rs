//! `inference-lsp`: the Language Server Protocol server for Inference.
//!
//! A synchronous, single-threaded stdio server built on `lsp-server`. It answers
//! diagnostics, hover, goto-definition, completion, document symbols, and inlay
//! hints for Inference source, delegating all analysis to the `ide` stack and
//! confining every protocol concern (framing, position encoding, URIs) to this
//! crate. Stdout is the JSON-RPC channel; all logging goes to stderr.

mod capabilities;
mod convert;
mod handlers;
mod server;
mod uri;

use anyhow::Result;
use lsp_server::Connection;

fn main() -> Result<()> {
    eprintln!("inference-lsp: starting on stdio");

    let server = std::thread::Builder::new()
        .name("inference-lsp-main".to_owned())
        .stack_size(server::SERVER_STACK_SIZE)
        .spawn(run_server)?;
    server.join().expect("the server thread panicked")
}

/// Owns the transport for one stdio session: handshake, message loop, teardown.
///
/// A malformed or oversized frame is a known limitation: `lsp-server`'s stdio
/// reader treats any framing/body parse failure as fatal to the connection (and
/// pre-allocates a body buffer straight from the `Content-Length` header, with no
/// upper bound), giving no seam to answer JSON-RPC `-32700` and resync. So a bad
/// frame surfaces through `io_threads.join()` and the process exits. Recovering
/// would require replacing `Connection::stdio()` with a vendored reader;
/// rust-analyzer accepts the same limitation on `lsp-server`. See the README's
/// known-limitations note.
fn run_server() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();

    // A failed initialize (`None`) has already answered the initialize request with
    // an error; there is nothing left to serve, so skip the message loop.
    if let Some(init_params) = server::initialize(&connection)? {
        server::run(&connection, &init_params)?;
    }

    // The transport's writer thread only ends once the connection (its sender) is
    // dropped, so drop it before joining — otherwise `join` would block forever.
    drop(connection);
    io_threads.join()?;

    eprintln!("inference-lsp: shut down cleanly");
    Ok(())
}
