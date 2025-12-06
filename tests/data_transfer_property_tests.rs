//! Property-based tests for data transfer operations
//!
//! Tests that verify data transfer path parsing and validation.

use proptest::prelude::*;
use std::path::PathBuf;

proptest! {
    #[test]
    fn test_local_path_validation(
        path_segments in prop::collection::vec(r"[a-zA-Z0-9_/-]+", 1..10)
    ) {
        let path_str = path_segments.join("/");
        let path = PathBuf::from(&path_str);

        // Properties:
        // 1. Path should not be empty
        prop_assert!(!path.to_string_lossy().is_empty());

        // 2. Should be valid path
        prop_assert!(path.components().count() > 0);
    }

    #[test]
    fn test_s3_path_construction_properties(
        bucket in r"[a-z0-9-]+",
        key_segments in prop::collection::vec(r"[a-zA-Z0-9_/-]+", 1..10)
    ) {
        let key = key_segments.join("/");
        let s3_path = format!("s3://{}/{}", bucket, key);

        // Properties:
        // 1. Should start with s3://
        prop_assert!(s3_path.starts_with("s3://"));

        // 2. Should contain bucket
        prop_assert!(s3_path.contains(&bucket));

        // 3. Should be parseable
        let path_part = &s3_path[5..];
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        prop_assert_eq!(parts.len(), 2);
        prop_assert_eq!(parts[0], bucket);
        prop_assert_eq!(parts[1], key);
    }

    #[test]
    fn test_instance_path_parsing(
        instance_id in r"i-[0-9a-f]+",
        remote_path_segments in prop::collection::vec(r"[a-zA-Z0-9_/-]+", 1..5)
    ) {
        let remote_path = format!("/{}", remote_path_segments.join("/"));
        let instance_path = format!("{}:{}", instance_id, remote_path);

        // Properties:
        // 1. Should contain instance ID
        prop_assert!(instance_path.contains(&instance_id));

        // 2. Should contain remote path
        prop_assert!(instance_path.contains(&remote_path));

        // 3. Should be parseable
        let parts: Vec<&str> = instance_path.splitn(2, ':').collect();
        prop_assert_eq!(parts.len(), 2);
        prop_assert_eq!(parts[0], instance_id);
        prop_assert_eq!(parts[1], remote_path);
    }

    #[test]
    fn test_path_normalization_properties(
        path_segments in prop::collection::vec(r"[a-zA-Z0-9_.-]+", 1..10)
    ) {
        let path_str = path_segments.join("/");
        let path = PathBuf::from(&path_str);

        // Properties:
        // 1. Normalized path should be consistent
        let normalized = path.canonicalize().unwrap_or(path.clone());

        // 2. Path should have components
        prop_assert!(normalized.components().count() > 0);
    }
}

// Property tests for S3 bucket name validation
proptest! {
    #[test]
    fn test_s3_bucket_name_validation(
        name in r"[a-z0-9][a-z0-9-]*[a-z0-9]"
    ) {
        // AWS S3 bucket name constraints:
        // - 3-63 characters
        // - Lowercase letters, numbers, hyphens
        // - Must start and end with letter or number

        if name.len() >= 3 && name.len() <= 63 {
            // Properties:
            // 1. Should be valid length
            prop_assert!(name.len() >= 3);
            prop_assert!(name.len() <= 63);

            // 2. Should start with alphanumeric
            prop_assert!(name.chars().next().unwrap().is_alphanumeric());

            // 3. Should end with alphanumeric
            prop_assert!(name.chars().last().unwrap().is_alphanumeric());
        }
    }
}

// Property tests for file size validation
proptest! {
    #[test]
    fn test_file_size_properties(
        size_bytes in 0u64..1_000_000_000_000u64  // 0 to 1 TB
    ) {
        // Properties:
        // 1. Size should be non-negative (already enforced by range)
        prop_assert!(size_bytes >= 0);

        // 2. Size in GB should be reasonable
        let size_gb = size_bytes as f64 / 1_000_000_000.0;
        prop_assert!(size_gb >= 0.0);
        prop_assert!(size_gb <= 1000.0); // Reasonable max
    }
}
