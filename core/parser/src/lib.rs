//! Custom parser for the Inference language
//!
//! Comprehensive parser implementation based on rust-analyzer's architecture.
//! Features >95% test coverage, modular grammar rules, and resilient error recovery.
//!
//! # Architecture
//!
//! The parser is organized into modular components following rust-analyzer patterns:
//!
//! - [`lexer`] - Tokenization of source code into a token stream
//! - [`syntax_kind`] - All token and node types for the Inference language
//! - [`parser`] - Core parsing logic with marker-based approach
//! - [`grammar`] - Grammar rules for items, expressions, types, patterns
//! - [`error`] - Error types and error collection for batch reporting
//!
//! # Marker-Based Parsing
//!
//! The parser uses markers to track node boundaries:
//! - `Parser::start()` creates a marker at current position
//! - `Marker::complete()` completes a node with specified kind
//! - Supports error recovery and efficient backtracking
//!
//! # Example
//!
//! ```ignore
//! use inference_parser::Parser;
//!
//! let source = r#"
//! fn add(a: i32, b: i32) -> i32 {
//!     return a + b;
//! }
//! "#;
//!
//! let mut parser = Parser::new(source);
//! match parser.parse_module() {
//!     Ok(ast) => println!("Parse successful"),
//!     Err(errors) => {
//!         for error in errors {
//!             eprintln!("Parse error: {}", error);
//!         }
//!     }
//! }
//! ```

pub mod error;
pub mod lexer;
pub mod syntax_kind;
pub mod token_kind_bridge;
pub mod parser;
pub mod grammar;

pub use error::{ParseError, ParseErrorCollector};
pub use lexer::{Lexer, Token, TokenKind};
pub use syntax_kind::SyntaxKind;
pub use parser::Parser;
