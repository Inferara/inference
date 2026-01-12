# Changelog

All notable changes to the Inference compiler project.

## Location Struct Optimization - Phase 1 (#69)

- Remove `source: String` field from `Location` struct
  - Eliminates ~250KB memory overhead per 2.5KB source file with 100 nodes
  - Source text now stored once in `SourceFile` (coming in Phase 2)
- Add `#[derive(Copy)]` to `Location` struct
  - Enables efficient stack copies instead of heap allocations
  - 60+ `.clone()` calls in type-checker now become trivial copies
  - Aligns with rustc `Span` and rust-analyzer `TextRange` patterns
- Update `Location::new()` constructor (6 parameters instead of 7)
- Add comprehensive doc comment explaining the design pattern
- Add 6 new tests for Copy trait verification:
  - `test_location_copy()` - Basic copy test
  - `test_location_copy_allows_multiple_uses()` - Multiple assignment
  - `test_location_copy_in_function_arguments()` - Pass-by-value
  - `test_location_edge_case_zero_values()` - Boundary testing (0)
  - `test_location_edge_case_max_values()` - Boundary testing (u32::MAX)
  - `test_location_display_zero_values()` - Edge case formatting

## Type Checker Implementation (#54)

- `c13371b` Add `type-checker` crate (#54)
  - Implement bidirectional type inference
  - Add symbol table with scope management
  - Add visibility handling for modules, structs, enums
  - Implement import system with registration and resolution phases
  - Add glob import resolution and external prelude loading
  - Add method resolution and inference tests for structs
  - Add error handling for type parameter inference and conflicting types
  - Implement enum support with variant access validation
  - Add comprehensive unit tests for TypeInfo methods

## Build and CI Optimization (#60)

- `b309291` Update README.md
- `bcdfa0b` Update README.md
- `ea846f7` Optimize local build time and refactor workflows (#60)
- `bc83c02` Delete external directory
- `69ab130` Refactor Windows and macOS artifact packaging and upload steps
- `c0d41f3` Fix artifact packaging command for Windows to include all files
- `d1704d2` Update checksum command to use shasum for macOS artifact
- `57c0284` Update build steps to use release profile for cargo builds
- `f12d6e4` Remove unused path tracking configuration from Codecov settings
- `00fb595` Add `codecov.yml`
- `aa5055b` Configure Codecov action to disable search and set working directory (#58)
- `aa32e3b` Add `codecov` support (#57)
- `ab15b2b` Add macOS-14 support to workflows and build scripts (#55)
- `d5edc79` Refactor build process: streamline test steps and add dependency check script for Windows
- `87af7bb` Update build_release workflow
- `7930bb2` Rollback local build tweaks
- `5443a33` Update build configuration and dependencies
- `1ef3a6c` Update contribution guide (#49)
- `107cc55` Add Git LFS setup for Linux and update build script to track additional files
- `49345d9` Remove redundant Git LFS setup steps from build workflow
- `35ce341` Add verification step for vendored LLVM in build workflow
- `a96482d` Fix codegen utils cross-platform calls
- `43578cd` Fix clippy and codegen tests
- `35e7ded` Fix build script
- `0941cb3` Disable `llvm-cov` check
- `466b63c` Improve dist bundle
- `e547d02` Add coverage badge to README
- `4fa3dda` Use self-runner for actions
- `94d6c01` Use self-hosted runner
- `aea664f` Add Windows development setup guide + inkwell branch that works on Windows + windows dist binaries

## LLVM-based WASM Codegen (#44)

- `fcc6736` Use `inf-llc` for codegen (#44)
  - Add custom LLVM intrinsics for non-deterministic instructions
  - Implement `forall`, `exists`, `uzumaki`, `assume`, `unique` blocks
  - Add `rust-lld` linker invocation
  - Add mutable globals support in WASM compilation
- `ab74736` Binaries access refactoring
- `a85d9fc` Add unit test for `uzumaki_i64`
- `277276f` Finalize minimal tests for inf blocks
- `be39e7b` WIP constant declaration statement
- `37abbef` Add `inf-blocks` support
- `b3caf0a` Adding `non-det` support WIP
- `c531e14` Add `rust-lld` invokation
- `dde01c3` Add mutable globals support in WASM compilation and include actual WASM output
- `f818965` Update build script logging and enhance WASM test execution with memory management

## Project Restructuring (#43)

- `41d13ee` Refactor project structure (#43)
  - Move crates to `core/` and `tools/` directories
  - Add `inf-wasmparser` crate (fork with non-det instruction support)
  - Add `inf-wat` crate
  - Update `wat-fmt` tool
  - Add `wasm-to-v-translator` crate
- `abd59e8` Update dependencies
- `50b9655` Add test suite execution for `wasm-to-v-translator`
- `bc6c7ff` Add `inf-wasmparser` crate
- `0e36a0e` Add `inf-wat` crate
- `c37f9ed` Update `wat-fmt` version
- `0e6ae7e` Add `wat-fmt` tool
- `a17776f` Add `wasm-to-v-translator` crate
- `5489869` Fix codegen tests paths
- `85d7054` Refactor project structure

## Test Suite Expansion (#41)

- `a6f6547` Add more tests (#41)
- `83e99d4` Remove redundant files
- `2943299` starting analyser
- `86baa90` test_wasm_to_coq
- `a2d2abc` Update tree-sitter-inference version and enhance test coverage with additional assertions
- `8fc6594` Bump dependency version
- `2731627` Refactor coverage workflow and improve test coverage with additional assertions
- `0bc3149` Repository clean up
- `89754a0` Fix some tests and bugs
- `d1b77ea` Skip Codecov upload until repository is public
- `6956ff5` Add more tests

## Base WASM Codegen (#29)

- `4924548` Add base WASM codegen (#29)
- `1beb6ae` tests WIP
- `34d6b15` Enhance WASM and V output generation in CLI; add minimal test case

## CLI Refactoring (#28)

- `ebef3ac` Refactor `cil` (#28)
- `6267085` Refactor `cil`

## AST and Type Checking Refactoring (#25)

- `a934935` Refactor ast + base type checking (#25)
  - Arena-based AST with ID references
  - Refactor AST Node Definitions to Use Rc for Shared Ownership
  - Add NodeKind support
  - Improve type inference for expressions
- `f333e74` Refactor AST structure to use Rc for shared ownership
- `030c047` Fix assertions in Arena and update Builder and types for Rc usage
- `f48bd08` Refactor AST and Type Definitions
- `0b7a04a` Refactor Builder and Expression types for improved clarity and type handling
- `5d91846` Refactor AST and Builder to integrate SymbolType and TypedAst; update dependencies and add tests for UzumakiExpression
- `cea5533` Refactor Builder and SymbolTable to integrate SymbolType; update UzumakiExpression handling in WatEmitter
- `956fd4d` Refactor AST Types and Expressions
- `47ed023` Refactor TypeChecker and SymbolTable to enhance variable scope management; update TypeInfo structure and add function definition retrieval in SourceFile
- `e568934` Refactor AST and Type Inference: Rename AssignExpression to AssignStatement; update related handling in Builder and TypeChecker; adjust TypeInfo and WatEmitter for consistency
- `32d1acc` Refactor Type Inference and Expression Handling: Update AssignStatement and VariableDefinitionStatement to use RefCell for expressions; enhance type inference logic for Uzumaki expressions; adjust WatEmitter to handle new expression structure.
- `975f45d` Refactor Expression Handling and Type Information: Remove unnecessary type info parameters from Expression variants; update related code in Builder, TypeChecker, and WatEmitter to handle new structure; enhance Uzumaki expression handling with type info management.
- `69d401e` Refactor type system and remove symbols module
- `9f4f464` Refactor expression handling and type information: Update expression fields to use RefCell for better mutability management; enhance type inference logic in TypeChecker and WatEmitter for improved expression handling.
- `5bb9c68` Refactor AST nodes and type information handling
- `4c8b8a8` Enhance type inference: Add support for "unit" type in type_kind_from_simple_type function
- `10f2cc8` Refactor operator formatting to use Rust's string interpolation
- `71389a1` Refactor operator formatting to use Rust's string interpolation
- `ec22ac2` Add support for struct expressions: Implement StructExpression handling in the AST builder and type inference
- `a30442e` Update tree-sitter dependency to version 0.25.5 and add dead code allowance in build_ast function
- `b1ab4b3` Enhance AST builder: Add TypeMemberAccessExpression handling and improve type inference for member access
- `efbf9bd` Cleanup README
- `1332138` fix typo
- `7b95430` fix typo
- `9a9afd3` Remove dead code from Node trait implementation and simplify macro definitions
- `fd381f4` Refactor AST building functions to improve structure and add NodeKind support
- `95b368a` Update dependencies and add end-to-end tests for inference AST parsing
- `7da82d3` Refactor AST Node Definitions to Use Rc for Shared Ownership
- `44007cf` Update tree-sitter-inference dependency to version 0.0.34
- `162a7ce` Update dependencies in Cargo.toml; refactor AST node definitions and implementations
- `c4e11f9` Remove `wat-fmt` from workspace and update dependencies in `Cargo.toml`; clean up `wat_emitter.rs`
- `8e7b407` Refactor WAT code generation: Replace `wat_generator` with `wat_emitter`
- `8d76af4` Update dependencies in Cargo.toml files and clean up test module
- `facc9f4` Refactor `wat-fmt` by removing outdated comments and improving inline signature handling in `format_node`

## V Translator Improvements (#23, #24)

- `22ce006` Propagate function names to `V` (#24)
- `ae4a326` Refactor Operator enum documentation for clarity and compliance with naming conventions
- `61980f1` Enhance local variable formatting in basic operator translation
- `b9f511c` Refactor WASM parsing and code generation for improved clarity and functionality
- `9170a85` Rewrite v translator (#23)
  - Migrate from wasm-coq-translator to wasm-v-translator
  - Update for WasmCertCoq theory syntax
- `7f76f39` fix: optimize performance in data processing functions
- `e95a263` fix: remove unused variables in parse_inf_file and wasm_parser functions
- `5c5a832` fix: improve operator matching and local variable handling in translation
- `a868b17` fix: update block type consumption in format_block function
- `5cb51d1` new blocks fix
- `11bfc61` uzumaki fix
- `73b3f4d` imports global fix
- `7664db9` memarg fix
- `5079d98` refactor: migrate from wasm-coq-translator to wasm-v-translator and update dependencies
- `03844a8` bump inf-wast dependency to version 0.0.3 and format mod_start output in translator
- `09f84b0` translate_export_module
- `d5ec722` mod_imports
- `f33ed9e` mod_start
- `4edb413` translate_data, unformatted
- `9d4b0d2` translate_element, unformatted
- `3a946d6` translate_global done, but needs formatting
- `eda4a97` refactor: replace string push with character push for newlines in translator
- `f06965d` refactor: enhance expression translation with block and condition structures
- `f14e682` refactor: improve formatting and clean up expression translation
- `f5b93ae` commented out reference expression translation and various minor improvements
- `2cf6a04` unfinished globals
- `e69ffbc` memories
- `adfcc4c` tables
- `969d236` mod_types
- `d4db31a` uncommented `created_function_types`
- `cec2556` Remove trailing character from created_functions and update commented-out code for clarity
- `d616902` Refactor wasm_to_v function to accept module name and update related calls
- `ed5369d` commenting out `translator` logic
- `ff96002` Add source option to CLI for specifying input format
- `e41b792` Simplify lifetime annotation in Expression implementation
- `a4a7e3d` Refactor expression translation to use Peekable iterator and improve operator handling
- `128e000` Remove debug print statement for custom section in wasm_parser
- `219dd4a` translate_expr redesign
- `3781b06` brackets
- `1d77d60` modfunc_body fix
- `4c96c07` reverting some wrong stuff
- `1e76eef` Refactor CLI to support new output generation options and update dependencies; rename wasm_to_coq to wasm_to_v
- `25c9875` Refactor CLI configuration and update dependencies; rename package for clarity
- `ff27ae7` ::
- `58b7bc9` modfunc_body and expr
- `084a2a6` block, loop and if
- `069a814a` Improve error messages and formatting in translator.rs
- `4593c9f` Update formatting for RLB and RRB constants in translator.rs
- `cf23a0a` Remove unused import for WasmModuleParseError in main.rs
- `f833b89` Use anyhow for error handling in parse function
- `0cfc4b1` Refactor wasm parser to use anyhow for error handling in translate_bytes function
- `9810984` WIP rewrite V translation
- `7bcb4ed` Rewrite V translator for WasmCertCoq theory syntax

## Error Handling Improvements (#22)

- `278e238` Wrap `parse_ast` with `anyhow::Result` (#22)
- `cdb5be7` Enhance documentation for build_ast function and remove unused import
- `bc58542` Rollback wrecked merge
- `c4c2532` Update dependencies and refactor error handling in AST building and compilation functions
- `089080f` Remove redundant comments in WAT formatting functions

## WAT Formatting (#21)

- `19fc56b` Improve `wat-fmt` (#21)
- `f9a9697` Update inf-wast dependency to 0.0.2 and refactor number literal handling in AST builder
- `f4a4077` Refactor literal formatting in generate_for_literal function
- `fce1b6b` Refactor literal formatting in generate_for_literal function
- `42de9f6` Update inf-wast dependency to 0.0.2 and refactor number literal handling in AST builder
- `437ac29` Enhance documentation for compile_to_wat, wat_to_wasm, and compile_to_wasm functions
- `67e4ece` Add `wat-fmt` crate for pretty-formatting WAT files and update `wat-generator` accordingly
- `dd33479` Update dependencies and enhance error handling in WASM compilation functions

## Project Flattening (#20)

- `cab816f` Flaten project structure (#20)
- `fae4e21` Flaten project structure
- `43223c3` Update Cargo.toml
- `eaa7144` Add debugging support and implement wat_to_wasm function
- `6a23793` Add wat dependency and implement compile_to_wasm function
- `1911969` Update VSCode launch and task configurations to use the CLI

## WAT Codegen Structure (#19)

- `8f6b5bc` [WIP] Add wat codegen structure (#19)
- `a10a9c5` Update tree-sitter-inference dependency and enhance block statement handling in AST builder
- `6dc6f1d` Add tree-sitter-inference dependency and refactor AST types for improved structure
- `32dba56` Refactor CLI argument handling and add tests for function generation
- `f4cea66` Remove unused import of wat_codegen and add wasm_to_coq_translator import
- `88b844e` Comment out unused WASM S-expression generation function and improve panic messages in AST builder
- `ad05cfa` Skip wasm_to_coq test on GitHub Actions
- `010bbc0` Add module structure for infc and infc_compiler, remove unused files
- `33440ab` Refactor function definition handling and improve WAT generation
- `d5933ed` Add wat codegen

## Grammar Sync (#18)

- `d5933ed` Sync builder with dec 24 grammar version (#18)
- `4e69aa0` Add Break and Function types to AST and update related structures
- `e1241d8` Update dependencies and refactor AST types for improved structure and clarity
- `36cb935` Sync `AST` module with grammar

## Repository Refactoring (#15)

- `8e68283` Repository refactoring (#15)
- `381d1e5` Update README.md
- `403ee35` update docstring
- `80652c1` remove redundant code
- `54e91b0` fix parsing
- `dd39cee` small updates
- `3952833` fix builder tests
- `d524fa8` Add Rust2Wasm pass

## Structs Parser Support (#14)

- `7b4315f` Add `Structs` parser support (#14)
- `ab6d03d` Add `Structs` parser support

## CLI Enhancements (#12)

- `2d4dae8` Add CLI (#12)
- `e2e6c1b` format out `v` file names
- `fad94ec` add parsing diagnostrics
- `3726f2c` fixing filename extraction
- `eab8017` file out file naming
- `676e6eb` add more diagnostics and fix error
- `e73d039` add `i_table` operations
- `31f826f` add missed ops
- `aece044` minor change

## Wasm2Coq Translator (#11)

- `5932415` Wasm2Coq translator (#11)
  - Complete WASM module translation to Rocq (Coq)
  - Add WasmModule representation
  - Implement instruction translation (memory ops, control flow, etc.)
  - Add element segment and data segment translation
  - Add function, table, global, memory translation
- `84fa368` fix hex output
- `0944964` fixes
- `c3c478e` add Some into l_max
- `1590355` update wasmglobal
- `c987141` complete wasm module translation
- `051149e` remove test out file
- `50e9684` add `WasmModule`
- `304a391` fix `ci_br_table`
- `cb86465` fix `ci_br`
- `15358b5` fix memory ops
- `48f5d3d` fix else branch
- `3cb03b2` fix brackets
- `31e8ddb` fix `bt_val`
- `f56a5c5` remove `i_vector`
- `9aaeb97` fix mem ops
- `13f404c` fix brackets
- `9842fc4` fix
- `d6779ef` wip
- `bb14891` fix parathensis WIP
- `984d3e1` add opes wrappers
- `2eaabdf` update es_init
- `4058d55` update element segment es_init
- `ad4398f` fix waselementsegment
- `b15ca89` fix wasmfunctoin definition
- `8620e74` fix brackets
- `5f177e5` fix element
- `17529b4` update import
- `92212ce` fix binary
- `5b541c5` add ds_init
- `7e7ca72` fix End op
- `55dd409` fix list tail
- `9b85f38` fix const decl
- `eb32d70` fix some bugs
- `89ae792` add function parser
- `8d9112c` add translator for element
- `8196b2d` add instructions
- `0a56493` add memory instructions
- `d307613` finish `translate_wasm_expression`
- `44a86d8` WIP translate block
- `8277672` add translate_global
- `dcaadee` add translation for table + refactoring
- `9aa2388` add import translation
- `8a2acf0` WIP add export translation
- `a03ebab` use `wasmparser`
- `d7432f0` init branch

## Tree-sitter Grammar Updates (#10)

- `6fe05a7` Merge pull request #10 from Inferara/add_tree_sitter_inference_pr_16_support
- `79da21c` Add tree-sitter-inference PR #19 support
- `0a0e689` add `enum` in ast
- `92fc890` add `undef` syntax support
- `837ce0a` rename `apply` to `verify`

## Initial AST and Parser Development

- `8b421bf` Update issue templates
- `e7edf7e` add some unit tests
- `29f6653` fix bugs
- `7eaf8dd` fix `build_binary_expression`
- `80ed1e6` move type impl to another file
- `b3a7fb5` Delete .vscode directory
- `792ef89` fix rest bugs
- `8342d83` fix `build_function_call_expression`
- `712de82` fix `build_type_definition` `build_type_definition`, `build_member_access_expression`, `build_generic_type`
- `0ad0858` fix `build_use_directive` and `build_external_function_definition`
- `c26635a` add base AST build code
- `a01c432` add compiler project
- `840fd66` try add more prc macroses

## Specification and Documentation

- `eb6bf0d` Update spec
- `0cc3d87` Create Specification.md

## Build System Setup (#1)

- `018902a` Merge pull request #1 from Inferara/add-build-ci
- `3b67b29` `allow(clippy::no_effect)` for `inference` macro
- `5217852` fix clippy warnings for `inference-documentation` `lib.rs`
- `c89d72f` fix clippy warnings in `docstrings_grabber`
- `104ad80` Update build.yml
- `2245e98` Update build.yml
- `c7fe71e` Update build.yml
- `d191990` Update build.yml
- `2c5ca8a` Update build.yml
- `5643836` add `working-directory` to `build.yml`
- `7c24984` Add build workflow
- `3d699fd` rename directory

## Initial Project Setup

- `a7fc5bc` Initial commit
- `2c5ee9f` add some initial code
- `8ea8be1` add output generation
- `46cbe90` add initial spec boilerplate
- `f274fab` fix `visit` functions
- `71dd0eb` suppress warnings for `inference_fun` macro
- `ae99dc5` add parse file and fn docsrings
- `2e2e360` extract `docstrings_grabber`
- `ea372c1` fix fn comments parsing
