//! This crate is used to generate documentation for the inference engine.
//! It generates documentation by extracting docstrings and inference specifications from the source code.

#![warn(clippy::all, clippy::pedantic)]

use std::{collections::HashMap, fs};
use syn::parse_file;
use walkdir::WalkDir;

mod docstrings_grabber;

/// Configuration for the inference documentation.
/// `working_directory` is the directory where the source code is located.
/// `output_directory` is the directory where the documentation will be saved.
#[derive(Debug)]
pub struct InferenceDocumentationConfig {
    pub working_directory: String,
    pub output_directory: String,
}

impl InferenceDocumentationConfig {
    pub fn from_cmd_line_args(
        mut args: impl Iterator<Item = String>,
    ) -> Result<InferenceDocumentationConfig, &'static str> {
        args.next();
        let working_directory = args.next().unwrap_or(String::from("."));

        let working_directory = match std::fs::canonicalize(&working_directory) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => return Err("Failed to convert to absolute path"),
        };

        if !std::path::Path::new(&working_directory).exists() {
            return Err("Working directory does not exist");
        }

        let output_directory = args
            .next()
            .unwrap_or(String::from("./inference_documentation_output"));
        if !std::path::Path::new(&output_directory).exists() {
            if let Err(_) = std::fs::create_dir(&output_directory) {
                return Err("Failed to create output directory");
            }
        }

        Ok(InferenceDocumentationConfig {
            working_directory,
            output_directory,
        })
    }
}

pub fn build_inference_documentation(config: &InferenceDocumentationConfig) {
    WalkDir::new(&config.working_directory)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "rs"))
        .for_each(|entry| {
            let file_content = fs::read_to_string(entry.path()).unwrap();
            let rust_file = parse_file(&file_content).unwrap();
            let mut visitor = docstrings_grabber::DocstringsGrabber {
                file_name: String::from(entry.path().to_str().unwrap()),
                file_content: &file_content,
                fn_loc_map: HashMap::new(),
            };
            visitor.visit_file(&rust_file);
            visitor.save(&config.working_directory, &config.output_directory);
        });
}
