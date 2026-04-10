# Security and Resource Management

This document outlines security considerations and resource management practices for the inko-tantivy library. Understanding these topics is essential for building secure and reliable applications.

## Overview

The inko-tantivy library provides basic protections against resource exhaustion and common attacks, but **applications must implement additional protections** for production use. This library follows the principle of "safe by default" while maintaining flexibility for diverse use cases.

## Library-Provided Protections

### Memory Limits

The Rust FFI layer enforces these limits to prevent memory exhaustion:

| Limit | Value | Purpose |
|-------|-------|---------|
| `MAX_FIELDS_PER_DOCUMENT` | 1,000 | Prevents excessive field count in documents |
| `MAX_FIELD_VALUE_LENGTH` | 10MB | Prevents massive field values |
| `MAX_QUERY_LENGTH` | 10KB | Prevents very long query strings |
| `MAX_SEARCH_LIMIT` | 10,000 | Maximum results per search operation |

These limits are enforced in the Rust FFI layer (`native/tantivy-c/src/lib.rs`) and cannot be bypassed at the Inko layer.

### Buffer Sizes

Fixed buffer sizes prevent buffer overflow attacks:

| Buffer | Size | Purpose |
|--------|-------|---------|
| `TANTIVY_JSON_BUFFER_SIZE` | 1MB | Document JSON parsing |
| `TANTIVY_STRING_FIELD_BUFFER_SIZE` | 4KB | Individual string field values |
| `TANTIVY_ERROR_BUFFER_SIZE` | 1KB | Error message generation |
| `TANTIVY_LOG_ERRORS` | (unset) | Set to `0` to suppress `[TANTIVY_ERROR]` stderr logging |

### Error Message Sanitization

The Rust FFI layer includes error message sanitization to prevent information disclosure in production builds:

**Sanitization Rules:**

1. **Debug builds**: No sanitization, full error information available for debugging
2. **Release builds**: Error messages are sanitized to hide sensitive information

**What Gets Sanitized:**

- Absolute filesystem paths are replaced with `<path>` placeholder
- This prevents leaking server directory structure
- Examples of sanitized patterns:
  - `/var/lib/tantivy/index` → `<path>`
  - `/home/user/data/backup` → `<path>`

**What Gets Preserved:**

- Error descriptions and context (what went wrong)
- Parameter names and values (without paths)
- Stack traces in debug builds
- All error messages in debug builds

**Examples:**

Before sanitization (debug build):
```
Error: Failed to create index: Io(Os { code: 2, kind: PermissionDenied, message: "Permission denied" })
Path: /var/lib/tantivy/user_index
```

After sanitization (release build):
```
Error: Failed to create index: Io(Os { code: 2, kind: PermissionDenied, message: "Permission denied" })
Path: <path>
```

**Implementation Details:**

The sanitization is implemented in `native/tantivy-c/src/lib.rs`:

```rust
// Compile-time flag for error sanitization
// Debug builds: No sanitization, full information available
// Release builds: Sanitized for production security
#[cfg(debug_assertions)]
const SANITIZE_ERRORS: bool = false;

#[cfg(not(debug_assertions))]
const SANITIZE_ERRORS: bool = true;

// Sanitize error messages to remove filesystem paths
fn sanitize_error_message(msg: &str) -> String {
    // Replace absolute paths with generic placeholders
    // See implementation in lib.rs for details
}
```

**Testing:**

Comprehensive tests verify sanitization behavior:
- Simple paths: `/var/lib/index` → `<path>`
- Multiple paths: All paths replaced with `<path>`
- No path: Messages unchanged
- Empty paths: Single `/` character preserved
- Special characters: Paths with quotes, colons, whitespace handled correctly

See `native/tantivy-c/tests/error_sanitization.rs` for test details.

### Path Validation

The library includes basic path validation to prevent obvious directory traversal attempts:

```inko
fn validate_index_path(path: String) -> Result[Bool, String] {
  # Reject paths containing '..' to prevent directory traversal
  if path.contains?('..') {
    return Result.Error(
      'index_path cannot contain ".." (directory traversal not allowed)',
    )
  }
  
  # Reject empty paths
  if path.size == 0 {
    return Result.Error('index_path cannot be empty')
  }
  
  Result.Ok(true)
}
```

**Important:** This is **basic validation** and does not provide complete protection against path-based attacks. See [Path Security](#path-security) below for comprehensive guidance.

## Application-Level Protections Required

For production use, applications **must** implement additional protections in these areas:

### 1. Rate Limiting

The library does not enforce rate limiting. Applications must limit:

- **Concurrent operations per index**: Prevent index lock contention
- **Operations per user/IP**: Prevent DoS through high-volume requests
- **Resource consumption**: Prevent memory and CPU exhaustion

#### Using the RateLimiter Type

The library includes a `RateLimiter` type for rate limiting operations. This uses a token bucket algorithm with configurable capacity and refill rate:

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

**For production deployments, consider:**

1. **Separate limiters for different operations**: Search, index, and delete operations may have different resource costs
2. **Per-user or per-IP tracking**: Implement multiple `RateLimiter` instances for multi-tenant systems
3. **Time-based refill**: Use a timer to periodically call `refill()` on each limiter
4. **Monitoring and alerting**: Track rate limit rejections to identify abuse patterns
5. **Graceful degradation**: Return partial results or cached data when rate limits are hit

### 2. Query Validation

Validate user-provided query strings to prevent expensive queries:

```inko
fn validate_query(query: String, max_length: Int = 1024) -> Result[Bool, String] {
  # Check query length
  if query.size > max_length {
    return Result.Error(
      'Query exceeds maximum length of ${max_length} characters. Got ${query.size} characters.'
    )
  }
  
  # Check for dangerous patterns (customize based on your use case)
  let dangerous_patterns = ['*:*', ' OR NOT ', ' AND NOT ']
  
  for pattern in dangerous_patterns {
    if query.contains?(pattern) {
      return Result.Error(
        'Query contains potentially expensive pattern: "${pattern}". This query may scan the entire index.'
      )
    }
  }
  
  # Check query complexity (simple heuristic)
  let complexity = query.size + (query.count?('OR') * 10) + (query.count?('AND') * 5)
  
  if complexity > 5000 {
    return Result.Error(
      'Query too complex. Complexity score: ${complexity}, maximum: 5000'
    )
  }
  
  Result.Ok(true)
}

# Usage
match validate_query(user_query) {
  case Ok(_) -> {
    match index.search(user_query, limit: 100, offset: 0) {
      case Ok(results) -> { /* process results */ }
      case Error(e) -> { /* handle error */ }
    }
  }
  case Error(e) -> {
    std.stdio.Stdout.new.print("Invalid query: ${e}")
  }
}
```

### 3. Operational Limits

Set appropriate limits for search and aggregation operations:

| Operation | Recommended Limit | Maximum Limit |
|-----------|------------------|---------------|
| `search()` | 100-1000 | 10,000 |
| `aggregate_terms()` | 100-1000 | 10,000 |
| `autocomplete()` | 5-10 | 10,000 |
| `did_you_mean()` | 5-10 | 10,000 |

**Example: Enforce operational limits**

```inko
fn safe_search(
  index: TantivyIndexManager,
  query: String,
  user_limit: Int,
) -> Result[Array[TantivyResult], String] {
  # Clamp user limit to safe range
  let limit = if user_limit < 1 {
    10  # Default
  } else if user_limit > 1000 {
    1000  # Maximum safe limit
  } else {
    user_limit
  }
  
  index.search(query: query, limit: limit, offset: 0)
}
```

### 4. Query Timeouts

Long-running queries can block threads and consume excessive resources. Applications should implement timeout mechanisms:

**Recommended approaches:**

1. **Use external process management**: Run search operations in separate processes and enforce timeouts via process supervision
2. **Set Tantivy query limits**: Use reasonable `limit` and `offset` values to prevent large result sets
3. **Monitor query duration**: Track execution times and alert on slow queries
4. **Implement query validation**: Reject overly complex queries before execution

**Example: Timeout wrapper (conceptual)**

```inko
# Applications should implement timeout wrappers using process supervision
# The exact implementation depends on your application architecture

fn search_safely(
  index: TantivyIndexManager,
  query: String,
  max_duration_ms: Int = 5000,
) -> Result[Array[TantivyResult], String] {
  # Use application-level timeout mechanism
  # This is a placeholder - actual implementation depends on your timeout strategy
  
  # Recommended approach:
  # 1. Start a timer
  # 2. Execute search
  # 3. Cancel or return timeout if timer expires
  
  index.search(query, limit: 100, offset: 0)
}
```

**For production systems:**

- Monitor average query duration and set timeouts at 3-5x the average
- Implement circuit breakers for failing endpoints
- Log timeouts for debugging and security analysis
- Consider per-query-type timeout limits (e.g., strict limits for complex aggregations)

### 5. Path Security

The library provides basic path validation, but applications must:

1. **Canonicalize paths** before use to resolve symlinks
2. **Check against allowed directories** to ensure paths are within expected locations
3. **Validate file permissions** before opening indices
4. **Use absolute paths** or explicitly define base directories

**Example: Safe path handling**

```inko
import std.fs.path (Path)
import std.fs.file (File)

fn safe_index_path(user_provided: String, allowed_base: String) -> Result[String, String] {
  let user_path = Path.new(user_provided)
  let base_path = Path.new(allowed_base)
  
  # Canonicalize both paths
  match user_path.canonicalize {
    case Ok(user_canonical) -> {
      match base_path.canonicalize {
        case Ok(base_canonical) -> {
          # Check if user path is within allowed base
          if !user_canonical.starts_with?(base_canonical.to_string) {
            return Result.Error(
              'Path "${user_provided}" is outside allowed directory "${allowed_base}"'
            )
          }
          
          # Check write permissions (for new indices)
          match File.new(user_canonical.to_string) {
            case Ok(_) -> Result.Ok(user_canonical.to_string)
            case Error(e) -> {
              # If file doesn't exist, check parent directory
              match user_path.parent {
                case Some(parent) -> {
                  match parent.canonicalize {
                    case Ok(parent_canonical) -> {
                      if parent_canonical.starts_with?(base_canonical.to_string) {
                        Result.Ok(user_canonical.to_string)
                      } else {
                        Result.Error(
                          'Parent directory is outside allowed base: ${allowed_base}'
                        )
                      }
                    }
                    case Error(e2) -> Result.Error("Failed to canonicalize parent: ${e2}")
                  }
                }
                case None -> Result.Error('Invalid path: no parent directory')
              }
            }
          }
        }
        case Error(e) -> Result.Error("Failed to canonicalize base path: ${e}")
      }
    }
    case Error(e) -> Result.Error("Failed to canonicalize user path: ${e}")
  }
}

# Usage
let allowed_dir = '/var/lib/myapp/indices'
match safe_index_path(user_path, allowed_dir) {
  case Ok(safe_path) -> {
    match TantivyConfig.new(safe_path) {
      case Ok(config) -> { /* use config */ }
      case Error(e) -> { /* handle error */ }
    }
  }
  case Error(e) -> {
    std.stdio.Stdout.new.print("Path rejected: ${e}")
  }
}
```

## Security Best Practices

### 1. Input Validation

Always validate user input before passing to library functions:

```inko
fn safe_add_doc(
  index: TantivyIndexManager,
  doc_id: String,
  fields: Array[(String, String)],
) -> Result[Bool, String] {
  # Validate doc_id
  if doc_id.size == 0 {
    return Result.Error('Document ID cannot be empty')
  }
  
  if doc_id.size > 256 {
    return Result.Error('Document ID too long (max 256 characters)')
  }
  
  # Validate fields
  for (name, value) in fields {
    if name.size == 0 {
      return Result.Error('Field name cannot be empty')
    }
    
    if value.size > 10_000_000 {
      return Result.Error(
        'Field "${name}" value too large (max 10MB)'
      )
    }
  }
  
  # Maximum field count
  if fields.size > 1000 {
    return Result.Error(
      'Too many fields (max 1000, got ${fields.size})'
    )
  }
  
  index.add_doc(doc_id: doc_id, fields: fields)
}
```

### 2. Error Handling

Never expose detailed error messages to end users:

```inko
match index.search(query, limit: limit, offset: offset) {
  case Ok(results) -> { /* process results */ }
  case Error(e) -> {
    # Log full error for debugging
    std.stdio.Stderr.new.print('[DEBUG] Search failed: ${e}')
    
    # Return generic error to user
    return Result.Error('Search failed. Please try again.')
  }
}
```

### 3. Resource Cleanup

Always close indices and free resources:

```inko
fn with_index[T](
  config: TantivyConfig,
  operation: fn (mut TantivyIndexManager) -> Result[T, String],
) -> Result[T, String] {
  let mut index = TantivyIndexManager.new(config)
  
  match index.open {
    case Ok(_) -> {
      let result = operation(index)
      
      # Always close, even if operation fails
      match index.close {
        case Ok(_) -> result
        case Error(e) -> {
          std.stdio.Stderr.new.print(
            '[WARNING] Failed to close index: ${e}'
          )
          result
        }
      }
    }
    case Error(e) -> Result.Error("Failed to open index: ${e}")
  }
}

# Usage
let result = with_index(config, fn (mut index) {
  index.add_doc('doc1', fields).or_panic
  index.search('query', limit: 10, offset: 0)
})
```

### 4. Monitoring and Alerting

Monitor resource usage and alert on anomalies:

```inko
import std.time (Time)

type SearchMetrics {
  let @total_searches: Int
  let @failed_searches: Int
  let @total_results: Int
  let @slow_queries: Int
  
  fn pub static new -> SearchMetrics {
    SearchMetrics(
      total_searches: 0,
      failed_searches: 0,
      total_results: 0,
      slow_queries: 0,
    )
  }
  
  fn pub mut record_search(result_count: Int, duration_ms: Int) {
    @total_searches = @total_searches + 1
    @total_results = @total_results + result_count
    
    if duration_ms > 1000 {
      @slow_queries = @slow_queries + 1
    }
  }
  
  fn pub mut record_failure {
    @failed_searches = @failed_searches + 1
  }
  
  fn pub error_rate -> Float {
    if @total_searches == 0 {
      0.0
    } else {
      (@failed_searches as Float) / (@total_searches as Float)
    }
  }
}

# Usage
let mut metrics = SearchMetrics.new

let start = std.time.monotonic
match index.search(query, limit: 100, offset: 0) {
  case Ok(results) -> {
    let duration = (std.time.monotonic - start) / 1_000_000  # Convert to ms
    metrics.record_search(results.size, duration)
    
    # Alert on high error rate
    if metrics.error_rate > 0.1 {
      std.stdio.Stderr.new.print(
        '[ALERT] High search error rate: ${metrics.error_rate * 100}%'
      )
    }
  }
  case Error(e) -> {
    metrics.record_failure
    std.stdio.Stderr.new.print("Search failed: ${e}")
  }
}
```

## Common Attack Vectors and Mitigations

### 1. Wildcard Query DoS

**Attack:** Search with `*:*` to scan entire index

```inko
index.search('*:*', limit: 10000, offset: 0)
```

**Mitigation:** Reject wildcard patterns in user queries

```inko
fn validate_query_no_wildcard(query: String) -> Result[Bool, String] {
  let wildcards = ['*:*', 'field:*', '* AND *']
  
  for pattern in wildcards {
    if query.contains?(pattern) {
      return Result.Error('Wildcard queries are not allowed')
    }
  }
  
  Result.Ok(true)
}
```

### 2. Fuzzy Query DoS

**Attack:** Request large edit distance to force expensive calculations

```inko
index.did_you_mean('field', 'term', distance: 10, limit: 10000)
```

**Mitigation:** Limit edit distance and result count

```inko
fn safe_did_you_mean(
  index: TantivyIndexManager,
  field: String,
  term: String,
  user_distance: Int,
) -> Result[Array[TantivySuggestion], String] {
  let distance = if user_distance > 2 {
    2  # Maximum safe distance
  } else {
    user_distance
  }
  
  index.did_you_mean(field: field, term: term, distance: distance, limit: 5)
}
```

### 3. Large Offset Abuse

**Attack:** Request offset beyond actual results

```inko
index.search('term', limit: 10000, offset: 100000)
```

**Mitigation:** Reject unreasonably large offsets

```inko
fn safe_search_with_offset(
  index: TantivyIndexManager,
  query: String,
  limit: Int,
  offset: Int,
) -> Result[Array[TantivyResult], String] {
  if offset < 0 {
    return Result.Error('Offset cannot be negative')
  }
  
  if offset > 10000 {
    return Result.Error('Offset too large (max 10000)')
  }
  
  index.search(query: query, limit: limit, offset: offset)
}
```

### 4. Aggregation Flood

**Attack:** Request aggregation of all unique values

```inko
index.aggregate_terms('large_field', '*', limit: 10000)
```

**Mitigation:** Limit aggregation result count

```inko
fn safe_aggregate_terms(
  index: TantivyIndexManager,
  field_name: String,
  query: String,
  user_limit: Int,
) -> Result[Array[TantivyAggregationResult], String] {
  let limit = if user_limit > 1000 {
    1000  # Maximum safe aggregation limit
  } else {
    user_limit
  }
  
  index.aggregate_terms(field_name: field_name, query: query, limit: limit)
}
```

## Related Documentation

- [memory.md](memory.md) - Memory ownership and safety
- [error-handling.md](error-handling.md) - Error handling patterns
- [thread-safety.md](thread-safety.md) - Concurrency considerations

## Security Disclosure

If you discover a security vulnerability in this library, please disclose it responsibly:

1. **Do not** create a public issue
2. **Do not** discuss it publicly
3. **Do** send an email to the maintainers privately
4. **Do** provide a detailed report with reproduction steps
5. **Do** allow time for the issue to be addressed before disclosure

The maintainers will acknowledge receipt, investigate the issue, and provide a timeline for fixing it.
