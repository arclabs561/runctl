//! Error types for runctl
//!
//! This module provides structured error handling with retry awareness
//! and clear error categorization.

use crate::provider::ResourceId;
use thiserror::Error;

/// Main error type for runctl
#[derive(Error, Debug)]
pub enum TrainctlError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Cloud provider error: {provider} - {message}")]
    CloudProvider {
        provider: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Resource error: {resource_type} - {operation} failed")]
    Resource {
        resource_type: String,
        operation: String,
        resource_id: Option<ResourceId>,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Resource already exists: {resource_type} - {resource_id}")]
    ResourceExists {
        resource_type: String,
        resource_id: ResourceId,
    },

    #[error("Resource not found: {resource_type} - {resource_id}")]
    ResourceNotFound {
        resource_type: String,
        resource_id: ResourceId,
    },

    #[error("Retryable error (attempt {attempt}/{max_attempts}): {reason}")]
    #[allow(dead_code)] // Reserved for future retry logic
    Retryable {
        attempt: u32,
        max_attempts: u32,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("AWS SDK error: {0}")]
    Aws(String),

    #[error("S3 error: {0}")]
    S3(String),

    #[error("SSM error: {0}")]
    Ssm(String),

    #[error("Validation error: {field} - {reason}")]
    Validation { field: String, reason: String },

    #[error("Cost tracking error: {0}")]
    #[allow(dead_code)] // Reserved for future cost tracking
    CostTracking(String),

    #[error("Cleanup error: {0}")]
    #[allow(dead_code)] // Reserved for future cleanup features
    Cleanup(String),

    #[error("Data transfer error: {0}")]
    DataTransfer(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Configuration-specific errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid cloud provider: {0}")]
    #[allow(dead_code)] // Reserved for future provider validation
    InvalidProvider(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for {field}: {reason}")]
    InvalidValue { field: String, reason: String },

    #[error("Config file not found: {0}")]
    #[allow(dead_code)] // Reserved for future resource lookup
    NotFound(String),

    #[error("Failed to parse config: {0}")]
    ParseError(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, TrainctlError>;

/// Trait for determining if an error is retryable
///
/// Used by `RetryPolicy` implementations to determine whether an error
/// should trigger a retry attempt.
pub trait IsRetryable {
    fn is_retryable(&self) -> bool;
}

impl IsRetryable for TrainctlError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            TrainctlError::Retryable { .. }
                | TrainctlError::CloudProvider { .. }
                | TrainctlError::Io(_)
        )
    }
}

// Helper to convert AWS SDK errors
// Note: AWS SDK v1 errors are complex, so we handle them manually in code
