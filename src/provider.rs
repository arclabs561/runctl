//! Provider-agnostic trait definitions for cloud training platforms
//!
//! This module defines traits that all cloud providers (AWS, RunPod, Lyceum AI, etc.)
//! must implement, allowing runctl to work with any provider through a unified interface.

use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resource identifier (instance ID, pod ID, etc.)
pub type ResourceId = String;

/// Training script and configuration
#[derive(Debug, Clone)]
/// Training job configuration
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub struct TrainingJob {
    pub script: PathBuf,
    pub args: Vec<String>,
    pub data_source: Option<String>, // S3 path, local path, etc.
    pub output_dest: Option<String>, // Where to save outputs
    pub checkpoint_dir: Option<PathBuf>,
    pub environment: Vec<(String, String)>, // Environment variables
}

/// Resource status information
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Status of a training resource (instance, pod, etc.)
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub struct ResourceStatus {
    pub id: ResourceId,
    pub name: Option<String>,
    pub state: ResourceState,
    pub instance_type: Option<String>, // GPU type, instance type, etc.
    pub launch_time: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
    pub public_ip: Option<String>,
    pub tags: Vec<(String, String)>,
}

/// Resource states across all providers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// State of a training resource
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub enum ResourceState {
    /// Resource is running and ready
    Running,
    /// Resource is starting up
    Starting,
    /// Resource is stopped but can be restarted
    Stopped,
    /// Resource is terminating
    Terminating,
    /// Resource has been terminated
    Terminated,
    /// Resource is in an error state
    Error(String),
    /// Unknown state
    Unknown,
}

/// Training execution status
#[derive(Debug, Clone)]
/// Status of a running training job
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub struct TrainingStatus {
    pub job_id: Option<String>,
    pub status: ExecutionStatus,
    pub log_output: Option<String>, // Path to log file or stream
    pub checkpoint_path: Option<PathBuf>,
}

/// Execution status
#[derive(Debug, Clone, PartialEq, Eq)]
/// Execution status of a training job
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Main trait for cloud training providers
#[async_trait]
/// Trait for abstracting training operations across cloud providers
///
/// Currently unused by the CLI, but kept for future multi-cloud support.
#[allow(dead_code)]
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
/// Reserved for future provider trait implementation
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CreateResourceOptions {
    /// Use spot/preemptible instances (if supported)
    pub use_spot: bool,
    /// Maximum price for spot instances
    pub spot_max_price: Option<String>,
    /// Custom image/AMI
    pub image: Option<String>,
    /// Disk size in GB
    pub disk_gb: Option<u32>,
    /// Memory in GB
    pub memory_gb: Option<u32>,
    /// Tags/labels to apply
    pub tags: Vec<(String, String)>,
    /// Additional provider-specific options
    pub custom: std::collections::HashMap<String, String>,
}

/// Helper to convert provider-specific states to ResourceState
#[allow(dead_code)] // Reserved for future state normalization
pub fn normalize_state(state_str: &str) -> ResourceState {
    let state_lower = state_str.to_lowercase();
    match state_lower.as_str() {
        "running" | "active" | "ready" => ResourceState::Running,
        "pending" | "starting" | "initializing" | "provisioning" => ResourceState::Starting,
        "stopping" => ResourceState::Stopped,
        "stopped" => ResourceState::Stopped,
        "terminating" | "shutting-down" => ResourceState::Terminating,
        "terminated" => ResourceState::Terminated,
        _ if state_lower.contains("error") || state_lower.contains("failed") => {
            ResourceState::Error(state_str.to_string())
        }
        _ => ResourceState::Unknown,
    }
}

/// Registry of available providers
/// Reserved for future provider trait implementation
#[allow(dead_code)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn TrainingProvider>>,
}

#[allow(dead_code)]
impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: Box<dyn TrainingProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, name: &str) -> Option<&dyn TrainingProvider> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.as_ref())
    }

    pub fn list(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
