//! Type definitions for AWS operations
//!
//! Shared types used across AWS modules for serialization and data structures.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct InstanceInfo {
    pub success: bool,
    pub instance_id: String,
    pub instance_type: String,
    pub public_ip: Option<String>,
    pub private_ip: Option<String>,
    pub state: String,
    pub cost_per_hour: f64,
    pub message: String,
}

impl std::fmt::Display for InstanceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "InstanceInfo {{ success: {}, instance_id: {}, instance_type: {}, state: {}, cost_per_hour: ${:.2}/hr, public_ip: {:?}, private_ip: {:?} }}",
            self.success, self.instance_id, self.instance_type, self.state, self.cost_per_hour, self.public_ip, self.private_ip
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct TrainingInfo {
    pub success: bool,
    pub method: String,
    pub instance_id: String,
    pub log_path: String,
    pub monitor_command: String,
}

impl std::fmt::Display for TrainingInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TrainingInfo {{ success: {}, method: {}, instance_id: {}, log_path: {}, monitor_command: {} }}",
            self.success, self.method, self.instance_id, self.log_path, self.monitor_command
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct StopInstanceResult {
    pub success: bool,
    pub instance_id: String,
    pub state: String,
    pub message: String,
}

impl std::fmt::Display for StopInstanceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StopInstanceResult {{ success: {}, instance_id: {}, state: {}, message: {} }}",
            self.success, self.instance_id, self.state, self.message
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct TerminateInstanceResult {
    pub success: bool,
    pub instance_id: String,
    pub state: String,
    pub has_data_volumes: bool,
    pub message: String,
}

impl std::fmt::Display for TerminateInstanceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TerminateInstanceResult {{ success: {}, instance_id: {}, state: {}, has_data_volumes: {}, message: {} }}",
            self.success, self.instance_id, self.state, self.has_data_volumes, self.message
        )
    }
}

#[derive(Serialize, Deserialize)]
pub struct ProcessListResult {
    pub success: bool,
    pub instance_id: String,
    pub timestamp: String,
    pub resource_usage: ProcessResourceUsage,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct ProcessResourceUsage {
    pub cpu_percent: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub memory_percent: f64,
    pub disk_usage: Vec<DiskUsage>,
    pub gpu_info: Option<GpuInfoJson>,
}

#[derive(Serialize, Deserialize)]
pub struct DiskUsage {
    pub filesystem: String,
    pub size_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub percent_used: f64,
    pub mount_point: String,
}

#[derive(Serialize, Deserialize)]
pub struct GpuInfoJson {
    pub gpus: Vec<GpuDetailJson>,
}

#[derive(Serialize, Deserialize)]
pub struct GpuDetailJson {
    pub index: usize,
    pub name: String,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub memory_percent: f64,
    pub utilization_percent: f64,
    pub temperature_c: Option<u32>,
    pub power_draw_w: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub command: String,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub memory_percent: f64,
    pub runtime: String,
}

#[derive(Debug, Clone)]
pub struct CreateInstanceOptions {
    pub instance_type: String,
    pub use_spot: bool,
    pub spot_max_price: Option<String>,
    pub no_fallback: bool,
    pub key_name: Option<String>,
    pub security_group: Option<String>,
    pub ami_id: Option<String>,
    pub root_volume_size: Option<i32>,
    pub data_volume_size: Option<i32>,
    pub project_name: String,
    pub iam_instance_profile: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrainInstanceOptions {
    pub instance_id: String,
    pub script: std::path::PathBuf,
    #[allow(dead_code)] // Reserved for future S3 data source support
    pub data_s3: Option<String>,
    #[allow(dead_code)] // Reserved for future S3 output support
    pub output_s3: Option<String>,
    pub sync_code: bool,
    pub include_patterns: Vec<String>,
    pub project_name: String,
    pub script_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CreateSpotInstanceOptions {
    pub instance_type: String,
    pub ami_id: String,
    pub user_data: String,
    pub max_price: Option<String>,
    pub key_name: Option<String>,
    pub security_group: Option<String>,
    pub root_volume_size: i32,
    pub iam_instance_profile: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct StartInstanceResult {
    pub success: bool,
    pub instance_id: String,
    pub state: String,
    pub public_ip: Option<String>,
    pub message: String,
}

impl std::fmt::Display for StartInstanceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StartInstanceResult {{ success: {}, instance_id: {}, state: {}, public_ip: {:?}, message: {} }}",
            self.success, self.instance_id, self.state, self.public_ip, self.message
        )
    }
}
