//! Common AWS utilities shared across modules
//!
//! This module provides reusable functions for AWS operations to reduce
//! code duplication and ensure consistent behavior.

use anyhow::{Context, Result};
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
use indicatif::{ProgressBar, ProgressStyle};

/// Execute SSM command and poll for completion
///
/// This is a unified implementation used by both `aws.rs` and `data_transfer.rs`
/// to ensure consistent behavior and reduce duplication.
pub async fn execute_ssm_command(
    client: &SsmClient,
    instance_id: &str,
    command: &str,
) -> Result<String> {
    info!("Executing SSM command on instance {}: {}", instance_id, command);
    
    // Send command
    let response = client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command.to_string()])
        .send()
        .await
        .context("Failed to send SSM command")?;
    
    let command_id = response.command()
        .and_then(|c| c.command_id())
        .context("No command ID in response")?
        .to_string();
    
    info!("Command ID: {}, waiting for completion...", command_id);
    
    // Poll for completion with exponential backoff and progress indicator
    let max_attempts = 60; // 5 minutes max
    let mut delay = Duration::from_secs(2); // Start with 2 seconds
    
    // Create progress bar for long-running commands
    let pb = ProgressBar::new(max_attempts as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-")
    );
    pb.set_message("Waiting for command completion...");
    
    for attempt in 0..max_attempts {
        sleep(delay).await;
        pb.set_position((attempt + 1) as u64);
        
        // Exponential backoff: 2s, 4s, 8s, then cap at 10s
        if attempt < 3 {
            delay = Duration::from_secs(2_u64.pow(attempt + 1));
        } else {
            delay = Duration::from_secs(10);
        }
        
        let invocation = client
            .get_command_invocation()
            .command_id(&command_id)
            .instance_id(instance_id)
            .send()
            .await
            .context("Failed to get command invocation")?;
        
        let status = invocation.status()
            .map(|s| s.as_str())
            .unwrap_or("Unknown");
        
        match status {
            "Success" => {
                pb.finish_with_message("Command completed");
                let output = invocation.standard_output_content()
                    .unwrap_or("")
                    .to_string();
                let error_output = invocation.standard_error_content()
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
                let error = invocation.standard_error_content()
                    .unwrap_or("Unknown error")
                    .to_string();
                anyhow::bail!("SSM command failed: {}", error);
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
    anyhow::bail!("SSM command timed out after {} attempts", max_attempts);
}

/// Wait for instance to reach running state
///
/// Polls EC2 API until instance is running and SSM is ready.
pub async fn wait_for_instance_running(
    client: &Ec2Client,
    instance_id: &str,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 60;
    const POLL_INTERVAL: Duration = Duration::from_secs(5);
    
    let pb = ProgressBar::new(MAX_ATTEMPTS as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
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
            .context("Failed to describe instance")?;
        
        let instance = response
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
            .context("Instance not found")?;
        
        let state = instance.state()
            .and_then(|s| s.name())
            .map(|s| s.as_str());
        
        match state {
            Some("running") => {
                pb.set_message("Instance running, waiting for SSM...");
                // Wait a bit more for SSM to be ready
                sleep(Duration::from_secs(30)).await;
                pb.finish_with_message("Instance ready");
                return Ok(());
            }
            Some("terminated") | Some("shutting-down") => {
                pb.finish_with_message("Instance terminated");
                anyhow::bail!(
                    "Instance {} terminated before becoming ready.\n💡 Suggestions:\n   - Check instance logs in AWS Console\n   - Verify AMI and instance type compatibility\n   - Check security group and IAM role settings",
                    instance_id
                );
            }
            _ => {
                pb.set_message(format!("State: {:?}...", state.unwrap_or("unknown")));
            }
        }
    }
    
    pb.finish_with_message("Timeout");
    anyhow::bail!(
        "Instance {} did not reach running state within {} minutes.\n💡 Suggestions:\n   - Check instance logs: aws ec2 get-console-output --instance-id {}\n   - Verify instance type and AMI compatibility\n   - Check security groups allow necessary traffic",
        instance_id,
        MAX_ATTEMPTS * 5 / 60,
        instance_id
    );
}

/// Wait for EBS volume to be attached
///
/// Polls EC2 API until volume shows as attached to the instance.
pub async fn wait_for_volume_attachment(
    client: &Ec2Client,
    volume_id: &str,
    instance_id: &str,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 30;
    const POLL_INTERVAL: Duration = Duration::from_secs(2);
    
    let pb = ProgressBar::new(MAX_ATTEMPTS as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
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
            .context("Failed to describe volume")?;
        
        let volume = response.volumes()
            .first()
            .context("Volume not found")?;
        
        let attachment = volume.attachments()
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
    anyhow::bail!(
        "Volume {} did not attach to instance {} within {} seconds.\n💡 Suggestions:\n   - Verify instance and volume are in the same availability zone\n   - Check instance is running: trainctl aws instances list\n   - Verify volume exists: trainctl aws ebs list",
        volume_id,
        instance_id,
        MAX_ATTEMPTS * 2
    );
}

/// Wait for EBS volume to be detached
///
/// Polls EC2 API until volume shows as available (detached).
pub async fn wait_for_volume_detached(
    client: &Ec2Client,
    volume_id: &str,
) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 30;
    const POLL_INTERVAL: Duration = Duration::from_secs(2);
    
    let pb = ProgressBar::new(MAX_ATTEMPTS as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap()
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
            .context("Failed to describe volume")?;
        
        let volume = response.volumes()
            .first()
            .context("Volume not found")?;
        
        let state = volume.state().map(|s| s.as_str()).unwrap_or("unknown");
        if state == "available" {
            pb.finish_with_message("Volume detached");
            info!("Volume {} detached", volume_id);
            return Ok(());
        }
        
        pb.set_message(format!("State: {}...", state));
    }
    
    pb.finish_with_message("Detachment timeout");
    anyhow::bail!(
        "Volume {} did not detach within {} seconds.\n💡 Suggestions:\n   - Check if instance is still using the volume\n   - Try force detach: aws ec2 detach-volume --volume-id {} --force\n   - Verify instance state: trainctl aws instances list",
        volume_id,
        MAX_ATTEMPTS * 2,
        volume_id
    );
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
                .build()
        )
        .send()
        .await
        .context("Failed to describe instances")?;
    
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

