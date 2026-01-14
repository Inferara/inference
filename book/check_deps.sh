#!/usr/bin/env bash
#
# Inference Dependency Check Script (Linux/macOS)
#
# Checks for required dependencies to build the Inference compiler.
# Can optionally download missing external binaries from Google Cloud Storage.
#
# Usage:
#   ./check_deps.sh          # Check dependencies
#   ./check_deps.sh --help   # Show help
#
# Required:
#   - LLVM 21 installed
#   - Rust nightly toolchain
#   - curl and unzip (for downloading binaries)
#

set -euo pipefail

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Color Output ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

print_found()   { echo -e "${GREEN}[FOUND]${NC}    $1"; }
print_missing() { echo -e "${RED}[MISSING]${NC}  $1"; }
print_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
print_info()    { echo -e "${CYAN}[INFO]${NC}     $1"; }

# --- Platform Detection ---
detect_platform() {
    case "$(uname -s)" in
        Linux*)  PLATFORM="linux" ;;
        Darwin*) PLATFORM="macos" ;;
        *)       echo "Unsupported platform: $(uname -s)"; exit 1 ;;
    esac
}

# --- Download URLs ---
get_download_url() {
    local platform="$1" binary="$2"
    local base="https://storage.googleapis.com/external_binaries"
    case "$platform-$binary" in
        linux-inf-llc)   echo "$base/linux/bin/inf-llc.zip" ;;
        linux-rust-lld)  echo "$base/linux/bin/rust-lld.zip" ;;
        linux-libLLVM)   echo "$base/linux/lib/libLLVM.so.21.1-rust-1.94.0-nightly.zip" ;;
        macos-inf-llc)   echo "$base/macos/bin/inf-llc.zip" ;;
        macos-rust-lld)  echo "$base/macos/bin/rust-lld.zip" ;;
        *)               echo "" ;;
    esac
}

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

check_llvm() {
    echo ""
    echo -e "${CYAN}--- LLVM 21 ---${NC}"
    local llvm_config="" llvm_version=""

    # Try different llvm-config names
    for cmd in llvm-config-21 llvm-config; do
        if command -v "$cmd" &> /dev/null; then
            llvm_config="$cmd"
            break
        fi
    done

    # Try Homebrew paths on macOS
    if [[ -z "$llvm_config" && "$PLATFORM" == "macos" ]]; then
        for path in "/opt/homebrew/opt/llvm@21/bin/llvm-config" \
                    "/opt/homebrew/opt/llvm/bin/llvm-config" \
                    "/usr/local/opt/llvm@21/bin/llvm-config" \
                    "/usr/local/opt/llvm/bin/llvm-config"; do
            if [[ -x "$path" ]]; then
                llvm_config="$path"
                break
            fi
        done
    fi

    if [[ -n "$llvm_config" ]]; then
        llvm_version=$("$llvm_config" --version 2>/dev/null || echo "")
        if [[ "$llvm_version" == 21.* ]]; then
            print_found "LLVM: $llvm_version (via $llvm_config)"
            return 0
        elif [[ -n "$llvm_version" ]]; then
            print_warning "LLVM found but version $llvm_version (need 21.x)"
            return 1
        fi
    fi

    print_missing "LLVM 21 not found"
    if [[ "$PLATFORM" == "linux" ]]; then
        echo "        Install: wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 21"
    else
        echo "        Install: brew install llvm@21"
    fi
    return 1
}

check_llvm_env() {
    echo ""
    echo -e "${CYAN}--- LLVM Environment ---${NC}"
    if [[ -n "${LLVM_SYS_211_PREFIX:-}" ]]; then
        if [[ -d "$LLVM_SYS_211_PREFIX" ]]; then
            print_found "LLVM_SYS_211_PREFIX=$LLVM_SYS_211_PREFIX"
            return 0
        else
            print_warning "LLVM_SYS_211_PREFIX set but directory missing: $LLVM_SYS_211_PREFIX"
            return 1
        fi
    else
        print_missing "LLVM_SYS_211_PREFIX not set"
        if [[ "$PLATFORM" == "linux" ]]; then
            echo "        Set: export LLVM_SYS_211_PREFIX=/usr/lib/llvm-21"
        else
            echo "        Set: export LLVM_SYS_211_PREFIX=\$(brew --prefix llvm@21)"
        fi
        return 1
    fi
}

check_external_binaries() {
    echo ""
    echo -e "${CYAN}--- External Binaries ($PLATFORM) ---${NC}"
    echo "Directory: $PROJECT_ROOT/external/"

    local bin_dir="$PROJECT_ROOT/external/bin/$PLATFORM"
    local lib_dir="$PROJECT_ROOT/external/lib/$PLATFORM"
    MISSING_BINARIES=()

    # Check inf-llc
    if [[ -x "$bin_dir/inf-llc" ]]; then
        print_found "inf-llc (in external/bin/$PLATFORM/)"
    else
        print_missing "inf-llc (not in external/bin/$PLATFORM/)"
        MISSING_BINARIES+=("inf-llc")
    fi

    # Check rust-lld
    if [[ -x "$bin_dir/rust-lld" ]]; then
        print_found "rust-lld (in external/bin/$PLATFORM/)"
    else
        print_missing "rust-lld (not in external/bin/$PLATFORM/)"
        MISSING_BINARIES+=("rust-lld")
    fi

    # Check libLLVM (Linux only)
    if [[ "$PLATFORM" == "linux" ]]; then
        if [[ -f "$lib_dir/libLLVM.so.21.1-rust-1.94.0-nightly" ]]; then
            print_found "libLLVM.so (in external/lib/$PLATFORM/)"
        else
            print_missing "libLLVM.so (not in external/lib/$PLATFORM/)"
            MISSING_BINARIES+=("libLLVM")
        fi
    else
        print_info "libLLVM.so not required on macOS"
    fi

    echo "---------------------------------"
    [[ ${#MISSING_BINARIES[@]} -eq 0 ]]
}

# --- Download Functions ---
download_binaries() {
    echo ""
    echo -e "${CYAN}The following binaries will be downloaded:${NC}"
    for binary in "${MISSING_BINARIES[@]}"; do
        echo "  - $binary: $(get_download_url "$PLATFORM" "$binary")"
    done

    echo ""
    read -r -p "Download missing binaries? (y/N) " answer
    if [[ ! "$answer" =~ ^[Yy]$ ]]; then
        echo "Download cancelled."
        return 1
    fi

    local bin_dir="$PROJECT_ROOT/external/bin/$PLATFORM"
    local lib_dir="$PROJECT_ROOT/external/lib/$PLATFORM"
    mkdir -p "$bin_dir"
    [[ "$PLATFORM" == "linux" ]] && mkdir -p "$lib_dir"

    for binary in "${MISSING_BINARIES[@]}"; do
        local url tmp_file="/tmp/${binary}.zip"
        url=$(get_download_url "$PLATFORM" "$binary")
        print_info "Downloading $binary..."

        if curl -fsSL "$url" -o "$tmp_file"; then
            if [[ "$binary" == "libLLVM" ]]; then
                unzip -o "$tmp_file" -d "$lib_dir/" > /dev/null
            else
                unzip -o "$tmp_file" -d "$bin_dir/" > /dev/null
                chmod +x "$bin_dir/$binary"
            fi
            rm -f "$tmp_file"
            print_found "Downloaded $binary"
        else
            print_missing "Failed to download $binary"
        fi
    done
}

# --- macOS Quarantine Check ---
check_macos_quarantine() {
    [[ "$PLATFORM" != "macos" ]] && return 0

    local bin_dir="$PROJECT_ROOT/external/bin/$PLATFORM"
    local quarantined=()

    for binary in inf-llc rust-lld; do
        local path="$bin_dir/$binary"
        if [[ -f "$path" ]] && xattr -l "$path" 2>/dev/null | grep -q "com.apple.quarantine"; then
            quarantined+=("$binary")
        fi
    done

    if [[ ${#quarantined[@]} -gt 0 ]]; then
        echo ""
        print_warning "Binaries quarantined by macOS Gatekeeper:"
        printf "  - %s\n" "${quarantined[@]}"
        echo ""
        read -r -p "Remove quarantine attribute? (y/N) " answer
        if [[ "$answer" =~ ^[Yy]$ ]]; then
            for binary in "${quarantined[@]}"; do
                xattr -d com.apple.quarantine "$bin_dir/$binary" 2>/dev/null || true
                print_found "Removed quarantine from $binary"
            done
        fi
    fi
}

# --- Main ---
show_help() {
    cat << 'EOF'
Inference Dependency Check Script

Usage: check_deps.sh [--help]

Checks for required dependencies to build the Inference compiler:
  - Rust nightly toolchain
  - LLVM 21
  - External binaries (inf-llc, rust-lld)
  - libLLVM shared library (Linux only)

Can optionally download missing external binaries.
EOF
    exit 0
}

main() {
    [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]] && show_help

    detect_platform
    echo ""
    echo -e "${CYAN}=== Inference Dependency Check ===${NC}"
    echo "Platform: $PLATFORM"
    echo "Project root: $PROJECT_ROOT"
    echo ""
    echo "This check is read-only. You will be prompted before any downloads or changes."

    local all_good=true
    MISSING_BINARIES=()

    check_rust  || all_good=false
    check_cargo || all_good=false
    check_llvm  || all_good=false
    check_llvm_env || all_good=false
    check_external_binaries || all_good=false

    # Offer to download missing binaries
    if [[ ${#MISSING_BINARIES[@]} -gt 0 ]]; then
        download_binaries && check_external_binaries || all_good=false
    fi

    check_macos_quarantine

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
