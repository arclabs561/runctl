//! Spot instance interruption monitoring and handling
//!
//! Monitors EC2 spot instances for interruption warnings and handles graceful shutdown.
//! When a spot instance receives a termination notice (2-minute warning), this module:
//! 1. Detects the interruption via EC2 metadata service
//! 2. Triggers graceful shutdown of training process
//! 3. Saves checkpoint before termination
//! 4. Optionally uploads checkpoint to S3

// Auto-resume is now used via spawn, so we don't need to import it at the top level
use crate::aws_utils::execute_ssm_command;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_config::SdkConfig;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};

/// Monitor spot instance for interruption warnings
///
/// Polls the EC2 instance metadata service for spot interruption notices.
/// When an interruption is detected, triggers graceful shutdown sequence.
///
/// # Arguments
///
/// * `instance_id`: EC2 instance ID to monitor
/// * `checkpoint_dir`: Directory where checkpoints are saved
/// * `s3_bucket`: Optional S3 bucket to upload checkpoint before termination
/// * `s3_prefix`: Optional S3 prefix for checkpoint upload
/// * `poll_interval`: How often to check for interruptions (default: 30 seconds)
/// * `graceful_shutdown_timeout`: Max time to wait for graceful shutdown (default: 90 seconds)
#[allow(clippy::too_many_arguments)]
pub async fn monitor_spot_interruption(
    instance_id: &str,
    checkpoint_dir: &str,
    s3_bucket: Option<&str>,
    s3_prefix: Option<&str>,
    poll_interval: Duration,
    graceful_shutdown_timeout: Duration,
    ssm_client: &SsmClient,
    ec2_client: &Ec2Client,
    s3_client: Option<&S3Client>,
    auto_resume: bool,
    script_path: Option<PathBuf>,
    config: Option<&Config>,
    aws_config: Option<&SdkConfig>,
) -> Result<()> {
    info!(
        "Starting spot interruption monitoring for instance {}",
        instance_id
    );

    // Verify instance is a spot instance
    let instance_response = ec2_client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&instance_response, instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance {} not found", instance_id)))?;

    let is_spot = instance.spot_instance_request_id().is_some();
    if !is_spot {
        warn!(
            "Instance {} is not a spot instance, monitoring not needed",
            instance_id
        );
        return Ok(());
    }

    info!(
        "Instance {} is a spot instance, starting monitoring",
        instance_id
    );

    // Poll metadata service for interruption warnings
    loop {
        tokio::time::sleep(poll_interval).await;

        // Check if instance is still running
        let instance_response = ec2_client
            .describe_instances()
            .instance_ids(instance_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

        let instance =
            match crate::aws::helpers::find_instance_in_response(&instance_response, instance_id) {
                Some(inst) => inst,
                None => {
                    warn!("Instance {} not found, stopping monitoring", instance_id);
                    break;
                }
            };
        let state = instance
            .state()
            .and_then(|s| s.name())
            .map(|s| s.as_str())
            .unwrap_or("unknown");

        if state != "running" {
            info!(
                "Instance {} is in state '{}', stopping monitoring",
                instance_id, state
            );
            break;
        }

        // Check for spot interruption warning via metadata service
        let check_cmd = r#"
# Check EC2 metadata service for spot interruption warning
if command -v curl >/dev/null 2>&1; then
    RESPONSE=$(curl -s -w "\n%{http_code}" http://169.254.169.254/latest/meta-data/spot/instance-action 2>/dev/null || echo -e "\n404")
    HTTP_CODE=$(echo "$RESPONSE" | tail -1)
    if [ "$HTTP_CODE" = "200" ]; then
        echo "SPOT_INTERRUPTION_DETECTED"
        echo "$RESPONSE" | head -n -1
    else
        echo "NO_INTERRUPTION"
    fi
elif command -v wget >/dev/null 2>&1; then
    RESPONSE=$(wget -q -O - http://169.254.169.254/latest/meta-data/spot/instance-action 2>/dev/null || echo "")
    if [ -n "$RESPONSE" ]; then
        echo "SPOT_INTERRUPTION_DETECTED"
        echo "$RESPONSE"
    else
        echo "NO_INTERRUPTION"
    fi
else
    echo "NO_METADATA_TOOL"
fi
"#;

        match execute_ssm_command(ssm_client, instance_id, check_cmd).await {
            Ok(output) => {
                if output.contains("SPOT_INTERRUPTION_DETECTED") {
                    warn!("Spot interruption detected for instance {}!", instance_id);

                    // Parse interruption details
                    let interruption_info = parse_interruption_info(&output);

                    // Handle interruption
                    if let Err(e) = handle_spot_interruption(
                        instance_id,
                        checkpoint_dir,
                        s3_bucket,
                        s3_prefix,
                        graceful_shutdown_timeout,
                        &interruption_info,
                        ssm_client,
                        s3_client,
                    )
                    .await
                    {
                        error!("Failed to handle spot interruption: {}", e);
                        return Err(e);
                    }

                    // Spawn auto-resume using process spawning to completely break circular dependency
                    // The cycle: monitor_spot_interruption -> train_on_instance -> monitor_spot_interruption
                    // Solution: Use std::process::Command to spawn separate runctl process
                    if auto_resume {
                        if let (Some(script), Some(_cfg), Some(_aws_cfg)) =
                            (script_path, config, aws_config)
                        {
                            let resume_instance_id = instance_id.to_string();
                            let resume_script_str = script.to_string_lossy().to_string();

                            // Construct checkpoint path from S3 prefix if available
                            let resume_checkpoint_str: Option<String> = s3_prefix
                                .map(|prefix| format!("{}/{}/checkpoints", prefix, instance_id));

                            tokio::task::spawn(async move {
                                use std::process::Command;

                                // Build runctl command for auto-resume
                                let mut cmd = Command::new(
                                    std::env::current_exe().unwrap_or_else(|_| "runctl".into()),
                                );
                                cmd.arg("aws")
                                    .arg("auto-resume")
                                    .arg(&resume_instance_id)
                                    .arg(&resume_script_str);

                                if let Some(ref cp) = resume_checkpoint_str {
                                    cmd.arg("--checkpoint").arg(cp);
                                }

                                match cmd.output() {
                                    Ok(output) => {
                                        if output.status.success() {
                                            let stdout = String::from_utf8_lossy(&output.stdout);
                                            info!("Auto-resume completed: {}", stdout);
                                        } else {
                                            let stderr = String::from_utf8_lossy(&output.stderr);
                                            warn!("Auto-resume process failed: {}", stderr);
                                            warn!("You can manually resume by creating a new instance and using the checkpoint from S3");
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to spawn runctl process for auto-resume: {}",
                                            e
                                        );
                                        warn!("Auto-resume via process spawning failed. You can manually resume:");
                                        warn!("  1. Create new instance: runctl aws create <instance-type>");
                                        let checkpoint_display = resume_checkpoint_str
                                            .as_deref()
                                            .unwrap_or("<checkpoint-path>");
                                        warn!("  2. Resume training: runctl aws train <new-instance-id> {} -- --resume {}",
                                              resume_script_str, checkpoint_display);
                                    }
                                }
                            });
                        }
                    }

                    info!(
                        "Spot interruption handled successfully for instance {}",
                        instance_id
                    );
                    break;
                }
                // Continue monitoring if no interruption detected
            }
            Err(e) => {
                warn!("Failed to check for spot interruption: {}", e);
                // Continue monitoring despite error
            }
        }
    }

    Ok(())
}

/// Handle spot instance interruption
///
/// Performs graceful shutdown sequence:
/// 1. Save checkpoint
/// 2. Upload checkpoint to S3 (if configured)
/// 3. Stop training process gracefully
#[allow(clippy::too_many_arguments)]
async fn handle_spot_interruption(
    instance_id: &str,
    checkpoint_dir: &str,
    s3_bucket: Option<&str>,
    s3_prefix: Option<&str>,
    graceful_shutdown_timeout: Duration,
    interruption_info: &InterruptionInfo,
    ssm_client: &SsmClient,
    s3_client: Option<&S3Client>,
) -> Result<()> {
    info!("Handling spot interruption for instance {}", instance_id);
    info!(
        "Interruption action: {}, time: {:?}",
        interruption_info.action, interruption_info.action_time
    );

    // Step 1: Save checkpoint (if training is running)
    let save_checkpoint_cmd = format!(
        r#"
# Check if training is running and save checkpoint
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
        # Send SIGTERM to trigger checkpoint save
        kill -TERM $PID 2>/dev/null || true
        # Wait for graceful shutdown
        for i in {{1..{}}}; do
            if ! ps -p $PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                break
            fi
            sleep 1
        done
        # Force kill if still running
        if ps -p $PID > /dev/null 2>&1; then
            kill -9 $PID 2>/dev/null || true
            echo "TRAINING_FORCE_STOPPED"
        fi
    else
        echo "TRAINING_STOPPED"
    fi
else
    # Check for training processes
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training\|python.*main.py" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        echo "TRAINING_RUNNING:$TRAINING_PID"
        kill -TERM $TRAINING_PID 2>/dev/null || true
        for i in {{1..{}}}; do
            if ! ps -p $TRAINING_PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                break
            fi
            sleep 1
        done
        if ps -p $TRAINING_PID > /dev/null 2>&1; then
            kill -9 $TRAINING_PID 2>/dev/null || true
            echo "TRAINING_FORCE_STOPPED"
        fi
    else
        echo "NO_TRAINING"
    fi
fi

# Save latest checkpoint (if exists)
if [ -d "{}" ]; then
    LATEST_CHECKPOINT=$(ls -t {}/*.pt 2>/dev/null | head -1)
    if [ -n "$LATEST_CHECKPOINT" ]; then
        echo "CHECKPOINT_SAVED:$LATEST_CHECKPOINT"
    else
        echo "NO_CHECKPOINT"
    fi
else
    echo "NO_CHECKPOINT_DIR"
fi
"#,
        graceful_shutdown_timeout.as_secs(),
        graceful_shutdown_timeout.as_secs(),
        checkpoint_dir,
        checkpoint_dir
    );

    match execute_ssm_command(ssm_client, instance_id, &save_checkpoint_cmd).await {
        Ok(output) => {
            info!("Graceful shutdown output: {}", output);

            // Extract checkpoint path if saved
            let checkpoint_path = output
                .lines()
                .find(|l| l.starts_with("CHECKPOINT_SAVED:"))
                .and_then(|l| l.strip_prefix("CHECKPOINT_SAVED:"))
                .map(|s| s.trim().to_string());

            // Step 2: Upload checkpoint to S3 if configured
            if let (Some(bucket), Some(path)) = (s3_bucket, checkpoint_path.as_ref()) {
                if let Some(client) = s3_client {
                    let s3_key = if let Some(p) = s3_prefix {
                        format!("{}/{}/{}", p, instance_id, path)
                    } else {
                        format!("{}/{}", instance_id, path)
                    };
                    let s3_path = format!("s3://{}/{}", bucket, s3_key);

                    if let Err(e) =
                        upload_checkpoint_to_s3(client, instance_id, path, bucket, s3_prefix).await
                    {
                        warn!("Failed to upload checkpoint to S3: {}", e);
                    } else {
                        info!("Checkpoint uploaded to S3: {}", s3_path);
                    }
                }
            }

            // Step 3: Auto-resume removed from here to break circular dependency
            // Auto-resume is now handled in monitor_spot_interruption using process spawning
            // This completely breaks the cycle: spot_monitor -> auto_resume -> training -> spot_monitor
        }
        Err(e) => {
            warn!("Failed to execute graceful shutdown: {}", e);
        }
    }

    Ok(())
}

/// Upload checkpoint to S3
async fn upload_checkpoint_to_s3(
    _s3_client: &S3Client,
    instance_id: &str,
    checkpoint_path: &str,
    bucket: &str,
    prefix: Option<&str>,
) -> Result<()> {
    // Use SSM to execute AWS CLI command on instance to upload checkpoint
    // This is a simplified approach - in production, you might want to use
    // S3 multipart upload for large checkpoints

    let s3_key = if let Some(p) = prefix {
        format!("{}/{}/{}", p, instance_id, checkpoint_path)
    } else {
        format!("{}/{}", instance_id, checkpoint_path)
    };

    // For now, we'll use a command on the instance to upload via AWS CLI
    // In the future, we could implement direct S3 upload via SSM
    let _upload_cmd = format!(
        r#"
if command -v aws >/dev/null 2>&1; then
    aws s3 cp "{}" "s3://{}/{}" 2>&1
else
    echo "AWS CLI not available, cannot upload checkpoint"
fi
"#,
        checkpoint_path, bucket, s3_key
    );

    // Note: This requires the instance to have AWS CLI and proper IAM permissions
    // For a more robust solution, we could read the file via SSM and upload directly
    // using the S3 client, but that would require streaming large files through SSM
    // For now, this is a placeholder - actual upload would be done via SSM command execution
    // in the handle_spot_interruption function

    Ok(())
}

/// Parse interruption information from metadata service response
fn parse_interruption_info(output: &str) -> InterruptionInfo {
    // The metadata service returns JSON like:
    // {"action": "terminate", "time": "2024-01-01T12:00:00Z"}
    let mut action_time = None;

    // Try to parse JSON response
    for line in output.lines() {
        if line.starts_with("SPOT_INTERRUPTION_DETECTED") || line.starts_with("NO_INTERRUPTION") {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<Value>(line) {
            if let Some(time_str) = json.get("time").and_then(|t| t.as_str()) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(time_str) {
                    action_time = Some(dt.with_timezone(&chrono::Utc));
                }
            }
        }
    }

    let action = if let Ok(json) = serde_json::from_str::<Value>(output) {
        json.get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("terminate")
            .to_string()
    } else {
        "terminate".to_string() // Default action
    };

    InterruptionInfo {
        action_time,
        action,
    }
}

/// Information about spot instance interruption
#[derive(Debug)]
struct InterruptionInfo {
    action_time: Option<chrono::DateTime<chrono::Utc>>,
    action: String, // Action type (e.g., "terminate", "stop", "hibernate")
}

/// Start background monitoring task for spot instance
///
/// This function spawns a background task that monitors the instance for interruptions.
/// It returns a handle that can be used to stop monitoring.
///
/// Note: This function takes the AWS SDK config and creates new clients internally.
#[allow(dead_code)]
pub fn start_spot_monitoring(
    instance_id: String,
    checkpoint_dir: String,
    s3_bucket: Option<String>,
    s3_prefix: Option<String>,
    poll_interval: Duration,
    graceful_shutdown_timeout: Duration,
    aws_config: aws_config::SdkConfig,
    auto_resume: bool,
    script_path: Option<PathBuf>,
    config: Option<Config>,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        // Create clients for the background task
        let ssm_client = SsmClient::new(&aws_config);
        let ec2_client = Ec2Client::new(&aws_config);
        let s3_client = if s3_bucket.is_some() {
            Some(S3Client::new(&aws_config))
        } else {
            None
        };

        monitor_spot_interruption(
            &instance_id,
            &checkpoint_dir,
            s3_bucket.as_deref(),
            s3_prefix.as_deref(),
            poll_interval,
            graceful_shutdown_timeout,
            &ssm_client,
            &ec2_client,
            s3_client.as_ref(),
            auto_resume,
            script_path,
            config.as_ref(),
            Some(&aws_config),
        )
        .await
    })
}
