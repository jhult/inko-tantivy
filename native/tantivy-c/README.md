# Tantivy C FFI Library

This directory contains the Rust-based C FFI wrapper for Tantivy, used by the Inko bindings.

## Building

```bash
cargo build --release
```

This will create `target/release/libtantivy_c.dylib` (macOS) or `target/release/libtantivy_c.so` (Linux).

## Installation

### Option 1: System Installation (Recommended for Production)

Install the library to a system library directory:

**macOS:**
```bash
sudo cp target/release/libtantivy_c.dylib /usr/local/lib/
```

**Linux:**
```bash
sudo cp target/release/libtantivy_c.so /usr/local/lib/
sudo ldconfig  # Update library cache
```

### Option 2: Environment Variable (Development)

Set the library search path before running Inko:

**macOS:**
```bash
export LIBRARY_PATH=/path/to/inko-tantivy/native/tantivy-c/target/release
export DYLD_LIBRARY_PATH=/path/to/inko-tantivy/native/tantivy-c/target/release
```

**Linux:**
```bash
export LIBRARY_PATH=/path/to/inko-tantivy/native/tantivy-c/target/release
export LD_LIBRARY_PATH=/path/to/inko-tantivy/native/tantivy-c/target/release
```

### Option 3: Use the Build Script (Easiest)

From the project root:

```bash
./build.sh test      # Build and run tests automatically
./build.sh install   # Install system-wide (requires sudo)
```

## FFI Functions

The library exports the following functions for use from Inko:

- `tantivy_index_open` - Open or create an index
- `tantivy_index_close` - Close an index
- `tantivy_index_add_doc` - Add a document to the index
- `tantivy_index_delete_doc` - Delete a document
- `tantivy_index_search` - Search the index
- `tantivy_index_commit` - Commit pending changes
- `tantivy_index_rollback` - Rollback pending changes
- `tantivy_index_get_doc` - Get a document by ID
- `tantivy_index_get_docs` - Get multiple documents by ID
- `tantivy_aggregate_terms` - Aggregate document counts by field values
- `tantivy_get_facet_counts` - Get facet counts for a field
- `tantivy_autocomplete` - Get autocomplete suggestions
- `tantivy_did_you_mean` - Get fuzzy spelling suggestions
- Memory management functions for freeing allocated results

## Troubleshooting

### "library 'tantivy_c' not found" error

This means the linker can't find the library. Solutions:

1. Install the library system-wide (see Option 1 above)
2. Set LIBRARY_PATH environment variable (see Option 2 above)
3. Use `make test` which handles this automatically

### Tests are skipped

If integration tests are skipped, it means the FFI library is not accessible. The tests check for FFI availability and gracefully skip if the library can't be loaded.

## Implementation Notes

### Facet Counting

The `tantivy_get_facet_counts` function currently uses manual term aggregation rather than Tantivy's hierarchical Facet features. This is appropriate for the email search use case, where we're counting documents by simple string values (mailbox names, flags, labels, etc.) rather than hierarchical categories.

For true hierarchical faceting, you would need to:
1. Define fields with the `FACET` option in the schema
2. Use Tantivy's `FacetCollector` in the implementation

The current implementation is optimized for the intended use case and provides good performance for flat faceting.
