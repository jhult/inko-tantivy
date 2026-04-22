#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

#
# Build and test script for inko-tantivy
# Builds the Rust FFI library and runs tests with proper library paths

set -o errexit  # Exit on error
set -o nounset  # Exit on undefined variable
set -o pipefail # Exit on pipe failure

# Use safe word splitting
IFS=$'\n\t'

# Script directory (absolute path)
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly FFI_DIR="${SCRIPT_DIR}/native/tantivy-c"
readonly FFI_TARGET_DIR="${FFI_DIR}/target/release"

# Detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    readonly LIB_NAME="libtantivy_c.dylib"
    readonly INSTALL_DIR="/usr/local/lib"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    readonly LIB_NAME="libtantivy_c.so"
    readonly INSTALL_DIR="/usr/local/lib"
else
    readonly LIB_NAME="tantivy_c.dll"
    readonly INSTALL_DIR="/usr/local/lib"
fi

readonly LIB_PATH="${FFI_TARGET_DIR}/${LIB_NAME}"

# Color output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m' # No Color

# Print colored message
log_info() {
    echo -e "${GREEN}==>${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}Warning:${NC} $*" >&2
}

log_error() {
    echo -e "${RED}Error:${NC} $*" >&2
}

# Print usage information
usage() {
    cat <<EOF
Usage: $(basename "${BASH_SOURCE[0]}") [COMMAND]

Build and test script for inko-tantivy

COMMANDS:
    build       Build the Rust FFI library (default)
    test        Build, check formatting, and run tests
    install     Install FFI library to ${INSTALL_DIR} (requires sudo)
    clean       Remove all build artifacts
    help        Show this help message

EXAMPLES:
    $(basename "${BASH_SOURCE[0]}")           # Build the FFI library
    $(basename "${BASH_SOURCE[0]}") test      # Build and run tests
    $(basename "${BASH_SOURCE[0]}") install   # Install system-wide

EOF
}

# Build the FFI library
build_ffi() {
    log_info "Building Rust FFI library..."

    if [[ ! -d "${FFI_DIR}" ]]; then
        log_error "FFI directory not found: ${FFI_DIR}"
        return 1
    fi

    cd "${FFI_DIR}"
    cargo build --release

    if [[ -f "${LIB_PATH}" ]]; then
        log_info "FFI library built successfully: ${LIB_PATH}"
    else
        log_error "Build succeeded but library not found at: ${LIB_PATH}"
        return 1
    fi
}

# Run tests with proper library paths
run_tests() {
    log_info "Building FFI library before testing..."
    build_ffi

    log_info "Checking Inko formatting..."
    cd "${SCRIPT_DIR}"

    if ! inko fmt --check; then
        log_error "Formatting check failed"
        return 1
    fi

    log_info "Running tests with FFI library..."

    # Suppress [TANTIVY_ERROR] stderr output during tests (errors are still
    # returned to callers and asserted in tests)
    export TANTIVY_LOG_ERRORS=0

    # Set library paths for the linker and runtime
    export LIBRARY_PATH="${FFI_TARGET_DIR}"
    export DYLD_LIBRARY_PATH="${FFI_TARGET_DIR}"  # macOS
    export LD_LIBRARY_PATH="${FFI_TARGET_DIR}"    # Linux

    if ! inko test --release; then
        log_error "Tests failed"
        return 1
    fi

    log_info "All checks passed!"
}

# Install library to system directory
install_lib() {
    log_info "Installing FFI library to ${INSTALL_DIR}..."

    # Build first if needed
    if [[ ! -f "${LIB_PATH}" ]]; then
        log_warn "Library not found, building first..."
        build_ffi
    fi

    # Check if we need sudo
    if [[ ! -w "${INSTALL_DIR}" ]]; then
        log_info "Installing to ${INSTALL_DIR} (requires sudo)..."
        sudo cp "${LIB_PATH}" "${INSTALL_DIR}/"
    else
        cp "${LIB_PATH}" "${INSTALL_DIR}/"
    fi

    # Run ldconfig on Linux to update library cache
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v ldconfig &> /dev/null; then
            log_info "Updating library cache..."
            sudo ldconfig
        fi
    fi

    log_info "Installation complete: ${INSTALL_DIR}/${LIB_NAME}"
    log_info "You can now run 'inko test --release' without setting LIBRARY_PATH"
}

# Clean build artifacts
clean_build() {
    log_info "Cleaning build artifacts..."

    if [[ -d "${FFI_DIR}" ]]; then
        cd "${FFI_DIR}"
        cargo clean
    fi

    if [[ -d "${SCRIPT_DIR}/build" ]]; then
        rm -rf "${SCRIPT_DIR}/build"
    fi

    log_info "Clean complete"
}

# Main entry point
_main() {
    local command="${1:-build}"

    case "${command}" in
        build)
            build_ffi
            ;;
        test)
            run_tests
            ;;
        install)
            install_lib
            ;;
        clean)
            clean_build
            ;;
        help|--help|-h)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown command: ${command}"
            echo ""
            usage
            exit 1
            ;;
    esac
}

# Run main function if script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    _main "$@"
fi
