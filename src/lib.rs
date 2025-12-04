//! trainctl library
//!
//! This library provides the core functionality for trainctl CLI.

pub mod training;
pub mod config;
pub mod utils;
pub mod provider;
pub mod providers;
pub mod checkpoint;
pub mod resources;
pub mod error;
pub mod resource_tracking;
pub mod safe_cleanup;
pub mod data_transfer;
pub mod fast_data_loading;
pub mod retry;
pub mod aws_utils;
pub mod diagnostics;

// Re-export commonly used types
pub use training::{TrainingSession, TrainingStatus};
pub use provider::{TrainingProvider, ResourceStatus, ResourceState, TrainingJob, CreateResourceOptions};
pub use error::{Result, TrainctlError, ConfigError, IsRetryable};

