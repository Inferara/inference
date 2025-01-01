#[cfg(test)]
mod function_tests {
    use crate::{ast::types::*, wasm_codegen::wat_generator::generate_for_function_definition};

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_simple_add_function() {
        //"fn add(a: i32, b: i32) -> i32 { return a + b; }"
        let function = FunctionDefinition {
            location: Location {
                start: Position { row: 0, column: 0 },
                end: Position { row: 0, column: 0 },
            },
            name: Identifier {
                location: Location {
                    start: Position { row: 0, column: 3 },
                    end: Position { row: 0, column: 6 },
                },
                name: "add".to_string(),
            },
            parameters: Some(vec![
                Parameter {
                    location: Location {
                        start: Position { row: 0, column: 7 },
                        end: Position { row: 0, column: 10 },
                    },
                    name: Identifier {
                        location: Location {
                            start: Position { row: 0, column: 7 },
                            end: Position { row: 0, column: 8 },
                        },
                        name: "a".to_string(),
                    },
                    type_: Type::Simple(SimpleType {
                        location: Location {
                            start: Position { row: 0, column: 11 },
                            end: Position { row: 0, column: 14 },
                        },
                        name: "i32".to_string(),
                    }),
                },
                Parameter {
                    location: Location {
                        start: Position { row: 0, column: 15 },
                        end: Position { row: 0, column: 18 },
                    },
                    name: Identifier {
                        location: Location {
                            start: Position { row: 0, column: 15 },
                            end: Position { row: 0, column: 16 },
                        },
                        name: "b".to_string(),
                    },
                    type_: Type::Simple(SimpleType {
                        location: Location {
                            start: Position { row: 0, column: 19 },
                            end: Position { row: 0, column: 22 },
                        },
                        name: "i32".to_string(),
                    }),
                },
            ]),
            returns: Some(Type::Simple(SimpleType {
                location: Location {
                    start: Position { row: 0, column: 27 },
                    end: Position { row: 0, column: 30 },
                },
                name: "i32".to_string(),
            })),
            body: Block {
                location: Location {
                    start: Position { row: 0, column: 32 },
                    end: Position { row: 0, column: 39 },
                },
                statements: vec![Statement::Return(ReturnStatement {
                    location: Location {
                        start: Position { row: 0, column: 32 },
                        end: Position { row: 0, column: 39 },
                    },
                    expression: Expression::Binary(Box::new(BinaryExpression {
                        location: Location {
                            start: Position { row: 0, column: 39 },
                            end: Position { row: 0, column: 39 },
                        },
                        operator: OperatorKind::Add,
                        left: Box::new(Expression::Identifier(Identifier {
                            location: Location {
                                start: Position { row: 0, column: 39 },
                                end: Position { row: 0, column: 40 },
                            },
                            name: "a".to_string(),
                        })),
                        right: Box::new(Expression::Identifier(Identifier {
                            location: Location {
                                start: Position { row: 0, column: 43 },
                                end: Position { row: 0, column: 44 },
                            },
                            name: "b".to_string(),
                        })),
                    })),
                })],
            },
        };

        let wat = generate_for_function_definition(&function, 0);
        let expected = "(func (export \"add\") (param $a i32) (param $b i32) (result i32)
    local.get $a
    local.get $b
    i32.add
)";
        assert_eq!(wat.trim(), expected);
    }
}
