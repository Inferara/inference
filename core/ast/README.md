# inference-ast

Arena-based Abstract Syntax Tree (AST) implementation for the Inference programming language compiler.

## Overview

This crate provides a memory-efficient AST representation with O(1) node lookups and parent-child traversal. All AST nodes are stored in a central arena with ID-based references, eliminating the need for raw pointers or lifetime management.

## Key Features

- **Arena-based allocation**: Single centralized storage for all AST nodes with O(1) access
- **Efficient parent-child lookup**: Hash map-based relationships for constant-time traversal
- **Zero-copy Location**: Lightweight location tracking with byte offsets and line/column positions
- **Source text retrieval**: Convenient API to get source code snippets for any node
- **Type-safe node representation**: Strongly-typed node enums with exhaustive matching

## Quick Start

### Building an AST

```rust
use inference_ast::builder::Builder;
use tree_sitter::Parser;

let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
let mut parser = Parser::new();
parser.set_language(&tree_sitter_inference::language()).unwrap();
let tree = parser.parse(source, None).unwrap();

let mut builder = Builder::new();
builder.add_source_code(tree.root_node(), source.as_bytes());
let arena = builder.build_ast()?;
```

### Querying the Arena

```rust
// Get all functions
let functions = arena.functions();
for func in functions {
    println!("Function: {}", func.name.name);
}

// Find any node by ID
if let Some(node) = arena.find_node(node_id) {
    // All nodes have id() and location() methods
    println!("Node ID: {}", node.id());
    println!("Location: {}:{}", node.location().start_line, node.location().start_column);
}

// Find parent of a node
if let Some(parent_id) = arena.find_parent_node(node_id) {
    let parent = arena.find_node(parent_id);
}

// Get source text for a node
if let Some(source_text) = arena.get_node_source(node_id) {
    println!("Source: {}", source_text);
}
```

## Architecture

### Arena Storage

The AST uses a three-tier storage system:

1. **Node Storage** (`nodes: FxHashMap<u32, AstNode>`): Maps node IDs to actual node data
2. **Parent Map** (`parent_map: FxHashMap<u32, u32>`): Maps child ID to parent ID for upward traversal
3. **Children Map** (`children_map: FxHashMap<u32, Vec<u32>>`): Maps parent ID to children IDs for downward traversal

This design provides:
- O(1) node lookup by ID
- O(1) parent lookup
- O(1) children list lookup (plus O(c) to access child nodes where c is the number of children)
- O(d) source file lookup where d is tree depth (typically < 20 levels)

### Node Type System

Node types are defined using custom macros that ensure consistency:
- `ast_node!` macro: Generates struct definitions with required `id` and `location` fields
- `ast_enum!` macro: Generates enum wrappers with uniform `id()` and `location()` accessors
- `@skip` annotation: Marks variants (like `SimpleTypeKind`) that are Copy types without ID/location

This macro-based approach eliminates boilerplate and ensures all nodes follow the same conventions.

## Documentation

Detailed documentation is available in the `docs/` directory:

- [Architecture Guide](docs/architecture.md) - System design and data structures
- [Location Optimization](docs/location.md) - Memory-efficient location tracking
- [Arena API Guide](docs/arena-api.md) - Comprehensive API reference with examples
- [Node Types](docs/nodes.md) - AST node type reference

## Example: Error Reporting

The AST makes it easy to generate precise error messages:

```rust
use inference_ast::nodes::AstNode;

fn report_error(arena: &Arena, node_id: u32, message: &str) {
    let node = arena.find_node(node_id).expect("Node not found");
    let location = node.location();
    let source = arena.get_node_source(node_id).unwrap_or("<unknown>");

    eprintln!(
        "Error at {}:{}: {}",
        location.start_line,
        location.start_column,
        message
    );
    eprintln!("  {}", source);
}
```

## Testing

The crate includes comprehensive test coverage:

```bash
cargo test -p inference-ast
```

Test coverage includes:
- Parent-child relationship integrity
- Source text retrieval accuracy
- Edge cases (root nodes, nonexistent IDs, deeply nested structures)
- Performance characteristics

## External Module Support

The crate provides utilities for parsing and managing external modules:

```rust
use inference_ast::extern_prelude::{create_empty_prelude, parse_external_module};
use std::path::Path;

let mut prelude = create_empty_prelude();
parse_external_module(Path::new("/path/to/stdlib"), "std", &mut prelude)?;

// Access parsed module
if let Some(parsed) = prelude.get("std") {
    let functions = parsed.arena.functions();
    // ... use stdlib functions for type checking
}
```

See `src/extern_prelude.rs` for the complete API.

**Note:** Multi-file parsing via `ParserContext` is a work in progress. Currently, the crate supports single-file compilation with external module resolution through `extern_prelude`.

## Dependencies

- `rustc-hash`: Fast hash maps (FxHashMap) for node storage
- `tree-sitter`: Parser integration for building AST from source
- `tree-sitter-inference`: Grammar for the Inference language
- `anyhow`: Error handling
- `thiserror`: Structured error types

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| Node lookup | O(1) | Hash map access |
| Parent lookup | O(1) | Hash map access |
| Children list lookup | O(1) | Hash map access |
| Source file lookup | O(d) | Tree depth, typically < 20 |
| Source text retrieval | O(d) + O(1) | Find source file + string slice |

## Contributing

When modifying AST structures:
1. Update node definitions in `src/nodes.rs`
2. Update builder logic in `src/builder.rs`
3. Add tests in `tests/src/ast/`
4. Update documentation in `docs/`

See the main project [CONTRIBUTING.md](/CONTRIBUTING.md) for general guidelines.

## License

See the main project LICENSE file.
