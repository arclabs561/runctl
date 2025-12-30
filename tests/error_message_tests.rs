//! Tests for error message quality and actionability
//!
//! Verifies that error messages are helpful and provide actionable guidance.

use runctl::error::{ConfigError, TrainctlError};
use runctl::validation;

#[test]
fn test_resource_not_found_error_message() {
    let err = TrainctlError::ResourceNotFound {
        resource_type: "instance".to_string(),
        resource_id: "i-1234567890abcdef0".to_string(),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("instance"));
    assert!(msg.contains("i-1234567890abcdef0"));
    assert!(msg.contains("not found"));
}

#[test]
fn test_resource_exists_error_message() {
    let err = TrainctlError::ResourceExists {
        resource_type: "instance".to_string(),
        resource_id: "i-1234567890abcdef0".to_string(),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("instance"));
    assert!(msg.contains("i-1234567890abcdef0"));
    assert!(msg.contains("already exists"));
}

#[test]
fn test_validation_error_message() {
    let err = TrainctlError::Validation {
        field: "instance_id".to_string(),
        reason: "Instance ID must start with 'i-', got: invalid".to_string(),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("instance_id"));
    assert!(msg.contains("must start with 'i-'"));
    assert!(msg.contains("invalid"));
}

#[test]
fn test_config_error_messages() {
    let err = ConfigError::InvalidValue {
        field: "region".to_string(),
        reason: "Region must be a valid AWS region code".to_string(),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("region"));
    assert!(msg.contains("Region must be a valid AWS region code") || msg.contains("AWS region"));
}

#[test]
fn test_validation_error_includes_field_and_reason() {
    let result = validation::validate_instance_id("invalid-id");
    assert!(result.is_err());

    if let Err(TrainctlError::Validation { field, reason }) = result {
        assert_eq!(field, "instance_id");
        assert!(!reason.is_empty());
        assert!(reason.contains("must start with 'i-'") || reason.contains("invalid"));
    } else {
        panic!("Expected Validation error");
    }
}

#[test]
fn test_validation_error_for_s3_path() {
    let result = validation::validate_s3_path("invalid-path");
    assert!(result.is_err());

    if let Err(TrainctlError::Validation { field, reason }) = result {
        assert_eq!(field, "s3_path");
        assert!(reason.contains("s3://"));
    } else {
        panic!("Expected Validation error");
    }
}

#[test]
fn test_cloud_provider_error_includes_provider() {
    let err = TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "Failed to describe instances".to_string(),
        source: None,
    };

    let msg = format!("{}", err);
    assert!(msg.contains("aws"));
    assert!(msg.contains("describe instances"));
}

#[test]
fn test_error_chain_preservation() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
    let err = TrainctlError::Io(io_err);

    let msg = format!("{}", err);
    assert!(msg.contains("I/O error"));
    // Source error should be preserved
    assert!(msg.contains("File not found") || msg.contains("not found"));
}
