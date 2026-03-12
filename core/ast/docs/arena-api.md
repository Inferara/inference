# Arena API Guide

Comprehensive reference for the `AstArena` API with practical examples for all experience levels.

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

- Basic Rust concepts (ownership, borrowing, `Option` types)
- Pattern matching with enums
- Rust's `Index` trait (the `arena[id]` syntax)
- Hash maps and their O(1) lookup characteristics

No prior compiler experience is required. AST concepts are explained as they appear.

## Core Concepts

### What is an Arena?

An **arena** is a memory management pattern where all objects are allocated in a single pool. In this AST implementation:

- `AstArena` owns all AST nodes, organized into seven typed `Vec`s
- Nodes reference each other by typed index, not by pointer
- The arena never deallocates individual nodes; the entire arena is freed at once

Because there are no `Arc<T>` or `RefCell<T>` wrappers, `AstArena` implements `Send + Sync` and can be freely shared across threads.

### What is an AST Node?

An **Abstract Syntax Tree (AST) node** represents a structural element of source code. For example:

```inference
fn add(a: i32, b: i32) -> i32 { return a + b; }
```

This creates nodes for:
- Function definition (`add`) — stored as `DefData` in `defs`
- Parameters (`a` and `b`) — stored as `ArgData` inline inside `Def::Function`
- Return type (`i32`) — stored as `TypeData` in `types`
- Body block — stored as `BlockData` in `blocks`
- Return statement — stored as `StmtData` in `stmts`
- Binary expression (`a + b`) — stored as `ExprData` in `exprs`
- Identifiers (`a`, `b`) — stored as `Ident` in `idents`

### Typed Indices

Every node category has its own index type, defined as a type alias over `la_arena::Idx<T>`:

| Index type | Targets | Example use |
|-----------|---------|-------------|
| `SourceFileId` | `arena.source_files` | Root of the tree |
| `DefId` | `arena.defs` | Function, struct, enum, … |
| `StmtId` | `arena.stmts` | Return, if, let, … |
| `ExprId` | `arena.exprs` | Binary, literal, call, … |
| `TypeId` | `arena.types` | `i32`, `[T; N]`, custom, … |
| `BlockId` | `arena.blocks` | `{ … }` bodies |
| `IdentId` | `arena.idents` | Identifiers and names |

The type system prevents using an `ExprId` to index `defs`. Because `Idx<T>` is parameterized over the node type, mismatches are caught at compile time.

The `NodeId` enum wraps any typed ID for use in heterogeneous contexts, such as type annotation storage:

```rust
pub enum NodeId {
    SourceFile(SourceFileId),
    Def(DefId),
    Stmt(StmtId),
    Expr(ExprId),
    Type(TypeId),
    Block(BlockId),
    Ident(IdentId),
}
```

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

What happens here:
1. Tree-sitter parses source code into a concrete syntax tree (CST)
2. `Builder` walks the CST and allocates typed AST nodes into the arena's `Vec`s
3. Returns an immutable `AstArena`, or an error if parse errors are present

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
use inference_ast::arena::AstArena;

let arena = AstArena::default();
```

Empty arenas are rare in practice. Usually, you build from source.

## Querying Nodes

### Indexing Directly

The primary access pattern is direct `Vec` indexing using a typed ID. The `Index` trait is implemented for each ID type:

```rust
use inference_ast::nodes::{Def, Stmt, Expr};

// Get all function definition IDs
let func_ids = arena.function_def_ids();

// Index into the arena — O(1) Vec access
let def_data = &arena[func_ids[0]];
println!("Location: {}", def_data.location);

// Match on the node kind
if let Def::Function { name, body, .. } = &def_data.kind {
    let fn_name = arena.ident_name(*name);
    println!("Function: {}", fn_name);

    // Index the body block
    let block = &arena[*body];
    println!("Statements: {}", block.stmts.len());

    // Index a statement in the block
    let stmt_data = &arena[block.stmts[0]];
    if let Stmt::Return { expr } = stmt_data.kind {
        let expr_data = &arena[expr];
        println!("Return expression: {:?}", expr_data.kind);
    }
}
```

This pattern — obtain a typed ID, index the arena, match on the `kind` field, follow inner typed IDs — is the primary way to traverse the AST.

### Getting All Source Files

```rust
let source_files = arena.source_files();  // &[SourceFileData]

for sf in source_files {
    println!("Source file: {} bytes", sf.source.len());
    println!("Definitions: {}", sf.defs.len());
}
```

Returns a slice borrowed from the arena. Currently, Inference supports single-file compilation, so this slice has one element.

### Getting All Functions

```rust
let func_ids = arena.function_def_ids();  // Vec<DefId>

for def_id in &func_ids {
    println!("Function: {}", arena.def_name(*def_id));
    println!("  Line: {}", arena[*def_id].location.start_line);
}
```

`function_def_ids` walks the source files and returns `DefId`s whose `kind` is `Def::Function`. It does not return methods (struct-associated functions) — only top-level function definitions.

### Getting Type Aliases

There is no dedicated method for type aliases. Iterate `source_files → defs` and filter by variant:

```rust
use inference_ast::nodes::Def;

let source_files = arena.source_files();
let type_aliases: Vec<_> = source_files[0]
    .defs
    .iter()
    .filter(|&&id| matches!(arena[id].kind, Def::TypeAlias { .. }))
    .collect();

println!("Type aliases: {}", type_aliases.len());
```

This structural traversal pattern replaces the old `filter_nodes` global scan.

### Getting the Name of Any Definition

```rust
let name = arena.def_name(def_id);  // &str
```

Works for functions, structs, enums, specs, constants, type aliases, and modules.

### Getting an Identifier Name

```rust
let name = arena.ident_name(ident_id);  // &str
```

## Traversing the Tree

### Structural Traversal (Primary Pattern)

The recommended way to traverse the AST is to follow typed IDs structurally, starting from `source_files`:

```
source_files[i]              SourceFileData
  .defs[j]                   DefId → DefData
    .kind = Def::Function
      .body                  BlockId → BlockData
        .stmts[k]            StmtId → StmtData
          .kind = Stmt::Return
            .expr            ExprId → ExprData
              .kind = Expr::Binary { left, right, op }
                .left        ExprId → ExprData
```

```rust
use inference_ast::nodes::{Def, Stmt, Expr, OperatorKind};

for sf in arena.source_files() {
    for &def_id in &sf.defs {
        if let Def::Function { body, .. } = &arena[def_id].kind {
            for &stmt_id in &arena[*body].stmts {
                if let Stmt::Return { expr } = arena[stmt_id].kind {
                    if let Expr::Binary { left, right, op } = &arena[expr].kind {
                        if *op == OperatorKind::Add {
                            println!("Found an addition at {}", arena[expr].location);
                        }
                        let _ = (left, right);
                    }
                }
            }
        }
    }
}
```

This approach is efficient because it follows the natural tree structure and only visits nodes you actually need.

### Walking Up to a Source File

```rust
use inference_ast::ids::NodeId;

let sf_id = arena.find_source_file_for_node(NodeId::Stmt(stmt_id));

if let Some(id) = sf_id {
    let sf = &arena[id];
    println!("Source file has {} bytes", sf.source.len());
}
```

For `Def` nodes, delegates to `find_source_file_for_def`, which searches the source files' `defs` lists. For other nodes, uses byte-offset matching against all source files.

## Source Text Retrieval

### Getting Source for Any Node

```rust
use inference_ast::ids::NodeId;

let source = arena.get_node_source(NodeId::Def(def_id));

match source {
    Some(text) => println!("Source: {}", text),
    None => println!("Could not retrieve source"),
}
```

Returns `None` when:
- The node ID is out of range
- The source file cannot be determined
- The byte offsets fall outside the source string

### Getting a Node's Location

```rust
use inference_ast::ids::NodeId;

let location = arena.node_location(NodeId::Expr(expr_id));

if let Some(loc) = location {
    println!("Node spans {}:{} to {}:{}", loc.start_line, loc.start_column, loc.end_line, loc.end_column);
    println!("Byte range: {}..{}", loc.offset_start, loc.offset_end);
}
```

`Location` is a 24-byte `Copy` type; it can be stored by value without cloning.

### Example: Printing Function Source

```rust
use inference_ast::ids::NodeId;

let func_ids = arena.function_def_ids();
for def_id in &func_ids {
    if let Some(source) = arena.get_node_source(NodeId::Def(*def_id)) {
        println!("Function {}:", arena.def_name(*def_id));
        println!("{}", source);
        println!();
    }
}
```

Output:
```
Function add:
fn add(a: i32, b: i32) -> i32 { return a + b; }

Function multiply:
fn multiply(x: i32, y: i32) -> i32 { return x * y; }
```

### Finding the Source File for a Node

```rust
use inference_ast::ids::NodeId;

if let Some(sf_id) = arena.find_source_file_for_node(NodeId::Stmt(stmt_id)) {
    let sf = &arena[sf_id];
    println!("Source file: {} bytes, {} definitions", sf.source.len(), sf.defs.len());
}
```

If you have a `DefId`, the more direct variant is:

```rust
let sf_id = arena.find_source_file_for_def(def_id);
```

## Filtering and Searching

### Structural Search (Recommended)

Walk the tree structurally instead of scanning the entire arena. This is faster and makes intent explicit:

```rust
use inference_ast::nodes::{Def, Stmt};

// Find all return statements inside a specific function
fn collect_returns(
    arena: &inference_ast::arena::AstArena,
    def_id: inference_ast::ids::DefId,
) -> Vec<inference_ast::ids::StmtId> {
    let mut returns = Vec::new();

    if let Def::Function { body, .. } = &arena[def_id].kind {
        collect_returns_in_block(arena, *body, &mut returns);
    }

    returns
}

fn collect_returns_in_block(
    arena: &inference_ast::arena::AstArena,
    block_id: inference_ast::ids::BlockId,
    out: &mut Vec<inference_ast::ids::StmtId>,
) {
    for &stmt_id in &arena[block_id].stmts {
        match &arena[stmt_id].kind {
            Stmt::Return { .. } => out.push(stmt_id),
            Stmt::If { then_block, else_block, .. } => {
                collect_returns_in_block(arena, *then_block, out);
                if let Some(eb) = else_block {
                    collect_returns_in_block(arena, *eb, out);
                }
            }
            Stmt::Loop { body, .. } => collect_returns_in_block(arena, *body, out),
            _ => {}
        }
    }
}
```

### Searching Across All Definitions

When you need to search the whole program, iterate `source_files → defs`:

```rust
use inference_ast::nodes::Def;

// Find all struct names
let mut struct_names: Vec<&str> = Vec::new();

for sf in arena.source_files() {
    for &def_id in &sf.defs {
        if let Def::Struct { name, .. } = &arena[def_id].kind {
            struct_names.push(arena.ident_name(*name));
        }
    }
}

println!("Structs: {:?}", struct_names);
```

### Find Definition by Name

```rust
use inference_ast::nodes::Def;
use inference_ast::ids::DefId;

fn find_function_by_name(
    arena: &inference_ast::arena::AstArena,
    target: &str,
) -> Option<DefId> {
    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            if matches!(arena[def_id].kind, Def::Function { .. })
                && arena.def_name(def_id) == target
            {
                return Some(def_id);
            }
        }
    }
    None
}

// Usage
if let Some(def_id) = find_function_by_name(&arena, "main") {
    println!("Found main at line {}", arena[def_id].location.start_line);
}
```

### Find Nodes by Source Location

```rust
// Find all definitions that start on line 10
let defs_on_line_10: Vec<_> = arena
    .source_files()
    .iter()
    .flat_map(|sf| sf.defs.iter())
    .filter(|&&id| arena[id].location.start_line == 10)
    .collect();
```

## Common Patterns

### Pattern 1: Analyzing a Function

```rust
use inference_ast::nodes::{Def, Stmt};
use inference_ast::ids::DefId;

fn analyze_function(
    arena: &inference_ast::arena::AstArena,
    def_id: DefId,
) -> Result<(), String> {
    let def_data = &arena[def_id];

    let (name, body) = match &def_data.kind {
        Def::Function { name, body, .. } => (*name, *body),
        _ => return Err("Not a function".to_string()),
    };

    println!("Analyzing: {}", arena.ident_name(name));

    let block = &arena[body];
    let return_count = block
        .stmts
        .iter()
        .filter(|&&s| matches!(arena[s].kind, Stmt::Return { .. }))
        .count();

    println!("Top-level return statements: {}", return_count);

    Ok(())
}
```

### Pattern 2: Building a Symbol Table

```rust
use std::collections::HashMap;
use inference_ast::nodes::Def;
use inference_ast::ids::DefId;

fn build_symbol_table(
    arena: &inference_ast::arena::AstArena,
) -> HashMap<String, DefId> {
    let mut symbols = HashMap::new();

    for sf in arena.source_files() {
        for &def_id in &sf.defs {
            let name = arena.def_name(def_id).to_string();
            symbols.insert(name, def_id);
        }
    }

    symbols
}
```

### Pattern 3: Error Reporting

```rust
use inference_ast::arena::AstArena;
use inference_ast::ids::NodeId;
use inference_ast::nodes::Location;

struct CompilerError {
    message: String,
    location: Location,
    source_snippet: String,
}

fn make_error(arena: &AstArena, node_id: NodeId, message: String) -> CompilerError {
    let location = arena.node_location(node_id).unwrap_or_default();
    let source_snippet = arena
        .get_node_source(node_id)
        .unwrap_or("<source unavailable>")
        .to_string();

    CompilerError { message, location, source_snippet }
}

// Usage
let err = make_error(&arena, NodeId::Expr(bad_expr_id), "Type mismatch".to_string());
eprintln!("Error at {}: {}", err.location, err.message);
eprintln!("  {}", err.source_snippet);
```

### Pattern 4: Structural Code Generation

```rust
use inference_ast::arena::AstArena;
use inference_ast::ids::DefId;
use inference_ast::nodes::{Def, Stmt, Expr};

fn emit_function(arena: &AstArena, def_id: DefId) -> String {
    let def_data = &arena[def_id];

    if let Def::Function { name, body, .. } = &def_data.kind {
        let fn_name = arena.ident_name(*name);
        let block = &arena[*body];
        let mut output = format!("func {}() {{\n", fn_name);

        for &stmt_id in &block.stmts {
            if let Stmt::Return { expr } = arena[stmt_id].kind {
                if let Expr::NumberLiteral { value } = &arena[expr].kind {
                    output.push_str(&format!("  return {};\n", value));
                }
            }
        }

        output.push('}');
        output
    } else {
        String::new()
    }
}
```

## Error Handling

### Dealing with Option Values

Allocation indices are always valid immediately after allocation. `Option` arises when you use an index that came from outside (for example, from a hash map or a saved ID). Use `?` or `match` as appropriate:

```rust
use inference_ast::ids::NodeId;

// Early return with ?
fn get_source(
    arena: &inference_ast::arena::AstArena,
    node_id: NodeId,
) -> Option<String> {
    let loc = arena.node_location(node_id)?;
    let source = arena.get_node_source(node_id)?;
    Some(format!("{}:{}: {}", loc.start_line, loc.start_column, source))
}

// Match expression
fn describe_node(
    arena: &inference_ast::arena::AstArena,
    node_id: NodeId,
) -> String {
    match arena.node_location(node_id) {
        Some(loc) => format!("Node at {}", loc),
        None => "Unknown node".to_string(),
    }
}
```

### Validating Node Kinds

Use `match` or `if let` to validate before using an ID:

```rust
use inference_ast::ids::DefId;
use inference_ast::nodes::Def;

fn require_function(
    arena: &inference_ast::arena::AstArena,
    def_id: DefId,
) -> Result<(), String> {
    match &arena[def_id].kind {
        Def::Function { .. } => Ok(()),
        _ => Err(format!("Definition {:?} is not a function", def_id)),
    }
}
```

### Guarding Against Out-of-Range IDs

Direct indexing (`arena[id]`) panics if the index is out of range, just like a plain `Vec`. If you have an ID from an external source (for example, deserialized from a file), use `node_location` first to test validity:

```rust
use inference_ast::ids::NodeId;

fn is_valid_expr(
    arena: &inference_ast::arena::AstArena,
    expr_id: inference_ast::ids::ExprId,
) -> bool {
    arena.node_location(NodeId::Expr(expr_id)).is_some()
}
```

## Performance Tips

### Tip 1: Prefer Structural Traversal over Global Scanning

Structural traversal (following typed IDs from `source_files → defs → …`) visits only the nodes you need. A global scan iterates every node in every `Vec`. For most compiler passes, structural traversal is both faster and more readable.

```rust
// Less efficient: visits every definition to find functions
let func_ids = arena.function_def_ids();

// More efficient when you already have a source file and only want one kind:
let funcs: Vec<_> = arena.source_files()[0]
    .defs
    .iter()
    .filter(|&&id| matches!(arena[id].kind, inference_ast::nodes::Def::Function { .. }))
    .collect();
```

In practice, for typical Inference source files the difference is negligible. Prefer whichever is clearer.

### Tip 2: Cache Query Results

Arena query methods like `function_def_ids()` and `source_files()` are cheap, but avoid calling them in tight loops when the result is stable:

```rust
// Good: collect once, iterate multiple times
let func_ids = arena.function_def_ids();
for def_id in &func_ids {
    // first pass
}
for def_id in &func_ids {
    // second pass
}
```

### Tip 3: Store Locations by Value

`Location` is `Copy` (24 bytes). Store it by value to avoid pointer indirection:

```rust
// Good: no borrow, no heap allocation
let loc: inference_ast::nodes::Location = arena[stmt_id].location;
process_location(loc);
```

### Tip 4: Use `def_name` and `ident_name` for String Access

These methods return `&str` borrowed from the arena, avoiding allocation:

```rust
// Good: zero allocation
let name: &str = arena.def_name(def_id);
let ident: &str = arena.ident_name(ident_id);
```

### Tip 5: Use Specific Query Methods

Use `function_def_ids()` instead of manually filtering all defs when you need all functions. This communicates intent clearly and is easy to extend if the method gains optimizations in the future.

## Troubleshooting

### Issue: Index out of bounds when accessing `arena[id]`

**Cause:** The ID was created for a different arena (for example, from a previous compilation run), or was manufactured from a raw value that exceeds the arena's current size.

**Solution:** Use `arena.node_location(NodeId::Expr(expr_id)).is_some()` to validate before indexing.

### Issue: `get_node_source` returns `None`

**Possible causes:**
1. The node ID is out of range — validate with `node_location`
2. The source file cannot be determined — the node's byte offsets do not fall within any `SourceFileData`
3. Byte offsets are outside the source string — this indicates a builder bug

**Diagnostic:**

```rust
use inference_ast::ids::NodeId;

let node_id = NodeId::Stmt(stmt_id);
if arena.node_location(node_id).is_none() {
    eprintln!("Node ID is out of range");
} else if arena.find_source_file_for_node(node_id).is_none() {
    eprintln!("No source file found for node");
} else {
    eprintln!("Byte offsets fall outside source string");
}
```

### Issue: Slow traversal

**Solution:** Replace global scans with structural traversal. If you still need to visit all nodes of a given category, iterate the relevant `Vec` directly:

```rust
// Iterates only expression nodes — no other categories visited
let arena_ref = &arena;
// (Vec fields are pub(crate); access through provided query methods)
```

For performance-sensitive paths, profile with `cargo flamegraph` to identify the real bottleneck before optimizing.

## Related Documentation

- [Architecture Guide](architecture.md) - System design and internals
- [Location Optimization](location.md) - Memory-efficient source tracking
- [Node Types](nodes.md) - Complete AST node reference
