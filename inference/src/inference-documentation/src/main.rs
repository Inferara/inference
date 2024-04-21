//! # Inference Documentation
//! Inference Documentation is a tool to generate documentation for rust code compiling
//! together docstrings documentation and inference specifications.
//!
//! It is a binary crate that can be downloaded from crates.io/inference-documenation.
//! Usage: inference-documentation [source directory]
//! Note: if no source directory is provided, the `.` will be used.
//! The result will be a set on .md files created in the `inference_documentation_output` directory.
//! The directory tree structure will be the same as in the source directory.
//!
//! Build in **Inferara**

#![warn(clippy::pedantic)]

use std::{env, process};

use inference_documentation::{build_inference_documentation, InferenceDocumentationConfig};
use inference_proc_macros::inference_spec;

/// Main function is the entry point of the inference documentation binary.
/// It parses the command line arguments and builds the inference documentation.
fn main() {
    let config =
        InferenceDocumentationConfig::from_cmd_line_args(env::args()).unwrap_or_else(|err| {
            eprintln!("Problem parsing arguments: {err}");
            process::exit(1);
        });

    build_inference_documentation(&config);
}

#[inference_spec(main)]
mod spec {
    use inference_proc_macros::{inference, inference_fun};

    #[inference_fun(main)]
    fn s_main_signature() {
        inference! {
            "main -> ()"
        };
    }

    #[inference_fun(main)]
    fn s_main_formula() {
        inference! {
            "
            InferenceDocumentationConfig::from_cmd_line_args env::args
            | Ok => build_inference_documentation
            | Err => {
                process::exit 1
            }
            "
        };
    }
}
