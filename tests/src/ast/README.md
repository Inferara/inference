# AST Module Tests

This directory contains comprehensive unit tests for the Inference AST module.

Target code coverage >98%.

## Test Structure

```
tests/src/ast/
├── mod.rs                 # Module declarations
├── builder.rs             # AST builder tests (66 tests)
├── builder_extended.rs    # Extended builder tests (36 tests)
├── nodes.rs               # Location struct tests (9 tests)
├── type_info.rs           # TypeInfo tests (29 tests)
└── type_inference.rs      # Type inference tests (1 test)
```

## Running Tests

### Run all AST tests
```bash
cargo test --package inference-tests --lib ast
```

### Run specific test module
```bash
cargo test --package inference-tests --lib ast::builder
cargo test --package inference-tests --lib ast::type_info
```

### Run with output
```bash
cargo test --package inference-tests --lib ast -- --show-output
```

### Run ignored tests (to see failures)
```bash
cargo test --package inference-tests --lib ast -- --ignored
```

## Coverage Measurement

### Quick coverage summary
```bash
cargo llvm-cov --workspace --lib \
  --exclude wasm-runner \
  --exclude inference-playground-server \
  --summary-only
```

### Generate HTML coverage report
```bash
cargo llvm-cov --workspace --lib \
  --exclude wasm-runner \
  --exclude inference-playground-server \
  --html
```

Then open `target/llvm-cov/html/index.html` in your browser.

### Coverage for specific module
```bash
cargo llvm-cov --package inference-ast --lib --summary-only
```

## Test Guidelines

### Writing New Tests

1. **Use the test utilities**: Import and use `build_ast()` from `crate::utils`
   ```rust
   use crate::utils::build_ast;
   
   #[test]
   fn test_my_feature() {
       let source = r#"
       fn example() -> i32 {
           return 42;
       }
       "#;
       let ast = build_ast(source.to_string());
       let source_files = &ast.source_files;
       assert_eq!(source_files.len(), 1);
   }
   ```

2. **Test one thing per test**: Each test should focus on a single language feature or edge case

3. **Use descriptive names**: Test names should clearly indicate what is being tested
   - Good: `test_parse_function_with_multiple_parameters`
   - Bad: `test_fn_2`

4. **Add comments for complex tests**: Explain the purpose of tests that aren't immediately obvious

### Test Organization

- **builder.rs**: Basic AST constructs (functions, structs, expressions, statements)
- **builder_extended.rs**: Advanced features (specs, forall, bitwise ops, method calls)
- **type_info.rs**: Type system tests (TypeInfo, TypeInfoKind, type checking methods)
- **nodes.rs**: AST node data structure tests (Location, etc.)
- **type_inference.rs**: Type inference algorithm tests

## Current Coverage

See [COVERAGE_PROGRESS.md](../../COVERAGE_PROGRESS.md) for detailed coverage metrics and progress tracking.

Quick summary:
- Total tests: 142 (120 passing, 22 ignored)
- Overall AST coverage: ~40%
- Top files:
  - t_ast.rs: 100%
  - nodes.rs: 91%
  - builder.rs: 90%

## Known Issues

### Ignored Tests (22 total)

Tests are ignored for features not yet supported by the grammar:

**From builder.rs (5 ignored)**:
- Chained member access (`obj.field.subfield`)
- Context definitions
- For loops
- Multiline comments
- Unary negate operator

**From builder_extended.rs (17 ignored)**:
- Nested array literals
- Multiline struct expressions
- Constructor function keyword
- Empty array literal syntax
- Filter blocks
- Forall with empty return
- Method call syntax
- Self/mixed parameters
- Type inference syntax (`:=`)
- Spec/total definitions
- And more...

These ignored tests serve as documentation of desired features and can be enabled as the grammar evolves.

## Contributing

When adding new tests:

1. Run tests locally to ensure they pass:
   ```bash
   cargo test --package inference-tests --lib ast
   ```

2. Check coverage impact:
   ```bash
   cargo llvm-cov --workspace --lib --exclude wasm-runner --exclude inference-playground-server --summary-only
   ```

3. Update COVERAGE_PROGRESS.md with new metrics

4. Ensure ignored tests have clear reason messages

## Continuous Integration

The coverage workflow (`.github/workflows/coverage.yml`) runs on every PR and push to main:
- Runs all tests
- Generates coverage report
- Checks coverage threshold (currently 40%)
- Uploads to Codecov for tracking

## References

- [cargo-llvm-cov documentation](https://github.com/taiki-e/cargo-llvm-cov)
- [Rust testing guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Inference language spec](../../README.md)
