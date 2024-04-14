#![warn(clippy::all, clippy::pedantic)]

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, MAIN_SEPARATOR},
};
use syn::{parse_file, spanned::Spanned, visit::Visit, Expr};
use walkdir::WalkDir;

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

struct DocstringsGrabber {
    file_name: String,
    item_doc_map: HashMap<String, String>,
}

impl DocstringsGrabber {
    fn save(&self, file_root_directory: &String, output_directory: &String) {
        let inner_file_path = self
            .file_name
            .replace(file_root_directory, "")
            .trim_start_matches(MAIN_SEPARATOR)
            .to_string();

        let path = Path::new(output_directory).join(inner_file_path.replace(".rs", ".md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(path).unwrap();
        for (item_name, docstring) in &self.item_doc_map {
            writeln!(file, "{}: {}", item_name, docstring).unwrap();
        }
    }
}

impl<'ast> Visit<'ast> for DocstringsGrabber {
    fn visit_item_fn(&mut self, item_fn: &'ast syn::ItemFn) {
        let fn_name = item_fn.sig.ident.to_string();
        let span_start = item_fn.span().start();
        let span_end = item_fn.span().end();
        self.item_doc_map.insert(
            fn_name,
            format!(
                "[{}:{} - {}:{}]",
                span_start.line, span_start.column, span_end.line, span_end.column
            ),
        );
    }

    fn visit_item_mod(&mut self, item_mod: &'ast syn::ItemMod) {
        for attr in &item_mod.attrs {
            if attr.path().is_ident("inference_spec") {
                let precondition: Expr = attr.parse_args().unwrap();
                println!("{:?}", precondition.span().source_text().unwrap());
            }
        }

    }
}

pub fn build_inference_documentation(config: &InferenceDocumentationConfig) {
    WalkDir::new(&config.working_directory)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "rs"))
        .for_each(|entry| {
            println!("{}", entry.path().to_str().unwrap());
            let file_content = fs::read_to_string(entry.path()).unwrap();
            let rust_file = parse_file(&file_content).unwrap();
            let mut visitor = DocstringsGrabber {
                file_name: String::from(entry.path().to_str().unwrap()),
                item_doc_map: HashMap::new(),
            };
            visitor.visit_file(&rust_file);
            visitor.save(&config.working_directory, &config.output_directory);
        });
}
