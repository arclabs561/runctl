//! Provider-agnostic trait definitions for cloud training platforms
//!
//! This module defines traits that all cloud providers (AWS, RunPod, Lyceum AI, etc.)
//! could implement, allowing runctl to work with any provider through a unified interface.
//!
//! **Current status**: These types are defined but not yet used by the CLI.
//! The CLI currently uses direct implementations in `aws.rs`, `runpod.rs`, etc.
//! This module is kept for potential future refactoring to a fully provider-agnostic design.

#![allow(dead_code)] // Reserved for future provider abstraction

use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resource identifier (instance ID, pod ID, etc.)
pub type ResourceId = String;

/// Training job configuration
#[derive(Debug, Clone)]
pub struct TrainingJob {
    pub script: PathBuf,
    pub args: Vec<String>,
    pub data_source: Option<String>,
    pub output_dest: Option<String>,
    pub checkpoint_dir: Option<PathBuf>,
    pub environment: Vec<(String, String)>,
}

/// Status of a training resource (instance, pod, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub id: ResourceId,
    pub name: Option<String>,
    pub state: ResourceState,
    pub instance_type: Option<String>,
    pub launch_time: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
    pub public_ip: Option<String>,
    pub tags: Vec<(String, String)>,
}

/// Resource states across all providers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceState {
    Running,
    Starting,
    Stopped,
    Terminating,
    Terminated,
    Error(String),
    Unknown,
}

/// Status of a running training job
#[derive(Debug, Clone)]
pub struct TrainingStatus {
    pub job_id: Option<String>,
    pub status: ExecutionStatus,
    pub log_output: Option<String>,
    pub checkpoint_path: Option<PathBuf>,
}

/// Execution status of a training job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Trait for abstracting training operations across cloud providers
#[async_trait]
pub trait TrainingProvider: Send + Sync {
    /// Provider name (e.g., "aws", "runpod", "lyceum")
    fn name(&self) -> &'static str;

    /// Create a new compute resource (instance, pod, etc.)
    async fn create_resource(
        &self,
        instance_type: &str,
        options: CreateResourceOptions,
    ) -> Result<ResourceId>;

    /// Get status of a resource
    async fn get_resource_status(&self, resource_id: &ResourceId) -> Result<ResourceStatus>;

    /// List all resources managed by this provider
    async fn list_resources(&self) -> Result<Vec<ResourceStatus>>;

    /// Execute a training job on a resource
    async fn train(&self, resource_id: &ResourceId, job: TrainingJob) -> Result<TrainingStatus>;

    /// Monitor training progress (logs, checkpoints, etc.)
    async fn monitor(&self, resource_id: &ResourceId, follow: bool) -> Result<()>;

    /// Download results from a resource
    async fn download(
        &self,
        resource_id: &ResourceId,
        remote_path: &Path,
        local_path: &Path,
    ) -> Result<()>;

    /// Terminate a resource
    async fn terminate(&self, resource_id: &ResourceId) -> Result<()>;

    /// Get cost estimate for a resource type
    fn estimate_cost(&self, instance_type: &str, hours: f64) -> f64;
}

/// Options for creating resources
#[derive(Debug, Clone, Default)]
pub struct CreateResourceOptions {
    pub use_spot: bool,
    pub spot_max_price: Option<String>,
    pub image: Option<String>,
    pub disk_gb: Option<u32>,
    pub memory_gb: Option<u32>,
    pub tags: Vec<(String, String)>,
    pub custom: std::collections::HashMap<String, String>,
}

/// Helper to convert provider-specific states to ResourceState
pub fn normalize_state(state_str: &str) -> ResourceState {
    let state_lower = state_str.to_lowercase();
    match state_lower.as_str() {
        "running" | "active" | "ready" => ResourceState::Running,
        "pending" | "starting" | "initializing" | "provisioning" => ResourceState::Starting,
        "stopping" | "stopped" => ResourceState::Stopped,
        "terminating" | "shutting-down" => ResourceState::Terminating,
        "terminated" => ResourceState::Terminated,
        _ if state_lower.contains("error") || state_lower.contains("failed") => {
            ResourceState::Error(state_str.to_string())
        }
        _ => ResourceState::Unknown,
    }
}
