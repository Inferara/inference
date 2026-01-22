//! Custom parser for the Inference language
//!
//! This crate implements a resilient LL parser with error recovery capabilities.
//! It tokenizes source code using a lexer, then parses tokens into a grammar-based
//! structure. The parser uses an advance tracking mechanism to prevent infinite loops
//! and ensure forward progress during error recovery.
//!
//! # Architecture
//!
//! The parser is organized into modular components:
//!
//! - [`lexer`] - Tokenization of source code into a token stream
//! - [`parser`] - Core parsing logic with advance tracking and error recovery
//! - [`error`] - Error types and error collection for batch reporting
//!
//! # Advance Tracking Mechanism
//!
//! The parser prevents infinite loops through an advance tracking stack:
//! - `advance_push()` marks the start of a parse attempt
//! - `advance_pop()` asserts we've consumed tokens or reported an error
//! - `advance_drop()` skips the check for error recovery paths
//!
//! This ensures the parser always makes progress and never gets stuck.
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
//!     Ok(()) => println!("Parse successful"),
//!     Err(errors) => {
//!         for error in errors {
//!             eprintln!("Parse error: {}", error);
//!         }
//!     }
//! }
//! ```

pub mod error;
pub mod lexer;
pub mod parser;

pub use error::{ParseError, ParseErrorCollector};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::Parser;
