//! Tests for S3 operations

use std::path::PathBuf;

#[test]
fn test_s3_path_validation() {
    // Valid S3 paths
    let valid_paths = vec![
        "s3://bucket/path",
        "s3://bucket-name/path/to/file",
        "s3://my-bucket/",
        "s3://bucket.with.dots/path",
    ];
    
    for path in valid_paths {
        assert!(path.starts_with("s3://"), "Path should start with s3://: {}", path);
        let without_prefix = &path[5..];
        assert!(!without_prefix.is_empty(), "Path should have content after s3://: {}", path);
    }
}

#[test]
fn test_s3_path_parsing() {
    fn parse_s3_path(path: &str) -> Option<(&str, &str)> {
        if !path.starts_with("s3://") {
            return None;
        }
        let rest = &path[5..];
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            Some((parts[0], parts[1]))
        } else {
            Some((parts[0], ""))
        }
    }
    
    let (bucket, key) = parse_s3_path("s3://my-bucket/path/to/file").unwrap();
    assert_eq!(bucket, "my-bucket");
    assert_eq!(key, "path/to/file");
    
    let (bucket, key) = parse_s3_path("s3://bucket-only").unwrap();
    assert_eq!(bucket, "bucket-only");
    assert_eq!(key, "");
}

#[test]
fn test_s3_key_normalization() {
    // Test key normalization (remove leading/trailing slashes)
    fn normalize_key(key: &str) -> String {
        key.trim_matches('/').to_string()
    }
    
    assert_eq!(normalize_key("/path/to/file"), "path/to/file");
    assert_eq!(normalize_key("path/to/file/"), "path/to/file");
    assert_eq!(normalize_key("/path/to/file/"), "path/to/file");
    assert_eq!(normalize_key("path/to/file"), "path/to/file");
}

#[test]
fn test_s3_object_naming() {
    // Test checkpoint naming patterns
    fn checkpoint_s3_key(epoch: u32, base_path: &str) -> String {
        format!("{}/checkpoint_epoch_{}.pt", base_path, epoch)
    }
    
    assert_eq!(
        checkpoint_s3_key(10, "checkpoints"),
        "checkpoints/checkpoint_epoch_10.pt"
    );
    assert_eq!(
        checkpoint_s3_key(0, "s3://bucket/training/run1"),
        "s3://bucket/training/run1/checkpoint_epoch_0.pt"
    );
}

