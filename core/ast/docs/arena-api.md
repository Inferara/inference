# Arena API Guide

Comprehensive reference for the Arena API with practical examples for all experience levels.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Core Concepts](#core-concepts)
3. [Building an Arena](#building-an-arena)
4. [Querying Nodes](#querying-nodes)
5. [Traversing the Tree](#traversing-the-tree)
6. [Source Text Retrieval](#source-text-retrieval)
7. [Filtering and Searching](#filtering-and-searching)
8. [Common Patterns](#common-patterns)
9. [Error Handling](#error-handling)
10. [Performance Tips](#performance-tips)

## Prerequisites

To understand this guide, you should be familiar with:

- Basic Rust concepts (ownership, borrowing, Option types)
- Pattern matching with enums
- Closures and iterator methods
- Hash maps and their O(1) lookup characteristics

No prior compiler experience required. We'll explain AST concepts as we go.

## Core Concepts

### What is an Arena?

An **arena** is a memory management pattern where all objects are allocated in a single pool. In our AST implementation:

- The `Arena` struct owns all AST nodes
- Nodes reference each other by ID (not pointers)
- The arena never deallocates individual nodes (only the entire arena at once)

### What is an AST Node?

An **Abstract Syntax Tree (AST) node** represents a piece of code structure. For example:

```inference
fn add(a: i32, b: i32) -> i32 { return a + b; }
```

This creates nodes for:
- Function definition ("add")
- Parameters ("a" and "b")
- Return type ("i32")
- Block statement
- Return statement
- Binary expression (a + b)
- Identifiers ("a" and "b")

### Node Identification

Every node has a unique `u32` ID:

```rust
let node = arena.find_node(42)?;
let id = node.id();  // Returns 42
```

IDs are:
- Unique within an arena
- Non-zero (0 is a sentinel value)
- Assigned sequentially during parsing
- Stable (never change after assignment)

## Building an Arena

### From Source Code

The primary way to create an arena is by parsing source code:

```rust
use inference_ast::builder::Builder;
use tree_sitter::Parser;

let source = r#"fn main() -> i32 { return 0; }"#;
let mut parser = Parser::new();
parser.set_language(&tree_sitter_inference::language()).unwrap();
let tree = parser.parse(source, None).unwrap();

let mut builder = Builder::new();
builder.add_source_code(tree.root_node(), source.as_bytes());
let arena = builder.build_ast()?;
```

**What happens here:**
1. Tree-sitter parses source code into a concrete syntax tree
2. `Builder` walks the CST and creates typed AST nodes
3. Assigns unique IDs sequentially starting from 1
4. Records parent-child relationships in the arena
5. Returns an immutable `Arena` or error if parse errors exist

### From a File

```rust
use std::fs;
use inference_ast::builder::Builder;
use tree_sitter::Parser;

let source = fs::read_to_string("examples/hello.inf")?;
let mut parser = Parser::new();
parser.set_language(&tree_sitter_inference::language()).unwrap();
let tree = parser.parse(&source, None).unwrap();

let mut builder = Builder::new();
builder.add_source_code(tree.root_node(), source.as_bytes());
let arena = builder.build_ast()?;
```

### Empty Arena

For testing or gradual construction:

```rust
let arena = Arena::default();
```

Note: Empty arenas are rare in practice. Usually, you build from source.

## Querying Nodes

### Finding a Node by ID

```rust
let node = arena.find_node(node_id);

match node {
    Some(n) => println!("Found node: {:?}", n),
    None => println!("Node {} does not exist", node_id),
}
```

**Complexity:** O(1) hash map lookup

**Returns:** `Option<AstNode>`

**Common uses:**
- Validating node existence
- Retrieving node details for error messages
- Following node references

### Getting All Source Files

```rust
let source_files = arena.source_files();

for file in source_files {
    println!("File: {} bytes", file.source.len());
}
```

**Returns:** `Vec<Rc<SourceFile>>`

**Note:** Currently, Inference supports single-file compilation, so this typically returns one file.

### Getting All Functions

```rust
let functions = arena.functions();

for func in functions {
    println!("Function: {}", func.name.name);
    println!("  Line: {}", func.location.start_line);
}
```

**Returns:** `Vec<Rc<FunctionDefinition>>`

**Common uses:**
- Building symbol tables
- Analyzing function signatures
- Generating function list documentation

### Getting All Type Definitions

```rust
let types = arena.list_type_definitions();

for type_def in types {
    println!("Type alias: {} = {:?}", type_def.name.name, type_def.ty);
}
```

**Returns:** `Vec<Rc<TypeDefinition>>`

**Example:**
```inference
type Age = i32;
type Name = str;
```

## Traversing the Tree

### Finding a Node's Parent

```rust
let parent_id = arena.find_parent_node(node_id);

match parent_id {
    Some(id) => {
        let parent = arena.find_node(id).unwrap();
        println!("Parent: {:?}", parent);
    }
    None => println!("This is a root node"),
}
```

**Complexity:** O(1)

**Returns:** `Option<u32>` (parent's ID, not the node itself)

**Returns None for:**
- Root nodes (SourceFile)
- Invalid node IDs

### Walking Up to the Root

```rust
fn print_ancestor_chain(arena: &Arena, node_id: u32) {
    let mut current_id = node_id;
    let mut depth = 0;

    loop {
        let node = arena.find_node(current_id).expect("Invalid node ID");
        println!("{:indent$}{:?}", "", node, indent = depth * 2);

        match arena.find_parent_node(current_id) {
            Some(parent_id) => {
                current_id = parent_id;
                depth += 1;
            }
            None => break,  // Reached root
        }
    }
}
```

**Example output:**
```
ReturnStatement
  Block
    FunctionDefinition
      SourceFile
```

### Getting Direct Children

```rust
let children = arena.get_children_cmp(node_id, |_| true);

println!("Node {} has {} children", node_id, children.len());
for child in children {
    println!("  Child {}: {:?}", child.id(), child);
}
```

**Parameters:**
- `node_id`: The parent node
- `comparator`: Filter function (return true to include)

**Complexity:** O(1) for children list + O(c) to iterate where c is child count

### Getting Children of Specific Type

```rust
use inference_ast::nodes::{AstNode, Statement};

// Get all statement children
let statements = arena.get_children_cmp(block_id, |node| {
    matches!(node, AstNode::Statement(_))
});

// Get all return statements
let returns = arena.get_children_cmp(function_id, |node| {
    matches!(node, AstNode::Statement(Statement::Return(_)))
});
```

### Recursive Traversal

`get_children_cmp` traverses the entire subtree, not just direct children:

```rust
// Find all identifiers in a function
let identifiers = arena.get_children_cmp(function_id, |node| {
    matches!(node, AstNode::Expression(Expression::Identifier(_)))
});

println!("Found {} identifier uses", identifiers.len());
```

**How it works:**
1. Starts at `function_id`
2. Visits all descendants depth-first
3. Returns nodes where comparator returns true

## Source Text Retrieval

### Getting Source for Any Node

```rust
let source = arena.get_node_source(node_id);

match source {
    Some(text) => println!("Source: {}", text),
    None => println!("Could not retrieve source"),
}
```

**Complexity:** O(d) where d is tree depth + O(1) string slice

**Returns:** `Option<&str>` (borrowed from SourceFile)

**Returns None when:**
- Node ID doesn't exist
- No SourceFile ancestor exists
- Byte offsets are invalid

### Example: Printing Function Source

```rust
let functions = arena.functions();
for func in functions {
    if let Some(source) = arena.get_node_source(func.id) {
        println!("Function {}:", func.name.name);
        println!("{}", source);
        println!();
    }
}
```

**Output:**
```
Function add:
fn add(a: i32, b: i32) -> i32 { return a + b; }

Function multiply:
fn multiply(x: i32, y: i32) -> i32 { return x * y; }
```

### Finding the Source File for a Node

```rust
let source_file_id = arena.find_source_file_for_node(node_id);

match source_file_id {
    Some(id) => {
        let file = arena.find_node(id).unwrap();
        if let AstNode::Ast(Ast::SourceFile(sf)) = file {
            println!("Source file has {} bytes", sf.source.len());
        }
    }
    None => println!("No source file ancestor"),
}
```

**Complexity:** O(d) where d is tree depth

**How it works:**
1. Checks if node itself is a SourceFile (early return)
2. Walks up parent chain to root
3. Checks if root is a SourceFile

## Filtering and Searching

### Filter Nodes by Predicate

```rust
// Find all variable definitions
let variables = arena.filter_nodes(|node| {
    matches!(node, AstNode::Statement(Statement::VariableDefinition(_)))
});

println!("Found {} variable definitions", variables.len());
```

**Complexity:** O(n) where n is total nodes in arena

**Returns:** `Vec<AstNode>`

**Common uses:**
- Finding all nodes of a type
- Building symbol tables
- Code analysis passes

### Extract Data from Nodes

```rust
use inference_ast::nodes::{Definition, AstNode};

// Get names of all structs
let struct_names: Vec<String> = arena
    .filter_nodes(|node| {
        matches!(node, AstNode::Definition(Definition::Struct(_)))
    })
    .iter()
    .filter_map(|node| {
        if let AstNode::Definition(Definition::Struct(s)) = node {
            Some(s.name.name.clone())
        } else {
            None
        }
    })
    .collect();

println!("Structs: {:?}", struct_names);
```

### Find Nodes by Name

```rust
// Find a function by name
fn find_function_by_name(arena: &Arena, name: &str) -> Option<Rc<FunctionDefinition>> {
    arena
        .functions()
        .into_iter()
        .find(|f| f.name.name == name)
}

// Usage
if let Some(func) = find_function_by_name(&arena, "main") {
    println!("Found main function at line {}", func.location.start_line);
}
```

### Find Nodes by Location

```rust
// Find all nodes on line 10
let nodes_on_line_10 = arena.filter_nodes(|node| {
    node.location().start_line == 10
});

println!("Line 10 contains {} nodes", nodes_on_line_10.len());
```

## Common Patterns

### Pattern 1: Type Checking a Function

```rust
use inference_ast::nodes::{AstNode, Statement, Definition};

fn check_function_types(arena: &Arena, func_id: u32) -> Result<(), String> {
    let func_node = arena.find_node(func_id)
        .ok_or("Function not found")?;

    let func = match func_node {
        AstNode::Definition(Definition::Function(f)) => f,
        _ => return Err("Not a function".to_string()),
    };

    // Get all return statements in function
    let returns = arena.get_children_cmp(func_id, |node| {
        matches!(node, AstNode::Statement(Statement::Return(_)))
    });

    println!("Function {} has {} return statements", func.name.name, returns.len());

    // Check each return matches function signature
    // ... type checking logic ...

    Ok(())
}
```

### Pattern 2: Building a Symbol Table

```rust
use std::collections::HashMap;
use inference_ast::nodes::{AstNode, Definition};

fn build_symbol_table(arena: &Arena) -> HashMap<String, u32> {
    let mut symbols = HashMap::new();

    // Add all top-level functions
    for func in arena.functions() {
        symbols.insert(func.name.name.clone(), func.id);
    }

    // Add all type definitions
    for type_def in arena.list_type_definitions() {
        symbols.insert(type_def.name.name.clone(), type_def.id);
    }

    // Add all structs
    let structs = arena.filter_nodes(|node| {
        matches!(node, AstNode::Definition(Definition::Struct(_)))
    });

    for struct_node in structs {
        if let AstNode::Definition(Definition::Struct(s)) = struct_node {
            symbols.insert(s.name.name.clone(), s.id);
        }
    }

    symbols
}
```

### Pattern 3: Error Reporting

```rust
struct CompilerError {
    message: String,
    location: Location,
    source_snippet: String,
}

fn report_error(arena: &Arena, node_id: u32, message: String) -> CompilerError {
    let node = arena.find_node(node_id).expect("Invalid node ID");
    let location = node.location();
    let source_snippet = arena.get_node_source(node_id)
        .unwrap_or("<source unavailable>")
        .to_string();

    CompilerError {
        message,
        location,
        source_snippet,
    }
}

// Usage
let error = report_error(&arena, bad_node_id, "Type mismatch".to_string());
eprintln!("Error at {}:{}: {}",
    error.location.start_line,
    error.location.start_column,
    error.message
);
eprintln!("  {}", error.source_snippet);
```

### Pattern 4: Code Generation

```rust
fn generate_code(arena: &Arena, node_id: u32) -> String {
    let node = arena.find_node(node_id).expect("Node not found");

    match node {
        AstNode::Statement(Statement::Return(ret)) => {
            // Generate code for return statement
            let expr_source = arena.get_node_source(ret.expression.borrow().id())
                .unwrap_or("0");
            format!("return {};", expr_source)
        }
        AstNode::Definition(Definition::Function(func)) => {
            // Generate code for function
            let body = arena.get_node_source(func.body.id())
                .unwrap_or("{}");
            format!("function {} {}", func.name.name, body)
        }
        _ => String::new(),
    }
}
```

### Pattern 5: Finding Enclosing Scope

```rust
use inference_ast::nodes::{AstNode, Definition, BlockType};

fn find_enclosing_function(arena: &Arena, node_id: u32) -> Option<Rc<FunctionDefinition>> {
    let mut current_id = node_id;

    loop {
        let node = arena.find_node(current_id)?;

        // Check if this node is a function
        if let AstNode::Definition(Definition::Function(func)) = node {
            return Some(func);
        }

        // Move up to parent
        current_id = arena.find_parent_node(current_id)?;
    }
}

// Usage
if let Some(func) = find_enclosing_function(&arena, expression_id) {
    println!("Expression is inside function: {}", func.name.name);
}
```

## Error Handling

### Dealing with Option Values

Most Arena methods return `Option` to handle missing nodes gracefully:

```rust
// Pattern 1: Early return with ?
fn process_node(arena: &Arena, node_id: u32) -> Option<String> {
    let node = arena.find_node(node_id)?;
    let source = arena.get_node_source(node_id)?;
    Some(format!("{:?}: {}", node, source))
}

// Pattern 2: Match expression
fn process_node_verbose(arena: &Arena, node_id: u32) -> String {
    match arena.find_node(node_id) {
        Some(node) => format!("Found: {:?}", node),
        None => format!("Node {} not found", node_id),
    }
}

// Pattern 3: unwrap_or with default
let source = arena.get_node_source(node_id).unwrap_or("<unavailable>");
```

### Validating Node Types

```rust
use inference_ast::nodes::{AstNode, Definition};

fn ensure_function(arena: &Arena, node_id: u32) -> Result<Rc<FunctionDefinition>, String> {
    let node = arena.find_node(node_id)
        .ok_or_else(|| format!("Node {} not found", node_id))?;

    match node {
        AstNode::Definition(Definition::Function(func)) => Ok(func),
        _ => Err(format!("Node {} is not a function", node_id)),
    }
}
```

### Handling Malformed ASTs

```rust
fn safe_traverse(arena: &Arena, node_id: u32, max_depth: u32) -> Vec<u32> {
    let mut path = Vec::new();
    let mut current_id = node_id;
    let mut depth = 0;

    loop {
        // Guard against cycles or extreme depth
        if depth >= max_depth {
            eprintln!("Warning: Maximum depth {} reached", max_depth);
            break;
        }

        path.push(current_id);

        match arena.find_parent_node(current_id) {
            Some(parent_id) => {
                current_id = parent_id;
                depth += 1;
            }
            None => break,
        }
    }

    path
}
```

## Performance Tips

### Tip 1: Reuse Filtered Results

```rust
// Bad: filters twice
let functions = arena.functions();
for func in &functions {
    // ...
}
let functions_again = arena.functions();  // Duplicate work!

// Good: filter once, reuse
let functions = arena.functions();
for func in &functions {
    // ...
}
for func in &functions {  // Reuse existing Vec
    // ...
}
```

### Tip 2: Use Early Returns

```rust
// Bad: unnecessary work
fn find_main(arena: &Arena) -> Option<Rc<FunctionDefinition>> {
    let all_functions = arena.functions();
    all_functions.into_iter().find(|f| f.name.name == "main")
}

// Good: iterator short-circuits
fn find_main(arena: &Arena) -> Option<Rc<FunctionDefinition>> {
    arena.functions().into_iter().find(|f| f.name.name == "main")
}
```

### Tip 3: Prefer Specific Queries

```rust
// Bad: filters all nodes
let functions = arena.filter_nodes(|node| {
    matches!(node, AstNode::Definition(Definition::Function(_)))
});

// Good: uses specialized method
let functions = arena.functions();
```

### Tip 4: Cache Source File Lookups

```rust
// Bad: repeated source file lookups
for node_id in node_ids {
    let sf_id = arena.find_source_file_for_node(node_id);  // O(depth) each time
    // ...
}

// Good: cache if all nodes share same source file
let source_file_id = arena.find_source_file_for_node(node_ids[0]).unwrap();
for node_id in node_ids {
    // Assume all nodes are in same file (validate in debug builds)
    debug_assert_eq!(arena.find_source_file_for_node(node_id), Some(source_file_id));
    // ...
}
```

### Tip 5: Avoid Unnecessary Cloning

```rust
// Bad: clones entire node
let node = arena.find_node(node_id).unwrap();
process_node(node.clone());  // Expensive!

// Good: borrow or extract only what you need
let node = arena.find_node(node_id).unwrap();
let location = node.location();  // Copy (cheap)
process_location(location);
```

## Advanced Examples

### Example 1: Control Flow Graph

```rust
use inference_ast::nodes::{AstNode, Statement};

fn build_cfg(arena: &Arena, function_id: u32) -> Vec<(u32, u32)> {
    let mut edges = Vec::new();

    let statements = arena.get_children_cmp(function_id, |node| {
        matches!(node, AstNode::Statement(_))
    });

    for (i, stmt) in statements.iter().enumerate() {
        match stmt {
            AstNode::Statement(Statement::If(if_stmt)) => {
                // Branch: if condition → then block + else block
                edges.push((if_stmt.id, if_stmt.if_arm.id()));
                if let Some(else_arm) = &if_stmt.else_arm {
                    edges.push((if_stmt.id, else_arm.id()));
                }
            }
            AstNode::Statement(Statement::Loop(loop_stmt)) => {
                // Loop: loop → body, body → loop
                edges.push((loop_stmt.id, loop_stmt.body.id()));
                edges.push((loop_stmt.body.id(), loop_stmt.id));
            }
            _ if i + 1 < statements.len() => {
                // Sequential: stmt[i] → stmt[i+1]
                edges.push((stmt.id(), statements[i + 1].id()));
            }
            _ => {}
        }
    }

    edges
}
```

### Example 2: Dead Code Detection

```rust
use inference_ast::nodes::{AstNode, Statement};

fn find_unreachable_code(arena: &Arena, function_id: u32) -> Vec<u32> {
    let mut unreachable = Vec::new();

    let statements = arena.get_children_cmp(function_id, |node| {
        matches!(node, AstNode::Statement(_))
    });

    let mut found_return = false;

    for stmt in statements {
        if found_return {
            unreachable.push(stmt.id());
        }

        if matches!(stmt, AstNode::Statement(Statement::Return(_))) {
            found_return = true;
        }
    }

    unreachable
}
```

### Example 3: Complexity Metrics

```rust
fn calculate_cyclomatic_complexity(arena: &Arena, function_id: u32) -> u32 {
    let mut complexity = 1;  // Base complexity

    let statements = arena.get_children_cmp(function_id, |node| {
        matches!(
            node,
            AstNode::Statement(Statement::If(_)) | AstNode::Statement(Statement::Loop(_))
        )
    });

    complexity += statements.len() as u32;

    complexity
}
```

## Troubleshooting

### Issue: "Node not found" errors

**Cause:** Stale node IDs or cross-arena references

**Solution:** Ensure node IDs are from the same arena:

```rust
// Bad: mixing IDs from different arenas
let arena1 = build_ast(source1);
let arena2 = build_ast(source2);
let node = arena2.find_node(arena1_node_id);  // Returns None!

// Good: use IDs from the correct arena
let node = arena1.find_node(arena1_node_id);
```

### Issue: "Source not found" errors

**Cause:** Node has no SourceFile ancestor

**Solution:** Validate the node has a source file:

```rust
if arena.find_source_file_for_node(node_id).is_none() {
    eprintln!("Warning: Node {} has no source file", node_id);
}
```

### Issue: Slow tree traversal

**Cause:** Inefficient traversal or redundant lookups

**Solution:** Profile with `cargo flamegraph` and optimize hot paths:

```bash
cargo flamegraph --test test_name
```

## Related Documentation

- [Architecture Guide](architecture.md) - System design and internals
- [Location Optimization](location.md) - Memory-efficient source tracking
- [Node Types](nodes.md) - Complete AST node reference

## Feedback

If you find this guide helpful or have suggestions for improvement, please open an issue or submit a pull request on the main repository.
