//! Provider-agnostic trait definitions for cloud training platforms
//!
//! ## Position Statement
//!
//! This trait system is **defined but not yet used** by the CLI. The CLI currently
//! uses direct implementations (`aws::handle_command()`, etc.). This is intentional
//! technical debt - see rationale below.
//!
//! ## Why This Approach?
//!
//! **Alternative 1**: Force migration now
//! - Pro: Consistent abstraction
//! - Con: High risk (incomplete implementations), breaks working code
//!
//! **Alternative 2**: Delete trait system
//! - Pro: No unused code
//! - Con: Harder to add multi-cloud support later
//!
//! **Chosen Approach**: Keep trait, don't force migration
//! - Pro: Future-ready, low risk, follows industry patterns (Terraform, Pulumi)
//! - Con: Some unused code (documented and marked)
//!
//! ## Architecture Pattern
//!
//! This follows industry patterns seen in:
//! - **Terraform**: Plugin-based architecture with external provider binaries
//! - **Pulumi**: Component-based abstraction with language-native implementations
//! - **Kubernetes**: CRD-based extensibility through custom controllers
//!
//! ## Current Status
//!
//! The trait system is defined but not yet used by the CLI. The CLI currently uses
//! direct implementations in `aws.rs`, `runpod.rs`, etc. This follows the pragmatic
//! pattern where abstraction layers are prepared but not forced until multi-cloud
//! support is actually needed.
//!
//! **Decision**: See `docs/PROVIDER_TRAIT_DECISION.md` for detailed rationale.
//!
//! ## Future Evolution Path
//!
//! When multi-cloud support becomes a priority:
//! 1. Complete provider implementations (currently skeletons)
//! 2. Add `ProviderRegistry` for dynamic provider selection
//! 3. Gradually migrate CLI commands to use providers
//! 4. Support both systems during transition (like Pulumi does)
//!
//! This approach mirrors how mature tools evolved: Terraform started with direct
//! integrations before the plugin system, and Pulumi maintains both abstracted
//! components and direct provider access.

use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resource identifier (instance ID, pod ID, etc.)
pub type ResourceId = String;

/// Training job configuration
///
/// This struct is part of the provider trait API and is kept for future use
/// when the provider trait system is fully integrated.
#[allow(dead_code)] // Reserved for future provider trait integration
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
///
/// This struct is part of the provider trait API and is kept for future use
/// when the provider trait system is fully integrated.
#[allow(dead_code)] // Reserved for future provider trait integration
#[derive(Debug, Clone)]
pub struct TrainingStatus {
    pub job_id: Option<String>,
    pub status: ExecutionStatus,
    pub log_output: Option<String>,
    pub checkpoint_path: Option<PathBuf>,
}

/// Execution status of a training job
///
/// This enum is part of the provider trait API and is kept for future use
/// when the provider trait system is fully integrated.
#[allow(dead_code)] // Reserved for future provider trait integration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Trait for abstracting training operations across cloud providers
///
/// This trait provides a unified interface for working with different cloud providers
/// (AWS EC2, RunPod, Lyceum AI, etc.) in a provider-agnostic way.
///
/// # Example
///
/// ```rust,no_run
/// use runctl::provider::{TrainingProvider, CreateResourceOptions, TrainingJob};
///
/// # async fn example(provider: impl TrainingProvider) -> runctl::error::Result<()> {
/// // Create a resource
/// let resource_id = provider.create_resource(
///     "g4dn.xlarge",
///     CreateResourceOptions::default()
/// ).await?;
///
/// // Get status
/// let status = provider.get_resource_status(&resource_id).await?;
/// println!("Resource state: {:?}", status.state);
///
/// // Execute training
/// let job = TrainingJob {
///     script: "train.py".into(),
///     args: vec![],
///     data_source: Some("s3://bucket/data".to_string()),
///     output_dest: None,
///     checkpoint_dir: Some("./checkpoints".into()),
///     environment: vec![],
/// };
/// let training_status = provider.train(&resource_id, job).await?;
/// # Ok(())
/// # }
/// ```
#[async_trait]
#[allow(dead_code)] // Reserved for future provider trait integration
pub trait TrainingProvider: Send + Sync {
    /// Provider name (e.g., "aws", "runpod", "lyceum")
    #[allow(dead_code)] // Reserved for future provider trait integration
    fn name(&self) -> &'static str;

    /// Create a new compute resource (instance, pod, etc.)
    ///
    /// # Arguments
    /// * `instance_type` - The instance/pod type (e.g., "g4dn.xlarge", "RTX 4090")
    /// * `options` - Additional options for resource creation
    ///
    /// # Returns
    /// The resource ID that can be used to reference this resource later
    async fn create_resource(
        &self,
        instance_type: &str,
        options: CreateResourceOptions,
    ) -> Result<ResourceId>;

    /// Get status of a resource
    ///
    /// # Arguments
    /// * `resource_id` - The ID of the resource to query
    ///
    /// # Returns
    /// Current status including state, cost, launch time, etc.
    async fn get_resource_status(&self, resource_id: &ResourceId) -> Result<ResourceStatus>;

    /// List all resources managed by this provider
    ///
    /// # Returns
    /// A vector of all resources with their current status
    async fn list_resources(&self) -> Result<Vec<ResourceStatus>>;

    /// Execute a training job on a resource
    ///
    /// # Arguments
    /// * `resource_id` - The resource to run the job on
    /// * `job` - The training job configuration
    ///
    /// # Returns
    /// Initial training status (job will continue running asynchronously)
    async fn train(&self, resource_id: &ResourceId, job: TrainingJob) -> Result<TrainingStatus>;

    /// Monitor training progress (logs, checkpoints, etc.)
    ///
    /// # Arguments
    /// * `resource_id` - The resource running the training job
    /// * `follow` - If true, continuously stream logs (like `tail -f`)
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
///
/// This struct is part of the provider trait API and is kept for future use
/// when the provider trait system is fully integrated.
#[allow(dead_code)] // Reserved for future provider trait integration
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
