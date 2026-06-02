# inference-ast

Arena-based Abstract Syntax Tree (AST) implementation for the Inference programming language compiler.

## Overview

This crate provides a memory-efficient AST representation using typed arena allocation. All AST nodes are stored in category-specific arenas inside a central `AstArena`, and node references are lightweight `Copy` typed indices (`ExprId`, `StmtId`, `DefId`, etc.). This eliminates raw pointers, reference counting, and lifetime management while remaining safe to share across threads.

## Key Features

- **Typed indices**: `ExprId`, `StmtId`, `DefId`, `TypeId`, `BlockId`, `IdentId`, and `SourceFileId` prevent accidentally mixing node categories at compile time
- **Arena-based storage**: Seven typed `la_arena::Arena<T>` fields provide O(1) index-based lookups with cache-friendly sequential layout
- **Send + Sync**: No `RefCell`, no `Arc` — the arena can be shared across threads without additional synchronization
- **Zero-copy locations**: `Location` is a 24-byte `Copy` struct; source text is stored once in `SourceFileData` and retrieved by byte-offset slicing

## Quick Start

### Building an Arena

```rust
use inference_ast::arena::AstArena;

let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;

// Parsing lives in the `inference-parser` crate, which lowers source
// directly into an `AstArena`. `inference-ast` is the data model only.
let arena: AstArena = inference_parser::parse(source).arena;
```

### Querying the Arena

```rust
// Get all function definition IDs
let func_ids = arena.function_def_ids();
for def_id in &func_ids {
    println!("Function: {}", arena.def_name(*def_id));
}

// Index directly into the arena with a typed ID — O(1) access
let def_data = &arena[func_ids[0]];
println!("Location: {}:{}", def_data.location.start_line, def_data.location.start_column);

// Match on the node kind
if let inference_ast::nodes::Def::Function { body, .. } = &def_data.kind {
    let block = &arena[*body];
    println!("Statements in body: {}", block.stmts.len());
}

// Retrieve source text for any node
let node_id = inference_ast::ids::NodeId::Def(func_ids[0]);
if let Some(source_text) = arena.get_node_source(node_id) {
    println!("Source: {}", source_text);
}
```

## Architecture

### Arena Storage

`AstArena` stores nodes in seven typed `la_arena::Arena<T>` fields:

```
AstArena {
    source_files : Arena<SourceFileData>  -- indexed by SourceFileId
    defs         : Arena<DefData>         -- indexed by DefId
    stmts        : Arena<StmtData>        -- indexed by StmtId
    exprs        : Arena<ExprData>        -- indexed by ExprId
    types        : Arena<TypeData>        -- indexed by TypeId
    blocks       : Arena<BlockData>       -- indexed by BlockId
    idents       : Arena<Ident>           -- indexed by IdentId
}
```

This design provides:
- O(1) node lookup by typed ID
- `Send + Sync` without locking (no interior mutability)

### Typed Indices

Every arena category has a dedicated index type that is a type alias over `la_arena::Idx<T>`:

| Type | Indexes into | Size |
|------|-------------|------|
| `SourceFileId` | `source_files` | 4 bytes |
| `DefId` | `defs` | 4 bytes |
| `StmtId` | `stmts` | 4 bytes |
| `ExprId` | `exprs` | 4 bytes |
| `TypeId` | `types` | 4 bytes |
| `BlockId` | `blocks` | 4 bytes |
| `IdentId` | `idents` | 4 bytes |

All typed IDs implement `Copy`, `Eq`, and `Hash`. Because `Idx<T>` is parameterized over the node type, an `ExprId` (i.e., `Idx<ExprData>`) can never accidentally index the `defs` arena.

The `NodeId` enum wraps any of the typed IDs for use in heterogeneous contexts such as type annotation storage:

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

### Node Type System

Each arena category uses a two-level structure: a wrapper struct that holds `location` plus a flat `kind` enum:

```
ExprData  { location: Location, kind: Expr     }
StmtData  { location: Location, kind: Stmt     }
DefData   { location: Location, kind: Def      }
TypeData  { location: Location, kind: TypeNode }
```

Blocks and identifiers are simpler:

```
BlockData { location: Location, block_kind: BlockKind, stmts: Vec<StmtId> }
Ident     { location: Location, name: String }
```

The top-level source file node stores the entire source string:

```
SourceFileData { location: Location, source: String, defs: Vec<DefId>, directives: Vec<Directive> }
```

Node kinds (`Expr`, `Stmt`, `Def`, `TypeNode`) are plain enums. References between nodes use typed IDs: for example, `Expr::Binary { left: ExprId, right: ExprId, op: OperatorKind }`.

## Example: Error Reporting

```rust
use inference_ast::arena::AstArena;
use inference_ast::ids::NodeId;

fn report_error(arena: &AstArena, node_id: NodeId, message: &str) {
    let location = arena.node_location(node_id).expect("Node not found");
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

## Documentation

Detailed documentation is available in the `docs/` directory:

- [Architecture Guide](docs/architecture.md) - System design and data structures
- [Location Optimization](docs/location.md) - Memory-efficient location tracking
- [Arena API Guide](docs/arena-api.md) - Comprehensive API reference with examples
- [Node Types](docs/nodes.md) - AST node type reference

## Testing

```bash
cargo test -p inference-ast
cargo test -p inference-tests ast
```

Test coverage includes:
- Typed allocation and index access
- Source text retrieval
- Structural traversal patterns (source files → defs → kinds)
- Edge cases: empty arena, out-of-range IDs, nodes without a source file

## Module Organization

| Module | Purpose |
|--------|---------|
| `arena` | `AstArena` struct, typed allocators, query methods, source text retrieval |
| `ids` | `SourceFileId`, `DefId`, `StmtId`, `ExprId`, `TypeId`, `BlockId`, `IdentId`, `NodeId` |
| `nodes` | All node wrapper structs and kind enums (`Expr`, `Stmt`, `Def`, `TypeNode`, `Location`, …) |
| `la_arena` | Vendored `la_arena` crate providing `Arena<T>` and `Idx<T>` |
| `extern_prelude` | Utilities for parsing external modules (stdlib, prelude) |
| `parser_context` | Multi-file parsing support |
| `errors` | Structured error types for parse failures |

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Node lookup by typed ID | O(1) | Direct arena index |
| Source file lookup (Def nodes) | O(n) | Scans source file defs lists |
| Source file lookup (other nodes) | O(n) | Byte-offset matching across source files |
| Source text retrieval | O(n) + O(1) | Find source file + string slice |

## Dependencies

- `rustc-hash`: Fast hash maps (`FxHashMap`) used in query methods
- `anyhow`: Error handling
- `thiserror`: Structured error types

## Contributing

When modifying AST structures:
1. Update node definitions in `src/nodes.rs`
2. Update builder logic in `src/builder.rs`
3. Add tests in `tests/src/ast/`
4. Update documentation in `docs/`

See the main project [CONTRIBUTING.md](/CONTRIBUTING.md) for general guidelines.

## License

See the main project LICENSE file.
