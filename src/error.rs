//! Error types for runctl
//!
//! This module defines the error handling strategy for runctl. There are two
//! error types: `TrainctlError` (main error enum) and `ConfigError` (configuration-specific).
//!
//! ## Error Handling Philosophy
//!
//! Library code uses `crate::error::Result<T>` which returns `TrainctlError`.
//! CLI code uses `anyhow::Result<T>` for top-level error handling. The conversion
//! happens at the CLI boundary using `anyhow::Error::from` to preserve error chains.
//!
//! This split exists because:
//! - Library code benefits from structured error types for programmatic handling
//! - CLI code benefits from `anyhow`'s context chains and user-friendly display
//! - Conversion preserves full error information (not just strings)
//!
//! ## Retry Awareness
//!
//! Errors implement `IsRetryable` to indicate whether an operation should be retried.
//! The `RetryPolicy` in `src/retry.rs` uses this to determine retry behavior.
//! Only `CloudProvider`, `Io`, and `Retryable` variants are retryable by default.
//!
//! Non-retryable errors (e.g., `Validation`, `Config`) fail immediately to avoid
//! wasting time on operations that cannot succeed.
//!
//! ## When to Use Which Error
//!
//! - `ConfigError`: Configuration parsing and validation issues
//!   - Automatically converted to `TrainctlError::Config` via `#[from]`
//!
//! - `CloudProvider`: Generic cloud API failures (provider-agnostic)
//!   - Use for provider-agnostic errors that could occur with any cloud
//!   - Retryable by default
//!
//! - `Aws`/`S3`/`Ssm`: AWS-specific errors
//!   - Use when AWS-specific context matters for debugging
//!   - `Aws` is retryable (wrapped in `CloudProvider` internally)
//!
//! - `ResourceNotFound`/`ResourceExists`: Resource lifecycle errors
//!   - Use when resources don't exist or already exist
//!   - Not retryable (idempotency issues, not transient failures)
//!
//! - `Validation`: Input validation failures
//!   - Use for user input validation (instance IDs, paths, etc.)
//!   - Not retryable (invalid input won't become valid)

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
///
/// This trait is actively used by `src/retry.rs` - do not mark as dead_code.
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
