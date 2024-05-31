use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, MAIN_SEPARATOR},
};
use syn::{spanned::Spanned, visit::Visit, Expr};

pub struct DocstringsGrabber<'file_content> {
    pub file_name: String,
    pub file_content: &'file_content String,
    pub fn_loc_map: HashMap<String, (usize, usize, usize, usize)>,

    current_mod: Vec<String>, //a sequence of modules that are currently being visited that are not part of the inference spec
    inference_spec_mod: Vec<String>, //a sequence of modules that are currently being visited that are part of the inference spec
    current_spec_function: String, //the name of the function that the visitor context in and that is part of the inference spec
    spec_functions: HashMap<String, String>, //a map of inference spec functions to the imperative functions
}

impl DocstringsGrabber<'_> {
    pub fn new(file_name: String, file_content: &String) -> DocstringsGrabber {
        DocstringsGrabber {
            file_name,
            file_content,
            fn_loc_map: HashMap::new(),
            current_mod: Vec::new(),
            inference_spec_mod: Vec::new(),
            current_spec_function: String::new(),
            spec_functions: HashMap::new(),
        }
    }

    pub fn save(&mut self, file_root_directory: &String, output_directory: &String) {
        let inner_file_path = self
            .file_name
            .replace(file_root_directory, "")
            .trim_start_matches(MAIN_SEPARATOR)
            .to_string();

        let path = Path::new(output_directory).join(inner_file_path.replace(".rs", ".md"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = fs::File::create(path).unwrap();
        writeln!(file, "# {}", inner_file_path.replace(MAIN_SEPARATOR, "::")).unwrap();
        writeln!(file, "{}", self.parse_file_level_docstring()).unwrap();
        let mut fn_loc_map: Vec<_> = self.fn_loc_map.iter().collect();
        fn_loc_map.sort_by(|a, b| a.1 .0.cmp(&b.1 .0));
        for (item_name, _) in fn_loc_map {
            writeln!(file, "### {}", item_name.clone()).unwrap();
            writeln!(file, "---").unwrap();
            writeln!(file, "{}", self.parse_fn_docstring(item_name)).unwrap();
        }
    }

    pub fn visit_file(&mut self, file: &syn::File) {
        syn::visit::visit_file(self, file);
    }

    fn parse_file_level_docstring(&mut self) -> String {
        let lines = self.file_content.lines();
        let mut docstring = String::new();
        for line in lines {
            if line.starts_with("//!") {
                let mut docstring_line = line.trim_start_matches("//!").trim().to_string();
                if docstring_line.starts_with('#') {
                    docstring_line = format!("#{docstring_line}");
                }
                docstring.push_str(docstring_line.as_str());
                docstring.push('\n');
            } else {
                break;
            }
        }
        docstring
    }

    fn parse_fn_docstring(&self, fn_name: &String) -> String {
        let line_number = self.fn_loc_map.get(fn_name).unwrap().0;
        let mut v_docstring = Vec::new();
        for line in self.file_content.lines().skip(line_number - 1) {
            if line.starts_with("fn") || !line.starts_with('/') {
                break;
            }
            let docstring_line = line
                .trim_start_matches(|c: char| c == '/')
                .trim()
                .to_string();
            v_docstring.push(docstring_line.clone());
            v_docstring.push(String::from("\n"));
        }
        v_docstring.join("")
    }

    fn insert_fn(&mut self, fn_name: String, item_fn: &syn::ItemFn) {
        let span_start = item_fn.span().start();
        let span_end = item_fn.span().end();
        self.fn_loc_map.insert(
            fn_name,
            (
                span_start.line,
                span_start.column,
                span_end.line,
                span_end.column,
            ),
        );
    }
}

impl<'ast, 'file_content> Visit<'ast> for DocstringsGrabber<'file_content> {
    fn visit_item_fn(&mut self, item_fn: &'ast syn::ItemFn) {
        let mut fn_name = item_fn.sig.ident.to_string();
        if self.current_mod.is_empty() && self.inference_spec_mod.is_empty() {
            let mod_name_from_file = self
                .file_name
                .split(MAIN_SEPARATOR)
                .last()
                .unwrap()
                .replace(".rs", "");
            fn_name = format!("{mod_name_from_file}::{fn_name}");
        }

        if self.inference_spec_mod.is_empty() {
            let mod_name = self.current_mod.join("::");
            fn_name = format!("{mod_name}::{fn_name}");
            self.insert_fn(fn_name.clone(), item_fn);
        } else {
            let mod_name = self.inference_spec_mod.join("::");

            for attr in &item_fn.attrs {
                if attr.path().is_ident("inference_fun") {
                    let spec_for_fn: Expr = attr.parse_args().unwrap();
                    let mut target_fn_name =
                        spec_for_fn.span().source_text().unwrap_or(fn_name.clone());
                    target_fn_name = format!("{mod_name}::{target_fn_name}");
                    self.spec_functions
                        .insert(target_fn_name.clone(), fn_name.clone());
                }
            }
        }
        syn::visit::visit_item_fn(self, item_fn);
    }

    fn visit_item_mod(&mut self, item_mod: &'ast syn::ItemMod) {
        let mut is_current_mod_inference_spec = false;
        for attr in &item_mod.attrs {
            if attr.path().is_ident("inference_spec") {
                let target_mod_name: Expr = attr.parse_args().unwrap();
                is_current_mod_inference_spec = true;
                self.inference_spec_mod.push(
                    target_mod_name
                        .span()
                        .source_text()
                        .unwrap_or(item_mod.ident.to_string())
                        .clone(),
                );
            }
        }

        if !is_current_mod_inference_spec {
            self.current_mod.push(item_mod.ident.to_string());
        }

        //TODO do we allow inference spec be not the first level module?
        // Example:
        // mod a {
        //    mod b {
        //       #[inference_spec]
        //      mod spec {
        //     }
        //   }
        // }

        syn::visit::visit_item_mod(self, item_mod);

        if is_current_mod_inference_spec {
            self.inference_spec_mod.pop();
        } else {
            self.current_mod.pop();
        }
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        syn::visit::visit_macro(self, i);
    }
}
