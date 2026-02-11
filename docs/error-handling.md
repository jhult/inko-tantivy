# Error Handling Strategy

This document describes error handling patterns, categorization, and best practices for the Tantivy FFI bindings.

## Error Types

### Transient (Retryable) Errors

Errors that may succeed on retry due to temporary conditions:

- **Network/IO errors**: File system permission issues (may resolve after system cleanup)
- **Resource exhaustion**: Out of memory during indexing (retry with smaller batch)
- **Lock contention**: Index locked by concurrent operation (wait and retry)

**Pattern:**
```inko
fn pub add_with_retry(doc_id: String, fields: Array[(String, String)]) -> Result[Bool, String] {
  let mut attempts = 0
  let max_attempts = 3

  while attempts < max_attempts {
    match add_doc(doc_id, fields) {
      case Ok(_) -> return Result.Ok(true)
      case Error(e) -> {
        if e.contains?('locked') or e.contains?('resource') {
          attempts = attempts + 1
          std.process.sleep(100)  # Wait 100ms
        } else {
          return Result.Error(e)
        }
      }
    }
  }

  Result.Error('Failed after ${max_attempts} attempts')
}
```

### Fatal (Non-Retryable) Errors

Errors that require code or configuration changes:

- **Validation errors**: Invalid index path, out-of-range memory budget
- **Schema errors**: Invalid JSON schema, field type mismatch
- **Not found**: Document or index doesn't exist
- **Configuration errors**: Invalid parameters, incompatible settings

**Pattern:**
```inko
match TantivyConfig.new(index_path) {
  case Error(e) -> {
    # Fatal error - return immediately, don't retry
    return Result.Error('Invalid configuration: ${e}')
  }
  case Ok(config) -> {}
}
```

### Warnings (Non-Fatal) Errors

Errors that don't prevent operation but indicate issues:

- **Document truncation**: Document exceeds buffer size, partially indexed
- **Field ignored**: Field not in schema, skipped during indexing
- **Performance degradation**: Search slower than expected

**Pattern:**
```inko
match add_doc(doc_id, fields) {
  case Ok(_) -> Result.Ok(true)
  case Error(e) -> {
    if e.contains?('truncated') or e.contains?('exceeds maximum size') {
      # Log warning but don't fail
      std.stdio.Stdout.new.print('Warning: ${e}')
      Result.Ok(true)  # Continue operation
    } else {
      Result.Error(e)
    }
  }
}
```

## Error Messages

### Sanitization Behavior

Error messages are sanitized differently based on build mode:

**Debug builds** (`inko build` or when `debug_assertions` is enabled):
- Full error messages returned to caller (no sanitization)
- Complete path information included for debugging
- Easier troubleshooting during development

**Release builds** (`inko build --release`):
- Error messages are sanitized to remove filesystem paths
- Paths replaced with `<path>` placeholder
- Prevents path disclosure in production environments

**Debug logging (always enabled):**
- Full error messages always logged to stderr with `[TANTIVY_ERROR]` prefix
- Captured in CI logs and development environment
- Not shown to end users in production

**Example:**

```
# Development (debug build)
Caller receives: "Failed to open index: No such file or directory (os error 2): /tmp/test/tantivy-index"
stderr: [TANTIVY_ERROR] Failed to open index: No such file or directory (os error 2): /tmp/test/tantivy-index

# Production (release build)
Caller receives: "Failed to open index: No such file or directory (os error 2): <path>"
stderr: [TANTIVY_ERROR] Failed to open index: No such file or directory (os error 2): /tmp/test/tantivy-index
```

### Why Sanitization Exists

1. **Security**: Prevents leaking filesystem structure in production logs
2. **Privacy**: Doesn't expose user data locations in error messages
3. **Compliance**: Some environments require sanitized logs
4. **Debuggability**: Full errors always logged for troubleshooting

### Include Operation Context

Add context about what operation failed:

**Good:**
```inko
Result.Error('Failed to index document with ID "${doc_id}": ${error}')
Result.Error('Search failed for query "${query}" (limit: ${limit}, offset: ${offset}): ${error}')
Result.Error('Failed to commit changes to Tantivy index at "${path}": ${error}')
```

**Bad:**
```inko
Result.Error('Indexing failed')
Result.Error('Search error')
Result.Error('Commit failed')
```

### Provide Actionable Information

Include suggestions for fixing the error:

**Good:**
```inko
Result.Error('Index path cannot be empty. Provide a valid path to the index directory.')
Result.Error('reader_memory_budget_bytes must be at least 10MB (10,000,000 bytes), got ${bytes}')
Result.Error('Document exceeds maximum size (${TANTIVY_JSON_BUFFER_SIZE} bytes). Reduce document size or increase buffer.')
```

### Categorize by Type

Use error prefixes to indicate category:

| Prefix | Meaning | Example |
|---------|----------|---------|
| `Invalid` | Validation error | `Invalid index path` |
| `Failed to` | Operation error | `Failed to open index` |
| `Timeout` | Time-based error | `Timeout while waiting for index` |
| `Not found` | Missing resource | `Document not found with ID` |
| `Locked` | Concurrency error | `Index locked by another process` |

## Error Handling Patterns

### Immediate Return on Error

Use when error is fatal and operation cannot continue:

```inko
fn pub search(query: String, limit: Int) -> Result[Array[TantivyResult], String] {
  if !is_open {
    return Result.Error('Index is not open')
  }

  if query.size > 10_000 {
    return Result.Error('Query exceeds maximum length of 10KB')
  }

  match search(query, limit, 0) {
    case Ok(results) -> Result.Ok(results)
    case Error(e) -> Result.Error("Search failed: ${e}")
  }
}
```

### Error Propagation

Propagate errors with additional context:

```inko
fn pub add_documents(docs: Array[(String, Array[(String, String)])]) -> Result[Int, String] {
  let mut indexed = 0
  let mut errors = []

  for (doc_id, fields) in docs {
    match add_doc(doc_id, fields) {
      case Ok(_) -> indexed = indexed + 1
      case Error(e) -> errors.push('Failed to index ${doc_id}: ${e}')
    }
  }

  if errors.size > 0 {
    return Result.Error('Partially completed: ${indexed}/${docs.size} indexed. Errors: ${errors.join(", ")}')
  }

  Result.Ok(indexed)
}
```

### Partial Success Handling

Handle operations where some items succeed:

```inko
fn pub batch_delete(doc_ids: Array[String]) -> Result[Int, String] {
  let mut deleted = 0
  let mut failures = []

  for doc_id in doc_ids {
    match delete_doc(doc_id) {
      case Ok(_) -> deleted = deleted + 1
      case Error(e) -> failures.push((doc_id, e))
    }
  }

  if failures.size > 0 {
    let mut error_details = []
    for (id, e) in failures {
      error_details.push('${id}: ${e}')
    }
    return Result.Error('Deleted ${deleted}/${doc_ids.size}. Failures: ${error_details.join("; ")}')
  }

  Result.Ok(deleted)
}
```

### Default Values for Errors

Provide fallback values when operation fails:

```inko
fn pub safe_search(query: String, limit: Int) -> Array[TantivyResult] {
  match search(query, limit, 0) {
    case Ok(results) -> results
    case Error(_) -> []
  }
}

# Or with warning
fn pub search_with_warning(query: String, limit: Int) -> Array[TantivyResult] {
  match search(query, limit, 0) {
    case Ok(results) -> results
    case Error(e) -> {
      std.stdio.Stdout.new.print("Warning: Search failed, returning empty results: ${e}")
      []
    }
  }
}
```

## Retry Strategies

### Exponential Backoff

For transient errors with increasing wait times:

```inko
fn pub retry_operation[T](operation: fn () -> Result[T, String], max_attempts: Int = 3) -> Result[T, String] {
  let mut attempts = 0
  let mut delay = 100

  while attempts < max_attempts {
    match operation() {
      case Ok(result) -> return Result.Ok(result)
      case Error(e) -> {
        if e.contains?('locked') or e.contains?('resource') {
          std.process.sleep(delay)
          delay = delay * 2  # Exponential backoff
          attempts = attempts + 1
        } else {
          return Result.Error(e)
        }
      }
    }
  }

  Result.Error('Operation failed after ${max_attempts} attempts')
}
```

### Linear Backoff

For simple retry scenarios:

```inko
fn pub linear_retry(operation: fn () -> Result[Bool, String], max_attempts: Int = 5, delay_ms: Int = 200) -> Result[Bool, String] {
  let mut attempts = 0

  while attempts < max_attempts {
    match operation() {
      case Ok(result) -> return Result.Ok(result)
      case Error(e) -> {
        if e.contains?('temp') or e.contains?('resource') {
          std.process.sleep(delay_ms)
          attempts = attempts + 1
        } else {
          return Result.Error(e)
        }
      }
    }
  }

  Result.Error('Operation failed after ${max_attempts} attempts')
}
```

## Best Practices

### 1. Always Check Result Types

Don't ignore errors or use `or_panic` in production code:

**Good:**
```inko
match index.commit {
  case Ok(_) -> {}
  case Error(e) -> {
    std.stdio.Stdout.new.print("Commit failed: ${e}")
    return Result.Error(e)
  }
}
```

**Bad:**
```inko
index.commit.or_panic  # Crashes on error
```

### 2. Provide Context in Error Messages

Include operation details and parameters:

**Good:**
```inko
Result.Error('Failed to search with query "${query}" (limit: ${limit}): ${error}')
```

**Bad:**
```inko
Result.Error(error)
```

### 3. Differentiate User Errors vs System Errors

User errors (invalid input) should be clear and actionable:
```inko
Result.Error('Query cannot be empty. Provide a search term.')
```

System errors should include technical details for debugging:
```inko
Result.Error('Tantivy FFI error (code: ${code}): ${error}')
```

### 4. Use Appropriate Error Categories

| Category | Example | Retryable? |
|-----------|---------|-------------|
| Validation | Invalid memory budget | No |
| Schema | Field type mismatch | No |
| IO | File not found | No |
| Network | Connection timeout | Yes |
| Resource | Out of memory | Maybe |
| Lock | Index locked | Yes |
| System | FFI error | Depends |

### 5. Log Errors Appropriately

- **Debug**: Full error details with stack traces
- **Info**: Error messages with operation context
- **Warning**: Non-fatal errors that don't stop operation
- **Error**: Fatal errors that require attention

```inko
match add_doc(doc_id, fields) {
  case Ok(_) -> {}
  case Error(e) -> {
    std.stdio.Stdout.new.print("[ERROR] Failed to index ${doc_id}: ${e}")
    return Result.Error(e)
  }
}
```

## Error Recovery Patterns

### Graceful Degradation

Continue operation with reduced functionality:

```inko
fn pub robust_search(query: String, limit: Int) -> Result[Array[TantivyResult], String] {
  match search(query, limit, 0) {
    case Ok(results) -> Result.Ok(results)
    case Error(e) -> {
      if e.contains?('memory') {
        # Reduce limit and retry
        match search(query, limit / 2, 0) {
          case Ok(results) -> {
            std.stdio.Stdout.new.print("Warning: Memory error, reduced limit to ${limit / 2}")
            Result.Ok(results)
          }
          case Error(e2) -> Result.Error(e2)
        }
      } else {
        Result.Error(e)
      }
    }
  }
}
```

### Circuit Breaker

Stop trying after repeated failures:

```inko
type CircuitBreaker {
  let mut @failures: Int
  let @threshold: Int
  let mut @last_failure: Int

  fn pub static new(threshold: Int) -> CircuitBreaker {
    CircuitBreaker(failures: 0, threshold: threshold, last_failure: 0)
  }

  fn pub mut record_success {
    @failures = 0
  }

  fn pub mut record_failure {
    @failures = @failures + 1
    @last_failure = std.time.monotonic
  }

  fn pub should_try -> Bool {
    if @failures >= @threshold {
      let cooldown = 60_000  # 1 minute
      let elapsed = std.time.monotonic - @last_failure
      return elapsed > cooldown
    }
    true
  }
}
```

## Testing Error Handling

### Test All Error Paths

```inko
t.test('add_doc returns error for invalid doc_id', fn (t) {
  match index.add_doc('', [('title', 'test')]) {
    case Ok(_) -> t.true(false)
    case Error(e) -> t.true(e.contains?('empty') or e.contains?('doc_id'))
  }
})

t.test('search returns error for empty query', fn (t) {
  match index.search('', 10, 0) {
    case Ok(_) -> t.true(false)
    case Error(e) -> t.true(e.contains?('empty') or e.contains?('query'))
  }
})
```

### Test Error Recovery

```inko
t.test('retry operation succeeds after transient failure', fn (t) {
  let mut attempts = 0
  let mut result = Result.Error('not yet')

  while attempts < 3 and result.is_error {
    match index.add_doc('test', []) {
      case Ok(_) -> {}
      case Error(e) -> result = Result.Error(e)
    }
    attempts = attempts + 1
  }

  t.true(result.is_ok)
})
```

## Common Error Scenarios

### Index Already Open

```inko
if is_open {
  return Result.Error('Index is already open. Close before opening again.')
}
```

### Index Not Open

```inko
if !is_open {
  return Result.Error('Index is not open. Call open() before performing operations.')
}
```

### Document Too Large

```inko
match add_doc(doc_id, large_fields) {
  case Ok(_) -> {}
  case Error(e) -> {
    if e.contains?('exceeds maximum size') {
      return Result.Error('Document too large. Maximum size: ${TANTIVY_JSON_BUFFER_SIZE} bytes. Actual: ${size} bytes.')
    }
    Result.Error(e)
  }
}
```

### Schema Mismatch

```inko
match add_doc(doc_id, fields) {
  case Ok(_) -> {}
  case Error(e) -> {
    if e.contains?('schema') or e.contains?('field type') {
      return Result.Error('Schema mismatch. Check that all fields match the index schema: ${e}')
    }
    Result.Error(e)
  }
}
```

## Summary

| Pattern | Use When | Benefit |
|---------|----------|---------|
| Immediate return | Fatal errors | Fast fail, clear error |
| Error propagation | Multi-step operations | Partial success reporting |
| Retry with backoff | Transient errors | Resilience |
| Circuit breaker | Repeated failures | Prevent cascading failures |
| Graceful degradation | Non-critical failures | Maintain availability |

**Key principles:**

1. Always check Result types, don't ignore errors
2. Provide context in error messages (operation, parameters)
3. Distinguish between transient (retryable) and fatal errors
4. Use appropriate error categories and prefixes
5. Test all error paths in unit tests
6. Log errors appropriately for debugging
7. Provide actionable error messages with suggestions
