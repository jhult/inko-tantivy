#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Enhanced benchmark script with regression detection
#
# Features:
# - Save results to timestamped JSON files
# - Compare against previous runs
# - Detect performance regressions
# - Output in machine-readable format (JSON)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default configuration
RESULTS_DIR="benchmarks/results"
COMPARE_FILE=""
TOLERANCE_PERCENT=10 # Alert if 10% slower
MIN_MS_DIFF=5        # Alert if at least 5ms slower (for small changes)

# Parse command line arguments
while [[ $# -gt 0 ]]; do
	case "$1" in
	--compare=*)
		COMPARE_FILE="${1#*=}"
		shift
		;;
	--tolerance=*)
		TOLERANCE_PERCENT="${1#*=}"
		shift
		;;
	--min-diff=*)
		MIN_MS_DIFF="${1#*=}"
		shift
		;;
	--results-dir=*)
		RESULTS_DIR="${1#*=}"
		shift
		;;
	-h | --help)
		cat <<EOF
Usage: $0 [OPTIONS]

Run performance benchmarks with regression detection.

OPTIONS:
  --compare=FILE    Compare results against previous run (JSON file)
  --tolerance=PERC  Regression tolerance percentage (default: 10)
  --min-diff=MS      Minimum difference in ms for alert (default: 5)
  --results-dir=DIR   Results directory (default: benchmarks/results)
  -h, --help          Show this help message

EXAMPLES:
  $0                                    # Run benchmarks, save results
  $0 --compare=results/baseline.json     # Compare against baseline
  $0 --tolerance=20                  # 20% tolerance for regression
EOF
		exit 0
		;;
	*)
		echo "Unknown option: $1"
		echo "Use --help for usage information"
		exit 1
		;;
	esac
done

# Ensure native library is built
if [ ! -f "libtantivy_c.so" ] && [ ! -f "libtantivy_c.dylib" ] && [ ! -f "tantivy_c.dll" ]; then
	echo -e "${YELLOW}Native library not found. Building...${NC}"
	./build.sh
	echo ""
fi

# Set library path
export LIBRARY_PATH=.

# Create results directory
mkdir -p "$RESULTS_DIR"

# Get current timestamp
TIMESTAMP=$(date +"%Y-%m-%d-%H-%M-%S")
RESULTS_FILE="$RESULTS_DIR/benchmark-$TIMESTAMP.json"

echo "==================================="
echo "Running Tantivy Performance Benchmarks"
echo "==================================="
echo -e "Timestamp: $TIMESTAMP${NC}"
echo -e "Results file: $RESULTS_FILE${NC}"
echo ""

# Run each benchmark and collect results
declare -A BENCHMARK_RESULTS

echo "Running indexing benchmarks..."
INDEXING_OUTPUT=$(inko run benches/bench_indexing.inko 2>&1)
echo "$INDEXING_OUTPUT" | tee "$RESULTS_DIR/indexing-$TIMESTAMP.log"

# Parse indexing results
INDEXING_MEDIAN=$(echo "$INDEXING_OUTPUT" | grep "Median:" | sed 's/.*Median: \([0-9]*\)ms.*/\1/')
INDEXING_P95=$(echo "$INDEXING_OUTPUT" | grep "P95:" | sed 's/.*P95: \([0-9]*\)ms.*/\1/')
INDEXING_P99=$(echo "$INDEXING_OUTPUT" | grep "P99:" | sed 's/.*P99: \([0-9]*\)ms.*/\1/')

BENCHMARK_RESULTS["indexing_median"]=$INDEXING_MEDIAN
BENCHMARK_RESULTS["indexing_p95"]=$INDEXING_P95
BENCHMARK_RESULTS["indexing_p99"]=$INDEXING_P99

echo ""
echo "Running search benchmarks..."
SEARCH_OUTPUT=$(inko run benches/bench_search.inko 2>&1)
echo "$SEARCH_OUTPUT" | tee "$RESULTS_DIR/search-$TIMESTAMP.log"

# Parse search results
SEARCH_MEDIAN=$(echo "$SEARCH_OUTPUT" | grep "Median:" | sed 's/.*Median: \([0-9]*\)ms.*/\1/')
SEARCH_P95=$(echo "$SEARCH_OUTPUT" | grep "P95:" | sed 's/.*P95: \([0-9]*\)ms.*/\1/')
SEARCH_P99=$(echo "$SEARCH_OUTPUT" | grep "P99:" | sed 's/.*P99: \([0-9]*\)ms.*/\1/')

BENCHMARK_RESULTS["search_median"]=$SEARCH_MEDIAN
BENCHMARK_RESULTS["search_p95"]=$SEARCH_P95
BENCHMARK_RESULTS["search_p99"]=$SEARCH_P99

echo ""
echo "Running aggregation benchmarks..."
AGG_OUTPUT=$(inko run benches/bench_aggregations.inko 2>&1)
echo "$AGG_OUTPUT" | tee "$RESULTS_DIR/aggregations-$TIMESTAMP.log"

# Parse aggregation results
AGG_MEDIAN=$(echo "$AGG_OUTPUT" | grep "Median:" | sed 's/.*Median: \([0-9]*\)ms.*/\1/')
AGG_P95=$(echo "$AGG_OUTPUT" | grep "P95:" | sed 's/.*P95: \([0-9]*\)ms.*/\1/')
AGG_P99=$(echo "$AGG_OUTPUT" | grep "P99:" | sed 's/.*P99: \([0-9]*\)ms.*/\1/')

BENCHMARK_RESULTS["aggregations_median"]=$AGG_MEDIAN
BENCHMARK_RESULTS["aggregations_p95"]=$AGG_P95
BENCHMARK_RESULTS["aggregations_p99"]=$AGG_P99

# Write results to JSON file
cat >"$RESULTS_FILE" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "benchmarks": {
    "indexing": {
      "median_ms": ${INDEXING_MEDIAN:-0},
      "p95_ms": ${INDEXING_P95:-0},
      "p99_ms": ${INDEXING_P99:-0}
    },
    "search": {
      "median_ms": ${SEARCH_MEDIAN:-0},
      "p95_ms": ${SEARCH_P95:-0},
      "p99_ms": ${SEARCH_P99:-0}
    },
    "aggregations": {
      "median_ms": ${AGG_MEDIAN:-0},
      "p95_ms": ${AGG_P95:-0},
      "p99_ms": ${AGG_P99:-0}
    }
  },
  "config": {
    "tolerance_percent": $TOLERANCE_PERCENT,
    "min_diff_ms": $MIN_MS_DIFF
  }
}
EOF

echo ""
echo -e "${GREEN}Results saved to: $RESULTS_FILE${NC}"

# Compare with previous run if requested
if [ -n "$COMPARE_FILE" ]; then
	echo ""
	echo "==================================="
	echo -e "${YELLOW}Comparing with: $COMPARE_FILE${NC}"
	echo "==================================="

	# Read baseline values
	BASE_INDEXING_MEDIAN=$(cat "$COMPARE_FILE" | grep -o '"indexing"' -A 3 | grep '"median_ms"' | sed 's/.*"median_ms": \([0-9]*\).*/\1/')
	BASE_SEARCH_MEDIAN=$(cat "$COMPARE_FILE" | grep -o '"search"' -A 3 | grep '"median_ms"' | sed 's/.*"median_ms": \([0-9]*\).*/\1/')
	BASE_AGG_MEDIAN=$(cat "$COMPARE_FILE" | grep -o '"aggregations"' -A 3 | grep '"median_ms"' | sed 's/.*"median_ms": \([0-9]*\).*/\1/')

	# Calculate differences
	if [ -n "$BASE_INDEXING_MEDIAN" ] && [ -n "$INDEXING_MEDIAN" ]; then
		INDEXING_DIFF=$((INDEXING_MEDIAN - BASE_INDEXING_MEDIAN))
		INDEXING_DIFF_PCT=$(echo "scale=2; $INDEXING_DIFF * 100 / $BASE_INDEXING_MEDIAN" | bc)

		if (($(echo "$INDEXING_DIFF_PCT >= $TOLERANCE_PERCENT" | bc -l) || $(echo "$INDEXING_DIFF >= $MIN_MS_DIFF" | bc -l))); then
			echo -e "${RED}REGRESSION: Indexing median increased by ${INDEXING_DIFF_PCT}% (${INDEXING_DIFF}ms)${NC}"
		elif [ "$INDEXING_DIFF_PCT" -lt 0 ]; then
			IMPROVEMENT=$(echo "-$INDEXING_DIFF_PCT" | bc)
			echo -e "${GREEN}IMPROVEMENT: Indexing median decreased by ${IMPROVEMENT}%${NC}"
		else
			echo -e "${GREEN}OK: Indexing median within tolerance (${INDEXING_DIFF}ms, ${INDEXING_DIFF_PCT}%)${NC}"
		fi
	fi

	if [ -n "$BASE_SEARCH_MEDIAN" ] && [ -n "$SEARCH_MEDIAN" ]; then
		SEARCH_DIFF=$((SEARCH_MEDIAN - BASE_SEARCH_MEDIAN))
		SEARCH_DIFF_PCT=$(echo "scale=2; $SEARCH_DIFF * 100 / $BASE_SEARCH_MEDIAN" | bc)

		if (($(echo "$SEARCH_DIFF_PCT >= $TOLERANCE_PERCENT" | bc -l) || $(echo "$SEARCH_DIFF >= $MIN_MS_DIFF" | bc -l))); then
			echo -e "${RED}REGRESSION: Search median increased by ${SEARCH_DIFF_PCT}% (${SEARCH_DIFF}ms)${NC}"
		elif [ "$SEARCH_DIFF_PCT" -lt 0 ]; then
			IMPROVEMENT=$(echo "-$SEARCH_DIFF_PCT" | bc)
			echo -e "${GREEN}IMPROVEMENT: Search median decreased by ${IMPROVEMENT}%${NC}"
		else
			echo -e "${GREEN}OK: Search median within tolerance (${SEARCH_DIFF}ms, ${SEARCH_DIFF_PCT}%)${NC}"
		fi
	fi

	if [ -n "$BASE_AGG_MEDIAN" ] && [ -n "$AGG_MEDIAN" ]; then
		AGG_DIFF=$((AGG_MEDIAN - BASE_AGG_MEDIAN))
		AGG_DIFF_PCT=$(echo "scale=2; $AGG_DIFF * 100 / $BASE_AGG_MEDIAN" | bc)

		if (($(echo "$AGG_DIFF_PCT >= $TOLERANCE_PERCENT" | bc -l) || $(echo "$AGG_DIFF >= $MIN_MS_DIFF" | bc -l))); then
			echo -e "${RED}REGRESSION: Aggregations median increased by ${AGG_DIFF_PCT}% (${AGG_DIFF}ms)${NC}"
		elif [ "$AGG_DIFF_PCT" -lt 0 ]; then
			IMPROVEMENT=$(echo "-$AGG_DIFF_PCT" | bc)
			echo -e "${GREEN}IMPROVEMENT: Aggregations median decreased by ${IMPROVEMENT}%${NC}"
		else
			echo -e "${GREEN}OK: Aggregations median within tolerance (${AGG_DIFF}ms, ${AGG_DIFF_PCT}%)${NC}"
		fi
	fi
fi

echo ""
echo "==================================="
echo -e "${GREEN}All benchmarks completed!${NC}"
echo "==================================="
echo ""
echo "Summary:"
echo -e "  Indexing:   Median=${INDEXING_MEDIAN:-0}ms, P95=${INDEXING_P95:-0}ms, P99=${INDEXING_P99:-0}ms${NC}"
echo -e "  Search:     Median=${SEARCH_MEDIAN:-0}ms, P95=${SEARCH_P95:-0}ms, P99=${SEARCH_P99:-0}ms${NC}"
echo -e "  Aggregations: Median=${AGG_MEDIAN:-0}ms, P95=${AGG_P95:-0}ms, P99=${AGG_P99:-0}ms${NC}"
if [ -n "$COMPARE_FILE" ]; then
	echo ""
	echo -e "Baseline: $COMPARE_FILE${NC}"
	echo -e "Tolerance: ${TOLERANCE_PERCENT}% or ${MIN_MS_DIFF}ms minimum${NC}"
fi
