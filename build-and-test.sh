#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

set -o errexit
set -o nounset
set -o pipefail

if [[ "${TRACE-0}" == "1" ]]; then
	set -o xtrace
fi

# Help text
usage() {
	cat <<'EOF'
Build native library and run Inko tests.

Usage: ./build-and-test.sh [OPTIONS]

Options:
  -h, --help    Show this help message and exit

This script:
  1. Builds the tantivy_c native library for the current platform
  2. Runs Inko formatting check
  3. Runs Inko tests

Environment Variables:
  TRACE=1       Enable debug tracing
EOF
}

# Parse command line arguments
while [[ "$#" -gt 0 ]]; do
	case "$1" in
	-h | --help)
		usage
		exit 0
		;;
	*)
		echo "Error: Unknown option '$1'" >&2
		usage >&2
		exit 1
		;;
	esac
done

# Directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
readonly SCRIPT_DIR
cd "$SCRIPT_DIR"

main() {
	echo "Building native library..."
	./build.sh

	echo ""
	echo "Checking Inko formatting..."
	inko fmt --check

	echo ""
	echo "Running Inko tests..."

	# Set library search path for the linker
	export LIBRARY_PATH="${SCRIPT_DIR}/native/tantivy-c/target/release:${LIBRARY_PATH:-}"

	# Set dynamic library path for macOS runtime
	export DYLD_LIBRARY_PATH="${SCRIPT_DIR}/native/tantivy-c/target/release:${DYLD_LIBRARY_PATH:-}"

	inko test

	echo ""
	echo "All checks passed!"
}

main "$@"
