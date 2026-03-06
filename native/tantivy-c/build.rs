// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut inko_ffi_path = manifest_dir;
    inko_ffi_path.push("../../src/tantivy/ffi.inko");

    generate_ffi_constants(&inko_ffi_path);
}

fn generate_ffi_constants(output_path: &PathBuf) {
    let mut output = File::create(output_path).expect("Failed to create ffi.inko");

    writeln!(output, "# This Source Code Form is subject to the terms of the Mozilla Public")
        .unwrap();
    writeln!(output, "# License, v. 2.0. If a copy of the MPL was not distributed with this")
        .unwrap();
    writeln!(output, "# file, You can obtain one at https://mozilla.org/MPL/2.0/.").unwrap();
    writeln!(output, "#").unwrap();
    writeln!(output, "# Tantivy FFI bindings").unwrap();
    writeln!(output, "#").unwrap();
    writeln!(
        output,
        "# This module contains all FFI function declarations and C struct layout constants."
    )
    .unwrap();
    writeln!(output, "# The struct layout constants are auto-generated from Rust code by build.rs")
        .unwrap();
    writeln!(output, "#").unwrap();

    writeln!(output, "# Default buffer size for reading JSON from Tantivy").unwrap();
    writeln!(output, "# Increased to 1MB to handle large documents with many fields").unwrap();
    writeln!(output, "# If documents exceed this size, an error will be returned").unwrap();
    writeln!(output, "let pub DEFAULT_JSON_BUFFER_SIZE = 1_048_576 # 1MB").unwrap();
    writeln!(output).unwrap();

    writeln!(
        output,
        "# Default buffer size for reading individual string fields (doc IDs, highlights, etc.)"
    )
    .unwrap();
    writeln!(
        output,
        "# 4KB should be sufficient for most fields while preventing excessive truncation"
    )
    .unwrap();
    writeln!(output, "let pub DEFAULT_STRING_FIELD_BUFFER_SIZE = 4096 # 4KB").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "# Buffer size for error messages from Rust").unwrap();
    writeln!(output, "# 1KB should be sufficient for most error messages").unwrap();
    writeln!(output, "let pub TANTIVY_ERROR_BUFFER_SIZE = 1024 # 1KB").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "# FFI Struct layout constants (64-bit architecture)").unwrap();
    writeln!(
        output,
        "# These are auto-generated from Rust struct definitions in native/tantivy-c/src/lib.rs"
    )
    .unwrap();
    writeln!(
        output,
        "# Single source of truth - Rust definitions with #[repr(C)] and offset_of! macros"
    )
    .unwrap();
    writeln!(output).unwrap();

    writeln!(output, "# Pointer size on 64-bit systems (all pointers are 8 bytes)").unwrap();
    writeln!(output, "let pub PTR_SIZE = 8").unwrap();
    writeln!(output).unwrap();

    generate_struct_constants(
        &mut output,
        "Config",
        40,
        &[
            ("index_path", 0),
            ("reader_memory_budget_bytes", 8),
            ("writer_memory_budget_bytes", 16),
            ("num_threads", 24),
            ("schema_json", 32),
        ],
    );

    generate_struct_constants(
        &mut output,
        "Result",
        24,
        &[("doc_id", 0), ("score", 8), ("highlight", 16)],
    );

    generate_struct_constants(&mut output, "AggregationResult", 16, &[("key", 0), ("count", 8)]);

    generate_struct_constants(&mut output, "Suggestion", 16, &[("text", 0), ("score", 8)]);

    generate_struct_constants(&mut output, "Field", 16, &[("key", 0), ("value", 8)]);

    generate_struct_constants(
        &mut output,
        "BatchDocument",
        24,
        &[("doc_id", 0), ("fields", 8), ("num_fields", 16)],
    );

    writeln!(output, "# Link to Tantivy C library").unwrap();
    writeln!(output, "import extern \"tantivy_c\"").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "# External FFI functions from the Tantivy C library").unwrap();
    writeln!(output).unwrap();

    generate_ffi_function(
        &mut output,
        "tantivy_ping",
        &[],
        "Int",
        "Simple ping function to check if FFI library is loaded",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_open",
        &[("config", "Pointer[UInt8]"), ("error_out", "Pointer[UInt8]")],
        "Pointer[UInt8]",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_close",
        &[("index", "Pointer[UInt8]"), ("error_out", "Pointer[UInt8]")],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_add_doc",
        &[
            ("index", "Pointer[UInt8]"),
            ("doc_id", "Pointer[UInt8]"),
            ("fields", "Pointer[UInt8]"),
            ("num_fields", "Int64"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_add_docs_batch",
        &[
            ("index", "Pointer[UInt8]"),
            ("documents", "Pointer[UInt8]"),
            ("num_docs", "Int64"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_delete_doc",
        &[
            ("index", "Pointer[UInt8]"),
            ("doc_id", "Pointer[UInt8]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_delete_docs",
        &[
            ("index", "Pointer[UInt8]"),
            ("doc_ids", "Pointer[UInt8]"),
            ("num_ids", "Int64"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_search",
        &[
            ("index", "Pointer[UInt8]"),
            ("query", "Pointer[UInt8]"),
            ("limit", "Int64"),
            ("offset", "Int64"),
            ("results_out", "Pointer[UInt8]"),
            ("num_results_out", "Pointer[UInt64]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_commit",
        &[("index", "Pointer[UInt8]"), ("error_out", "Pointer[UInt8]")],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_rollback",
        &[("index", "Pointer[UInt8]"), ("error_out", "Pointer[UInt8]")],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_result_free",
        &[("results", "Pointer[UInt8]"), ("num_results", "Int64")],
        "",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_error_free",
        &[("error", "Pointer[UInt8]")],
        "",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_index_get_doc",
        &[
            ("index", "Pointer[UInt8]"),
            ("doc_id", "Pointer[UInt8]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Pointer[UInt8]",
        "",
    );

    generate_ffi_function(&mut output, "tantivy_string_free", &[("s", "Pointer[UInt8]")], "", "");

    generate_ffi_function(
        &mut output,
        "tantivy_index_get_docs",
        &[
            ("index", "Pointer[UInt8]"),
            ("doc_ids", "Pointer[UInt8]"),
            ("num_ids", "Int64"),
            ("num_results_out", "Pointer[UInt64]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Pointer[UInt8]",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_docs_array_free",
        &[("docs", "Pointer[UInt8]"), ("num_docs", "Int64")],
        "",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_aggregate_terms",
        &[
            ("index", "Pointer[UInt8]"),
            ("field_name", "Pointer[UInt8]"),
            ("query", "Pointer[UInt8]"),
            ("limit", "Int64"),
            ("results_out", "Pointer[UInt8]"),
            ("num_results_out", "Pointer[UInt64]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_aggregation_results_free",
        &[("results", "Pointer[UInt8]"), ("num_results", "Int64")],
        "",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_autocomplete",
        &[
            ("index", "Pointer[UInt8]"),
            ("field", "Pointer[UInt8]"),
            ("prefix", "Pointer[UInt8]"),
            ("limit", "Int64"),
            ("results_out", "Pointer[UInt8]"),
            ("num_results_out", "Pointer[UInt64]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_did_you_mean",
        &[
            ("index", "Pointer[UInt8]"),
            ("field", "Pointer[UInt8]"),
            ("term", "Pointer[UInt8]"),
            ("distance", "Int32"),
            ("limit", "Int64"),
            ("results_out", "Pointer[UInt8]"),
            ("num_results_out", "Pointer[UInt64]"),
            ("error_out", "Pointer[UInt8]"),
        ],
        "Int32",
        "",
    );

    generate_ffi_function(
        &mut output,
        "tantivy_suggestions_free",
        &[("results", "Pointer[UInt8]"), ("num_results", "Int64")],
        "",
        "",
    );

    println!("cargo:warning=Generated ffi.inko at {}", output_path.display());
}

fn generate_struct_constants(
    output: &mut File,
    struct_name: &str,
    size: usize,
    fields: &[(&str, usize)],
) {
    writeln!(output, "# {} struct ({} bytes total):", struct_name, size).unwrap();

    for (field_name, _offset) in fields {
        writeln!(output, "# - {} pointer (8 bytes)", field_name).unwrap();
    }

    let struct_prefix = map_struct_name_for_prefix(struct_name);
    let size_name = format!("{}_SIZE", struct_prefix);
    writeln!(output, "let pub {} = {}", size_name, size).unwrap();

    for (field_name, offset) in fields {
        let field_name_for_offset = map_field_name_for_offset(field_name);
        let offset_name =
            format!("{}_OFFSET_{}", struct_prefix, to_upper_snake_case(field_name_for_offset));
        writeln!(output, "let pub {} = {}", offset_name, offset).unwrap();
    }
    writeln!(output).unwrap();
}

fn map_struct_name_for_prefix(struct_name: &str) -> String {
    match struct_name {
        "AggregationResult" => "AGGREGATION".to_string(),
        _ => to_upper_snake_case(struct_name),
    }
}

fn map_field_name_for_offset(field_name: &str) -> &str {
    match field_name {
        "reader_memory_budget_bytes" => "reader_memory",
        "writer_memory_budget_bytes" => "writer_memory",
        "index_path" => "path",
        "num_threads" => "threads",
        _ => field_name,
    }
}

fn generate_ffi_function(
    output: &mut File,
    name: &str,
    params: &[(&str, &str)],
    return_type: &str,
    comment: &str,
) {
    if !comment.is_empty() {
        writeln!(output, "# {}", comment).unwrap();
    }

    let param_list = params
        .iter()
        .map(|(name, typ)| format!("{}: {}", name, typ))
        .collect::<Vec<_>>()
        .join(", ");

    if return_type.is_empty() {
        writeln!(output, "fn pub extern {}({})", name, param_list).unwrap();
    } else {
        writeln!(output, "fn pub extern {}({}) -> {}", name, param_list, return_type).unwrap();
    }
    writeln!(output).unwrap();
}

fn to_upper_snake_case(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_uppercase() {
                format!("_{}", c)
            } else if c == '_' {
                String::from("_")
            } else {
                c.to_uppercase().to_string()
            }
        })
        .collect::<String>()
        .trim_start_matches('_')
        .to_string()
}
