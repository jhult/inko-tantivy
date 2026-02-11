# Performance Benchmarks

This directory contains performance benchmarks for the inko-tantivy library.

## Running Benchmarks

Each benchmark file can be run independently:

```bash
# Build the native library first
./build.sh

# Run individual benchmarks
inko run benches/bench_indexing.inko
inko run benches/bench_search.inko
inko run benches/bench_aggregations.inko

# Or run all benchmarks
./run_benchmarks.sh
```

## Benchmark Categories

### Indexing Benchmarks (`bench_indexing.inko`)
- **Single document indexing**: Measures time to index a single document
- **Batch document indexing**: Measures throughput for bulk indexing (100 docs)
- **Large document indexing**: Tests performance with large documents (10KB)

### Search Benchmarks (`bench_search.inko`)
- **Simple term search**: Basic keyword search performance
- **Complex query search**: Multi-field queries with multiple terms
- **Autocomplete query**: Prefix-based autocomplete suggestions
- **Paginated search**: Search with offset/pagination

### Aggregation Benchmarks (`bench_aggregations.inko`)
- **Facet by folder**: Count documents by folder (20 buckets, 5000 docs)
- **Facet by label**: Count documents by label (50 buckets, 5000 docs)
- **Terms aggregation**: Top-N aggregation with limit
- **Faceted search**: Aggregations with query filters

## Performance Goals

These benchmarks help ensure:
1. **No regressions**: Changes don't slow down existing operations
2. **Optimization validation**: Performance improvements actually improve
3. **Scale understanding**: How operations scale with data size
4. **Cross-platform consistency**: Performance is acceptable on all platforms

## Interpreting Results

- **ms per operation**: Lower is better
- **Compare across runs**: Track changes over time
- **Platform differences**: Some variation expected between OS/architectures
- **Watch for outliers**: Sudden spikes may indicate issues

## Future Improvements

- Add memory usage profiling
- Support for larger datasets (100K+ documents)
- Concurrent operation benchmarks
- Integration with CI for regression detection
