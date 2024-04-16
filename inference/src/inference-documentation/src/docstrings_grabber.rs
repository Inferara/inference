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
}

impl DocstringsGrabber<'_> {
    fn parse_file_level_docstring(&mut self) -> String {
        let mut lines = self.file_content.lines();
        let mut docstring = String::new();
        while let Some(line) = lines.next() {
            if line.starts_with("//!") {
                let mut docstring_line = line.trim_start_matches("//!").trim().to_string();
                if docstring_line.starts_with("#") {
                    docstring_line = format!("#{}", docstring_line);
                }
                docstring.push_str(docstring_line.as_str());
                docstring.push('\n');
            } else {
                break;
            }
        }
        docstring
    }

    fn parse_fn_docstring(&self, fn_name: String) -> String {
        let line_number = self.fn_loc_map.get(&fn_name).unwrap().0;
        let mut v_docstring = Vec::new();
        for line in self.file_content.lines().rev().skip(self.file_content.lines().count() - line_number - 1).into_iter() {
            if line.starts_with("/") {
                let docstring_line = line.trim_start_matches(|c: char| c == '/').trim().to_string();
                v_docstring.push(docstring_line.clone());
                v_docstring.push(String::from("\n"));
            } else {
                break;
            }
        }
        v_docstring.reverse();
        v_docstring.join("")
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
        fn_loc_map.sort_by(|a, b| a.1.0.cmp(&b.1.0));
        for (item_name, loc) in fn_loc_map {
            writeln!(
                file,
                "### {}: {}",
                item_name,
                format!("[{}:{} - {}:{}]", loc.0, loc.1, loc.2, loc.3)
            )
            .unwrap();
            writeln!(file, "---").unwrap();
            writeln!(file, "{}", self.parse_fn_docstring(item_name.clone())).unwrap();
        }
    }

    pub fn visit_file(&mut self, file: &syn::File) {
        syn::visit::visit_file(self, file);
    }

}

impl<'ast, 'file_content> Visit<'ast> for DocstringsGrabber<'file_content> {
    fn visit_item_fn(&mut self, item_fn: &'ast syn::ItemFn) {
        let fn_name = item_fn.sig.ident.to_string();
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
        syn::visit::visit_item_fn(self, item_fn);
    }

    fn visit_item_mod(&mut self, item_mod: &'ast syn::ItemMod) {
        for attr in &item_mod.attrs {
            if attr.path().is_ident("inference_spec") {
                let _: Expr = attr.parse_args().unwrap();
            }
        }
        syn::visit::visit_item_mod(self, item_mod);
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        syn::visit::visit_macro(self, i);
    }
}
