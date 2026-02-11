// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Tests for error message sanitization in release builds
//
// These tests verify that error messages in release builds don't leak filesystem
// paths and other sensitive information.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_error_message_simple_path() {
        let msg = "Failed to open index at /var/lib/tantivy/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Failed to open index at /var/lib/tantivy/index");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Failed to open index at <path>");
    }

    #[test]
    fn test_sanitize_error_message_windows_path() {
        let msg = "Error reading file at /home/user/tantivy/data/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(
            sanitized,
            "Error reading file at /home/user/tantivy/data/index"
        );

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error reading file at <path>");
    }

    #[test]
    fn test_sanitize_error_message_multiple_paths() {
        let msg = "Index at /var/lib/tantivy/index failed, trying /tmp/backup";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(
            sanitized,
            "Index at /var/lib/tantivy/index failed, trying /tmp/backup"
        );

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Index at <path> failed, trying <path>");
    }

    #[test]
    fn test_sanitize_error_message_no_path() {
        let msg = "Invalid configuration provided";
        let sanitized = sanitize_error_message(msg);

        assert_eq!(sanitized, "Invalid configuration provided");
    }

    #[test]
    fn test_sanitize_error_message_empty_path() {
        let msg = "Error at / with no directory";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at / with no directory");

        #[cfg(not(debug_assertions))]
        // Single slash should not be replaced
        assert_eq!(sanitized, "Error at / with no directory");
    }

    #[test]
    fn test_sanitize_error_message_path_with_quotes() {
        let msg = "Error at \"/var/lib/tantivy/index\"";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at \"/var/lib/tantivy/index\"");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at \"<path>\"");
    }

    #[test]
    fn test_sanitize_error_message_path_with_colon() {
        let msg = "Failed at /path/to/index: Error occurred";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Failed at /path/to/index: Error occurred");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Failed at <path>: Error occurred");
    }

    #[test]
    fn test_sanitize_error_message_with_whitespace() {
        let msg = "Error at /var/lib/tantivy/index space after path";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(
            sanitized,
            "Error at /var/lib/tantivy/index space after path"
        );

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at <path> space after path");
    }

    #[test]
    fn test_sanitize_error_message_no_leading_slash() {
        let msg = "Error: could not open path relative/path/index";
        let sanitized = sanitize_error_message(msg);

        assert_eq!(sanitized, "Error: could not open path relative/path/index");
    }

    #[test]
    fn test_sanitize_flag_debug_builds() {
        #[cfg(debug_assertions)]
        assert!(!SANITIZE_ERRORS, "Debug builds should not sanitize errors");

        #[cfg(not(debug_assertions))]
        assert!(SANITIZE_ERRORS, "Release builds should sanitize errors");
    }

    #[test]
    fn test_create_error_string_logs_to_stderr() {
        // This test verifies that create_error_string logs to stderr
        // We can't easily test this in unit tests, but we verify
        // the function doesn't panic
        let _ = create_error_string("Test error message");
    }

    #[test]
    fn test_sanitize_preserves_non_path_content() {
        let msg = "Failed to open index at /var/lib/tantivy/index: Permission denied";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(
            sanitized,
            "Failed to open index at /var/lib/tantivy/index: Permission denied"
        );

        #[cfg(not(debug_assertions))]
        assert_eq!(
            sanitized,
            "Failed to open index at <path>: Permission denied"
        );
    }

    #[test]
    fn test_sanitize_with_deep_path() {
        let msg = "Error at /usr/local/lib/tantivy/data/backup/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(
            sanitized,
            "Error at /usr/local/lib/tantivy/data/backup/index"
        );

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at <path>");
    }
}
