#!/usr/bin/env bash
#
# Inference Dependency Check Script (Linux/macOS)
#
# Checks for required dependencies to build the Inference compiler.
#
# Usage:
#   ./check_deps.sh          # Check dependencies
#   ./check_deps.sh --help   # Show help
#
# Required:
#   - Rust nightly toolchain
#

set -euo pipefail

# --- Color Output ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

print_found()   { echo -e "${GREEN}[FOUND]${NC}    $1"; }
print_missing() { echo -e "${RED}[MISSING]${NC}  $1"; }
print_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }

# --- Dependency Checks ---
check_rust() {
    echo ""
    echo -e "${CYAN}--- Rust Toolchain ---${NC}"
    if command -v rustc &> /dev/null; then
        local version
        version=$(rustc --version 2>/dev/null)
        if [[ "$version" == *"nightly"* ]]; then
            print_found "Rust: $version"
            return 0
        else
            print_warning "Rust installed but not nightly: $version"
            echo "        Run: rustup default nightly"
            return 1
        fi
    else
        print_missing "Rust not found"
        echo "        Install from: https://rustup.rs/"
        return 1
    fi
}

check_cargo() {
    if command -v cargo &> /dev/null; then
        print_found "Cargo: $(cargo --version 2>/dev/null)"
        return 0
    else
        print_missing "Cargo not found"
        return 1
    fi
}

# --- Main ---
show_help() {
    cat << 'EOF'
Inference Dependency Check Script

Usage: check_deps.sh [--help]

Checks for required dependencies to build the Inference compiler:
  - Rust nightly toolchain
EOF
    exit 0
}

main() {
    [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]] && show_help

    echo ""
    echo -e "${CYAN}=== Inference Dependency Check ===${NC}"
    echo ""

    local all_good=true

    check_rust  || all_good=false
    check_cargo || all_good=false

    # Final summary
    echo ""
    echo "---------------------------------"
    if [[ "$all_good" == "true" ]]; then
        echo -e "${GREEN}SUCCESS: All dependencies are present.${NC}"
        echo -e "${YELLOW}Ready to build: cargo build${NC}"
        exit 0
    else
        echo -e "${RED}FAILURE: Some dependencies are missing.${NC}"
        echo "Please install missing dependencies and run this script again."
        exit 1
    fi
}

main "$@"
