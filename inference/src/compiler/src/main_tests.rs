#[allow(unused_imports)]
pub(crate) mod test {

    use super::super::{parse_inf_file, wasm_to_coq_translator::wasm_parser};
    use walrus::Module;

    #[test]
    fn test_parse() {
        let path = get_test_data_path().join("inf").join("example.inf");
        let absolute_path = path.canonicalize().unwrap();
        let ast = parse_inf_file(absolute_path.to_str().unwrap());
        assert!(!ast.definitions.is_empty());
        // std::fs::write(
        //     current_dir.join(""),
        //     format!("{ast:#?}"),
        // )
        // .unwrap();
    }

    #[test]
    fn test_wasm_to_coq() {
        let path = get_test_data_path().join("wasm").join("comments.0.wasm");
        let absolute_path = path.canonicalize().unwrap();

        let bytes = std::fs::read(absolute_path).unwrap();
        let mod_name = String::from("index");
        let coq = wasm_parser::translate_bytes(&mod_name, bytes.as_slice());
        assert!(coq.is_ok());
        let coq_file_path = get_out_path().join("test_wasm_to_coq.v");
        std::fs::write(coq_file_path, coq.unwrap()).unwrap();
    }

    #[test]
    fn test_walrys() {
        let path = get_test_data_path().join("wasm").join("comments.0.wasm");
        let absolute_path = path.canonicalize().unwrap();
        let module = Module::from_file(absolute_path).unwrap();
        for func in module.funcs.iter() {
            println!("{} : {:?}", func.id().index(), func.name);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn get_test_data_path() -> std::path::PathBuf {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .parent() // compiler
            .unwrap()
            .parent() // src
            .unwrap()
            .parent() // inference
            .unwrap()
            .join("test_data")
    }

    #[allow(dead_code)]
    fn get_out_path() -> std::path::PathBuf {
        get_test_data_path().parent().unwrap().join("out")
    }
}
