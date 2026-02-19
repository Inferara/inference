# Type Checker API Guide

This guide provides practical examples and patterns for using the type checker API in your code.

## Table of Contents

- [Basic Usage](#basic-usage)
- [Querying Type Information](#querying-type-information)
- [Working with TypeInfo](#working-with-typeinfo)
- [Error Handling](#error-handling)
- [Advanced Patterns](#advanced-patterns)
- [Integration Examples](#integration-examples)

## Basic Usage

### Running the Type Checker

The primary entry point is `TypeCheckerBuilder`:

```rust
use inference_ast::arena::Arena;
use inference_type_checker::TypeCheckerBuilder;

// Assume you have an Arena from parsing
let arena: Arena = parse_source(source_code)?;

// Run type checking
let result = TypeCheckerBuilder::build_typed_context(arena)?;

// Extract the typed context
let typed_context = result.typed_context();
```

### Typestate Pattern

The `TypeCheckerBuilder` uses the typestate pattern to ensure type checking completes before accessing results:

```rust
// Initial state
let builder = TypeCheckerBuilder::<TypeCheckerInitState>::new();

// Can only call build_typed_context() in InitState
let completed_builder = TypeCheckerBuilder::build_typed_context(arena)?;
// completed_builder is now TypeCheckerBuilder<TypeCheckerCompleteState>

// Can only call typed_context() in CompleteState
let context = completed_builder.typed_context();
```

This design prevents accessing the typed context before type checking runs.

## Querying Type Information

### Getting Type Information for a Node

```rust
use inference_type_checker::type_info::TypeInfo;

// Get type info by node ID
let node_id: u32 = /* from AST node */;

if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
    println!("Node {} has type: {}", node_id, type_info);
} else {
    println!("No type information for node {}", node_id);
}
```

### Type Checking Helpers

```rust
// Check if a node is a specific type
if typed_context.is_node_i32(node_id) {
    println!("Node is i32");
}

if typed_context.is_node_i64(node_id) {
    println!("Node is i64");
}
```

### Finding Nodes by Type

```rust
use inference_ast::nodes::{AstNode, Expression, Literal};

// Find all numeric literals
let number_literals = typed_context.filter_nodes(|node| {
    matches!(
        node,
        AstNode::Expression(Expression::Literal(Literal::Number(_)))
    )
});

// Check their types
for node in number_literals {
    if let AstNode::Expression(Expression::Literal(Literal::Number(lit))) = node {
        let type_info = typed_context.get_node_typeinfo(lit.id);
        println!("Number literal {} has type: {:?}", lit.value, type_info);
    }
}
```

### Getting Function Definitions

```rust
// Get all function definitions
let functions = typed_context.functions();

for func in functions {
    println!("Function: {}", func.name());

    // Check return type
    if let Some(return_type_node) = &func.returns {
        let return_type = typed_context.get_node_typeinfo(return_type_node.id());
        println!("  Returns: {:?}", return_type);
    }

    // Check parameters
    if let Some(arguments) = &func.arguments {
        for arg in arguments {
            let arg_type = typed_context.get_node_typeinfo(arg.id());
            println!("  Param: {:?}", arg_type);
        }
    }
}
```

### Accessing Source Files

```rust
// Get all source files in the arena
let source_files = typed_context.source_files();

for source_file in source_files {
    println!("File: {}", source_file.name);

    // Iterate over definitions
    for definition in &source_file.definitions {
        match definition {
            Definition::Function(func) => {
                println!("  Function: {}", func.name());
            }
            Definition::Struct(struct_def) => {
                println!("  Struct: {}", struct_def.name());
            }
            // ... other definition types
            _ => {}
        }
    }
}
```

## Working with TypeInfo

### Type Information Structure

```rust
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind, NumberType};

// TypeInfo has two main components:
// - kind: The actual type (primitive, compound, etc.)
// - type_params: Generic type parameters (if any)

let type_info = typed_context.get_node_typeinfo(node_id)?;

match &type_info.kind {
    TypeInfoKind::Unit => println!("Unit type"),
    TypeInfoKind::Bool => println!("Boolean"),
    TypeInfoKind::String => println!("String"),
    TypeInfoKind::Number(num_type) => {
        println!("Number type: {}", num_type.as_str());
    }
    TypeInfoKind::Array(elem_type, size) => {
        println!("Array of {} with size {}", elem_type, size);
    }
    TypeInfoKind::Struct(name) => {
        println!("Struct: {}", name);
    }
    TypeInfoKind::Enum(name) => {
        println!("Enum: {}", name);
    }
    TypeInfoKind::Generic(name) => {
        println!("Generic type parameter: {}", name);
    }
    _ => println!("Other type: {}", type_info.kind),
}
```

### Creating TypeInfo from AST Types

```rust
use inference_ast::nodes::Type;
use inference_type_checker::type_info::TypeInfo;

// Convert AST Type to TypeInfo
let ast_type: Type = /* from AST */;
let type_info = TypeInfo::new(&ast_type);

// With type parameters (for generic contexts)
let type_params = vec!["T".to_string(), "U".to_string()];
let type_info = TypeInfo::new_with_type_params(&ast_type, &type_params);
```

### Type Checking Predicates

```rust
let type_info = typed_context.get_node_typeinfo(node_id)?;

// Check type categories
if type_info.is_number() {
    println!("This is a numeric type");
}

if type_info.is_bool() {
    println!("This is a boolean");
}

if type_info.is_array() {
    println!("This is an array type");
}

if type_info.is_struct() {
    println!("This is a struct");
}

if type_info.is_generic() {
    println!("This is a generic type parameter");
}

// Check for signed integers (supports negation)
if type_info.is_signed_integer() {
    println!("This is a signed integer (i8/i16/i32/i64)");
}
```

### Working with Number Types

```rust
use inference_type_checker::type_info::NumberType;

// Iterate over all number types
for num_type in NumberType::ALL {
    println!("Number type: {}", num_type.as_str());
}

// Check if signed
let i32_type = NumberType::I32;
if i32_type.is_signed() {
    println!("i32 is signed");
}

// Parse from string
use std::str::FromStr;

match NumberType::from_str("i64") {
    Ok(num_type) => println!("Parsed: {}", num_type.as_str()),
    Err(_) => println!("Not a valid number type"),
}
```

### Type Substitution for Generics

```rust
use rustc_hash::FxHashMap;
use inference_type_checker::type_info::{TypeInfo, TypeInfoKind};

// Create a substitution map
let mut substitutions = FxHashMap::default();
substitutions.insert(
    "T".to_string(),
    TypeInfo {
        kind: TypeInfoKind::Number(NumberType::I32),
        type_params: vec![],
    },
);

// Substitute type parameters
let generic_type = TypeInfo {
    kind: TypeInfoKind::Generic("T".to_string()),
    type_params: vec![],
};

let concrete_type = generic_type.substitute(&substitutions);
// Result: TypeInfo { kind: Number(I32), type_params: [] }

// Works recursively for compound types
let array_of_generic = TypeInfo {
    kind: TypeInfoKind::Array(Box::new(generic_type), 10),
    type_params: vec![],
};

let array_of_concrete = array_of_generic.substitute(&substitutions);
// Result: TypeInfo { kind: Array(Box<i32>, 10), type_params: [] }
```

### Checking for Unresolved Type Parameters

```rust
let type_info = typed_context.get_node_typeinfo(node_id)?;

if type_info.has_unresolved_params() {
    println!("Warning: Type has unresolved generic parameters");
}
```

## Error Handling

### Handling Type Check Errors

```rust
use inference_type_checker::TypeCheckerBuilder;
use inference_type_checker::errors::TypeCheckError;

match TypeCheckerBuilder::build_typed_context(arena) {
    Ok(completed_builder) => {
        let typed_context = completed_builder.typed_context();
        // Type checking succeeded
    }
    Err(e) => {
        // Type checking failed with error
        eprintln!("Type check error: {}", e);

        // Error message contains all collected errors
        // Example: "type mismatch in return: expected `i32`, found `bool`; use of undeclared variable `x`"
    }
}
```

### Understanding Error Types

The type checker produces 29 different error variants. Here are the most common:

```rust
use inference_type_checker::errors::{
    TypeCheckError,
    TypeMismatchContext,
    RegistrationKind,
    VisibilityContext
};

// Common error patterns:

// 1. Type Mismatch
TypeCheckError::TypeMismatch {
    expected: TypeInfo { kind: Number(I32), ... },
    found: TypeInfo { kind: Bool, ... },
    context: TypeMismatchContext::Return,
    location: Location { ... }
}
// Message: "expected `i32`, found `bool` in return statement"

// 2. Unknown Identifier
TypeCheckError::UnknownIdentifier {
    name: "undefined_var".to_string(),
    location: Location { ... }
}
// Message: "use of undeclared variable `undefined_var`"

// 3. Undefined Function
TypeCheckError::UndefinedFunction {
    name: "unknown_func".to_string(),
    location: Location { ... }
}
// Message: "call to undefined function `unknown_func`"

// 4. Visibility Violation
TypeCheckError::VisibilityViolation {
    context: VisibilityContext::Function { name: "private_fn".to_string() },
    location: Location { ... }
}
// Message: "function `private_fn` is private"
```

### Error Location Information

All errors include location information:

```rust
// Errors have a location field
match error {
    TypeCheckError::TypeMismatch { location, .. } => {
        println!("Error at {}:{}", location.start.line, location.start.column);
    }
    TypeCheckError::UnknownIdentifier { location, .. } => {
        println!("Error at {}:{}", location.start.line, location.start.column);
    }
    // ... all variants have location
}
```

## Advanced Patterns

### Verifying All Expressions Have Types

```rust
// The type checker includes a debugging assertion that verifies
// all value expressions have type information after checking.

let typed_context = TypeCheckerBuilder::build_typed_context(arena)?
    .typed_context();

// In debug builds, this verification happens automatically
// You can also manually check:
let untyped = typed_context.find_untyped_expressions();

if !untyped.is_empty() {
    for missing in &untyped {
        eprintln!(
            "BUG: Expression {} at {} has no type",
            missing.kind,
            missing.location
        );
    }
    panic!("Type checker bug: expressions without type info");
}
```

### Getting Parent Nodes

```rust
// Get the parent of a node
if let Some(parent) = typed_context.get_parent_node(node_id) {
    println!("Parent node: {:?}", parent);

    // You can traverse up the tree
    let mut current_id = node_id;
    while let Some(parent) = typed_context.get_parent_node(current_id) {
        println!("Ancestor: {:?}", parent);
        current_id = parent.id();
    }
}
```

### Custom Node Filtering

```rust
use inference_ast::nodes::{AstNode, Statement};

// Find all variable definitions
let var_defs = typed_context.filter_nodes(|node| {
    matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
});

// Find all binary operations
let binary_ops = typed_context.filter_nodes(|node| {
    matches!(node, AstNode::Expression(Expression::Binary(_)))
});

// Complex filtering with multiple conditions
let filtered = typed_context.filter_nodes(|node| {
    match node {
        AstNode::Expression(Expression::Literal(Literal::Number(num))) => {
            // Only numeric literals with value > 100
            num.value.parse::<i32>().unwrap_or(0) > 100
        }
        _ => false
    }
});
```

## Integration Examples

### Code Generator Integration

```rust
use inference_type_checker::TypeCheckerBuilder;
use inference_wasm_codegen::CodeGenerator;

// Parse and type-check
let arena = parse_source(source_code)?;
let typed_context = TypeCheckerBuilder::build_typed_context(arena)?
    .typed_context();

// Pass to code generator
let codegen = CodeGenerator::new(typed_context);
let wasm_module = codegen.generate()?;
```

### REPL Integration

```rust
use inference_type_checker::TypeCheckerBuilder;

fn repl() {
    loop {
        // Read input
        let input = read_line()?;

        // Parse
        let arena = parse_source(input)?;

        // Type check
        match TypeCheckerBuilder::build_typed_context(arena) {
            Ok(completed) => {
                let typed_context = completed.typed_context();
                println!("Type check passed");

                // Execute or display type info
                display_types(&typed_context);
            }
            Err(e) => {
                eprintln!("Type error: {}", e);
            }
        }
    }
}
```

### Testing Patterns

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use inference_ast::builder::build_ast;
    use inference_type_checker::TypeCheckerBuilder;

    fn type_check(source: &str) -> anyhow::Result<TypedContext> {
        let arena = build_ast(source.to_string());
        Ok(TypeCheckerBuilder::build_typed_context(arena)?
            .typed_context())
    }

    #[test]
    fn test_valid_code() {
        let source = r#"fn test() -> i32 { return 42; }"#;
        let ctx = type_check(source).expect("Should type check");

        // Query types
        let functions = ctx.functions();
        assert_eq!(functions.len(), 1);
    }

    #[test]
    fn test_invalid_code() {
        let source = r#"fn test() -> i32 { return true; }"#;
        let result = type_check(source);

        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("type mismatch"));
        assert!(error.contains("expected `i32`, found `bool`"));
    }
}
```

### Diagnostic Collection

```rust
use inference_type_checker::TypeCheckerBuilder;

fn collect_diagnostics(source_code: &str) -> Vec<String> {
    let arena = parse_source(source_code).unwrap();

    match TypeCheckerBuilder::build_typed_context(arena) {
        Ok(_) => vec![],  // No errors
        Err(e) => {
            // Error message contains all errors separated by "; "
            e.to_string()
                .split("; ")
                .map(|s| s.to_string())
                .collect()
        }
    }
}

// Usage
let diagnostics = collect_diagnostics(source);
for (i, diagnostic) in diagnostics.iter().enumerate() {
    println!("[{}] {}", i + 1, diagnostic);
}
```

## Best Practices

### 1. Always Use TypedContext for Node Queries

```rust
// ❌ Don't create a new arena
let arena = build_ast(source);  // Creates new node IDs
let typed_context = type_check(arena)?;
let new_arena = build_ast(source);  // Different IDs!
let node = new_arena.find_node(id);  // Won't match typed_context

// ✅ Use the arena from typed_context
let typed_context = type_check(arena)?;
let nodes = typed_context.filter_nodes(predicate);  // Uses correct IDs
```

### 2. Handle Type Checking Errors Gracefully

```rust
// ❌ Don't panic on errors
let typed_context = TypeCheckerBuilder::build_typed_context(arena)
    .unwrap()  // Panics on type errors
    .typed_context();

// ✅ Handle errors properly
match TypeCheckerBuilder::build_typed_context(arena) {
    Ok(completed) => {
        let typed_context = completed.typed_context();
        // Continue with valid typed context
    }
    Err(e) => {
        // Report error to user
        eprintln!("Type checking failed: {}", e);
        // Return or handle gracefully
    }
}
```

### 3. Use Type Predicates for Clarity

```rust
// ❌ Match on kind directly
if matches!(type_info.kind, TypeInfoKind::Number(_)) { /* ... */ }

// ✅ Use predicate methods
if type_info.is_number() { /* ... */ }
```

### 4. Check for Type Info Before Using

```rust
// ❌ Assume type info exists
let type_info = typed_context.get_node_typeinfo(node_id).unwrap();

// ✅ Handle missing type info
if let Some(type_info) = typed_context.get_node_typeinfo(node_id) {
    // Use type_info safely
} else {
    // Handle missing type info (structural node, not a value)
}
```

## Common Patterns

### Pattern: Type-Directed Code Generation

```rust
fn generate_code(node_id: u32, typed_context: &TypedContext) -> String {
    let type_info = typed_context.get_node_typeinfo(node_id)
        .expect("Node should have type info");

    match &type_info.kind {
        TypeInfoKind::Number(NumberType::I32) => {
            // Generate i32-specific code
            "i32.const".to_string()
        }
        TypeInfoKind::Bool => {
            // Generate bool-specific code
            "i32.const".to_string()  // WASM represents bool as i32
        }
        // ... other cases
        _ => unimplemented!("Type not yet supported"),
    }
}
```

### Pattern: Validation Pass

```rust
fn validate_code(typed_context: &TypedContext) -> Vec<String> {
    let mut warnings = Vec::new();

    // Find all function returns
    for source_file in typed_context.source_files() {
        for definition in &source_file.definitions {
            if let Definition::Function(func) = definition {
                // Check for unused return value
                if func.returns.is_some() {
                    warnings.push(format!(
                        "Function {} returns a value",
                        func.name()
                    ));
                }
            }
        }
    }

    warnings
}
```

### Pattern: Type-Based Optimization

```rust
fn can_optimize(expr_id: u32, typed_context: &TypedContext) -> bool {
    let type_info = typed_context.get_node_typeinfo(expr_id)?;

    // Optimize i32 operations
    if typed_context.is_node_i32(expr_id) {
        return true;
    }

    // Don't optimize generic types
    if type_info.has_unresolved_params() {
        return false;
    }

    false
}
```

## Troubleshooting

### Issue: Missing Type Information

**Problem**: `get_node_typeinfo()` returns `None` for a node.

**Possible Causes**:
1. The node is structural (like `Expression::Type` in a type annotation)
2. The node is an identifier that's a name, not a value reference
3. Type checking failed for that node (check errors)

**Solution**: Check if the node is a value expression:
```rust
// Only value expressions get type info
if is_value_expression(node) {
    let type_info = typed_context.get_node_typeinfo(node.id())
        .expect("Value expressions should have type info");
}
```

### Issue: Node ID Mismatch

**Problem**: Node IDs don't match between arena and typed context.

**Cause**: Creating a new arena after type checking.

**Solution**: Always use the arena from `TypedContext`:
```rust
let typed_context = type_check(arena)?;
// Don't create new arena - use typed_context for queries
```

### Issue: Generic Type Not Substituted

**Problem**: Generic type parameter appears in generated code.

**Cause**: Type substitution not applied at call site.

**Solution**: Manually substitute if needed:
```rust
let substitutions = build_substitution_map(call_site);
let concrete_type = generic_type.substitute(&substitutions);
```

## Performance Considerations

### Efficient Node Queries

The type checker uses `FxHashMap` for O(1) lookups:

```rust
// Fast type lookup by node ID
let type_info = typed_context.get_node_typeinfo(node_id); // O(1)

// Predicate helpers are also O(1)
if typed_context.is_node_i32(node_id) { /* ... */ }
```

### Minimizing Allocations

When querying multiple types, clone only when necessary:

```rust
// Clone only when storing
let nodes_and_types: Vec<(u32, TypeInfo)> = nodes
    .iter()
    .filter_map(|&node_id| {
        typed_context.get_node_typeinfo(node_id)
            .map(|type_info| (node_id, type_info))
    })
    .collect();

// Use references when just checking
for node_id in &nodes {
    if let Some(type_info) = typed_context.get_node_typeinfo(*node_id) {
        println!("Type: {}", type_info);  // Displays, doesn't clone
    }
}
```

### Arena-Based Iteration

The arena provides efficient iteration without allocations:

```rust
// Efficient filtering without intermediate collections
let binary_ops = typed_context.filter_nodes(|node| {
    matches!(node, AstNode::Expression(Expression::Binary(_)))
});

// Filter predicate runs once per node, result collected efficiently
```

## Integration Patterns

### Error Reporting with Source Context

```rust
use inference_type_checker::TypeCheckerBuilder;

fn type_check_with_diagnostics(source_code: &str) {
    let arena = parse_source(source_code).unwrap();

    match TypeCheckerBuilder::build_typed_context(arena) {
        Ok(builder) => {
            let typed_context = builder.typed_context();
            println!("Type checking passed!");
        }
        Err(e) => {
            eprintln!("Type checking errors:");
            eprintln!("{}", e);

            // Parse individual errors if needed
            for (idx, error_msg) in e.to_string().split("; ").enumerate() {
                eprintln!("  [{}] {}", idx + 1, error_msg);
            }
        }
    }
}
```

### Progressive Type Checking

```rust
// Type check multiple files separately
fn type_check_files(files: &[String]) -> Vec<Result<TypedContext, anyhow::Error>> {
    files.iter().map(|file| {
        let source = std::fs::read_to_string(file)?;
        let arena = parse_source(&source)?;

        TypeCheckerBuilder::build_typed_context(arena)
            .map(|builder| builder.typed_context())
    }).collect()
}
```

## Further Reading

- [Architecture Documentation](./architecture.md) - Internal design details and implementation patterns
- [Error Reference](./errors.md) - Complete catalog of all 29+ error types
- [Type System Reference](./type-system.md) - Complete type system rules and semantics
- [Parent Project README](../README.md) - Overview and quick start guide
