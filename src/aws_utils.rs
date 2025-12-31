//! Common AWS utilities shared across modules
//!
//! This module provides reusable functions for AWS operations to reduce
//! code duplication and ensure consistent behavior.

use crate::error::{Result, TrainctlError};
use crate::retry::{ExponentialBackoffPolicy, RetryPolicy};
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
// Removed: INSTANCE_SSM_READY_DELAY_SECS - now using actual SSM connectivity test instead of fixed delay
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
        .map_err(|e| {
            let error_msg = format!("{}", e);
            let mut detailed_msg = format!("Failed to send SSM command: {}", error_msg);

            // Provide specific guidance based on error type
            if error_msg.contains("does not exist") || error_msg.contains("not found") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Verify instance has IAM instance profile: aws ec2 describe-instances --instance-ids <id> --query 'Reservations[0].Instances[0].IamInstanceProfile'\n  2. Create IAM instance profile: ./scripts/setup-ssm-role.sh\n  3. Attach profile to instance: runctl aws create ... --iam-instance-profile runctl-ssm-profile\n  4. Wait 60-90 seconds after instance start for SSM agent to register");
            } else if error_msg.contains("not authorized") || error_msg.contains("AccessDenied") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Verify IAM instance profile has AmazonSSMManagedInstanceCore policy\n  2. Check IAM role trust policy allows ec2.amazonaws.com\n  3. Verify instance profile is attached: aws ec2 describe-instances --instance-ids <id>");
            } else if error_msg.contains("not registered") || error_msg.contains("not online") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Wait 60-90 seconds after instance start for SSM agent to register\n  2. Check SSM agent status: aws ssm describe-instance-information --filters 'Key=InstanceIds,Values=<id>'\n  3. Verify instance has internet connectivity to SSM endpoints\n  4. Check SSM agent is running on instance (if you have SSH access)");
            } else if error_msg.contains("service error") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Verify instance has IAM instance profile with SSM permissions\n  2. Check instance is running: runctl resources list --platform aws\n  3. Wait 60-90 seconds after instance start for SSM to be ready\n  4. Verify SSM agent is installed (Amazon Linux has it by default, Ubuntu may need: snap install amazon-ssm-agent --classic)\n  5. Check network connectivity: instance needs access to SSM endpoints\n                  6. Setup SSM: ./scripts/setup-ssm-role.sh then use --iam-instance-profile runctl-ssm-profile");
            }

            TrainctlError::Ssm(detailed_msg)
        })?;

    let command_id = response
        .command()
        .and_then(|c| c.command_id())
        .ok_or_else(|| TrainctlError::Ssm("No command ID in response".to_string()))?
        .to_string();

    info!("Command ID: {}, waiting for completion...", command_id);

    // Poll for completion with exponential backoff and progress indicator
    // Note: We use manual polling here instead of RetryPolicy because we need
    // to check status and handle different states (Success, InProgress, etc.)
    // RetryPolicy is better suited for simple retry-on-failure scenarios.
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

        // Use RetryPolicy for the actual API call (handles transient failures)
        let retry_policy = ExponentialBackoffPolicy::for_cloud_api();
        let command_id_clone = command_id.clone();
        let instance_id_clone = instance_id.to_string();
        let invocation_result = retry_policy
            .execute_with_retry(|| {
                let command_id = command_id_clone.clone();
                let instance_id = instance_id_clone.clone();
                async move {
                    client
                        .get_command_invocation()
                        .command_id(&command_id)
                        .instance_id(&instance_id)
                        .send()
                        .await
                        .map_err(|e| {
                            TrainctlError::Ssm(format!("Failed to get command invocation: {}", e))
                        })
                }
            })
            .await;

        let invocation = match invocation_result {
            Ok(inv) => inv,
            Err(e) => {
                warn!(
                    "Failed to get command invocation (attempt {}): {}",
                    attempt + 1,
                    e
                );
                continue; // Retry on next iteration
            }
        };

        let status = invocation.status().map(|s| s.as_str()).unwrap_or("Unknown");
        let instance_id_for_error = instance_id.to_string();

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
                let detailed_error = if error.is_empty() || error == "Unknown error" {
                    format!("SSM command failed with status: {}\n\nTo resolve:\n  1. Check instance logs: aws ec2 get-console-output --instance-id {}\n  2. Verify SSM agent is running: aws ssm describe-instance-information --filters 'Key=InstanceIds,Values={}'\n  3. Try command manually via AWS Console SSM Session Manager", status, instance_id_for_error, instance_id_for_error)
                } else {
                    format!("SSM command failed: {}\n\nTo resolve:\n  1. Review error message above\n  2. Check instance logs: aws ec2 get-console-output --instance-id {}\n  3. Verify instance has required permissions and dependencies", error, instance_id_for_error)
                };
                return Err(TrainctlError::Ssm(detailed_error));
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
        "SSM command timed out after {} attempts ({} seconds).\n\n\
        To resolve:\n\
          1. Check if instance is still running: runctl resources list --platform aws\n\
          2. Verify SSM connectivity: aws ssm describe-instance-information --filters 'Key=InstanceIds,Values={}'\n\
          3. Check instance logs: aws ec2 get-console-output --instance-id {}\n\
          4. The command may still be running - check AWS Console SSM Run Command history",
        max_attempts,
        max_attempts as u64 * SSM_COMMAND_MAX_DELAY_SECS,
        instance_id,
        instance_id
    )))
}

/// Wait for instance to reach running state
///
/// Polls EC2 API until instance is running and SSM is ready.
/// If aws_config is provided and instance has IAM profile, verifies SSM connectivity.
pub async fn wait_for_instance_running(
    client: &Ec2Client,
    instance_id: &str,
    aws_config: Option<&aws_config::SdkConfig>,
) -> Result<()> {
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
                // Check if instance has IAM profile (required for SSM)
                let has_iam_profile = instance.iam_instance_profile().is_some();

                if has_iam_profile {
                    if let Some(config) = aws_config {
                        pb.set_message("Instance running, verifying SSM connectivity...");
                        // Verify SSM is actually ready by attempting a simple command
                        // This is more reliable than just waiting a fixed time
                        let ssm_client = SsmClient::new(config);
                        let test_command = "echo 'SSM_READY'";

                        // Try SSM command with retries (SSM may take 30-90 seconds to be ready)
                        let mut ssm_attempts = 0;
                        let max_ssm_attempts = 20; // 20 attempts * 3 seconds = 60 seconds max
                        loop {
                            match ssm_client
                                .send_command()
                                .instance_ids(instance_id)
                                .document_name("AWS-RunShellScript")
                                .parameters("commands", vec![test_command.to_string()])
                                .send()
                                .await
                            {
                                Ok(_) => {
                                    // SSM command accepted - SSM is ready
                                    pb.finish_with_message("Instance ready and SSM connected");
                                    return Ok(());
                                }
                                Err(_e) => {
                                    ssm_attempts += 1;
                                    if ssm_attempts >= max_ssm_attempts {
                                        // SSM not ready after max attempts
                                        warn!("SSM not ready after {} attempts, but instance is running", max_ssm_attempts);
                                        pb.finish_with_message(
                                            "Instance running (SSM may not be ready yet)",
                                        );
                                        // Don't fail - instance is running, SSM may become ready later
                                        return Ok(());
                                    }
                                    // Wait and retry
                                    sleep(Duration::from_secs(3)).await;
                                    pb.set_message(format!(
                                        "Waiting for SSM... (attempt {}/{})",
                                        ssm_attempts + 1,
                                        max_ssm_attempts
                                    ));
                                }
                            }
                        }
                    } else {
                        // No aws_config provided - can't verify SSM, just assume ready
                        pb.finish_with_message("Instance ready (SSM verification skipped)");
                        return Ok(());
                    }
                } else {
                    // No IAM profile - SSM won't work, instance is ready for SSH
                    pb.finish_with_message("Instance ready (SSM not available, use SSH)");
                    return Ok(());
                }
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
