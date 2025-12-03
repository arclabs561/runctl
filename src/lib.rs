//! trainctl library
//!
//! This library provides the core functionality for trainctl CLI.

pub mod training;
pub mod config;
pub mod utils;

// Re-export commonly used types
pub use training::{TrainingSession, TrainingStatus};

