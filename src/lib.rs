//! runctl library
//!
//! This library provides the core functionality for runctl CLI, a unified tool for
//! ML training orchestration across multiple cloud providers (AWS, RunPod, Lyceum AI).
//!
//! ## Architecture
//!
//! The library follows industry patterns from Terraform (plugin registry), Pulumi (component model),
//! and Kubernetes (CRD extensibility). See `docs/ARCHITECTURE.md` for details.
//!
//! ## Key Modules
//!
//! - **Provider System**: `provider` and `providers` modules for multi-cloud abstraction
//! - **Error Handling**: `error` module with structured error types and retry awareness
//! - **Resource Tracking**: `resource_tracking` for cost awareness and lifecycle management
//! - **Retry Logic**: `retry` module with exponential backoff for cloud API calls
//!
//! ## Usage
//!
//! ### Basic Example
//!
//! ```rust,no_run
//! use runctl::{Config, ResourceTracker};
//!
//! # async fn example() -> runctl::error::Result<()> {
//! // Load configuration
//! let config = Config::load(None)?;
//!
//! // Track resources
//! let tracker = ResourceTracker::new();
//! let running = tracker.get_running().await;
//! # Ok(())
//! # }
//! ```
//!
//! ### Using Convenience Re-exports
//!
//! Common types are re-exported at the crate root for convenience:
//!
//! ```rust,no_run
//! use runctl::{Config, Result, TrainctlError};
//! use runctl::{CreateInstanceOptions, TrainInstanceOptions};
//!
//! # async fn example() -> runctl::Result<()> {
//! let config = Config::load(None)?;
//! // Use re-exported types directly
//! # Ok(())
//! # }
//! ```
//!
//! ### Provider Trait (Future)
//!
//! The provider trait system is defined but not yet used by the CLI. When
//! multi-cloud support is enabled:
//!
//! ```rust,no_run
//! use runctl::{Config, TrainingProvider};
//!
//! # async fn example() -> runctl::error::Result<()> {
//! let config = Config::load(None)?;
//! // let provider = config.get_provider("aws")?;
//! // let resource_id = provider.create_resource("g4dn.xlarge", options).await?;
//! # Ok(())
//! # }
//! ```

pub mod aws;
pub mod aws_utils;
pub mod checkpoint;
pub mod config;
pub mod dashboard;
pub mod data_transfer;
pub mod diagnostics;
pub mod docker;
pub mod ebs;
pub mod ebs_optimization;
pub mod error;
pub mod error_helpers;
pub mod fast_data_loading;
pub mod local;
pub mod monitor;
pub mod provider;
pub mod providers;
pub mod resource_tracking;
pub mod resources;
pub mod retry;
pub mod runpod;
pub mod s3;
pub mod safe_cleanup;
pub mod ssh_sync;
pub mod training;
pub mod utils;
pub mod validation;
pub mod workflow;

// Re-export commonly used types
pub use error::{ConfigError, IsRetryable, Result, TrainctlError};
pub use provider::{
    CreateResourceOptions, ResourceState, ResourceStatus, TrainingJob, TrainingProvider,
};
pub use providers::ProviderRegistry;
pub use resource_tracking::{ResourceTracker, ResourceUsage, TrackedResource};
pub use retry::{ExponentialBackoffPolicy, RetryPolicy};
pub use safe_cleanup::{safe_cleanup, CleanupResult, CleanupSafety};
pub use training::{TrainingSession, TrainingStatus};
pub use validation::{validate_path, validate_path_path};

// Re-export commonly used types for convenience
pub use aws::{CreateInstanceOptions, TrainInstanceOptions};
pub use config::Config;
pub use resources::estimate_instance_cost;
