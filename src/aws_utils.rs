//! Common AWS utilities shared across modules
//!
//! This module provides reusable functions for AWS operations to reduce
//! code duplication and ensure consistent behavior.

use crate::error::{Result, TrainctlError};
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// Polling configuration constants
const SSM_COMMAND_MAX_ATTEMPTS: u32 = 60;
const SSM_COMMAND_INITIAL_DELAY_SECS: u64 = 2;
const SSM_COMMAND_MAX_DELAY_SECS: u64 = 10;
const INSTANCE_WAIT_MAX_ATTEMPTS: u32 = 60;
const INSTANCE_WAIT_POLL_INTERVAL_SECS: u64 = 5;
const INSTANCE_SSM_READY_DELAY_SECS: u64 = 30;
const VOLUME_ATTACH_MAX_ATTEMPTS: u32 = 30;
const VOLUME_ATTACH_POLL_INTERVAL_SECS: u64 = 2;
const VOLUME_DETACH_MAX_ATTEMPTS: u32 = 30;
const VOLUME_DETACH_POLL_INTERVAL_SECS: u64 = 2;

/// Execute SSM command and poll for completion
///
/// This is a unified implementation used by both `aws.rs` and `data_transfer.rs`
/// to ensure consistent behavior and reduce duplication.
pub async fn execute_ssm_command(
    client: &SsmClient,
    instance_id: &str,
    command: &str,
) -> Result<String> {
    info!(
        "Executing SSM command on instance {}: {}",
        instance_id, command
    );

    // Send command
    let response = client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command.to_string()])
        .send()
        .await
        .map_err(|e| TrainctlError::Ssm(format!("Failed to send SSM command: {}", e)))?;

    let command_id = response
        .command()
        .and_then(|c| c.command_id())
        .ok_or_else(|| TrainctlError::Ssm("No command ID in response".to_string()))?
        .to_string();

    info!("Command ID: {}, waiting for completion...", command_id);

    // Poll for completion with exponential backoff and progress indicator
    let max_attempts = SSM_COMMAND_MAX_ATTEMPTS;
    let mut delay = Duration::from_secs(SSM_COMMAND_INITIAL_DELAY_SECS);

    // Create progress bar for long-running commands
    let pb = ProgressBar::new(max_attempts as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("Progress bar template should be valid")
            .progress_chars("#>-"),
    );
    pb.set_message("Waiting for command completion...");

    for attempt in 0..max_attempts {
        sleep(delay).await;
        pb.set_position((attempt + 1) as u64);

        // Exponential backoff: 2s, 4s, 8s, then cap at max delay
        if attempt < 3 {
            delay = Duration::from_secs(SSM_COMMAND_INITIAL_DELAY_SECS.pow(attempt + 1));
        } else {
            delay = Duration::from_secs(SSM_COMMAND_MAX_DELAY_SECS);
        }

        let invocation = client
            .get_command_invocation()
            .command_id(&command_id)
            .instance_id(instance_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Ssm(format!("Failed to get command invocation: {}", e)))?;

        let status = invocation.status().map(|s| s.as_str()).unwrap_or("Unknown");

        match status {
            "Success" => {
                pb.finish_with_message("Command completed");
                let output = invocation
                    .standard_output_content()
                    .unwrap_or("")
                    .to_string();
                let error_output = invocation
                    .standard_error_content()
                    .unwrap_or("")
                    .to_string();

                if !error_output.is_empty() && !error_output.trim().is_empty() {
                    warn!("Command stderr: {}", error_output);
                }

                info!("SSM command completed successfully");
                return Ok(output);
            }
            "Failed" | "Cancelled" | "TimedOut" => {
                pb.finish_with_message("Command failed");
                let error = invocation
                    .standard_error_content()
                    .unwrap_or("Unknown error")
                    .to_string();
                return Err(TrainctlError::Ssm(format!("SSM command failed: {}", error)));
            }
            "InProgress" | "Pending" => {
                pb.set_message(format!("Status: {}...", status));
            }
            _ => {
                pb.set_message(format!("Status: {} (unknown)", status));
                warn!("Unknown command status: {}", status);
            }
        }
    }

    pb.finish_with_message("Command timed out");
    Err(TrainctlError::Ssm(format!(
        "SSM command timed out after {} attempts",
        max_attempts
    )))
}

/// Wait for instance to reach running state
///
/// Polls EC2 API until instance is running and SSM is ready.
pub async fn wait_for_instance_running(client: &Ec2Client, instance_id: &str) -> Result<()> {
    const MAX_ATTEMPTS: u32 = INSTANCE_WAIT_MAX_ATTEMPTS;
    const POLL_INTERVAL: Duration = Duration::from_secs(INSTANCE_WAIT_POLL_INTERVAL_SECS);

    let pb = ProgressBar::new(u64::from(MAX_ATTEMPTS));
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .expect("Progress bar template should be valid"),
    );
    pb.set_message("Waiting for instance to start...");

    for attempt in 0..MAX_ATTEMPTS {
        sleep(POLL_INTERVAL).await;
        pb.set_position((attempt + 1) as u64);

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
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "instance".to_string(),
                resource_id: instance_id.to_string(),
            })?;

        let state = instance.state().and_then(|s| s.name());

        match state.as_ref().map(|s| s.as_str()) {
            Some("running") => {
                pb.set_message("Instance running, waiting for SSM...");
                // Wait a bit more for SSM to be ready
                sleep(Duration::from_secs(INSTANCE_SSM_READY_DELAY_SECS)).await;
                pb.finish_with_message("Instance ready");
                return Ok(());
            }
            Some("terminated" | "shutting-down") => {
                pb.finish_with_message("Instance terminated");
                return Err(TrainctlError::Resource {
                    resource_type: "instance".to_string(),
                    operation: "wait_for_running".to_string(),
                    resource_id: Some(instance_id.to_string()),
                    message: format!(
                        "Instance {} terminated before becoming ready. Suggestions: Check instance logs in AWS Console, verify AMI and instance type compatibility, check security group and IAM role settings",
                        instance_id
                    ),
                    source: None,
                });
            }
            _ => {
                let state_str = state.as_ref().map(|s| s.as_str()).unwrap_or("unknown");
                pb.set_message(format!("State: {}...", state_str));
            }
        }
    }

    pb.finish_with_message("Timeout");
    Err(TrainctlError::Resource {
        resource_type: "instance".to_string(),
        operation: "wait_for_running".to_string(),
        resource_id: Some(instance_id.to_string()),
        message: format!(
            "Instance {} did not reach running state within {} minutes. Suggestions: Check instance logs: aws ec2 get-console-output --instance-id {}, verify instance type and AMI compatibility, check security groups allow necessary traffic",
            instance_id,
            MAX_ATTEMPTS * 5 / 60,
            instance_id
        ),
        source: None,
    })
}

/// Wait for EBS volume to be attached
///
/// Polls EC2 API until volume shows as attached to the instance.
pub async fn wait_for_volume_attachment(
    client: &Ec2Client,
    volume_id: &str,
    instance_id: &str,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = VOLUME_ATTACH_MAX_ATTEMPTS;
    const POLL_INTERVAL: Duration = Duration::from_secs(VOLUME_ATTACH_POLL_INTERVAL_SECS);

    let pb = ProgressBar::new(u64::from(MAX_ATTEMPTS));
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .expect("Progress bar template should be valid"),
    );
    pb.set_message("Attaching volume...");

    for attempt in 0..MAX_ATTEMPTS {
        sleep(POLL_INTERVAL).await;
        pb.set_position((attempt + 1) as u64);

        let response = client
            .describe_volumes()
            .volume_ids(volume_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

        let volume = response
            .volumes()
            .first()
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "volume".to_string(),
                resource_id: volume_id.to_string(),
            })?;

        let attachment = volume
            .attachments()
            .iter()
            .find(|a| a.instance_id().map(|id| id == instance_id).unwrap_or(false));

        if let Some(att) = attachment {
            let state = att.state().map(|s| s.as_str()).unwrap_or("unknown");
            if state == "attached" {
                pb.finish_with_message("Volume attached");
                info!("Volume {} attached to instance {}", volume_id, instance_id);
                return Ok(());
            }
            pb.set_message(format!("State: {}...", state));
        } else {
            pb.set_message("Waiting for attachment...");
        }
    }

    pb.finish_with_message("Attachment timeout");
    Err(TrainctlError::Resource {
        resource_type: "volume".to_string(),
        operation: "attach".to_string(),
        resource_id: Some(volume_id.to_string()),
        message: format!(
            "Volume {} did not attach to instance {} within {} seconds. Suggestions: Verify instance and volume are in the same availability zone, check instance is running: runctl aws instances list, verify volume exists: runctl aws ebs list",
            volume_id,
            instance_id,
            MAX_ATTEMPTS * 2
        ),
        source: None,
    })
}

/// Wait for EBS volume to be detached
///
/// Polls EC2 API until volume shows as available (detached).
pub async fn wait_for_volume_detached(client: &Ec2Client, volume_id: &str) -> Result<()> {
    const MAX_ATTEMPTS: u32 = VOLUME_DETACH_MAX_ATTEMPTS;
    const POLL_INTERVAL: Duration = Duration::from_secs(VOLUME_DETACH_POLL_INTERVAL_SECS);

    let pb = ProgressBar::new(u64::from(MAX_ATTEMPTS));
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .expect("Progress bar template should be valid"),
    );
    pb.set_message("Detaching volume...");

    for attempt in 0..MAX_ATTEMPTS {
        sleep(POLL_INTERVAL).await;
        pb.set_position((attempt + 1) as u64);

        let response = client
            .describe_volumes()
            .volume_ids(volume_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

        let volume = response
            .volumes()
            .first()
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "volume".to_string(),
                resource_id: volume_id.to_string(),
            })?;

        let state = volume.state().map(|s| s.as_str()).unwrap_or("unknown");
        if state == "available" {
            pb.finish_with_message("Volume detached");
            info!("Volume {} detached", volume_id);
            return Ok(());
        }

        pb.set_message(format!("State: {}...", state));
    }

    pb.finish_with_message("Detachment timeout");
    Err(TrainctlError::Resource {
        resource_type: "volume".to_string(),
        operation: "detach".to_string(),
        resource_id: Some(volume_id.to_string()),
        message: format!(
            "Volume {} did not detach within {} seconds. Suggestions: Check if instance is still using the volume, try force detach: aws ec2 detach-volume --volume-id {} --force, verify instance state: runctl aws instances list",
            volume_id,
            MAX_ATTEMPTS * 2,
            volume_id
        ),
        source: None,
    })
}

/// Count running EC2 instances (safety check)
///
/// Returns the number of instances currently in "running" state.
pub async fn count_running_instances(client: &Ec2Client) -> Result<i32> {
    let response = client
        .describe_instances()
        .filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("instance-state-name")
                .values("running")
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instances: {}", e)))?;

    let count = response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .filter(|i| {
            i.state()
                .and_then(|s| s.name())
                .map(|s| s.as_str() == "running")
                .unwrap_or(false)
        })
        .count() as i32;

    Ok(count)
}
