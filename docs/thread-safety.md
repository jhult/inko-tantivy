# Thread Safety and Concurrency

This document describes thread safety guarantees and concurrent access patterns for the Tantivy FFI bindings.

## Concurrency Model in Inko

Inko uses **lightweight processes** for safe concurrency:
- Each `type async` instance spawns an isolated process
- Processes communicate via messages (no shared memory)
- Data races are impossible by design
- Preemptive multitasking across a fixed-size thread pool

## TantivyIndexManager Thread Safety

### Current Behavior

`TantivyIndexManager` is **not thread-safe** for concurrent access:

```inko
# DANGER: Multiple mutable borrows cause runtime panics
let mut index = TantivyIndexManager.new(config)
index.open.or_panic

# Process 1 tries to write
process.send(mut index)
process_write(mut index)  # PANIC: Already borrowed

# Process 2 tries to read
process.read(mut index)  # PANIC: Already borrowed
```

**Why:** Inko's ownership system prevents multiple mutable borrows, and regular types cannot be sent between processes.

### Safe Concurrent Patterns

#### Option 1: One Manager Per Process (Recommended)

Create a separate `TantivyIndexManager` for each process that needs access:

```inko
import std.sync (Promise)

type async IndexWriter {
  let @manager: TantivyIndexManager

  fn async new(config: TantivyConfig) -> IndexWriter {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    IndexWriter(manager: manager)
  }

  fn async pub mut add_doc(doc_id: String, fields: Array[(String, String)]) -> Result[Bool, String] {
    @manager.add_doc(doc_id, fields)
  }

  fn async pub mut commit -> Result[Bool, String] {
    @manager.commit
  }
}

type async IndexReader {
  let @manager: TantivyIndexManager

  fn async new(config: TantivyConfig) -> IndexReader {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    IndexReader(manager: manager)
  }

  fn async pub search(query: String, limit: Int, offset: Int) -> Result[Array[TantivyResult], String] {
    @manager.search(query, limit, offset)
  }
}

type async Main {
  fn async main {
    let config = TantivyConfig.new('/tmp/index').or_panic

    # Separate manager for writes
    let writer = IndexWriter.new(config)

    # Separate manager for reads
    let reader = IndexReader.new(config)

    # Concurrent operations are safe
    let writer_result = writer.add_doc('doc1', [('title', 'Hello')])
    let reader_result = reader.search('world', 10, 0)

    # Both operations run concurrently
  }
}
```

**Benefits:**
- No locking required
- Safe by design
- Simplified error handling
- Each process has its own connection to Tantivy

**Trade-offs:**
- Multiple open connections to the same index
- Higher memory usage
- Tantivy's internal synchronization handles concurrent access

#### Option 2: Single Writer, Multiple Readers

For workloads with one writer and multiple readers:

```inko
type async WriteWorker {
  let @manager: TantivyIndexManager

  fn async new(config: TantivyConfig) -> WriteWorker {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    WriteWorker(manager: manager)
  }

  fn async pub mut write_all(docs: Array[(String, Array[(String, String)])]) -> Result[Int, String] {
    let mut count = 0
    for (doc_id, fields) in docs {
      match @manager.add_doc(doc_id, fields) {
        case Ok(_) -> count = count + 1
        case Error(_) -> {}
      }
    }
    @manager.commit.or_panic
    Result.Ok(count)
  }
}

type async ReadWorker {
  let @manager: TantivyIndexManager

  fn async new(config: TantivyConfig) -> ReadWorker {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    ReadWorker(manager: manager)
  }

  fn async pub search(query: String) -> Result[Array[TantivyResult], String] {
    @manager.search(query, 10, 0)
  }
}
```

## Tantivy's Internal Thread Safety

Tantivy (the underlying Rust library) is **thread-safe** for:
- **Concurrent reads**: Multiple threads can search the same index
- **Single writer + multiple readers**: One thread writes while others read
- **Batch operations**: Efficient concurrent indexing

The `num_threads` parameter in `TantivyConfig` controls parallelism within Tantivy:
```inko
let config = TantivyConfig.new('/tmp/index').or_panic
let config = config.with_threads(8).or_panic  # Use 8 threads internally
```

**Internal threading handles:**
- Parallel document indexing
- Concurrent search operations
- Multi-segment merging

## Rust FFI Thread Safety

The FFI layer is safe for concurrent access:

### Safe Operations
- Multiple `TantivyIndexManager` instances can access the same index
- Each instance has its own `@index` pointer to the same underlying data
- Tantivy's internal synchronization prevents data races

### Unsafe Patterns to Avoid

```inko
# DANGER: Sharing pointers across processes
let mut manager = TantivyIndexManager.new(config)
manager.open.or_panic

# Extract raw pointer (unsafe!)
let raw_ptr = match manager.index {
  case Some(ptr) -> ptr
  case None -> panic("Index not open")
}

# Send pointer to another process (UNSAFE!)
process.send(raw_ptr)  # Use-after-free risk!
```

## Memory Safety in Concurrent Scenarios

### Drop Safety

`TantivyIndexManager` implements the `Drop` trait for automatic cleanup:

```inko
impl Drop for TantivyIndexManager {
  fn mut drop {
    match @index {
      case Some(idx) -> {
        let result = tantivy_index_close(idx)
        @index = Option.None
      }
      case None -> {}
    }
  }
}
```

**Guarantees:**
- Each `TantivyIndexManager` instance closes its own index
- No double-free (multiple instances reference same underlying data)
- Tantivy's reference counting handles shared data

### Process Cleanup

When a process terminates, all its owned values are dropped:

```inko
type async SearchWorker {
  let @manager: TantivyIndexManager

  fn async new(config: TantivyConfig) -> SearchWorker {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    SearchWorker(manager: manager)
  }

  fn async pub search(query: String) -> Result[Array[TantivyResult], String] {
    @manager.search(query, 10, 0)
  }
}

# When worker goes out of scope, TantivyIndexManager is dropped automatically
{
  let worker = SearchWorker.new(config)
  worker.search('query').or_panic
}  # worker dropped, index closed automatically
```

## Recommended Patterns

### For High-Throughput Indexing

```inko
type async BatchIndexer {
  let @manager: TantivyIndexManager
  let @batch_size: Int

  fn async new(config: TantivyConfig, batch_size: Int) -> BatchIndexer {
    let mut manager = TantivyIndexManager.new(config)
    manager.open.or_panic
    BatchIndexer(manager: manager, batch_size: batch_size)
  }

  fn async pub mut index_docs(docs: Array[(String, Array[(String, String)])]) -> Result[Int, String] {
    let mut indexed = 0
    let mut batch = []

    for (doc_id, fields) in docs {
      batch.push((doc_id, fields))

      if batch.size >= @batch_size {
        match @manager.add_docs_batch(batch) {
          case Ok(count) -> indexed = indexed + count
          case Error(e) -> return Result.Error(e)
        }
        batch = []
      }
    }

    if batch.size > 0 {
      match @manager.add_docs_batch(batch) {
        case Ok(count) -> indexed = indexed + count
        case Error(e) -> return Result.Error(e)
      }
    }

    @manager.commit.or_panic
    Result.Ok(indexed)
  }
}
```

### For Concurrent Search

```inko
type async SearchPool {
  let @workers: Array[SearchWorker]

  fn async new(config: TantivyConfig, pool_size: Int) -> SearchPool {
    let mut workers = []
    let mut i = 0

    while i < pool_size {
      workers.push(SearchWorker.new(config))
      i = i + 1
    }

    SearchPool(workers: workers)
  }

  fn async pub search(query: String) -> Result[Array[TantivyResult], String] {
    # Round-robin worker selection (simplified)
    let worker_index = query.hash.mod(@workers.size)
    @workers.get(worker_index).unwrap.search(query)
  }
}
```

## Performance Considerations

### Connection Overhead

Each `TantivyIndexManager` instance:
- Opens a separate connection to Tantivy
- Maintains its own reader/writer state
- ~100KB overhead per instance

**Guideline:** 2-4 instances typical, up to 8 for high-throughput systems.

### Thread Pool Tuning

Match `num_threads` to your workload:
- **Indexing-heavy**: 4-8 threads (parallel document processing)
- **Search-heavy**: 2-4 threads (parallel query execution)
- **Mixed**: 4 threads (balanced)

### Batch Size for Indexing

```inko
# Larger batches = fewer commits = higher throughput
# Smaller batches = lower latency = more frequent updates

let config = config.with_writer_memory(200_000_000).or_panic  # 200MB

# Batch 100-1000 documents per commit
batch_indexer.index_docs(large_doc_array)
```

## Testing Concurrent Access

See `test/test_concurrency.inko` for concurrent access tests covering:
- Multiple readers
- Single writer + multiple readers
- Batch operations across processes
- Stress testing with high concurrency

## Summary

| Pattern | Thread Safe? | Recommended For |
|---------|--------------|-----------------|
| Single `TantivyIndexManager` | No (Inko ownership) | Single-threaded apps |
| One manager per process | Yes | Most use cases |
| Shared references | No (unsafe) | Never recommended |
| Read-only snapshots | Yes | Analytics, reporting |

**Best practice:** Create a separate `TantivyIndexManager` for each process that needs access. Inko's ownership system guarantees safety, and Tantivy's internal synchronization handles concurrent access to the same index.
