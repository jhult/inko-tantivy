# Memory Ownership in Tantivy FFI

This document describes the memory ownership model across the Rust-Inko FFI boundary.

## Overview

The Tantivy FFI uses explicit ownership transfer to ensure memory safety without garbage collection. Understanding who owns which allocations is critical to prevent memory leaks and use-after-free errors.

## Ownership Rules

### Rust-Owned Memory

**Returned pointers are owned by Inko** - When Rust returns a pointer to Inko, Inko must free it:

- `tantivy_index_open` returns `*mut TantivyIndexWrapper` - **owned by Inko**
- `tantivy_index_search` populates `results_out` with `*mut TantivyResult` array - **owned by Inko**
- `tantivy_index_get_doc` returns `*mut c_char` (JSON string) - **owned by Inko**
- `tantivy_index_get_docs` returns array of JSON strings - **owned by Inko**
- `tantivy_aggregate_terms` populates `results_out` with `TantivyAggregationResult` array - **owned by Inko**
- `tantivy_autocomplete` and `tantivy_did_you_mean` populate suggestions arrays - **owned by Inko**

**Free functions MUST be called** after use:

| Function | Returns | Free Function |
|---------|---------|---------------|
| `tantivy_index_open` | `*mut TantivyIndexWrapper` | `tantivy_index_close` |
| `tantivy_index_search` | `*mut TantivyResult` array | `tantivy_result_free` |
| `tantivy_index_get_doc` | `*mut c_char` (JSON) | `tantivy_string_free` |
| `tantivy_index_get_docs` | `*mut *mut c_char` array | `tantivy_docs_array_free` |
| `tantivy_aggregate_terms` | `*mut TantivyAggregationResult` array | `tantivy_aggregation_results_free` |
| `tantivy_autocomplete` | `*mut TantivySuggestion` array | `tantivy_suggestions_free` |
| `tantivy_did_you_mean` | `*mut TantivySuggestion` array | `tantivy_suggestions_free` |
| Error strings | `*mut c_char` | `tantivy_error_free` |

### Inko-Owned Memory

**Input pointers are borrowed by Rust** - When Inko passes pointers to Rust, Rust does not free them:

- `*const TantivyConfig` - borrowed, not freed by Rust
- `*const c_char` strings in `DocField` - borrowed, not freed by Rust
- `*const c_char` query strings - borrowed, not freed by Rust

These remain owned by Inko and can be freed after the function returns.

### String Array to C Pointers Pattern

The `string_array_to_c_pointers` function (tantivy.inko:944) converts an array of Inko strings to C string pointers for FFI calls.

**Critical Lifetime Requirement:**

The returned pointers are only valid for a very short window - from when the function returns until the FFI call completes. This function creates intermediate ByteArray buffers for each string but only returns the pointer array.

**Correct Usage:**

```inko
# Use pointers immediately in same scope
let c_strings = string_array_to_c_pointers(doc_ids)
let result = tantivy_index_delete_docs(idx, c_strings.pointer, count, error)
# FFI completes here, buffers can be dropped
```

**Incorrect Usage:**

```inko
# BAD: Storing for later use causes use-after-free
let c_strings = string_array_to_c_pointers(doc_ids)
some_global_state.store(c_strings)  # Pointers become invalid here
```

**Why This Works:**

The current design works because:
1. Caller uses pointers immediately in same scope (see delete_documents:904)
2. Inko's memory model keeps intermediate buffers alive during the function call
3. Returned ByteArray and intermediate buffers drop together after FFI call

**Alternative Design Considerations:**

Returning `(pointers: ByteArray, buffers: Array[ByteArray])` would make ownership explicit but adds overhead. Current design prioritizes simplicity for this specific use case.

## Lifetimes

### Index Wrapper

The `TantivyIndexWrapper` pointer returned by `tantivy_index_open` is valid until `tantivy_index_close` is called:

```inko
let config = TantivyConfig.new('/tmp/index')
let manager = TantivyIndexManager.new(config)
match manager.open {
  case Ok(_) -> {
    # Index wrapper is valid here
    let _ = manager.search(...)
    let _ = manager.close  # Must be called to free memory
  }
  case Error(_) -> {}
}
```

### Result Arrays

Result arrays are allocated by Rust and must be freed after use:

```inko
match manager.search('query', 10, 0) {
  case Ok(results) -> {
    # results array is valid here
    for result in results.iter {
      # Each result contains owned strings (doc_id, highlight)
    }
    # results and its contents are freed when results is dropped
  }
  case Error(e) -> {}
}
```

### Error Strings

Error strings are allocated by Rust and must be freed:

```inko
match manager.add_doc('id', fields) {
  case Ok(_) -> {}
  case Error(msg) -> {
    # msg is owned by Inko, must be freed
  }
}
```

## Implementation Details

### Rust Side

Rust uses `ManuallyDrop` for explicit ownership transfer:

```rust
// Transfer ownership of Vec to C
fn vec_into_raw_parts<T>(vec: Vec<T>) -> (*mut T, usize) {
    let mut vec = ManuallyDrop::new(vec);
    let ptr = vec.as_mut_ptr();
    let len = vec.len();
    (ptr, len)
}
```

This ensures ownership is explicitly transferred and documented.

### Inko Side

Inko automatically drops values when they go out of scope. The TantivyIndexManager handles freeing result arrays using `Drop` trait:

```inko
impl Drop for TantivyIndexManager {
  fn mut drop {
    match @index {
      case Some(idx) -> {
        let result = tantivy_index_close(idx)
        @index = Option.None

        if (result as Int) != 0 {
          std.stdio.Stderr.new.print(
            '[TANTIVY_DROP] Warning: Failed to close index during ' ++
            'automatic cleanup. Consider calling close() explicitly. ' ++
            'Error code: ${result}'
          )
        }
      }
      case None -> {}
    }
  }
}
```

**Drop Behavior:**

- Drop attempts to close the index if not already closed via `close()` method
- If close fails, the error is logged to stderr (not to user-facing output)
- This allows detection of close errors in logs without crashing or panicking
- **Always call `close()` explicitly** for proper error handling in production code

The `close()` method returns `Result[Bool, String]` and allows proper error handling:

```inko
let mut index = TantivyIndexManager.new(config)
index.open.or_panic

# ... use index ...

# Explicit close with error handling
match index.close {
  case Ok(_) -> {}
  case Error(e) -> {
    std.stdio.Stderr.new.print("Failed to close index: ${e}")
  }
}
```

## Common Mistakes

### Memory Leaks

**Don't forget to close the index:**

```inko
# BAD - Memory leak
let manager = TantivyIndexManager.new(config)
manager.open
# Never closed!

# GOOD - Memory freed
let manager = TantivyIndexManager.new(config)
manager.open
manager.close
```

### Use-After-Free

**Don't use freed pointers:**

```inko
# BAD - Use after close
let manager = TantivyIndexManager.new(config)
manager.open
manager.close
manager.search  # Crash! Index is freed

# GOOD - Use only when open
let manager = TantivyIndexManager.new(config)
manager.open
manager.search
manager.close
```

### Double Free

**Don't free owned pointers twice:**

The TantivyIndexManager automatically frees result arrays when dropped. Don't manually free them.

## Security Considerations

### Buffer Sizes

All buffers have size limits to prevent DoS attacks:

| Buffer | Size | Purpose |
|--------|-------|---------|
| `TANTIVY_JSON_BUFFER_SIZE` | 1MB | Document JSON |
| `TANTIVY_STRING_FIELD_BUFFER_SIZE` | 4KB | Individual string fields |
| `TANTIVY_ERROR_BUFFER_SIZE` | 1KB | Error messages |

### Resource Limits

Additional limits prevent resource exhaustion:

- `MAX_FIELDS_PER_DOCUMENT: 1000` - Prevents excessive field count
- `MAX_FIELD_VALUE_LENGTH: 10MB` - Prevents large field values
- `MAX_QUERY_LENGTH: 10KB` - Prevents long query strings
- `MAX_SEARCH_LIMIT: 10000` - Prevents large result sets

## Performance Optimization Opportunities

The current implementation prioritizes correctness and simplicity over maximum performance. Potential optimizations exist but should only be pursued if benchmarks show >10% impact.

### Current Performance Characteristics

- **Default aggregation limit**: 1000 results (~16KB memory)
- **Maximum search limit**: 10000 results (~240KB memory)
- **Typical document size**: 1-10KB of indexed content
- **Batch indexing**: ~21,000 allocations for 1000 documents with 10 fields each

### Identified Optimization Opportunities

#### 1. Buffer Pooling for string_to_c_buffer (Issue inko-tantivy-axm)

**Current behavior:**
- Each FFI call allocates temporary ByteArray for string conversion
- Batch operations create many allocations (~21,000 for 1000 docs)

**Potential improvement:**
- Reuse buffers via object pool
- Reduces GC pressure and allocation overhead

**Action:** Benchmark before optimizing - may not be significant for typical workloads

#### 2. Streaming Iterators for Aggregations (Issue inko-tantivy-kpa)

**Current behavior:**
- `aggregate_terms`, `autocomplete`, `did_you_mean` read all results into memory
- Caller receives entire array at once

**Potential improvement:**
- Implement lazy iterators that yield results on-demand
- Allows early termination and reduced memory footprint

**Action:** Only if benchmarks show >10% memory or time impact for large limits

#### 3. Builder Pattern for Batch Operations

**Current behavior:**
- Batch documents built incrementally with individual allocations

**Potential improvement:**
- Accumulate documents, convert in single pass
- Cleaner API with better separation of concerns

**Action:** Consider for API clarity, not primarily for performance

### Recommendations

1. **Profile first**: Use realistic workloads to measure actual performance
2. **Measure impact**: Only optimize if improvement >10% on key metrics
3. **Document trade-offs**: Any optimizations should note complexity vs benefit
4. **Maintain safety**: Don't compromise memory safety for performance
5. **Test thoroughly**: Ensure optimizations maintain correctness

## References

- [Rust FFI Best Practices](https://doc.rust-lang.org/nomicon/ffi.html)
- [Inko Memory Management](https://docs.inko-lang.org/manual/latest/memory-management.html)
- [Inko FFI Guide](https://docs.inko-lang.org/manual/latest/ffi.html)
