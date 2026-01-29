# Manual QA Test Suite for `infs` CLI

This document contains tests that require manual verification or are not yet automated.

> **Automated Tests:** Run `cargo test -p infs` to execute 429 automated tests (360 unit + 69 integration).

---

## Manual Tests

### TC-9.1: TUI Visual Verification
**Category:** TUI and Headless
**Reason:** Requires human eyes to verify visual rendering

**Steps:**
1. Run `infs` (no arguments) in an interactive terminal
2. Verify TUI interface launches with visible menu
3. Verify navigation works (arrow keys)
4. Exit with `q` or Ctrl+C

**Expected Result:**
- TUI renders correctly with logo and menu
- Navigation highlights correct menu items
- Exit works cleanly without artifacts

---

### TC-4.1: Verify Command - Basic Verification
**Category:** Verify Command
**Reason:** Requires coqc installed

**Steps:**
1. Run `infs verify trivial.inf`

**Expected Result:**
- Exit code: 0
- Generates `.v` file
- `coqc` processes file successfully

---

### TC-4.3: Verify Command - Missing coqc Error
**Category:** Verify Command
**Reason:** Requires coqc NOT in PATH

**Steps:**
1. Ensure `coqc` is NOT in PATH
2. Run `infs verify trivial.inf`

**Expected Result:**
- Exit code: Non-zero
- Error indicates coqc not found
- Suggests installation

---

### TC-6.16: Toolchain Directory Permissions
**Category:** Toolchain Management
**Reason:** Requires specific filesystem permission setup

**Steps:**
1. Make directory read-only: `chmod 555 ~/.inference/toolchains`
2. Run `infs install`
3. Restore permissions: `chmod 755 ~/.inference/toolchains`

**Expected Result:**
- Exit code: Non-zero
- Error indicates permission denied

---

### TC-8.1: Self Update Available
**Category:** Self Update
**Reason:** Requires actual newer version in distribution

**Steps:**
1. Ensure running an older version of `infs`
2. Run `infs self update`

**Expected Result:**
- Exit code: 0
- Download progress shown
- Binary updated to new version

---

### TC-8.2: Self Update Already Latest
**Category:** Self Update
**Reason:** Requires actual version check against distribution

**Steps:**
1. Ensure running latest version
2. Run `infs self update`

**Expected Result:**
- Exit code: 0
- Message indicates already on latest version
- No download attempted

---

### TC-8.5: Windows Update Strategy
**Category:** Self Update (Windows only)
**Reason:** Requires actual Windows platform

**Steps:**
1. Run on Windows with update available
2. Run `infs self update`
3. Check for `infs.old` file

**Expected Result:**
- Update completes successfully
- `infs.old` may exist (old binary renamed)

---

### TC-11.1-11.3: Cross-Platform Full Build
**Category:** Cross-Platform
**Reason:** Requires actual target platform (Linux/Windows/macOS)

These should be verified via CI on each platform.

**Steps:**
1. Run `infs build trivial.inf --parse --codegen -o`
2. Verify WASM output exists
3. Run `infs run trivial.inf --entry-point hello_world`

**Expected Result:**
- Build succeeds on each platform
- WASM binary valid and executable

---

### TC-12.2: Disk Full Scenario
**Category:** Error Handling
**Reason:** Disk full simulation is unreliable

**Steps:**
1. Fill available disk space (use ramdisk or small partition)
2. Run `infs install`
3. Clean up

**Expected Result:**
- Exit code: Non-zero
- Error indicates disk full
- Partial downloads cleaned up

---

## Test Data Files

Test fixtures are located in `apps/infs/tests/fixtures/`:

| File | Purpose |
|------|---------|
| `trivial.inf` | Simple valid program (returns 42) |
| `example.inf` | Complex example with multiple functions |
| `nondet.inf` | Non-deterministic features (forall, exists, assume, unique) |
| `syntax_error.inf` | Syntax error handling |
| `type_error.inf` | Type error detection |
| `empty.inf` | Empty file edge case |
| `uzumaki.inf` | Uzumaki operator (`@`) |
| `forall_test.inf` | Forall block with binding |
| `exists_test.inf` | Exists block with binding |
| `assume_test.inf` | Assume block |
| `unique_test.inf` | Unique block with binding |

---

## Running Automated Tests

```bash
# Run all infs tests
cargo test -p infs

# Run only integration tests
cargo test -p infs --test cli_integration

# Run with verbose output
cargo test -p infs -- --nocapture
```

---

*Last Updated: 2026-01-27*
