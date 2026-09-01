//! This module contains various infc end to end tests
#![allow(dead_code)]
#![allow(unused_imports)]

mod analysis;
mod ast;
mod codegen;
mod diagnostics_file_context;
mod hassert_translation;
mod parser_literal_diagnostics;
mod robustness;
mod rocq_decls;
pub mod rocq_dischargeability;
mod rocq_stub_drift;
mod rocq_test_support;
mod rocq_typecheck;
mod spec_propagation;
mod spec_propagation_inf;
mod stock_validity;
mod type_checker;
mod utils;

#[cfg(test)]
mod general_tests {
    use crate::utils::{build_ast, get_test_data_path};

    #[test]
    fn test_example_inf_parsing() {
        let test_data_path = get_test_data_path().join("inf");
        let source_code = std::fs::read_to_string(test_data_path.join("example.inf")).unwrap();
        let ast = build_ast(source_code);
        assert_eq!(ast.source_files().len(), 1);
    }
}
