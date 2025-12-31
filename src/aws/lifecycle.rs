//! Unified lifecycle management for EC2 instances
//!
//! Provides consistent lifecycle management for both spot and on-demand instances,
//! including checkpoint saving, state persistence, and resume capability.

use crate::aws_utils::execute_ssm_command;
use crate::error::{Result, TrainctlError};
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

/// Instance lifecycle state
///
/// This enum is defined for future use in tracking instance state transitions.
/// Currently, state is tracked implicitly through EC2 instance state and training metadata.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    /// Instance is being created
    Pending,
    /// Instance is running, training may or may not be active
    Running { training_active: bool },
    /// Instance is stopping (graceful shutdown in progress)
    Stopping { reason: StopReason },
    /// Instance is stopped (can be restarted)
    Stopped,
    /// Instance was interrupted (spot interruption)
    Interrupted { checkpoint_saved: bool },
    /// Instance is terminated
    Terminated,
}

/// Reason for stopping an instance
///
/// This enum is defined for future use in tracking stop reasons.
/// Currently, stop reasons are tracked implicitly through training metadata.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Manual stop by user
    Manual,
    /// Auto-stop after training completion
    AutoStop,
    /// Spot interruption
    SpotInterruption,
    /// System shutdown
    System,
}

/// Training metadata stored in instance tags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetadata {
    /// Training script path
    pub script_path: PathBuf,
    /// Script arguments
    pub script_args: Vec<String>,
    /// Checkpoint directory on instance
    pub checkpoint_dir: PathBuf,
    /// Latest checkpoint path (if available)
    pub last_checkpoint: Option<PathBuf>,
    /// S3 bucket for checkpoints (if configured)
    pub s3_bucket: Option<String>,
    /// S3 prefix for checkpoints
    pub s3_prefix: Option<String>,
    /// Project directory
    pub project_dir: String,
    /// Hyperparameters (if provided)
    pub hyperparams: Option<String>,
}

/// Save checkpoint before instance stop/terminate
///
/// This function:
/// 1. Sends SIGTERM to training process (if running)
/// 2. Waits for graceful shutdown
/// 3. Finds latest checkpoint
/// 4. Uploads checkpoint to S3 (if configured)
/// 5. Stores checkpoint location in instance tags
pub async fn save_checkpoint_before_stop(
    instance_id: &str,
    checkpoint_dir: &str,
    s3_bucket: Option<&str>,
    s3_prefix: Option<&str>,
    graceful_shutdown_timeout: Duration,
    ssm_client: &SsmClient,
    s3_client: Option<&S3Client>,
    ec2_client: &Ec2Client,
) -> Result<Option<String>> {
    let _ = ec2_client; // Used for metadata updates below
    info!("Saving checkpoint before stop/terminate for instance {}", instance_id);

    // Step 1: Graceful shutdown and checkpoint save
    let save_checkpoint_cmd = format!(
        r#"
# Check if training is running and save checkpoint
if [ -f {}/training.pid ]; then
    PID=$(cat {}/training.pid 2>/dev/null)
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

# Find latest checkpoint (supports .pt, .ckpt, .pth, .pkl, .json, .safetensors)
# Use portable find command (works on both GNU and BSD find)
if [ -d "{}" ]; then
    # Try GNU find first (with -printf), fallback to portable method
    LATEST_CHECKPOINT=$(find {} -type f \( -name "*.pt" -o -name "*.ckpt" -o -name "*.pth" -o -name "*.pkl" -o -name "*.json" -o -name "*.safetensors" \) -printf '%T@ %p\n' 2>/dev/null | sort -n | tail -1 | cut -d' ' -f2-)
    
    # Fallback: if GNU find failed or no result, use ls -t (works everywhere)
    if [ -z "$LATEST_CHECKPOINT" ]; then
        for ext in pt ckpt pth pkl json safetensors; do
            CHECKPOINT=$(ls -t {}/*.$ext 2>/dev/null | head -1)
            if [ -n "$CHECKPOINT" ]; then
                LATEST_CHECKPOINT=$CHECKPOINT
                break
            fi
        done
    fi
    
    if [ -n "$LATEST_CHECKPOINT" ] && [ -f "$LATEST_CHECKPOINT" ]; then
        echo "CHECKPOINT_SAVED:$LATEST_CHECKPOINT"
    else
        echo "NO_CHECKPOINT"
    fi
else
    echo "NO_CHECKPOINT_DIR"
fi
"#,
        checkpoint_dir,
        checkpoint_dir,
        graceful_shutdown_timeout.as_secs(),
        graceful_shutdown_timeout.as_secs(),
        checkpoint_dir,
        checkpoint_dir,
        checkpoint_dir,
    );

    match execute_ssm_command(ssm_client, instance_id, &save_checkpoint_cmd).await {
        Ok(output) => {
            info!("Checkpoint save output: {}", output);

            // Extract checkpoint path if saved
            // Handle paths that may contain spaces or special characters
            let checkpoint_path = output
                .lines()
                .find(|l| l.starts_with("CHECKPOINT_SAVED:"))
                .and_then(|l| {
                    let path = l.strip_prefix("CHECKPOINT_SAVED:")?.trim();
                    if path.is_empty() {
                        None
                    } else {
                        Some(path.to_string())
                    }
                });

            // Step 2: Upload checkpoint to S3 if configured
            let checkpoint_s3_path = if let (Some(bucket), Some(path)) = (s3_bucket, checkpoint_path.as_ref()) {
                if let Some(_client) = s3_client {
                    // Upload checkpoint to S3
                    let s3_key = if let Some(p) = s3_prefix {
                        format!("{}/{}/{}", p, instance_id, path)
                    } else {
                        format!("checkpoints/{}/{}", instance_id, path)
                    };

                    // Use SSM to execute AWS CLI command on instance to upload
                    let upload_cmd = format!(
                        r#"
if command -v aws >/dev/null 2>&1; then
    aws s3 cp "{}" "s3://{}/{}" 2>&1 && echo "UPLOAD_SUCCESS:s3://{}/{}" || echo "UPLOAD_FAILED"
else
    echo "AWS_CLI_NOT_AVAILABLE"
fi
"#,
                        path, bucket, s3_key, bucket, s3_key
                    );

                    match execute_ssm_command(ssm_client, instance_id, &upload_cmd).await {
                        Ok(upload_output) => {
                            if upload_output.contains("UPLOAD_SUCCESS") {
                                let s3_path = format!("s3://{}/{}", bucket, s3_key);
                                info!("Checkpoint uploaded to S3: {}", s3_path);
                                Some(s3_path)
                            } else {
                                warn!("Failed to upload checkpoint to S3: {}", upload_output);
                                None
                            }
                        }
                        Err(e) => {
                            warn!("Failed to execute checkpoint upload: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Update training metadata with checkpoint location if we found one
            if let Some(checkpoint_location) = checkpoint_s3_path.as_ref().or(checkpoint_path.as_ref()) {
                if let Ok(Some(mut metadata)) = get_training_metadata(instance_id, ec2_client).await {
                    metadata.last_checkpoint = Some(PathBuf::from(checkpoint_location));
                    if let Err(e) = store_training_metadata(instance_id, &metadata, ec2_client).await {
                        warn!("Failed to update training metadata with checkpoint location: {}", e);
                    } else {
                        info!("Updated training metadata with checkpoint: {}", checkpoint_location);
                    }
                }
            }

            Ok(checkpoint_s3_path)
        }
        Err(e) => {
            warn!("Failed to save checkpoint: {}", e);
            Ok(None)
        }
    }
}

/// Store training metadata in instance tags
///
/// AWS tags have a 256 character limit per value. For large metadata, we split
/// the base64-encoded JSON across multiple tags: `runctl:training_metadata:0`,
/// `runctl:training_metadata:1`, etc.
pub async fn store_training_metadata(
    instance_id: &str,
    metadata: &TrainingMetadata,
    ec2_client: &Ec2Client,
) -> Result<()> {
    let metadata_json = serde_json::to_string(metadata)
        .map_err(|e| TrainctlError::Aws(format!("Failed to serialize metadata: {}", e)))?;

    // Store as base64-encoded JSON in tag (AWS tags have 256 char limit, so we encode)
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&metadata_json);

    // AWS tag value limit is 256 characters
    const TAG_VALUE_LIMIT: usize = 256;
    
    if encoded.len() <= TAG_VALUE_LIMIT {
        // Single tag is sufficient
        ec2_client
            .create_tags()
            .resources(instance_id)
            .tags(
                aws_sdk_ec2::types::Tag::builder()
                    .key("runctl:training_metadata")
                    .value(&encoded)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to store training metadata: {}", e)))?;
    } else {
        // Split across multiple tags
        let chunks: Vec<String> = encoded
            .as_bytes()
            .chunks(TAG_VALUE_LIMIT)
            .map(|chunk| {
                std::str::from_utf8(chunk)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| String::new())
            })
            .collect();
        
        let mut tag_builder = ec2_client
            .create_tags()
            .resources(instance_id);
        
        // Add chunk tags
        for (idx, chunk) in chunks.iter().enumerate() {
            tag_builder = tag_builder.tags(
                aws_sdk_ec2::types::Tag::builder()
                    .key(format!("runctl:training_metadata:{}", idx))
                    .value(chunk)
                    .build(),
            );
        }
        
        // Also store chunk count in main tag for easy retrieval
        tag_builder = tag_builder.tags(
            aws_sdk_ec2::types::Tag::builder()
                .key("runctl:training_metadata:count")
                .value(chunks.len().to_string())
                .build(),
        );

        tag_builder
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to store training metadata: {}", e)))?;
        
        info!("Stored training metadata across {} tags for instance {}", chunks.len(), instance_id);
    }

    info!("Stored training metadata for instance {}", instance_id);
    Ok(())
}

/// Retrieve training metadata from instance tags
pub async fn get_training_metadata(
    instance_id: &str,
    ec2_client: &Ec2Client,
) -> Result<Option<TrainingMetadata>> {
    let response = ec2_client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&response, instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance {} not found", instance_id)))?;

    // Find training metadata tag(s)
    // Check if we have chunked metadata (multiple tags)
    let chunk_count_tag = instance
        .tags()
        .iter()
        .find(|t| t.key().map(|k| k == "runctl:training_metadata:count").unwrap_or(false));
    
    let encoded = if let Some(count_tag) = chunk_count_tag {
        // Multi-tag metadata: reconstruct from chunks
        if let Some(count_str) = count_tag.value() {
            if let Ok(count) = count_str.parse::<usize>() {
                let mut chunks = Vec::new();
                for idx in 0..count {
                    if let Some(tag) = instance
                        .tags()
                        .iter()
                        .find(|t| t.key().map(|k| k == &format!("runctl:training_metadata:{}", idx)).unwrap_or(false))
                    {
                        if let Some(chunk) = tag.value() {
                            chunks.push(chunk);
                        }
                    }
                }
                if chunks.len() == count {
                    Some(chunks.join(""))
                } else {
                    warn!("Incomplete metadata chunks for instance {}: expected {}, got {}", instance_id, count, chunks.len());
                    None
                }
            } else {
                warn!("Invalid chunk count in metadata tag for instance {}", instance_id);
                None
            }
        } else {
            None
        }
    } else {
        // Single tag metadata
        instance
            .tags()
            .iter()
            .find(|t| t.key().map(|k| k == "runctl:training_metadata").unwrap_or(false))
            .and_then(|t| t.value().map(|s| s.to_string()))
    };

    if let Some(encoded_metadata) = encoded {
        use base64::Engine;
        match base64::engine::general_purpose::STANDARD.decode(&encoded_metadata) {
            Ok(decoded) => {
                match String::from_utf8(decoded) {
                    Ok(json_str) => {
                        match serde_json::from_str::<TrainingMetadata>(&json_str) {
                            Ok(metadata) => {
                                info!("Retrieved training metadata for instance {}", instance_id);
                                return Ok(Some(metadata));
                            }
                            Err(e) => {
                                warn!("Failed to parse training metadata: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to decode training metadata UTF-8: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to base64 decode training metadata: {}", e);
            }
        }
    }

    Ok(None)
}

/// Check if instance has checkpoint to resume from
pub async fn get_resume_checkpoint(
    instance_id: &str,
    ec2_client: &Ec2Client,
    s3_client: Option<&S3Client>,
) -> Result<Option<String>> {
    // First, try to get checkpoint from training metadata
    if let Some(metadata) = get_training_metadata(instance_id, ec2_client).await? {
        // Check S3 first (most reliable)
        if let (Some(bucket), Some(prefix)) = (metadata.s3_bucket.as_ref(), metadata.s3_prefix.as_ref()) {
            if let Some(client) = s3_client {
                // List checkpoints in S3
                let s3_prefix = format!("{}/{}/", prefix, instance_id);
                match find_latest_checkpoint_in_s3(client, bucket, &s3_prefix).await {
                    Ok(Some(checkpoint)) => {
                        info!("Found resume checkpoint in S3: {}", checkpoint);
                        return Ok(Some(checkpoint));
                    }
                    Ok(None) => {
                        // No checkpoint in S3, check if we have a local path
                        if let Some(local_path) = &metadata.last_checkpoint {
                            // If instance is stopped, checkpoint might be on EBS volume
                            // For now, return the local path (caller can handle)
                            return Ok(Some(local_path.to_string_lossy().to_string()));
                        }
                    }
                    Err(e) => {
                        warn!("Failed to find checkpoint in S3: {}", e);
                    }
                }
            }
        } else if let Some(local_path) = &metadata.last_checkpoint {
            // No S3, but we have local checkpoint path
            return Ok(Some(local_path.to_string_lossy().to_string()));
        }
    }

    Ok(None)
}

/// Find latest checkpoint in S3
async fn find_latest_checkpoint_in_s3(
    s3_client: &S3Client,
    bucket: &str,
    prefix: &str,
) -> Result<Option<String>> {
    let response = s3_client
        .list_objects_v2()
        .bucket(bucket)
        .prefix(prefix)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list S3 objects: {}", e)))?;

    let mut checkpoints: Vec<(String, chrono::DateTime<chrono::Utc>)> = Vec::new();

    for obj in response.contents() {
        if let Some(key) = obj.key() {
            // Check for checkpoint file extensions (including safetensors)
            if key.ends_with(".pt") || key.ends_with(".ckpt") || key.ends_with(".pth") 
                || key.ends_with(".pkl") || key.ends_with(".json") || key.ends_with(".safetensors") {
                if let Some(last_modified) = obj.last_modified() {
                    let dt = chrono::DateTime::from_timestamp(
                        last_modified.secs(),
                        last_modified.subsec_nanos(),
                    )
                    .ok_or_else(|| {
                        TrainctlError::Aws("Invalid timestamp in S3 object".to_string())
                    })?;
                    checkpoints.push((format!("s3://{}/{}", bucket, key), dt));
                }
            }
        }
    }

    // Sort by modification time (newest first)
    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(checkpoints.first().map(|(path, _)| path.clone()))
}

