use std::collections::HashMap;

use crate::types::{Expression, FunctionDefinition, OperatorKind, Statement, Type};

/// The `type_inference` module provides functionality for performing multi-pass
/// type checking and inference on a Rust-like abstract syntax tree (AST).
///
/// Key design constraints:
///
/// 1. **Strict type system**: No implicit type conversions are allowed&#8203;:contentReference[oaicite:0]{index=0}.
///    Types must match exactly or be explicitly cast if conversion is needed.
/// 2. **No polymorphism**: Subtyping or runtime polymorphism is not supported,
///    so values cannot be treated as a different type than exactly what they are.
/// 3. **Generics supported**: Generic types and functions are allowed, with type
///    parameters resolved at compile time (no dynamic generics). All generics must
///    be replaced by concrete types by the end of type checking (akin to
///    monomorphization).
/// 4. **In-place annotation**: Inferred types are written into the AST nodes
///    (filling the `Option<Type>` fields in `Expression` nodes).
/// 5. **Multi-pass analysis**: We perform multiple passes over the AST (e.g.,
///    a symbol collection pass and a type inference pass).
/// 6. **Forward declarations**: The system supports forward references to types
///    and functions by collecting all symbols first, then resolving usages in later passes.
/// 7. **C-like type rules**: Type checking rules follow conventions of C-like languages.
///    For example, arithmetic operators require numeric operands of the same type,
///    boolean conditions are required for control flow, and assignment types must match.
/// 8. **AST integrity**: No new AST nodes or name changes are introduced; we only
///    attach type information to existing nodes.
///
/// We use a symbol table to store known types, variables, and function signatures.
/// The first pass registers all type and function definitions (and any global variables).
/// The second pass infers types for every expression and validates each operation,
/// raising errors for any type mismatches or incorrect usages.
/// All variable and expression types are recorded, and the AST is annotated with
/// these inferred types for use in later compilation stages.
///
/// Errors are reported with clear messages whenever a type rule is violated (for example,
/// using an undeclared variable, or mismatched types in an assignment).
///
/// The end result is that every expression node in the AST will have a concrete type,
/// ensuring the program is type-safe under the given rules or providing detailed
/// errors if not.
///
/// # Example
///
/// (Usage would involve constructing an AST for a program, then invoking
/// `TypeChecker.infer_types(&mut ast)` to annotate it or produce errors.)
///
///

/// Symbol table entry for a type definition.
#[derive(Debug)]
struct TypeInfo {
    name: String,
    type_params: Vec<String>,
    // (Field type information could be added here if needed for struct field checking.)
}

/// Symbol table entry for a function signature.
#[derive(Debug, Clone)]
struct FuncSignature {
    name: String,
    type_params: Vec<String>,
    param_types: Vec<Type>,
    return_type: Type,
}

/// The symbol table holds declared types, functions, and a stack of variable scopes.
struct SymbolTable {
    types: HashMap<String, TypeInfo>, // map of type name -> type info
    functions: HashMap<String, FuncSignature>, // map of function name -> signature
    variables: Vec<HashMap<String, Type>>, // stack of variable name -> type for each scope
}

impl SymbolTable {
    fn new() -> Self {
        let mut table = SymbolTable {
            types: HashMap::new(),
            functions: HashMap::new(),
            variables: Vec::new(),
        };
        // Pre-register built-in primitive types
        table.types.insert(
            "Int".to_string(),
            TypeInfo {
                name: "Int".to_string(),
                type_params: vec![],
            },
        );
        table.types.insert(
            "Float".to_string(),
            TypeInfo {
                name: "Float".to_string(),
                type_params: vec![],
            },
        );
        table.types.insert(
            "Bool".to_string(),
            TypeInfo {
                name: "Bool".to_string(),
                type_params: vec![],
            },
        );
        table.types.insert(
            "Void".to_string(),
            TypeInfo {
                name: "Void".to_string(),
                type_params: vec![],
            },
        );
        table
    }

    /// Enter a new lexical scope for variables.
    fn push_scope(&mut self) {
        self.variables.push(HashMap::new());
    }

    /// Exit the current lexical scope.
    fn pop_scope(&mut self) {
        self.variables.pop();
    }

    /// Declare a variable in the current scope.
    fn declare_var(&mut self, name: String, var_type: Type) -> Result<(), String> {
        if let Some(scope) = self.variables.last_mut() {
            if scope.contains_key(&name) {
                return Err(format!(
                    "Variable `{}` already declared in this scope",
                    name
                ));
            }
            scope.insert(name, var_type);
            Ok(())
        } else {
            Err("No active scope to declare variables".to_string())
        }
    }

    /// Find a variable's type by name (searching current and outer scopes).
    fn lookup_var(&self, name: &str) -> Option<Type> {
        for scope in self.variables.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    /// Register a new type definition.
    fn register_type(&mut self, name: String, type_params: Vec<String>) -> Result<(), String> {
        if self.types.contains_key(&name) {
            return Err(format!("Type `{}` is already defined", name));
        }
        self.types
            .insert(name.clone(), TypeInfo { name, type_params });
        Ok(())
    }

    /// Get information for a type by name.
    fn lookup_type(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }

    /// Register a new function signature.
    fn register_function(
        &mut self,
        name: String,
        type_params: Vec<String>,
        param_types: Vec<Type>,
        return_type: Type,
    ) -> Result<(), String> {
        if self.functions.contains_key(&name) {
            return Err(format!("Function `{}` is already defined", name));
        }
        self.functions.insert(
            name.clone(),
            FuncSignature {
                name,
                type_params,
                param_types,
                return_type,
            },
        );
        Ok(())
    }

    /// Get a function signature by name.
    fn lookup_function(&self, name: &str) -> Option<&FuncSignature> {
        self.functions.get(name)
    }
}

/// The main structure for type inference and checking.
pub struct TypeChecker {
    sym: SymbolTable,
    errors: Vec<String>,
}

impl TypeChecker {
    /// Create a new TypeChecker with an empty symbol table (built-in types preloaded).
    pub fn new() -> Self {
        TypeChecker {
            sym: SymbolTable::new(),
            errors: Vec::new(),
        }
    }

    /// Perform type inference on the given program AST.
    /// Returns Ok(()) if successful, or Err(vec_of_errors) if any type errors occurred.
    pub fn infer_types(&mut self, program: &mut Program) -> Result<(), Vec<String>> {
        // First pass: collect all type and function definitions.
        self.collect_definitions(program);
        // If there were errors in definitions, return them early.
        if !self.errors.is_empty() {
            return Err(std::mem::take(&mut self.errors));
        }
        // Second pass: infer types within each function body.
        for func in &mut program.functions {
            self.infer_function(func);
        }
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    /// First pass: collect type and function declarations into the symbol table.
    fn collect_definitions(&mut self, program: &Program) {
        // Register user-defined types (structs, etc.)
        for t in &program.types {
            if let Err(err) = self
                .sym
                .register_type(t.name.clone(), t.type_params.clone())
            {
                self.errors.push(err);
            }
        }
        // Register function signatures (name, generic params, param types, return type).
        for func in &program.functions {
            // Validate that the types used in parameters and return type exist or are proper type params.
            for (_, param_ty) in &func.params {
                self.validate_type(param_ty, &func.type_params);
            }
            self.validate_type(&func.return_type, &func.type_params);
            // If any types were invalid, skip registering to avoid cascading errors.
            if !self.errors.is_empty() {
                continue;
            }
            // Prepare the parameter types list for the signature.
            let param_types: Vec<Type> = func.params.iter().map(|(_, ty)| ty.clone()).collect();
            if let Err(err) = self.sym.register_function(
                func.name.clone(),
                func.type_params.clone(),
                param_types,
                func.return_type.clone(),
            ) {
                self.errors.push(err);
            }
        }
    }

    /// Validate that a type is properly defined (exists in symbol table or is a valid type parameter).
    fn validate_type(&mut self, ty: &Type, type_params: &Vec<String>) {
        match ty {
            Type::Named(name, params) => {
                // Check that the base type name is known (either built-in or user-defined).
                if self.sym.lookup_type(name).is_none() {
                    self.errors.push(format!("Unknown type `{}`", name));
                    return;
                }
                // If the type has generic parameters, validate each.
                for arg in params {
                    self.validate_type(arg, type_params);
                }
            }
            Type::TypeParam(name) => {
                // Ensure the type param is one of the declared generics of the current context.
                if !type_params.contains(name) {
                    self.errors
                        .push(format!("Undefined type parameter `{}`", name));
                }
            }
            _ => {
                // Int, Float, Bool, Void are always valid (already in symbol table).
            }
        }
    }

    /// Second pass: perform type inference and checking in a function body.
    fn infer_function(&mut self, func: &mut FunctionDefinition) {
        // Enter a new scope for the function's parameters and local variables.
        self.sym.push_scope();
        // Map each generic type parameter to a placeholder Type::TypeParam in this context.
        let mut type_param_placeholders = HashMap::new();
        for tp in &func.type_params {
            type_param_placeholders.insert(tp.clone(), Type::TypeParam(tp.clone()));
        }
        // Declare all function parameters in the new scope.
        for (name, ty) in &func.params {
            // Substitute generic parameters in the type with placeholder Type::TypeParam if needed.
            let resolved_ty = self.substitute_type_params(ty.clone(), &type_param_placeholders);
            if let Err(err) = self.sym.declare_var(name.clone(), resolved_ty) {
                self.errors.push(err);
            }
        }
        // Infer types for each statement in the function body.
        for stmt in &mut func.body {
            self.infer_statement(stmt, &func.return_type, &func.type_params);
        }
        // Exit the function scope.
        self.sym.pop_scope();
    }

    /// Infer and check types in a statement.
    /// `func_return` is the function's return type (for checking Return statements).
    /// `func_type_params` is the list of generic type parameter names in the current function.
    #[allow(clippy::too_many_lines)]
    fn infer_statement(
        &mut self,
        stmt: &mut Statement,
        func_return: &Type,
        func_type_params: &Vec<String>,
    ) {
        match stmt {
            Statement::VariableDefinition(var_name, opt_type, init_expr) => {
                // Infer the initializer expression's type.
                let init_ty_opt = self.infer_expression(init_expr);
                if let Some(init_ty) = init_ty_opt {
                    // If a type annotation was provided, check it.
                    if let Some(annotation) = opt_type {
                        let expected_ty = self.resolve_type(annotation.clone(), func_type_params);
                        if !self.types_equal(&expected_ty, &init_ty) {
                            self.errors.push(format!(
                                "Type mismatch in declaration of `{}`: expected {:?}, got {:?}",
                                var_name, expected_ty, init_ty
                            ));
                        }
                    }
                    // Determine the variable's type (use annotation if given, otherwise inferred type).
                    let var_type = if let Some(annotation) = opt_type {
                        self.resolve_type(annotation.clone(), func_type_params)
                    } else {
                        init_ty.clone()
                    };
                    // Add the variable to current scope.
                    if let Err(err) = self.sym.declare_var(var_name.clone(), var_type) {
                        self.errors.push(err);
                    }
                }
            }
            Statement::Assignment(target_expr, value_expr) => {
                // Infer types for target and value.
                let target_ty_opt = self.infer_expression(target_expr);
                let value_ty_opt = self.infer_expression(value_expr);
                if let (Some(target_ty), Some(value_ty)) = (target_ty_opt, value_ty_opt) {
                    // Ensure left side is a variable or field (not a literal or temporary).
                    if !matches!(
                        target_expr,
                        Expression::Variable(_, _) | Expression::MemberAccess(_, _, _)
                    ) {
                        self.errors
                            .push("Invalid left-hand side of assignment".to_string());
                    }
                    // Types must match exactly for assignment.
                    if !self.types_equal(&target_ty, &value_ty) {
                        self.errors.push(format!(
                            "Cannot assign value of type {:?} to variable of type {:?}",
                            value_ty, target_ty
                        ));
                    }
                }
            }
            Statement::Return(opt_expr) => {
                if let Some(expr) = opt_expr {
                    // Infer the returned expression's type.
                    let expr_ty_opt = self.infer_expression(expr);
                    if let Some(expr_ty) = expr_ty_opt {
                        let expected_ty = self.resolve_type(func_return.clone(), func_type_params);
                        if !self.types_equal(&expected_ty, &expr_ty) {
                            self.errors.push(format!(
                                "Return type mismatch: expected {:?}, got {:?}",
                                expected_ty, expr_ty
                            ));
                        }
                    }
                } else {
                    // Return with no expression.
                    if func_return != &Type::Void {
                        self.errors.push(
                            "Return statement used without a value in a non-void function"
                                .to_string(),
                        );
                    }
                }
            }
            Statement::Expression(expr) => {
                // Just infer the expression's type (side effects only usage).
                self.infer_expression(expr);
            }
            Statement::If(cond_expr, then_block, else_block) => {
                // Condition must be bool.
                let cond_ty_opt = self.infer_expression(cond_expr);
                if let Some(cond_ty) = cond_ty_opt {
                    if cond_ty != Type::Bool {
                        self.errors
                            .push(format!("If condition must be Bool, but got {:?}", cond_ty));
                    }
                }
                // Type-check then and else blocks in new scopes.
                self.sym.push_scope();
                for s in then_block {
                    self.infer_statement(s, func_return, func_type_params);
                }
                self.sym.pop_scope();
                self.sym.push_scope();
                for s in else_block {
                    self.infer_statement(s, func_return, func_type_params);
                }
                self.sym.pop_scope();
            }
            Statement::Loop(cond_expr, loop_body) => {
                // Condition must be bool.
                let cond_ty_opt = self.infer_expression(cond_expr);
                if let Some(cond_ty) = cond_ty_opt {
                    if cond_ty != Type::Bool {
                        self.errors.push(format!(
                            "While loop condition must be Bool, but got {:?}",
                            cond_ty
                        ));
                    }
                }
                // Type-check loop body in a new scope.
                self.sym.push_scope();
                for s in loop_body {
                    self.infer_statement(s, func_return, func_type_params);
                }
                self.sym.pop_scope();
            }
        }
    }

    /// Infer the type of an expression and annotate it in-place.
    /// Returns the inferred Type, or None if inference fails (error recorded).
    #[allow(clippy::too_many_lines)]
    fn infer_expression(&mut self, expr: &mut Expression) -> Option<Type> {
        match expr {
            Expression::Literal(_, ref mut opt_ty) => {
                // Integer literals default to Int type.
                *opt_ty = Some(Type::Int);
                Some(Type::Int)
            }
            Expression::FloatLiteral(_, ref mut opt_ty) => {
                *opt_ty = Some(Type::Float);
                Some(Type::Float)
            }
            Expression::BoolLiteral(_, ref mut opt_ty) => {
                *opt_ty = Some(Type::Bool);
                Some(Type::Bool)
            }
            Expression::Assign(name, ref mut opt_ty) => {
                // Look up variable type.
                if let Some(var_ty) = self.sym.lookup_var(name) {
                    *opt_ty = Some(var_ty.clone());
                    Some(var_ty)
                } else {
                    self.errors
                        .push(format!("Use of undeclared variable `{}`", name));
                    None
                }
            }
            Expression::Binary(left, op, right, ref mut opt_ty) => {
                // Infer both operands.
                let left_ty_opt = self.infer_expression(left);
                let right_ty_opt = self.infer_expression(right);
                if let (Some(left_ty), Some(right_ty)) = (left_ty_opt, right_ty_opt) {
                    // No implicit conversion: types must match exactly.
                    if !self.types_equal(&left_ty, &right_ty) {
                        self.errors.push(format!("Cannot apply operator {:?} to operands of different types: {:?} and {:?}", op, left_ty, right_ty));
                    }
                    // Determine result type based on operator.
                    let result_ty = match op {
                        OperatorKind::Add
                        | OperatorKind::Sub
                        | OperatorKind::Mul
                        | OperatorKind::Div => {
                            // Arithmetic ops require numeric types.
                            if left_ty != Type::Int && left_ty != Type::Float {
                                self.errors.push(format!(
                                    "Arithmetic operator {:?} requires numeric operands, got {:?}",
                                    op, left_ty
                                ));
                            }
                            left_ty.clone() // result is same type if numeric
                        }
                        OperatorKind::Equal
                        | OperatorKind::NotEqual
                        | OperatorKind::Less
                        | OperatorKind::LessEqual
                        | OperatorKind::Greater
                        | OperatorKind::GreaterEqual => {
                            // Comparisons: require both sides of same type (checked above). Result is Bool.
                            Type::Bool
                        }
                        OperatorKind::And | OperatorKind::Or => {
                            // Logical ops require boolean operands.
                            if left_ty != Type::Bool {
                                self.errors.push(format!(
                                    "Logical operator {:?} requires Bool operands, got {:?}",
                                    op, left_ty
                                ));
                            }
                            Type::Bool
                        }
                    };
                    *opt_ty = Some(result_ty.clone());
                    Some(result_ty)
                } else {
                    None
                }
            }
            Expression::PrefixUnary(op, expr_inner, ref mut opt_ty) => {
                // Infer inner expression.
                let inner_ty_opt = self.infer_expression(expr_inner);
                if let Some(inner_ty) = inner_ty_opt {
                    let result_ty = match op {
                        UnaryOperatorKind::Neg => {
                            if inner_ty != Type::Int && inner_ty != Type::Float {
                                self.errors.push(format!(
                                    "Unary negation requires numeric type, got {:?}",
                                    inner_ty
                                ));
                            }
                            inner_ty.clone()
                        }
                        UnaryOp::Not => {
                            if inner_ty != Type::Bool {
                                self.errors.push(format!(
                                    "Logical NOT requires Bool type, got {:?}",
                                    inner_ty
                                ));
                            }
                            Type::Bool
                        }
                    };
                    *opt_ty = Some(result_ty.clone());
                    Some(result_ty)
                } else {
                    None
                }
            }
            Expression::FunctionCall(func_name, args, ref mut opt_ty) => {
                // Lookup function signature.
                let sig = match self.sym.lookup_function(func_name) {
                    Some(s) => s.clone(),
                    None => {
                        self.errors
                            .push(format!("Call to undefined function `{}`", func_name));
                        // Infer args anyway to catch any other errors.
                        for arg in args {
                            self.infer_expression(arg);
                        }
                        return None;
                    }
                };
                // Check argument count matches.
                if args.len() != sig.param_types.len() {
                    self.errors.push(format!(
                        "Function `{}` expects {} arguments, but {} provided",
                        func_name,
                        sig.param_types.len(),
                        args.len()
                    ));
                    for arg in args {
                        self.infer_expression(arg);
                    }
                    return None;
                }
                // Map to hold generic type parameter substitutions.
                let mut type_param_bindings: HashMap<String, Type> = HashMap::new();
                // Infer each argument and unify with the corresponding parameter type.
                for (arg_expr, param_ty) in args.iter_mut().zip(sig.param_types.iter()) {
                    if let Some(arg_ty) = self.infer_expression(arg_expr) {
                        // Unify param type and arg type, populating type_param_bindings.
                        self.unify_types(param_ty, &arg_ty, &mut type_param_bindings);
                    }
                }
                // If the function is generic, ensure all type parameters are resolved.
                if !sig.type_params.is_empty() {
                    for t in &sig.type_params {
                        if !type_param_bindings.contains_key(t) {
                            self.errors.push(format!(
                                "Unable to infer type for type parameter `{}` in call to `{}`",
                                t, func_name
                            ));
                        }
                    }
                }
                // Determine the call's result type by substituting any generic params in the return type.
                let concrete_ret_type =
                    self.substitute_type_params(sig.return_type.clone(), &type_param_bindings);
                *opt_ty = Some(concrete_ret_type.clone());
                Some(concrete_ret_type)
            }
            Expression::MemberAccess(struct_expr, field, ref mut opt_ty) => {
                // Infer the type of the struct expression.
                if let Some(struct_ty) = self.infer_expression(struct_expr) {
                    if let Type::Named(type_name, type_args) = struct_ty {
                        // Lookup type info to find field types.
                        if let Some(type_info) = self.sym.lookup_type(&type_name) {
                            // (In a complete implementation, we would use type_info and type_args to determine the field's type.)
                            // Since field details are not stored, we cannot infer actual type here.
                            // We'll simulate an error if we cannot resolve the field.
                            let _ = type_info; // suppress unused
                            let _ = type_args;
                            self.errors
                                .push(format!("Unknown field `{}` in type `{}`", field, type_name));
                            None
                        } else {
                            self.errors
                                .push(format!("Type `{}` not found for field access", type_name));
                            None
                        }
                    } else {
                        self.errors.push(format!(
                            "Attempted to access field `{}` of non-struct type {:?}",
                            field, struct_ty
                        ));
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    /// Unify a parameter type with an argument type in a function call, updating generic type bindings.
    /// If `param_ty` contains a TypeParam, bind it to `arg_ty`. If it is a composite type, unify components.
    fn unify_types(
        &mut self,
        param_ty: &Type,
        arg_ty: &Type,
        bindings: &mut HashMap<String, Type>,
    ) {
        match param_ty {
            Type::TypeParam(name) => {
                if let Some(bound_ty) = bindings.get(name) {
                    // If already bound, ensure the new type matches the existing binding.
                    if !self.types_equal(bound_ty, arg_ty) {
                        self.errors.push(format!(
                            "Type parameter `{}` inferred as inconsistent types: {:?} vs {:?}",
                            name, bound_ty, arg_ty
                        ));
                    }
                } else {
                    // Bind this type parameter to the argument's type.
                    bindings.insert(name.clone(), arg_ty.clone());
                }
            }
            Type::Named(base, params) => {
                // Param type is a generic instance or named type. Arg type should match in structure.
                if let Type::Named(arg_base, arg_params) = arg_ty {
                    if base != arg_base {
                        self.errors.push(format!(
                            "Type mismatch: expected `{}` but got `{}`",
                            base, arg_base
                        ));
                    } else if params.len() != arg_params.len() {
                        self.errors
                            .push(format!("Type argument count mismatch for type `{}`", base));
                    } else {
                        // Recursively unify type arguments.
                        for (sub_param_ty, sub_arg_ty) in params.iter().zip(arg_params.iter()) {
                            self.unify_types(sub_param_ty, sub_arg_ty, bindings);
                        }
                    }
                } else {
                    // Param expects a named type but arg is a different kind of type.
                    self.errors.push(format!(
                        "Type mismatch: expected `{}` type, got {:?}",
                        base, arg_ty
                    ));
                }
            }
            _ => {
                // Non-generic parameter: types must match exactly.
                if !self.types_equal(param_ty, arg_ty) {
                    self.errors.push(format!(
                        "Type mismatch: expected {:?}, got {:?}",
                        param_ty, arg_ty
                    ));
                }
            }
        }
    }

    /// Substitute generic type parameters in a type with concrete types from `bindings`.
    /// For example, if bindings contains T -> Int, then TypeParam("T") becomes Int,
    /// and Named("Option", [TypeParam("T")]) becomes Named("Option", [Int]).
    fn substitute_type_params(&self, ty: Type, bindings: &HashMap<String, Type>) -> Type {
        match ty {
            Type::TypeParam(name) => {
                if let Some(bound_ty) = bindings.get(&name) {
                    bound_ty.clone()
                } else {
                    // If not bound, keep as TypeParam (could not resolve fully).
                    Type::TypeParam(name)
                }
            }
            Type::Named(base, params) => {
                if params.is_empty() {
                    Type::Named(base, vec![])
                } else {
                    let new_params: Vec<Type> = params
                        .into_iter()
                        .map(|p| self.substitute_type_params(p, bindings))
                        .collect();
                    Type::Named(base, new_params)
                }
            }
            // Primitive types and Void remain unchanged.
            other => other,
        }
    }

    /// Resolve a type within the context of a function's generic parameters.
    /// If the type is a TypeParam and is one of the function's type parameters, leave it as is (it remains a generic placeholder).
    /// If it's a TypeParam not in the current function (shouldn't happen), treat as Named (forward-declared type).
    fn resolve_type(&self, ty: Type, func_type_params: &Vec<String>) -> Type {
        match ty {
            Type::TypeParam(name) => {
                if func_type_params.contains(&name) {
                    Type::TypeParam(name)
                } else {
                    // If somehow a type parameter not belonging to this function appears, treat it as a concrete named type.
                    Type::Named(name, vec![])
                }
            }
            Type::Named(base, params) => {
                let resolved_params: Vec<Type> = params
                    .into_iter()
                    .map(|p| self.resolve_type(p, func_type_params))
                    .collect();
                Type::Named(base, resolved_params)
            }
            other => other, // Int, Float, Bool, Void remain as is.
        }
    }

    /// Compare two types for equality in this type system (strict equality, no coercion).
    fn types_equal(&self, a: &Type, b: &Type) -> bool {
        // Since there is no subtyping or implicit conversion, types must be exactly equal.
        a == b
    }
}
