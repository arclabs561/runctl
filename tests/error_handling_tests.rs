//! Tests for error handling migration from anyhow to crate::error::Result
//!
//! These tests verify that:
//! 1. Functions return crate::error::Result instead of anyhow::Result
//! 2. Error conversion at CLI boundary works correctly
//! 3. Error messages are preserved and structured

use runctl::error::{ConfigError, Result, TrainctlError};

#[test]
fn test_error_conversion_to_anyhow() {
    // Test that TrainctlError can be converted to anyhow::Error
    let custom_error = TrainctlError::Validation {
        field: "instance_id".to_string(),
        reason: "Invalid format".to_string(),
    };

    let anyhow_error = anyhow::anyhow!("{}", custom_error);
    assert!(anyhow_error.to_string().contains("Validation error"));
    assert!(anyhow_error.to_string().contains("instance_id"));
}

#[test]
fn test_config_error_conversion() {
    let config_error = ConfigError::NotFound("/path/to/config".to_string());
    let runctl_error: TrainctlError = config_error.into();

    assert!(matches!(runctl_error, TrainctlError::Config(_)));
    assert!(runctl_error.to_string().contains("Config file not found"));
}

#[test]
fn test_io_error_conversion() {
    use std::io;

    let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
    let runctl_error: TrainctlError = io_error.into();

    assert!(matches!(runctl_error, TrainctlError::Io(_)));
    assert!(runctl_error.to_string().contains("I/O error"));
}

#[test]
fn test_error_chain_preservation() {
    // Test that error chains are preserved when converting
    let inner_error =
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Permission denied");
    let runctl_error = TrainctlError::Io(inner_error);

    // Convert to anyhow and back - should preserve message
    let anyhow_error = anyhow::anyhow!("{}", runctl_error);
    assert!(anyhow_error.to_string().contains("I/O error"));
    assert!(anyhow_error.to_string().contains("Permission denied"));
}

#[test]
fn test_result_type_alias() {
    // Test that Result<T> is correctly aliased
    fn returns_result() -> Result<()> {
        Ok(())
    }

    fn returns_error() -> Result<()> {
        Err(TrainctlError::Validation {
            field: "test".to_string(),
            reason: "test reason".to_string(),
        })
    }

    assert!(returns_result().is_ok());
    assert!(returns_error().is_err());

    if let Err(e) = returns_error() {
        assert!(matches!(e, TrainctlError::Validation { .. }));
    }
}

#[test]
fn test_error_display_format() {
    // Test that error Display implementation is correct
    let error = TrainctlError::ResourceNotFound {
        resource_type: "instance".to_string(),
        resource_id: "i-123".to_string(),
    };

    let display = format!("{}", error);
    assert!(display.contains("Resource not found"));
    assert!(display.contains("instance"));
    assert!(display.contains("i-123"));
}

#[test]
fn test_retryable_trait() {
    use runctl::error::IsRetryable;

    let retryable_error = TrainctlError::Retryable {
        attempt: 1,
        max_attempts: 3,
        reason: "Network timeout".to_string(),
        source: None,
    };

    assert!(retryable_error.is_retryable());

    let non_retryable_error = TrainctlError::Validation {
        field: "test".to_string(),
        reason: "Invalid".to_string(),
    };

    // Validation errors are not retryable
    assert!(!non_retryable_error.is_retryable());
}
