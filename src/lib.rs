//! trainctl library
//!
//! This library provides the core functionality for trainctl CLI.

pub mod aws_utils;
pub mod checkpoint;
pub mod config;
pub mod dashboard;
pub mod data_transfer;
pub mod diagnostics;
pub mod ebs_optimization;
pub mod error;
pub mod fast_data_loading;
pub mod provider;
pub mod providers;
pub mod resource_tracking;
pub mod resources;
pub mod retry;
pub mod safe_cleanup;
pub mod ssh_sync;
pub mod training;
pub mod utils;
pub mod validation;

// Re-export commonly used types
pub use error::{ConfigError, IsRetryable, Result, TrainctlError};
pub use provider::{
    CreateResourceOptions, ResourceState, ResourceStatus, TrainingJob, TrainingProvider,
};
pub use training::{TrainingSession, TrainingStatus};
