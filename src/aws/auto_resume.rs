//! Auto-resume training after spot instance interruption
//!
//! When a spot instance is interrupted, this module can automatically:
//! 1. Find the latest checkpoint (from S3 or EBS)
//! 2. Create a new spot instance
//! 3. Resume training from the checkpoint

use crate::aws::instance::create_instance;
use crate::aws::types::{CreateInstanceOptions, TrainInstanceOptions};
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_config::SdkConfig;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use std::path::PathBuf;
use tracing::{info, warn};

/// Resume training on a new instance after spot interruption
///
/// This function:
/// 1. Finds the latest checkpoint from S3 or previous instance
/// 2. Creates a new spot instance
/// 3. Syncs code and resumes training from checkpoint
///
/// # Arguments
///
/// * `original_instance_id`: The instance that was interrupted
/// * `checkpoint_s3_path`: Optional S3 path where checkpoint was saved
/// * `script_path`: Training script to resume
/// * `config`: Configuration
/// * `aws_config`: AWS SDK configuration
/// Returns: (new_instance_id, train_options) - caller should start training
/// NOTE: Currently used by CLI command handler
pub async fn prepare_auto_resume(
    original_instance_id: &str,
    checkpoint_s3_path: Option<&str>,
    script_path: PathBuf,
    config: &Config,
    aws_config: &SdkConfig,
) -> Result<(String, TrainInstanceOptions)> {
    info!(
        "Auto-resuming training after interruption of instance {}",
        original_instance_id
    );

    let aws_cfg = config.aws.as_ref().ok_or_else(|| {
        TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
    })?;

    // Step 1: Find latest checkpoint
    let checkpoint_path = if let Some(s3_path) = checkpoint_s3_path {
        info!("Checkpoint should be at: {}", s3_path);
        // In a full implementation, we would download and verify the checkpoint
        // For now, we'll pass the S3 path to the training script
        Some(s3_path.to_string())
    } else {
        // Try to find checkpoint from S3 bucket if configured
        if let Some(bucket) = &aws_cfg.s3_bucket {
            let s3_client = aws_sdk_s3::Client::new(aws_config);
            let prefix = format!("checkpoints/spot-interruptions/{}/", original_instance_id);
            
            // List objects in S3 to find latest checkpoint
            match find_latest_checkpoint_in_s3(&s3_client, bucket, &prefix).await {
                Ok(Some(checkpoint)) => {
                    info!("Found latest checkpoint in S3: {}", checkpoint);
                    Some(checkpoint)
                }
                Ok(None) => {
                    warn!("No checkpoint found in S3, will start from beginning");
                    None
                }
                Err(e) => {
                    warn!("Failed to find checkpoint in S3: {}, will start from beginning", e);
                    None
                }
            }
        } else {
            warn!("No S3 bucket configured, cannot find checkpoint. Will start from beginning.");
            None
        }
    };

    // Step 2: Create new spot instance
    info!("Creating new spot instance to resume training...");
    
    let create_options = CreateInstanceOptions {
        wait: true,
        instance_type: aws_cfg.default_instance_type.clone(),
        use_spot: true, // Always use spot for auto-resume
        spot_max_price: aws_cfg.spot_max_price.clone(),
        no_fallback: false,
        key_name: None,
        security_group: None,
        ami_id: None,
        root_volume_size: None,
        data_volume_size: None,
        project_name: "runctl-auto-resume".to_string(),
        iam_instance_profile: aws_cfg.iam_instance_profile.clone(),
    };

    // Create instance (this will print instance ID)
    create_instance(create_options, config, aws_config, "text").await?;

    // Extract instance ID from output (simplified - in production, return from create_instance)
    // For now, we'll need to query for the most recent instance
    let ec2_client = Ec2Client::new(aws_config);
    let new_instance_id = find_newest_spot_instance(&ec2_client, original_instance_id).await?;

    info!("Created new instance: {}", new_instance_id);

    // Step 3: Resume training
    let mut script_args = vec![];
    if let Some(checkpoint) = checkpoint_path {
        script_args.push("--resume".to_string());
        script_args.push(checkpoint);
    }

    let train_options = TrainInstanceOptions {
        instance_id: new_instance_id.clone(),
        script: script_path,
        data_s3: None,
        output_s3: None,
        sync_code: true,
        include_patterns: vec![],
        project_name: "runctl-auto-resume".to_string(),
        script_args,
        wait: true,
        timeout_minutes: 120,
        docker: false,
        docker_image: None,
    };

    // Return the instance ID and training options instead of starting training
    // The caller will start training, breaking the circular dependency
    Ok((new_instance_id, train_options))
}

/// Handle auto-resume CLI command
///
/// This is called when `runctl aws auto-resume` is invoked.
/// It creates a new instance and resumes training from the checkpoint.
pub async fn handle_auto_resume_command(
    original_instance_id: String,
    script: PathBuf,
    checkpoint: Option<String>,
    config: &Config,
    aws_config: &SdkConfig,
    output_format: &str,
) -> Result<()> {
    info!("Auto-resuming training after interruption of instance {}", original_instance_id);
    
    // Use prepare_auto_resume to get instance and training options
    let (new_instance_id, train_options) = prepare_auto_resume(
        &original_instance_id,
        checkpoint.as_deref(),
        script,
        config,
        aws_config,
    )
    .await?;
    
    if output_format != "json" {
        println!("Created new instance: {}", new_instance_id);
        println!("Starting training on new instance...");
    }
    
    // Start training on the new instance
    crate::aws::training::train_on_instance(
        train_options,
        config,
        aws_config,
        output_format,
    )
    .await?;
    
    if output_format != "json" {
        println!("Training resumed successfully on instance: {}", new_instance_id);
    }
    
    Ok(())
}

/// Find the latest checkpoint in S3
/// NOTE: Currently used by prepare_auto_resume
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
            if key.ends_with(".pt") || key.ends_with(".ckpt") {
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

/// Find the newest spot instance (for auto-resume)
/// NOTE: Currently used by prepare_auto_resume
async fn find_newest_spot_instance(
    ec2_client: &Ec2Client,
    exclude_instance_id: &str,
) -> Result<String> {
    let response = ec2_client
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

    let mut instances: Vec<(String, chrono::DateTime<chrono::Utc>)> = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                if instance_id == exclude_instance_id {
                    continue; // Skip the interrupted instance
                }

                // Check if it's a spot instance
                if instance.spot_instance_request_id().is_some() {
                    if let Some(launch_time) = instance.launch_time() {
                        let dt = chrono::DateTime::from_timestamp(launch_time.secs(), 0)
                            .ok_or_else(|| {
                                TrainctlError::Aws("Invalid launch time".to_string())
                            })?;
                        instances.push((instance_id.to_string(), dt));
                    }
                }
            }
        }
    }

    // Sort by launch time (newest first)
    instances.sort_by(|a, b| b.1.cmp(&a.1));

    instances
        .first()
        .map(|(id, _)| id.clone())
        .ok_or_else(|| {
            TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "No new spot instance found after creation".to_string(),
                source: None,
            }
        })
}
