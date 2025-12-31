//! Helper functions for AWS operations
//!
//! Utility functions used across AWS modules for common operations.
//!
//! ## When to Use Helpers vs Direct SDK Calls
//!
//! These helpers provide:
//! - **Abstraction**: Convert AWS-specific types to provider-agnostic types
//! - **Consistency**: Standardized tagging, naming, and status conversion
//! - **Fallback logic**: Auto-detection when config values aren't set
//!
//! Use helpers when you need provider-agnostic types (`ResourceStatus`) or
//! consistent behavior (project names, user IDs). Use direct SDK calls when
//! you need AWS-specific features not in the abstraction.
//!
//! ## Key Functions
//!
//! - `ec2_instance_to_resource_status()`: Converts AWS EC2 instances to
//!   provider-agnostic `ResourceStatus` for use with `ResourceTracker`
//! - `get_user_id()`: Returns user ID with fallback chain (config → env → "unknown")
//! - `get_project_name()`: Derives project name from config or current directory

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::provider::{normalize_state, ResourceStatus};
use aws_sdk_ec2::types::Instance as Ec2Instance;
use chrono::{DateTime, Utc};

use super::types;

/// Get user identifier for tagging
pub(crate) fn get_user_id(config: &Config) -> String {
    // Try config first
    if let Some(aws_cfg) = &config.aws {
        if let Some(user_id) = &aws_cfg.user_id {
            return user_id.clone();
        }
    }

    // Auto-detect from username
    if let Ok(username) = std::env::var("USER") {
        return username;
    }
    if let Ok(username) = std::env::var("USERNAME") {
        return username;
    }

    // Fallback
    "unknown".to_string()
}

/// Convert EC2 instance to ResourceStatus for ResourceTracker
///
/// Extracts relevant information from an AWS EC2 instance and converts it
/// to the provider-agnostic ResourceStatus format.
///
/// # Arguments
/// * `instance` - AWS EC2 instance
/// * `instance_id` - The instance ID (for error messages)
///
/// # Returns
/// A ResourceStatus with normalized state, cost, and metadata.
pub(crate) fn ec2_instance_to_resource_status(
    instance: &Ec2Instance,
    instance_id: &str,
) -> Result<ResourceStatus> {
    let state = normalize_state(
        instance
            .state()
            .and_then(|s| s.name())
            .map(|s| s.as_str())
            .unwrap_or("unknown"),
    );

    let tags: Vec<(String, String)> = instance
        .tags()
        .iter()
        .filter_map(|tag| {
            tag.key()
                .zip(tag.value())
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();

    let instance_type = instance.instance_type().map(|t| t.as_str().to_string());
    let launch_time = instance
        .launch_time()
        .map(|lt| DateTime::<Utc>::from_timestamp(lt.secs(), 0).unwrap_or_else(Utc::now));

    let public_ip = instance.public_ip_address().map(|s| s.to_string());

    let cost_per_hour =
        crate::utils::get_instance_cost(instance_type.as_deref().unwrap_or("unknown"));

    Ok(ResourceStatus {
        id: instance_id.to_string(),
        name: tags
            .iter()
            .find(|(k, _)| k == "Name")
            .map(|(_, v)| v.clone()),
        state,
        instance_type,
        launch_time,
        cost_per_hour,
        public_ip,
        tags,
    })
}

/// Find an instance in an EC2 DescribeInstances response
///
/// This is a common pattern used throughout the AWS module to find a specific
/// instance by ID in the response structure.
pub(crate) fn find_instance_in_response<'a>(
    response: &'a aws_sdk_ec2::operation::describe_instances::DescribeInstancesOutput,
    instance_id: &str,
) -> Option<&'a aws_sdk_ec2::types::Instance> {
    response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
}

/// Update resource status in ResourceTracker
pub(crate) async fn update_resource_status_in_tracker(
    instance_id: &str,
    client: &aws_sdk_ec2::Client,
    config: &Config,
) {
    if let Some(tracker) = &config.resource_tracker {
        if let Ok(response) = client
            .describe_instances()
            .instance_ids(instance_id)
            .send()
            .await
        {
            if let Some(instance) = find_instance_in_response(&response, instance_id) {
                if let Ok(resource_status) = ec2_instance_to_resource_status(instance, instance_id)
                {
                    // Try to update existing resource, or register if new
                    let instance_id_string = instance_id.to_string();
                    if tracker.exists(&instance_id_string).await {
                        // Use update_state to update the resource state
                        if let Err(e) = tracker
                            .update_state(&instance_id_string, resource_status.state)
                            .await
                        {
                            tracing::warn!("Failed to update resource state: {}", e);
                        }
                    } else if let Err(e) = tracker.register(resource_status).await {
                        tracing::warn!("Failed to update resource in tracker: {}", e);
                    } else {
                        tracing::info!(
                            "Updated instance {} status in ResourceTracker",
                            instance_id
                        );
                    }
                }
            }
        }
    }
}

/// Get project name, deriving from current directory if not provided
pub fn get_project_name(provided: Option<String>, config: &Config) -> String {
    // Use provided value if given
    if let Some(name) = provided {
        return name;
    }

    // Try config
    if let Some(aws_cfg) = &config.aws {
        if let Some(project) = &aws_cfg.default_project_name {
            return project.clone();
        }
    }

    // Derive from current directory
    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(dir_name) = current_dir.file_name() {
            if let Some(name_str) = dir_name.to_str() {
                // Sanitize directory name for use as project name
                let sanitized = name_str
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>();
                if !sanitized.is_empty() {
                    return sanitized;
                }
            }
        }
    }

    // Final fallback
    "runctl-project".to_string()
}

/// Validate project name if provided, with helpful error messages
#[allow(dead_code)] // Reserved for future validation
pub(crate) fn validate_project_name_option(name: Option<&String>) -> Result<()> {
    if let Some(name) = name {
        crate::validation::validate_project_name(name)?;
    }
    Ok(())
}

/// Get instance info as JSON structure
pub(crate) async fn get_instance_info_json(
    client: &aws_sdk_ec2::Client,
    instance_id: &str,
    instance_type: &str,
) -> Result<types::InstanceInfo> {
    use types::InstanceInfo;

    let response = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();

    let cost_per_hour = crate::utils::get_instance_cost(instance_type);

    Ok(InstanceInfo {
        success: true,
        instance_id: instance_id.to_string(),
        instance_type: instance_type.to_string(),
        public_ip: instance.public_ip_address().map(|s| s.to_string()),
        private_ip: instance.private_ip_address().map(|s| s.to_string()),
        state,
        cost_per_hour,
        message: format!("Instance {} created successfully", instance_id),
    })
}
