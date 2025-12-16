use crate::{
    arena::Arena,
    nodes::{
        Argument as HirArgument, ArgumentType as HirArgumentType,
        ArrayIndexAccessExpression as HirArrayIndexAccessExpression,
        ArrayLiteral as HirArrayLiteral, AssertStatement as HirAssertStatement,
        AssignStatement as HirAssignStatement, BinaryExpression as HirBinaryExpression,
        Block as HirBlock, BlockType as HirBlockType, BoolLiteral as HirBoolLiteral,
        BreakStatement as HirBreakStatement, ConstantDefinition as HirConstantDefinition,
        Definition as HirDefinition, EnumDefinition as HirEnumDefinition,
        Expression as HirExpression, ExternalFunctionDefinition as HirExternalFunctionDefinition,
        FunctionCallExpression as HirFunctionCallExpression,
        FunctionDefinition as HirFunctionDefinition, IfStatement as HirIfStatement,
        IgnoreArgument as HirIgnoreArgument, Literal as HirLiteral,
        LoopStatement as HirLoopStatement, MemberAccessExpression as HirMemberAccessExpression,
        NumberLiteral as HirNumberLiteral, OperatorKind as HirOperatorKind,
        ParenthesizedExpression as HirParenthesizedExpression,
        PrefixUnaryExpression as HirPrefixUnaryExpression, ReturnStatement as HirReturnStatement,
        SelfReference as HirSelfReference, SourceFile as HirSourceFile,
        SpecDefinition as HirSpecDefinition, Statement as HirStatement,
        StringLiteral as HirStringLiteral, StructDefinition as HirStructDefinition,
        StructExpression as HirStructExpression, StructField as HirStructField,
        TypeDefinition as HirTypeDefinition, TypeDefinitionStatement as HirTypeDefinitionStatement,
        TypeMemberAccessExpression as HirTypeMemberAccessExpression,
        UnaryOperatorKind as HirUnaryOperator, UnitLiteral as HirUnitLiteral,
        UzumakiExpression as HirUzumakiExpression,
        VariableDefinitionStatement as HirVariableDefinitionStatement,
    },
    symbol_table::{ScopeRef, SymbolTable},
    type_info::{NumberTypeKindNumberType, TypeInfo, TypeInfoKind},
};
use inference_ast::{
    arena::Arena as AstArena,
    nodes as ast,
    nodes::{Definition as AstDefinition, SourceFile as AstSourceFile},
};
use std::rc::Rc;

#[derive(Clone, Default)]
pub struct Hir {
    pub arena: Arena,
    pub symbol_table: SymbolTable,
}

impl Hir {
    #[must_use]
    pub fn new(ast_arena: &AstArena) -> Self {
        let mut symbol_table = SymbolTable::new();
        // Create root scope
        let root_scope = crate::symbol_table::Scope::new(0, "crate".to_string(), None);
        symbol_table.insert_scope(root_scope.clone());

        // Pass 1: Build Symbol Table
        // ast_arena.sources is Vec<SourceFile>.
        // Process all definitions in all source files.
        for file in &ast_arena.sources {
            for def in &file.definitions {
                crate::symbol_table::process_definition(
                    &root_scope,
                    def.clone(),
                    &mut symbol_table,
                );
            }
        }

        symbol_table.build_symbol_tables();

        // Pass 2: Build HIR
        let mut hir_sources = Vec::new();
        for file in &ast_arena.sources {
            hir_sources.push(transform_source_file(file, &symbol_table, &root_scope));
        }

        Self {
            arena: Arena {
                sources: hir_sources,
            },
            symbol_table,
        }
    }
}

fn transform_source_file(
    file: &AstSourceFile,
    table: &SymbolTable,
    scope: &ScopeRef,
) -> HirSourceFile {
    let definitions = file
        .definitions
        .iter()
        .map(|d| transform_definition(d, table, scope))
        .collect();

    HirSourceFile {
        directives: vec![],
        definitions,
    }
}

#[allow(clippy::too_many_lines)]
fn transform_definition(
    def: &AstDefinition,
    table: &SymbolTable,
    scope: &ScopeRef,
) -> HirDefinition {
    match def {
        AstDefinition::Function(f) => {
            let fn_scope = table
                .scopes
                .get(&f.id)
                .cloned()
                .unwrap_or_else(|| scope.clone());

            let arguments = f.arguments.as_ref().map(|args| {
                args.iter()
                    .map(|arg| match arg {
                        ast::ArgumentType::Argument(a) => {
                            HirArgumentType::Argument(Rc::new(HirArgument {
                                name: a.name.name.clone(),
                                is_mut: a.is_mut,
                                type_info: TypeInfo::new(&a.ty),
                            }))
                        }
                        ast::ArgumentType::SelfReference(s) => {
                            HirArgumentType::SelfReference(Rc::new(HirSelfReference {
                                is_mut: s.is_mut,
                            }))
                        }
                        ast::ArgumentType::IgnoreArgument(i) => {
                            HirArgumentType::IgnoreArgument(Rc::new(HirIgnoreArgument {
                                type_info: TypeInfo::new(&i.ty),
                            }))
                        }
                        ast::ArgumentType::Type(t) => HirArgumentType::TypeInfo(TypeInfo::new(t)),
                    })
                    .collect()
            });

            let body = match &f.body {
                ast::BlockType::Block(b) => {
                    HirBlockType::Block(Rc::new(transform_block(b, table, &fn_scope)))
                }
                ast::BlockType::Assume(b) => {
                    HirBlockType::Assume(Rc::new(transform_block(b, table, &fn_scope)))
                }
                ast::BlockType::Forall(b) => {
                    HirBlockType::Forall(Rc::new(transform_block(b, table, &fn_scope)))
                }
                ast::BlockType::Exists(b) => {
                    HirBlockType::Exists(Rc::new(transform_block(b, table, &fn_scope)))
                }
                ast::BlockType::Unique(b) => {
                    HirBlockType::Unique(Rc::new(transform_block(b, table, &fn_scope)))
                }
            };

            HirDefinition::Function(Rc::new(HirFunctionDefinition {
                name: f.name.name.clone(),
                type_parameters: f
                    .type_parameters
                    .as_ref()
                    .map(|tp| tp.iter().map(|t| t.name.clone()).collect()),
                arguments,
                returns: f.returns.as_ref().map(TypeInfo::new),
                body,
            }))
        }
        ast::Definition::Struct(s) => HirDefinition::Struct(Rc::new(HirStructDefinition {
            name: s.name.name.clone(),
            fields: s
                .fields
                .iter()
                .map(|f| {
                    Rc::new(HirStructField {
                        name: f.name.name.clone(),
                        type_info: TypeInfo::new(&f.type_),
                    })
                })
                .collect(),
            methods: s
                .methods
                .iter()
                .map(|m| {
                    if let HirDefinition::Function(f) =
                        transform_definition(&ast::Definition::Function(m.clone()), table, scope)
                    {
                        f
                    } else {
                        panic!("Expected Function definition");
                    }
                })
                .collect(),
        })),
        ast::Definition::Enum(e) => HirDefinition::Enum(Rc::new(HirEnumDefinition {
            name: e.name.name.clone(),
            variants: e.variants.iter().map(|v| v.name.clone()).collect(),
        })),
        ast::Definition::Constant(c) => HirDefinition::Constant(Rc::new(HirConstantDefinition {
            name: c.name.name.clone(),
            type_info: TypeInfo::new(&c.ty),
            value: transform_literal(&c.value, table, scope),
        })),
        ast::Definition::Type(t) => HirDefinition::Type(Rc::new(HirTypeDefinition {
            name: t.name.name.clone(),
            type_info: TypeInfo::new(&t.ty),
        })),
        ast::Definition::Spec(s) => {
            let spec_scope = table
                .scopes
                .get(&s.name.id)
                .cloned()
                .unwrap_or(scope.clone());
            HirDefinition::Spec(Rc::new(HirSpecDefinition {
                name: s.name.name.clone(),
                definitions: s
                    .definitions
                    .iter()
                    .map(|d| transform_definition(d, table, &spec_scope))
                    .collect(),
            }))
        }
        ast::Definition::ExternalFunction(f) => {
            HirDefinition::ExternalFunction(Rc::new(HirExternalFunctionDefinition {
                name: f.name.name.clone(),
                arguments: f.arguments.as_ref().map(|args| {
                    args.iter()
                        .map(|_| HirArgumentType::TypeInfo(TypeInfo::default()))
                        .collect()
                }),
                returns: f.returns.as_ref().map(TypeInfo::new),
            }))
        }
    }
}

fn transform_block(block: &ast::Block, table: &SymbolTable, scope: &ScopeRef) -> HirBlock {
    HirBlock {
        statements: block
            .statements
            .iter()
            .map(|s| transform_statement(s, table, scope))
            .collect(),
    }
}

fn transform_statement(
    stmt: &ast::Statement,
    table: &SymbolTable,
    scope: &ScopeRef,
) -> HirStatement {
    match stmt {
        ast::Statement::VariableDefinition(v) => {
            HirStatement::VariableDefinition(Rc::new(HirVariableDefinitionStatement {
                name: v.name.name.clone(),
                type_info: TypeInfo::new(&v.ty),
                value: v
                    .value
                    .as_ref()
                    .map(|e| transform_expression(&e.borrow(), table, scope)),
                is_uzumaki: v.is_uzumaki,
            }))
        }
        ast::Statement::Expression(e) => {
            HirStatement::Expression(transform_expression(e, table, scope))
        }
        ast::Statement::Return(r) => HirStatement::Return(Rc::new(HirReturnStatement {
            expression: transform_expression(&r.expression.borrow(), table, scope),
        })),
        ast::Statement::Assign(a) => HirStatement::Assign(Rc::new(HirAssignStatement {
            left: transform_expression(&a.left.borrow(), table, scope),
            right: transform_expression(&a.right.borrow(), table, scope),
        })),
        ast::Statement::If(i) => HirStatement::If(Rc::new(HirIfStatement {
            condition: transform_expression(&i.condition.borrow(), table, scope),
            if_arm: transform_block_type(&i.if_arm, table, scope),
            else_arm: i
                .else_arm
                .as_ref()
                .map(|b| transform_block_type(b, table, scope)),
        })),
        ast::Statement::Loop(l) => HirStatement::Loop(Rc::new(HirLoopStatement {
            condition: l
                .condition
                .borrow()
                .as_ref()
                .map(|c| transform_expression(c, table, scope)),
            body: transform_block_type(&l.body, table, scope),
        })),
        ast::Statement::Break(_) => HirStatement::Break(Rc::new(HirBreakStatement {})),
        ast::Statement::Block(b) => HirStatement::Block(transform_block_type(b, table, scope)),
        ast::Statement::Assert(a) => HirStatement::Assert(Rc::new(HirAssertStatement {
            expression: transform_expression(&a.expression.borrow(), table, scope),
        })),
        ast::Statement::ConstantDefinition(c) => {
            HirStatement::ConstantDefinition(Rc::new(HirConstantDefinition {
                name: c.name.name.clone(),
                type_info: TypeInfo::new(&c.ty),
                value: transform_literal(&c.value, table, scope),
            }))
        }
        ast::Statement::TypeDefinition(t) => {
            HirStatement::TypeDefinition(Rc::new(HirTypeDefinitionStatement {
                name: t.name.name.clone(),
                type_info: TypeInfo::new(&t.ty),
            }))
        }
    }
}

fn transform_block_type(
    bt: &ast::BlockType,
    table: &SymbolTable,
    scope: &ScopeRef,
) -> HirBlockType {
    match bt {
        ast::BlockType::Block(b) => HirBlockType::Block(Rc::new(transform_block(b, table, scope))),
        ast::BlockType::Assume(b) => {
            HirBlockType::Assume(Rc::new(transform_block(b, table, scope)))
        }
        ast::BlockType::Forall(b) => {
            HirBlockType::Forall(Rc::new(transform_block(b, table, scope)))
        }
        ast::BlockType::Exists(b) => {
            HirBlockType::Exists(Rc::new(transform_block(b, table, scope)))
        }
        ast::BlockType::Unique(b) => {
            HirBlockType::Unique(Rc::new(transform_block(b, table, scope)))
        }
    }
}

fn transform_expression(
    expr: &ast::Expression,
    table: &SymbolTable,
    scope: &ScopeRef,
) -> HirExpression {
    let type_info = table.infer_expr_type(scope.borrow().id, expr);

    match expr {
        ast::Expression::Binary(b) => {
            // b is Rc<BinaryExpression>
            HirExpression::Binary(Rc::new(HirBinaryExpression {
                left: transform_expression(&b.left.borrow(), table, scope),
                operator: map_operator(&b.operator),
                right: transform_expression(&b.right.borrow(), table, scope),
                type_info,
            }))
        }
        ast::Expression::Literal(l) => HirExpression::Literal(transform_literal(l, table, scope)),
        ast::Expression::FunctionCall(fc) => {
            let name = if let ast::Expression::Identifier(id) = &fc.function {
                id.name.clone()
            } else {
                String::new()
            };

            HirExpression::FunctionCall(Rc::new(HirFunctionCallExpression {
                name,
                function: transform_expression(&fc.function, table, scope),
                type_parameters: fc
                    .type_parameters
                    .as_ref()
                    .map(|tp| tp.iter().map(|t| t.name.clone()).collect()),
                arguments: fc.arguments.as_ref().map(|args| {
                    args.iter()
                        .map(|(n, e)| {
                            (
                                n.as_ref().map(|x| x.name.clone()),
                                transform_expression(&e.borrow(), table, scope),
                            )
                        })
                        .collect()
                }),
                type_info,
            }))
        }
        ast::Expression::Struct(s) => HirExpression::Struct(Rc::new(HirStructExpression {
            name: s.name.name.clone(),
            fields: s.fields.as_ref().map(|f| {
                f.iter()
                    .map(|(n, e)| {
                        (
                            n.name.clone(),
                            transform_expression(&e.borrow(), table, scope),
                        )
                    })
                    .collect()
            }),
            type_info,
        })),
        ast::Expression::MemberAccess(ma) => {
            HirExpression::MemberAccess(Rc::new(HirMemberAccessExpression {
                expression: transform_expression(&ma.expression.borrow(), table, scope),
                name: ma.name.name.clone(),
                type_info,
            }))
        }
        ast::Expression::ArrayIndexAccess(a) => {
            HirExpression::ArrayIndexAccess(Rc::new(HirArrayIndexAccessExpression {
                array: transform_expression(&a.array.borrow(), table, scope),
                index: transform_expression(&a.index.borrow(), table, scope),
                type_info,
            }))
        }
        ast::Expression::TypeMemberAccess(t) => {
            HirExpression::TypeMemberAccess(Rc::new(HirTypeMemberAccessExpression {
                expression: transform_expression(&t.expression.borrow(), table, scope),
                name: t.name.name.clone(),
                type_info,
            }))
        }
        ast::Expression::Parenthesized(p) => {
            HirExpression::Parenthesized(Rc::new(HirParenthesizedExpression {
                expression: transform_expression(&p.expression.borrow(), table, scope),
                type_info,
            }))
        }
        ast::Expression::PrefixUnary(u) => {
            HirExpression::PrefixUnary(Rc::new(HirPrefixUnaryExpression {
                expression: transform_expression(&u.expression.borrow(), table, scope),
                operator: match u.operator {
                    ast::UnaryOperatorKind::Neg => HirUnaryOperator::Neg,
                },
                type_info,
            }))
        }
        ast::Expression::Uzumaki(_) => {
            HirExpression::Uzumaki(Rc::new(HirUzumakiExpression { type_info }))
        }
        _ => HirExpression::TypeInfo(type_info),
    }
}

fn transform_literal(lit: &ast::Literal, table: &SymbolTable, scope: &ScopeRef) -> HirLiteral {
    match lit {
        ast::Literal::Bool(b) => HirLiteral::Bool(Rc::new(HirBoolLiteral { value: b.value })),
        ast::Literal::String(s) => HirLiteral::String(Rc::new(HirStringLiteral {
            value: s.value.clone(),
        })),
        ast::Literal::Number(n) => HirLiteral::Number(Rc::new(HirNumberLiteral {
            value: n.value.clone(),
            type_info: TypeInfo {
                kind: TypeInfoKind::Number(NumberTypeKindNumberType::I32),
                type_params: vec![],
            },
        })),
        ast::Literal::Unit(_) => HirLiteral::Unit(Rc::new(HirUnitLiteral {})),
        ast::Literal::Array(a) => {
            let elements = a.elements.as_ref().map(|el| {
                el.iter()
                    .map(|e| transform_expression(&e.borrow(), table, scope))
                    .collect::<Vec<_>>()
            });
            let type_info = if let Some(elems) = &elements {
                if let Some(first) = elems.first() {
                    let inner_type = first.type_info();
                    let array_len = u32::try_from(elems.len()).ok();
                    TypeInfo {
                        kind: TypeInfoKind::Array(Box::new(inner_type), array_len),
                        type_params: vec![],
                    }
                } else {
                    TypeInfo::default()
                }
            } else {
                TypeInfo::default()
            };
            HirLiteral::Array(Rc::new(HirArrayLiteral {
                elements,
                type_info,
            }))
        }
    }
}

fn map_operator(op: &ast::OperatorKind) -> HirOperatorKind {
    match op {
        ast::OperatorKind::Pow => HirOperatorKind::Pow,
        ast::OperatorKind::Add => HirOperatorKind::Add,
        ast::OperatorKind::Sub => HirOperatorKind::Sub,
        ast::OperatorKind::Mul => HirOperatorKind::Mul,
        ast::OperatorKind::Div => HirOperatorKind::Div,
        ast::OperatorKind::Mod => HirOperatorKind::Mod,
        ast::OperatorKind::And => HirOperatorKind::And,
        ast::OperatorKind::Or => HirOperatorKind::Or,
        ast::OperatorKind::Eq => HirOperatorKind::Eq,
        ast::OperatorKind::Ne => HirOperatorKind::Ne,
        ast::OperatorKind::Lt => HirOperatorKind::Lt,
        ast::OperatorKind::Le => HirOperatorKind::Le,
        ast::OperatorKind::Gt => HirOperatorKind::Gt,
        ast::OperatorKind::Ge => HirOperatorKind::Ge,
        ast::OperatorKind::BitAnd => HirOperatorKind::BitAnd,
        ast::OperatorKind::BitOr => HirOperatorKind::BitOr,
        ast::OperatorKind::BitXor => HirOperatorKind::BitXor,
        ast::OperatorKind::BitNot => HirOperatorKind::BitNot,
        ast::OperatorKind::Shl => HirOperatorKind::Shl,
        ast::OperatorKind::Shr => HirOperatorKind::Shr,
    }
}
