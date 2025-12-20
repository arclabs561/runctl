//! Property-based tests for error handling
//!
//! Tests that verify error types and error handling properties.

use proptest::prelude::*;
use runctl::error::{ConfigError, IsRetryable, TrainctlError};

proptest! {
    #[test]
    fn test_error_display_formatting(
        provider in r"[a-zA-Z0-9]+",
        message in r".+"
    ) {
        let err = TrainctlError::CloudProvider {
            provider: provider.clone(),
            message: message.clone(),
            source: None,
        };

        let display = format!("{}", err);

        // Properties:
        // 1. Should contain provider name
        prop_assert!(display.contains(&provider));

        // 2. Should contain message
        prop_assert!(display.contains(&message));

        // 3. Should not be empty
        prop_assert!(!display.is_empty());
    }

    #[test]
    fn test_config_error_display(
        field in r"[a-zA-Z0-9_]+",
        reason in r".+"
    ) {
        let err = ConfigError::InvalidValue {
            field: field.clone(),
            reason: reason.clone(),
        };

        let display = format!("{}", err);

        // Properties:
        // 1. Should contain field name
        prop_assert!(display.contains(&field));

        // 2. Should contain reason
        prop_assert!(display.contains(&reason));
    }

    #[test]
    fn test_error_retryability_properties(
        error_type in prop_oneof![
            Just("retryable"),
            Just("cloud_provider"),
            Just("io"),
            Just("non_retryable"),
        ]
    ) {
        let err: TrainctlError = match error_type {
            "retryable" => TrainctlError::Retryable {
                attempt: 1,
                max_attempts: 5,
                reason: "test".to_string(),
                source: None,
            },
            "cloud_provider" => TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "test".to_string(),
                source: None,
            },
            "io" => TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "test"
            )),
            _ => TrainctlError::DataTransfer("test".to_string()),
        };

        let is_retryable = err.is_retryable();

        // Properties:
        // 1. Retryable errors should be retryable
        if matches!(error_type, "retryable" | "cloud_provider" | "io") {
            prop_assert!(is_retryable, "Error type {} should be retryable", error_type);
        } else {
            prop_assert!(!is_retryable, "Error type {} should not be retryable", error_type);
        }
    }
}

// Property tests for error conversion
proptest! {
    #[test]
    fn test_error_conversion_properties(
        provider in r"[a-zA-Z0-9]+"
    ) {
        let config_err = ConfigError::InvalidProvider(provider.clone());
        let runctl_err: TrainctlError = config_err.into();

        // Property: Should convert to TrainctlError::Config
        match runctl_err {
            TrainctlError::Config(_) => {
                // Correct conversion
            }
            _ => {
                prop_assert!(false, "ConfigError should convert to TrainctlError::Config");
            }
        }
    }
}
