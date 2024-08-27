#[cfg(test)]
mod tests {

    use crate::{ast::types::Definition, parse_inference};

    fn read_sample_by_name(name: &str) -> String {
        let current_dir = std::env::current_dir().unwrap();
        let path = current_dir.join(format!("samples/{name}.inf"));
        let absolute_path = path.canonicalize().unwrap();
        std::fs::read_to_string(absolute_path).unwrap()
    }

    #[test]
    fn test_parse_source_file_1() {
        let source_code = read_sample_by_name("test_parse_source_file_1");
        let ast = parse_inference(source_code.as_str());

        assert_eq!(ast.location.start.row, 0);
        assert_eq!(ast.location.start.column, 0);
        assert_eq!(ast.location.end.row, 104);
        assert_eq!(ast.location.end.column, 0);

        assert_eq!(ast.use_directives.len(), 4);
        assert_eq!(ast.definitions.len(), 1);
        match ast.definitions.first().unwrap() {
            Definition::Context(context_definition) => context_definition,
            _ => panic!("Expected a context definition at index 0"),
        };
    }

    #[test]
    fn test_parse_source_file_2() {
        let source_code = read_sample_by_name("test_parse_source_file_2");
        let ast = parse_inference(source_code.as_str());

        assert_eq!(ast.location.start.row, 0);
        assert_eq!(ast.location.start.column, 0);
        assert_eq!(ast.location.end.row, 5);
        assert_eq!(ast.location.end.column, 0);

        assert_eq!(ast.use_directives.len(), 1);
        assert_eq!(ast.definitions.len(), 2);

        match ast.definitions.first().unwrap() {
            Definition::Context(context_definition) => context_definition,
            _ => panic!("Expected a context definition at index 0"),
        };

        match ast.definitions.get(1).unwrap() {
            Definition::Context(context_definition) => context_definition,
            _ => panic!("Expected a context definition at index 1"),
        };
    }

    #[test]
    fn test_parse_source_file_3() {
        let source_code = read_sample_by_name("test_parse_source_file_3");
        let ast = parse_inference(source_code.as_str());

        assert_eq!(ast.location.start.row, 0);
        assert_eq!(ast.location.start.column, 0);
        assert_eq!(ast.location.end.row, 7);
        assert_eq!(ast.location.end.column, 0);

        assert_eq!(ast.use_directives.len(), 1);
        assert_eq!(ast.definitions.len(), 3);

        match ast.definitions.first().unwrap() {
            Definition::Function(function_definition) => function_definition,
            _ => panic!("Expected a function definition at index 0"),
        };

        match ast.definitions.get(1).unwrap() {
            Definition::Context(context_definition) => context_definition,
            _ => panic!("Expected a context definition at index 1"),
        };

        match ast.definitions.get(2).unwrap() {
            Definition::Context(context_definition) => context_definition,
            _ => panic!("Expected a context definition at index 2"),
        };
    }
}
