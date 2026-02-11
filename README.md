# inko-tantivy

Tantivy FFI bindings for full-text search in Inko.

## Overview

This library provides FFI bindings to [Tantivy](https://github.com/quickwit-oss/tantivy), a Rust-based full-text search engine. It offers:
- Full-text search with relevance scoring
- Fast indexing and querying
- Field-level indexing (string, integer, boolean, etc.)
- Faceted search and aggregation
- Autocomplete and did-you-mean suggestions

## Score Precision

Search result scores are f32 precision in Tantivy for performance. They are represented as Float (f64) in Inko, but this does not add precision—it merely stores the f32 value.

**Key implications:**

- Use scores for ranking/sorting only, not precise calculations
- Avoid exact equality comparisons on scores (e.g., `score_a == score_b`)
- Small differences (< 1e-7) between scores are likely due to f32 rounding
- When displaying scores, round to reasonable precision (4-6 decimal places)
- Scores range from 0.0 to 1.0 for BM25 scoring

**Recommended practices:**

```inko
# Compare scores with a tolerance, not exact equality
fn are_scores_equal(a: Float, b: Float, tolerance: Float = 0.0001) -> Bool {
  (a - b).abs < tolerance
}

# Sort by score for ranking (this is the intended use case)
let mut results = index.search(query, limit: 100, offset: 0).or_panic
let sorted = results.sort(fn (a, b) -> { b.score <=> a.score })
```

**Test coverage:**

See `test/test_score_precision.inko` for comprehensive tests covering f32 to f64 conversion, edge cases (zero, negative values), and comparison behavior.

## API Differences and Limitations

This document describes differences between the Inko API and the underlying Tantivy library, along with current limitations.

### Supported Tantivy Features

The Inko API exposes a subset of Tantivy's capabilities:

**✓ Fully supported:**
- Full-text search with BM25 scoring
- Custom schema configuration
- Document CRUD operations (create, read, update, delete)
- Faceted search and aggregations
- Autocomplete suggestions
- Did-you-mean suggestions
- Boolean query operators (AND, OR, NOT)

**⚠ Partially supported:**
- Query builders with basic operators
- Batch document indexing
- Result limits and pagination

**✗ Not currently supported:**
- Highlighting (TantivyResult has a highlight field, but always returns None)
- Advanced query operators (range queries, proximity search)
- Multi-term phrase search with slop
- Index snapshots and point-in-time queries
- Query cancellation and timeouts
- Index merging and optimization
- Advanced scoring functions (TF-IDF customization)
- Facet drill-down with hierarchical facets

### Current Limitations

**Concurrency:**
- Multiple processes can access the same index using separate TantivyIndexManager instances
- Single TantivyIndexManager instance cannot be shared across processes
- No built-in thread-safe concurrent access to the same manager

**Memory Management:**
- Automatic cleanup via Drop trait (with manual close() recommended)
- No garbage collection (manual memory management)
- Buffer allocation for each FFI call (potential optimization candidate)

**Query Features:**
- No query caching
- No query optimization suggestions
- No query explain/analysis
- No result highlighting (field exists but not implemented)

**Index Management:**
- No index statistics or metadata
- No index compaction control
- No index backup/restore operations
- No schema migration support

### Planned Features

The following features are under consideration for future releases:

1. **Query timeouts**: Time-based cancellation of long-running queries
2. **Streaming results**: Lazy iteration over large result sets
3. **Performance monitoring**: Built-in metrics for operations
4. **Advanced query syntax**: Support for more Tantivy query operators
5. **Result highlighting**: Proper implementation of search result highlighting

**If you need features not listed here:**

Consider using the Tantivy Rust library directly, or open an issue to discuss adding the feature to the Inko API.

## Installation

```bash
inko pkg add github.com/jhult/inko-tantivy <latest-version>
inko pkg sync
```

## Building the Native Library

### Quick Start (Recommended)

Use the build script for the easiest development experience:

```bash
./build.sh build    # Build the FFI library (or just ./build.sh)
./build.sh test     # Build and run tests (automatically sets library path)
./build.sh install  # Install to /usr/local/lib (requires sudo, optional)
./build.sh clean    # Clean all build artifacts
./build.sh help     # Show all commands
```

The script follows shell best practices with strict error handling, proper quoting, and helpful colored output.

### Building Only the Native Library

This library uses **cargo-zigbuild** for cross-platform builds, creating native libraries that run on Linux, macOS, and Windows.

### Option 1: Build Script (Local Platform)

```bash
./build.sh
```

Builds for your current platform only:
- macOS (Intel/ARM): `libtantivy_c.dylib`
- Linux (x64/ARM): `libtantivy_c.so`
- Windows: `tantivy_c.dll`

### Option 2: Cross-Platform Builds (All Platforms) {#cross-platform-builds}

Install cargo-zigbuild:
```bash
cargo install cargo-zigbuild
```

Build for all platforms:
```bash
cd native/tantivy-c

# Linux x64
cargo zigbuild --release --target x86_64-unknown-linux-gnu

# Linux ARM64
cargo zigbuild --release --target aarch64-unknown-linux-gnu

# macOS x64 (Intel)
cargo zigbuild --release --target x86_64-apple-darwin

# macOS ARM64 (Apple Silicon)
cargo zigbuild --release --target aarch64-apple-darwin

# Windows x64
cargo zigbuild --release --target x86_64-pc-windows-gnu
```

### Option 3: Download from GitHub Releases {#option-3-download-from-github-releases}

Pre-built libraries for all platforms are available from [GitHub Releases](https://github.com/jhult/inko-tantivy/releases).

**Available Platforms:**

| Platform | Release File |
|----------|--------------|
| Linux x64 | [`linux_x64_tantivy_c.so`](https://github.com/jhult/inko-tantivy/releases/latest/download/linux_x64_tantivy_c.so) |
| Linux ARM64 | [`linux_arm64_tantivy_c.so`](https://github.com/jhult/inko-tantivy/releases/latest/download/linux_arm64_tantivy_c.so) |
| macOS x64 (Intel) | [`macos_x64_tantivy_c.dylib`](https://github.com/jhult/inko-tantivy/releases/latest/download/macos_x64_tantivy_c.dylib) |
| macOS ARM64 (Apple Silicon) | [`macos_arm64_tantivy_c.dylib`](https://github.com/jhult/inko-tantivy/releases/latest/download/macos_arm64_tantivy_c.dylib) |
| Windows x64 | [`windows_x64_tantivy_c.dll`](https://github.com/jhult/inko-tantivy/releases/latest/download/windows_x64_tantivy_c.dll) |

### Using the Native Library

The native library must be available when the Inko program runs:

**Option A: Library in project root**
```bash
# Build or download library to project root
./build.sh
# OR download GitHub release (for your platform):
wget https://github.com/jhult/inko-tantivy/releases/latest/download/linux_x64_tantivy_c.so -O libtantivy_c.so
# (see [Download from GitHub Releases](#option-3-download-from-github-releases) for all files)

# Run your Inko program
inko run src/main.inko
```

**Option B: System library path**
```bash
# Install to system library directory
sudo cp libtantivy_c.so /usr/local/lib/

# Run from anywhere
inko run src/main.inko
```

**Option C: LD_LIBRARY_PATH (Linux)**
```bash
export LD_LIBRARY_PATH=/path/to/library:$LD_LIBRARY_PATH
inko run src/main.inko
```

**Option D: DYLD_LIBRARY_PATH (macOS)**
```bash
export DYLD_LIBRARY_PATH=/path/to/library:$DYLD_LIBRARY_PATH
inko run src/main.inko
```

## Usage

### Best Practices

#### Explicit Cleanup

Always call `close()` explicitly to handle cleanup errors:

```inko
# Explicit close with error handling
match index.close {
  case Ok(_) -> {}
  case Error(e) -> std.stdio.Stderr.new.print("Failed to close index: ${e}")
}
```

**Why explicit close is recommended:**

- The `Drop` trait attempts to close the index automatically if `close()` is not called
- Automatic cleanup only logs errors to stderr, making them harder to detect
- Explicit `close()` allows proper error handling and recovery
- This is especially important in production environments where cleanup failures should be monitored

If you prefer to panic on close errors (for quick scripts), use:

```inko
index.close.or_panic  # Panics with the error message
```

#### Rate Limiting

For production use, implement rate limiting to prevent abuse and ensure fair resource allocation:

```inko
import rate_limiter (RateLimiter)

# Create a rate limiter: 100 requests per second, burst of 200
let mut search_limiter = RateLimiter.new(200.0, 100.0)

# Use rate limiter with search operations
fn search_with_limit(query: String) -> Result[Array[TantivyResult], String] {
  if search_limiter.acquire_token {
    index.search(query, 10, 0)
  } else {
    Result.Error("Rate limit exceeded. Please try again later.")
  }
}

# For batch operations, acquire multiple tokens at once
fn index_with_limit(doc_id: String, fields: Array[(String, String)]) -> Result[Bool, String] {
  if search_limiter.acquire_tokens(5) {
    index.add_doc(doc_id, fields)
  } else {
    Result.Error("Rate limit exceeded. Please try again later.")
  }
}
```

**Key considerations for rate limiting:**

- Adjust capacity and refill rate based on your hardware capabilities
- Different operations may require different token costs (e.g., batch index vs. single search)
- Consider per-user or per-IP rate limiters for multi-tenant systems
- Manually refill tokens based on time or other application-specific logic
- The provided `RateLimiter` type is a simplified implementation; extend it for production time-based refill

#### Query Timeouts

Prevent long-running queries from blocking your application:

```inko
# Set reasonable limits on result size
let result = index.search(query, limit: 100, offset: 0)

# For aggregations, use smaller limits to prevent excessive computation
let facets = index.aggregate_terms("category", query, limit: 50)

# Monitor query duration in production
import std.process (current_time_in_nanos)

fn timed_search(query: String, max_ms: Int = 5000) -> Result[Array[TantivyResult], String] {
  let start = current_time_in_nanos
  
  match index.search(query, limit: 100, offset: 0) {
    case Ok(results) -> {
      let elapsed_ms = (current_time_in_nanos - start) / 1_000_000
      
      if elapsed_ms > max_ms {
        Result.Error("Query exceeded timeout limit")
      } else {
        Result.Ok(results)
      }
    }
    case Error(e) -> Result.Error(e)
  }
}
```

**Best practices for query timeouts:**

- Set timeout at 3-5x your average query duration
- Use smaller limits for complex queries (aggregations, faceted search)
- Log slow queries for debugging and performance optimization
- Implement circuit breakers for frequently slow operations
- Consider per-query-type timeouts based on complexity

#### Observability and Metrics

The library includes a `Metrics` type for collecting performance data and monitoring search operations:

```inko
import metrics (Metrics)

# Create metrics collector
let mut metrics = Metrics.new

# Track search operations with metrics
fn search_with_metrics(
  index: TantivyIndexManager,
  metrics: mut Metrics,
  query: String,
) -> Result[Array[TantivyResult], String] {
  let start = std.process.current_time_in_nanos

  match index.search(query, limit: 100, offset: 0) {
    case Ok(results) -> {
      let duration = std.process.current_time_in_nanos - start

      metrics.increment_operation("search")
      metrics.record_latency("search", duration)

      Result.Ok(results)
    }
    case Error(e) -> {
      metrics.increment_error("search_failure")

      Result.Error(e)
    }
  }
}

# Print metrics summary
fn print_metrics_summary(metrics: Metrics) {
  std.stdio.Stdout.new.print(metrics.format_summary)
}
```

**Key metrics to track:**

- **Operation counts**: How many searches, indexes, deletions are performed
- **Latency**: Average duration of each operation type
- **Error rates**: Frequency and types of errors
- **Resource usage**: Memory consumption over time (external monitoring)

**Production monitoring recommendations:**

1. **Integrate with observability platforms**: Export metrics to Prometheus, Datadog, or similar systems
2. **Set up alerts**: Notify on high error rates or slow queries
3. **Track percentiles**: Monitor P95 and P99 latencies, not just averages
4. **Correlate metrics**: Link search performance with system metrics (CPU, memory, I/O)
5. **Sample efficiently**: Don't track every single operation in high-traffic systems

### Basic Usage (Default Email Schema)

### Basic Usage (Default Email Schema)

```inko
import tantivy (TantivyConfig, TantivyIndexManager, TantivyQueryBuilder)

# Create index manager
let config = TantivyConfig.new('/path/to/index')
let mut index = TantivyIndexManager.new(config)

index.open.or_panic

# Add a document
index.add_doc(
  doc_id: 'doc1',
  fields: [
    ('title', 'Hello World'),
    ('body', 'This is a test document'),
  ]
).or_panic

# Search
let results = index.search(
  query: 'test',
  limit: 10,
  offset: 0,
)

match results {
  case Ok(docs) -> {
    for doc in docs {
      # Note: Scores are f32 precision (~6-7 significant digits)
      std.stdio.Stdout.new.print('Found: ${doc.doc_id} (score: ${doc.score})')
    }
  }
  case Error(e) -> std.stdio.Stdout.new.print("Search failed: ${e}")
}

# Commit changes
index.commit.or_panic

# Close index
index.close.or_panic
```

### Custom Schema Configuration

Use custom schemas for indexing non-email data:

```inko
import tantivy (TantivyConfig, TantivyIndexManager)

# Custom schema for documents
let schema_json = '{
  "fields": [
    {"name": "title", "type": "text", "indexed": true, "stored": true},
    {"name": "content", "type": "text", "indexed": true, "stored": true},
    {"name": "timestamp", "type": "u64", "indexed": true, "stored": true},
    {"name": "published", "type": "bool", "indexed": true, "stored": true}
  ],
  "default_search_fields": ["title", "content"]
}'

match TantivyConfig.new('/path/to/index') {
  case Ok(config) -> {
    let config_with_schema = config.with_schema_json(schema_json).or_panic
    let mut index = TantivyIndexManager.new(config_with_schema)

    index.open.or_panic

    # Add documents with custom fields
    index.add_doc(
      doc_id: 'doc-1',
      fields: [
        ('title', 'Introduction to Search'),
        ('content', 'Full-text search is powerful'),
        ('timestamp', '1704067200'),
        ('published', 'true'),
      ],
    ).or_panic
  }
  case Error(e) -> { panic("Failed to create config: ${e}") }
}
```

**Supported field types:**
- `text` - Full-text searchable with tokenization
- `string` - Exact match without tokenization
- `u64` - Unsigned 64-bit integers
- `i64` - Signed 64-bit integers
- `f64` - Double-precision floats
- `bool` - Boolean values

**Schema Design Best Practices:**

1. **Use text fields for full-text search**: Text fields are tokenized and support relevance scoring
2. **Use string fields for exact matches**: String fields are faster for exact lookups (IDs, tags)
3. **Store vs. indexed**: Only index fields you search, store fields you retrieve
4. **Set default search fields**: Specify which fields to search by default
5. **Avoid over-indexing**: Don't index fields you never search

**Example: E-commerce schema**

```inko
let ecommerce_schema = '{
  "fields": [
    {"name": "name", "type": "text", "indexed": true, "stored": true},
    {"name": "description", "type": "text", "indexed": true, "stored": true},
    {"name": "price", "type": "f64", "indexed": true, "stored": true},
    {"name": "category", "type": "string", "indexed": true, "stored": true},
    {"name": "in_stock", "type": "bool", "indexed": true, "stored": true},
    {"name": "sku", "type": "string", "indexed": true, "stored": true}
  ],
  "default_search_fields": ["name", "description"]
}'
```

**Example: Log search schema**

```inko
let log_schema = '{
  "fields": [
    {"name": "timestamp", "type": "u64", "indexed": true, "stored": true},
    {"name": "level", "type": "string", "indexed": true, "stored": true},
    {"name": "message", "type": "text", "indexed": true, "stored": true},
    {"name": "service", "type": "string", "indexed": true, "stored": true}
  ],
  "default_search_fields": ["message"]
}'
```

**Example: Document search schema**

```inko
let document_schema = '{
  "fields": [
    {"name": "title", "type": "text", "indexed": true, "stored": true},
    {"name": "body", "type": "text", "indexed": true, "stored": true},
    {"name": "author", "type": "string", "indexed": true, "stored": true},
    {"name": "tags", "type": "string", "indexed": true, "stored": true},
    {"name": "created_at", "type": "u64", "indexed": true, "stored": true},
    {"name": "word_count", "type": "i64", "indexed": true, "stored": true}
  ],
  "default_search_fields": ["title", "body"]
}'
```

**Performance considerations:**

- Text fields are larger and slower to index than other types
- String fields provide faster exact matches but don't support relevance scoring
- Store only the fields you need to display (reduces index size)
- Indexed fields increase index size and search time
- Use appropriate data types (don't store numbers as strings)

### Query Building

```inko
import tantivy (TantivyQueryBuilder)

let mut builder = TantivyQueryBuilder.new

builder.search_text('title', 'search term')
builder.filter('status', 'published')
builder.range('date', 2020, 2024)

let query = builder.build
```

### Faceted Search

```inko
let facets = index.facet_counts(
  field_name: 'category',
  query: 'electronics',
  limit: 10,
)

for facet in facets.or_panic {
  std.stdio.Stdout.new.print('${facet.key}: ${facet.count}')
}
```

### Autocomplete

```inko
let suggestions = index.autocomplete(
  field: 'title',
  prefix: 'elect',
  limit: 5,
)

for suggestion in suggestions.or_panic {
  std.stdio.Stdout.new.print('${suggestion.text} (score: ${suggestion.score})')
}
```

### Did-You-Mean

```inko
let suggestions = index.did_you_mean(
  field: 'title',
  term: 'electrnics',
  distance: 2,
  limit: 5,
)

for suggestion in suggestions.or_panic {
  std.stdio.Stdout.new.print('${suggestion.text} (score: ${suggestion.score})')
}
```

## Architecture

The library consists of:

- [`src/tantivy.inko`](src/tantivy.inko) - Main FFI bindings and `TantivyIndexManager`
- [`src/pointer_helpers.inko`](src/pointer_helpers.inko) - Pointer arithmetic helpers for FFI
- [`src/marshal.inko`](src/marshal.inko) - C marshalling utilities
- [`native/tantivy-c/`](native/tantivy-c/) - Rust cdylib wrapper exposing C API

## Memory Safety

All memory allocated for FFI calls is managed automatically using `ByteArray`. The library handles:
- Proper null-terminated string conversion
- Automatic cleanup of C-allocated memory
- Safe pointer arithmetic with bounds checking

See [docs/memory.md](docs/memory.md) for detailed documentation on memory ownership across the FFI boundary.

## Security Considerations

This library provides basic protections against resource exhaustion and common attacks, but **applications must implement additional protections** for production use. See [docs/security.md](docs/security.md) for comprehensive guidance on:

- **Rate limiting**: Limit concurrent operations and requests per user/IP
- **Query validation**: Validate and sanitize user-provided query strings
- **Operational limits**: Set appropriate limits for search and aggregation operations
- **Path security**: Secure handling of index paths and file system access
- **Input validation**: Validate all user input before passing to library functions
- **Error handling**: Proper error handling without exposing sensitive information
- **Resource cleanup**: Ensure indices and resources are properly closed
- **Monitoring and alerting**: Track resource usage and alert on anomalies

### Library-Provided Protections

The Rust FFI layer enforces these limits:

| Limit | Value | Purpose |
|-------|-------|---------|
| `MAX_FIELDS_PER_DOCUMENT` | 1,000 | Prevents excessive field count |
| `MAX_FIELD_VALUE_LENGTH` | 10MB | Prevents massive field values |
| `MAX_QUERY_LENGTH` | 10KB | Prevents long query strings |
| `MAX_SEARCH_LIMIT` | 10,000 | Maximum results per search |

### Application Responsibilities

For production use, implement:

1. **Rate limiting** - Limit concurrent operations per user/IP
2. **Query validation** - Validate user-provided query strings
3. **Operational limits** - Set appropriate result limits (recommend: 100-1000)
4. **Query timeouts** - Implement timeouts for long-running queries
5. **Path validation** - Canonicalize paths and check against allowed directories
6. **Input validation** - Validate all user input (doc_id, fields, etc.)
  7. **Error sanitization** - Don't expose detailed error messages to end users
  8. **Monitoring** - Track query performance and resource usage

**Note on error message sanitization:**
- **Debug builds**: Full error messages including paths (easier debugging)
- **Release builds**: Paths sanitized to `<path>` placeholder (production security)
- Full errors always logged to stderr for troubleshooting

**Path security:**
The library does not enforce path restrictions. Applications using this library should:
- Canonicalize user-provided paths before use
- Verify resolved path is within allowed directories
- Check file permissions before opening index
- Be aware of symlink attacks and path traversal

Example:
```inko
import std.fs.path (Path)

let user_path = Path.new(user_provided_path)
let canonical = user_path.canonicalize

if !canonical.starts_with?('/allowed/app/data') {
  return Result.Error('Path outside allowed directory')
}
```

## CI/CD

The project uses GitHub Actions to:
1. Build **5 platform variants** using **cargo-zigbuild** in Docker (see [Cross-Platform Builds](#cross-platform-builds) for target details)
2. Run Inko tests and format checking (downloads Linux x64 library artifact)
3. Upload build artifacts for each platform
4. Upload binaries directly to GitHub releases when tags are pushed

**About Docker container:**
- Pre-installed with Rust stable and cargo-zigbuild
- Includes macOS SDK for cross-compilation
- Eliminates installation overhead
- Official image from [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)

To trigger a release:
```bash
git tag v0.1.0
git push origin v0.1.0
```

After release, download the appropriate library for your platform from the [Releases](https://github.com/jhult/inko-tantivy/releases) page.

## Troubleshooting

### "library 'tantivy_c' not found" Error

This is the most common issue. The linker can't find the FFI library during compilation.

**Solutions (in order of preference):**

1. **Use the build script (easiest for development):**
   ```bash
   ./build.sh test  # Automatically sets the correct library path
   ```

2. **Install system-wide:**
   ```bash
   ./build.sh install  # Installs to /usr/local/lib
   ```

3. **Set LIBRARY_PATH before building/testing:**
   ```bash
   # macOS
   export LIBRARY_PATH=$PWD/native/tantivy-c/target/release
   inko test

   # Linux
   export LIBRARY_PATH=$PWD/native/tantivy-c/target/release
   inko test
   ```

4. **Download pre-built library from releases:**
   - Download the appropriate library for your platform from [Releases](https://github.com/jhult/inko-tantivy/releases)
   - Copy to `/usr/local/lib/` or set LIBRARY_PATH

### Integration Tests Are Skipped

The test suite includes an FFI availability check. If the library can't be loaded, integration tests are automatically skipped with a message like:

```
Finished running 19 tests in 1 milliseconds, 0 failures
```

This means only unit tests ran. To run integration tests, ensure the library is accessible using one of the methods above.

### Library Builds But Tests Fail to Link

If `cargo build --release` succeeds but `inko test` fails with linking errors, the library is built but not in the linker search path. Use `./build.sh test` or set LIBRARY_PATH as shown above.

## License

Mozilla Public License Version 2.0

See [LICENSE](LICENSE) for the full license text.
