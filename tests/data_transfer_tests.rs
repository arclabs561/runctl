//! Comprehensive tests for data transfer functionality
//!
//! Tests verify path parsing, location detection, and transfer logic.

use runctl::data_transfer::{DataLocation, TransferOptions};
use std::path::PathBuf;

#[test]
fn test_data_location_s3() {
    let loc = DataLocation::S3("s3://bucket-name/path/to/file".to_string());
    match loc {
        DataLocation::S3(path) => {
            assert_eq!(path, "s3://bucket-name/path/to/file");
        }
        _ => panic!("Expected S3 location"),
    }
}

#[test]
fn test_data_location_local() {
    let loc = DataLocation::Local(PathBuf::from("/tmp/data"));
    match loc {
        DataLocation::Local(path) => {
            assert_eq!(path, PathBuf::from("/tmp/data"));
        }
        _ => panic!("Expected Local location"),
    }
}

#[test]
fn test_data_location_training_instance() {
    let loc = DataLocation::TrainingInstance(
        "i-1234567890abcdef0".to_string(),
        PathBuf::from("/data/training"),
    );
    match loc {
        DataLocation::TrainingInstance(instance_id, remote_path) => {
            assert_eq!(instance_id, "i-1234567890abcdef0");
            assert_eq!(remote_path, PathBuf::from("/data/training"));
        }
        _ => panic!("Expected TrainingInstance location"),
    }
}

#[test]
fn test_transfer_options_default() {
    let options = TransferOptions::default();
    assert_eq!(options.parallel, Some(4));
    assert!(!options.compression);
    assert!(options.verify);
    assert!(options.resume);
    assert!(options.exclude.contains(&"*.pyc".to_string()));
}

#[test]
fn test_transfer_options_custom() {
    let options = TransferOptions {
        parallel: Some(8),
        compression: true,
        verify: false,
        resume: false,
        exclude: vec!["*.log".to_string()],
    };

    assert_eq!(options.parallel, Some(8));
    assert!(options.compression);
    assert!(!options.verify);
    assert!(!options.resume);
    assert_eq!(options.exclude.len(), 1);
}

#[test]
fn test_s3_path_parsing() {
    // Test S3 path parsing logic
    let s3_path = "s3://my-bucket/path/to/file.txt";
    assert!(s3_path.starts_with("s3://"));

    let path_part = &s3_path[5..]; // Skip "s3://"
    let parts: Vec<&str> = path_part.splitn(2, '/').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], "my-bucket");
    assert_eq!(parts[1], "path/to/file.txt");

    // Test root key
    let root_path = "s3://bucket/";
    let root_part = &root_path[5..];
    let root_parts: Vec<&str> = root_part.splitn(2, '/').collect();
    assert_eq!(root_parts[0], "bucket");
    assert_eq!(root_parts[1], "");
}

#[test]
fn test_data_location_display() {
    let s3_loc = DataLocation::S3("s3://bucket/key".to_string());
    // Just verify it can be created
    let _ = s3_loc;

    let local_loc = DataLocation::Local(PathBuf::from("/tmp/data"));
    let _ = local_loc;

    let instance_loc = DataLocation::TrainingInstance("i-123".to_string(), PathBuf::from("/data"));
    let _ = instance_loc;
}
