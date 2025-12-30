//! Helper functions for creating actionable error messages
//!
//! Provides utilities to create error messages with suggestions
//! and actionable guidance for users.
//!
//! Note: These functions are currently unused but reserved for future
//! error message improvements. They provide a consistent API for
//! creating rich error messages with suggestions.

use crate::error::{ConfigError, TrainctlError};
use crate::provider::ResourceId;

/// Create a ResourceNotFound error with helpful suggestions
#[allow(dead_code)] // Reserved for future use
pub fn resource_not_found_with_suggestions(
    resource_type: impl Into<String>,
    resource_id: impl Into<ResourceId>,
    suggestions: &[&str],
) -> TrainctlError {
    let resource_type = resource_type.into();
    let resource_id = resource_id.into();
    let mut message = format!("Resource not found: {} - {}", resource_type, resource_id);
    if !suggestions.is_empty() {
        message.push_str("\n\nTo resolve:\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            message.push_str(&format!("  {}. {}\n", i + 1, suggestion));
        }
    }
    TrainctlError::Resource {
        resource_type,
        operation: "find".to_string(),
        resource_id: Some(resource_id),
        message,
        source: None,
    }
}

/// Create a validation error with format examples
#[allow(dead_code)] // Reserved for future use
pub fn validation_error_with_examples(
    field: impl Into<String>,
    reason: impl Into<String>,
    examples: &[&str],
) -> TrainctlError {
    let field = field.into();
    let mut reason = reason.into();
    if !examples.is_empty() {
        reason.push_str("\n\nValid examples:\n");
        for example in examples {
            reason.push_str(&format!("  - {}\n", example));
        }
    }
    TrainctlError::Validation { field, reason }
}

/// Create a cloud provider error with troubleshooting steps
#[allow(dead_code)] // Reserved for future use
pub fn cloud_provider_error_with_troubleshooting(
    provider: impl Into<String>,
    message: impl Into<String>,
    troubleshooting: &[&str],
) -> TrainctlError {
    let provider = provider.into();
    let mut message = message.into();
    if !troubleshooting.is_empty() {
        message.push_str("\n\nTroubleshooting:\n");
        for (i, step) in troubleshooting.iter().enumerate() {
            message.push_str(&format!("  {}. {}\n", i + 1, step));
        }
    }
    TrainctlError::CloudProvider {
        provider,
        message,
        source: None,
    }
}

/// Create a config error with fix suggestions
#[allow(dead_code)] // Reserved for future use
pub fn config_error_with_fix(
    field: impl Into<String>,
    reason: impl Into<String>,
    fix: impl Into<String>,
) -> TrainctlError {
    TrainctlError::Config(ConfigError::InvalidValue {
        field: field.into(),
        reason: format!("{}\n\nFix: {}", reason.into(), fix.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_not_found_with_suggestions() {
        let err = resource_not_found_with_suggestions(
            "instance",
            "i-123",
            &[
                "List all instances: runctl resources list",
                "Check instance ID format",
            ],
        );

        // The error format from thiserror is "Resource error: {resource_type} - {operation} failed"
        // The detailed message is in the message field, but Display only shows the format string
        // So we check the message field directly
        if let TrainctlError::Resource {
            resource_type,
            message,
            ..
        } = &err
        {
            assert_eq!(resource_type, "instance");
            assert!(message.contains("i-123"));
            assert!(message.contains("List all instances"));
        } else {
            panic!("Expected Resource error variant, got: {:?}", err);
        }

        // Also verify the Display format includes the resource type
        let msg = format!("{}", err);
        assert!(msg.contains("instance"));
    }

    #[test]
    fn test_validation_error_with_examples() {
        let err = validation_error_with_examples(
            "instance_id",
            "Invalid format",
            &["i-1234567890abcdef0", "i-0abcdef1234567890"],
        );

        let msg = format!("{}", err);
        assert!(msg.contains("instance_id"));
        assert!(msg.contains("Invalid format"));
        assert!(msg.contains("i-1234567890abcdef0"));
    }

    #[test]
    fn test_cloud_provider_error_with_troubleshooting() {
        let err = cloud_provider_error_with_troubleshooting(
            "aws",
            "Failed to create instance",
            &[
                "Check AWS credentials: aws sts get-caller-identity",
                "Verify IAM permissions",
            ],
        );

        let msg = format!("{}", err);
        assert!(msg.contains("aws"));
        assert!(msg.contains("Failed to create instance"));
        assert!(msg.contains("Check AWS credentials"));
    }
}
