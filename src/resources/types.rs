//! Type definitions for resource management
//!
//! Contains data structures for representing resources across different platforms
//! (AWS, RunPod, local) and options for listing and filtering resources.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Summary of all resources across platforms
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub aws_instances: Vec<AwsInstance>,
    pub runpod_pods: Vec<RunPodPod>,
    pub local_processes: Vec<LocalProcess>,
    pub total_cost_estimate: f64,
    pub timestamp: DateTime<Utc>,
}

/// AWS EC2 instance information
#[derive(Debug, Serialize, Deserialize)]
pub struct AwsInstance {
    pub instance_id: String,
    pub instance_type: String,
    pub state: String,
    pub launch_time: Option<DateTime<Utc>>,
    pub tags: Vec<(String, String)>,
    pub cost_per_hour: f64,
}

/// RunPod pod information
#[derive(Debug, Serialize, Deserialize)]
pub struct RunPodPod {
    pub pod_id: String,
    pub name: String,
    pub status: String,
    pub gpu_type: String,
    pub created_at: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
}

/// Local process information
#[derive(Debug, Serialize, Deserialize)]
pub struct LocalProcess {
    pub pid: u32,
    pub command: String,
    pub started: Option<DateTime<Utc>>,
    pub cpu_percent: f32,
    pub memory_mb: f32,
}

/// Options for listing resources
#[derive(Debug, Clone)]
pub struct ListResourcesOptions {
    pub detailed: bool,
    pub platform: String,
    pub output_format: String,
    pub format: String,
    pub filter: String,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub show_terminated: bool,
    pub export: Option<String>,
    pub export_file: Option<String>,
    pub project_filter: Option<String>,
    pub user_filter: Option<String>,
}

/// AWS instance information for display
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    pub id: String,
    pub instance_type: String,
    pub state: String,
    pub launch_time: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
    pub accumulated_cost: f64,
    pub runtime: Option<String>,
    pub is_spot: bool,
    pub _spot_request_id: Option<String>,
    pub public_ip: Option<String>,
    pub private_ip: Option<String>,
    pub tags: Vec<(String, String)>,
    pub is_old: bool,
}

/// Options for listing AWS instances
#[derive(Debug, Clone)]
pub struct ListAwsInstancesOptions {
    pub detailed: bool,
    pub format: String,
    pub filter: String,
    pub sort: Option<String>,
    pub limit: Option<usize>,
    pub show_terminated: bool,
    pub project_filter: Option<String>,
    pub user_filter: Option<String>,
}
