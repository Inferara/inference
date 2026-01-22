#[cfg(test)]
mod tests {
    use inf_wast::{
        Wat,
        core::*,
        parser::{self, ParseBuffer},
    };

    #[test]
    fn parse_vanilla_module() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input)?;
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            assert_eq!(expression.instrs.len(), 3);
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  local.get 1
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input)?;
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //LocalGet
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 5, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_embedded_forall_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (forall
                  local.get 1
                  i32.add
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Forall
                            //LocalGet
                            //I32Add
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 6, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_inner_forall_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (forall
                  local.get 1
                  (forall
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Forall
                            //LocalGet
                            //Forall
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_exists_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (exists
                  local.get 0
                  local.get 1
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Exists
                            //LocalGet
                            //LocalGet
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 5, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_embedded_exists_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (exists
                  local.get 1
                  i32.add
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Exists
                            //LocalGet
                            //I32Add
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 6, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_inner_exists_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (exists
                  local.get 1
                  (exists
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Exists
                            //LocalGet
                            //Exists
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_and_exists_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (exists
                    local.get 1
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Exists
                            //LocalGet
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_assume_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (assume
                  local.get 0
                  local.get 1
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Assume
                            //LocalGet
                            //LocalGet
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 5, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_embedded_assume_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (assume
                  local.get 1
                  i32.add
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Assume
                            //LocalGet
                            //I32Add
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 6, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_inner_assume_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (assume
                  local.get 1
                  (assume
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Assume
                            //LocalGet
                            //Assume
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_and_assume_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (assume
                    local.get 1
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Assume
                            //LocalGet
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_exists_and_assume_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (exists
                  local.get 0
                  (assume
                    local.get 1
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Exists
                            //LocalGet
                            //Assume
                            //LocalGet
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_exists_and_assume_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (exists
                    local.get 1
                    (assume
                      i32.add
                    )
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Exists
                            //LocalGet
                            //Assume
                            //I32Add
                            //End(None)
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 10, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_unique_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (unique
                  local.get 0
                  local.get 1
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Unique
                            //LocalGet
                            //LocalGet
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 5, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_embedded_unique_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (unique
                  local.get 1
                  i32.add
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Unique
                            //LocalGet
                            //I32Add
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 6, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_inner_unique_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                local.get 0
                (unique
                  local.get 1
                  (unique
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //LocalGet
                            //Unique
                            //LocalGet
                            //Unique
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_and_unique_block() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (unique
                    local.get 1
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Unique
                            //LocalGet
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_exists_and_unique_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (exists
                  local.get 0
                  (unique
                    local.get 1
                    i32.add
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Exists
                            //LocalGet
                            //Unique
                            //LocalGet
                            //I32Add
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 8, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_exists_and_unique_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (exists
                    local.get 1
                    (unique
                      i32.add
                    )
                  )
                )
                i32.add))
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Exists
                            //LocalGet
                            //Unique
                            //I32Add
                            //End(None)
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 10, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }

    #[test]
    fn parse_function_with_forall_exists_assume_and_unique_blocks() -> anyhow::Result<()> {
        let input = ParseBuffer::new(
            r#"
            (module
              (func (export "addTwo") (param i32 i32) (result i32)
                (forall
                  local.get 0
                  (exists
                    local.get 1
                    (assume
                      (unique
                        i32.add
                      )
                    )
                  )
                )
                i32.add
              )
            )
        "#,
        )?;
        let module = parser::parse::<Wat>(&input).unwrap();
        match module {
            Wat::Module(module) => {
                assert_eq!(module.id, None);
                assert_eq!(module.name, None);
                if let ModuleKind::Text(text_module) = &module.kind {
                    let _functions: Vec<&Func> = text_module
                        .iter()
                        .filter(|field| matches!(field, ModuleField::Func(_)))
                        .filter_map(|field| {
                            if let ModuleField::Func(func) = field {
                                Some(func)
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(_functions.len(), 1);
                    let function = _functions[0];
                    assert_eq!(function.id, None);
                    assert_eq!(function.name, None);
                    assert_eq!(function.exports.names.len(), 1);
                    assert_eq!(function.exports.names[0], "addTwo");

                    match &function.kind {
                        FuncKind::Inline { locals, expression } => {
                            assert_eq!(locals.len(), 0);
                            //Forall
                            //LocalGet
                            //Exists
                            //LocalGet
                            //Assume
                            //Unique
                            //I32Add
                            //End(None)
                            //End(None)
                            //End(None)
                            //End(None)
                            //I32Add
                            assert_eq!(expression.instrs.len(), 12, "{expression:?}");
                        }
                        _ => panic!("expected inline function"),
                    }
                } else {
                    panic!("expected text module");
                }
            }
            Wat::Component(_) => panic!("expected module"),
        }
        Ok(())
    }
}
