//! Instance lifecycle management
//!
//! Handles creation, starting, stopping, and termination of EC2 instances.
//! Includes spot instance support, AMI detection, and user data generation.

use crate::aws::helpers::{
    ec2_instance_to_resource_status, get_instance_info_json, get_user_id,
    update_resource_status_in_tracker,
};
use crate::aws::types::{
    CreateInstanceOptions, CreateSpotInstanceOptions, StartInstanceResult, StopInstanceResult,
    TerminateInstanceResult,
};
use crate::aws_utils::count_running_instances;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::safe_cleanup::{safe_cleanup, CleanupSafety};
use aws_sdk_ec2::types::InstanceType as Ec2InstanceType;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use base64::Engine;
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tracing::{info, warn};

/// Create an EC2 instance and return the instance ID
///
/// Internal helper that creates an instance and returns just the ID.
/// Used by workflow commands that need the ID for subsequent operations.
#[allow(dead_code)] // Used in workflow module (main.rs only, not in lib)
pub async fn create_instance_and_get_id(
    options: CreateInstanceOptions,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
) -> Result<String> {
    // Call create_instance with instance-id output format to get just the ID
    // We'll need to capture stdout, but that's complex. Instead, let's
    // extract the instance creation logic and return the ID directly.

    // For now, let's create a simplified version that does the core creation
    // and returns the ID. We can refactor create_instance later.

    let client = aws_sdk_ec2::Client::new(aws_config);
    let aws_cfg = config.aws.as_ref().ok_or_else(|| {
        TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
    })?;

    // Safety check
    let running_count = crate::aws_utils::count_running_instances(&client).await?;
    if running_count >= 50 {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!("Too many instances running ({})", running_count),
            source: None,
        });
    }

    // Get AMI (simplified - reuse logic from create_instance)
    let final_ami = if let Some(ami) = &options.ami_id {
        ami.clone()
    } else {
        let is_gpu = options.instance_type.starts_with("g")
            || options.instance_type.starts_with("p")
            || options.instance_type.contains("gpu");
        if is_gpu {
            find_deep_learning_ami(&client, &aws_cfg.region).await?
        } else {
            // Use Amazon Linux 2023
            "ami-0c55b159cbfafe1f0".to_string()
        }
    };

    // Create instance (simplified)
    let instance_id = if options.use_spot {
        // Create spot instance
        let spot_options = CreateSpotInstanceOptions {
            instance_type: options.instance_type.clone(),
            ami_id: final_ami,
            user_data: String::new(), // Simplified
            max_price: options.spot_max_price,
            key_name: options.key_name.clone(),
            security_group: options.security_group.clone(),
            root_volume_size: options.root_volume_size.unwrap_or(30),
            iam_instance_profile: options.iam_instance_profile.clone(),
        };
        create_spot_instance(&client, spot_options, "text").await?
    } else {
        create_ondemand_instance(
            &client,
            &options.instance_type,
            &final_ami,
            "",
            options.key_name.as_deref(),
            options.security_group.as_deref(),
            options.root_volume_size.unwrap_or(30),
            options.iam_instance_profile.as_deref(),
        )
        .await?
    };

    // Tag instance
    if let Err(e) = tag_instance(&client, &instance_id, &options.project_name, config).await {
        warn!("Failed to tag instance {}: {}", instance_id, e);
        // Continue - instance is created, tagging is non-critical
    }

    // Wait if requested
    if options.wait {
        if let Err(e) =
            crate::aws_utils::wait_for_instance_running(&client, &instance_id, Some(aws_config))
                .await
        {
            warn!("Failed to wait for instance ready: {}", e);
            // Continue - instance is created, just may not be ready yet
        }
    }

    Ok(instance_id)
}

/// Create an EC2 instance with the specified options
///
/// Creates a new EC2 instance for ML training with automatic configuration:
/// - Auto-detects Deep Learning AMI for GPU instances
/// - Applies safety limits (blocks if >50 instances running)
/// - Registers instance with ResourceTracker if available
/// - Supports spot instances with automatic fallback to on-demand
///
/// # Safety Features
///
/// - Blocks creation if >50 instances already running (prevents accidental mass creation)
/// - Warns if >10 instances running
/// - Validates instance type and configuration before creation
///
/// # Errors
///
/// Returns `TrainctlError::CloudProvider` if:
/// - Too many instances running (>=50)
/// - Instance type is invalid
/// - AMI not found (for GPU instances)
/// - AWS API errors
pub async fn create_instance(
    options: CreateInstanceOptions,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let aws_cfg = config.aws.as_ref().ok_or_else(|| {
        TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
    })?;

    let client = Ec2Client::new(aws_config);

    // Safety check: Prevent accidental mass creation
    let running_count = count_running_instances(&client).await?;
    if running_count >= 50 {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Too many instances running ({}). Creation blocked to prevent accidental mass creation.\n\n\
                To resolve:\n\
                  1. List running instances: runctl resources list --platform aws\n\
                  2. Terminate unused instances: runctl aws terminate <instance-id>\n\
                  3. Stop instances (preserves data): runctl aws stop <instance-id>\n\
                  4. Use a different AWS account or region\n\n\
                To override this limit, modify the safety check in the code.",
                running_count
            ),
            source: None,
        });
    } else if running_count >= 10 {
        println!(
            "WARNING: {} instances already running. Proceeding with caution.",
            running_count
        );
        println!("  Use 'runctl resources list' to review running instances.");
    }

    info!(
        "Creating EC2 instance: type={}, spot={}",
        options.instance_type, options.use_spot
    );

    // Validate IAM instance profile if provided
    if let Some(ref profile_name) = options.iam_instance_profile {
        if profile_name.trim().is_empty() {
            return Err(TrainctlError::Aws(
                "IAM instance profile name cannot be empty".to_string(),
            ));
        }
        if output_format != "json" {
            info!("Using IAM instance profile: {}", profile_name);
            println!("   Note: Ensure profile has AmazonSSMManagedInstanceCore policy");
            println!("   Verify: aws iam get-instance-profile --instance-profile-name {}", profile_name);
        }
    }

    // Check if IAM instance profile is needed but not provided
    // If no SSH key and no IAM profile, warn user
    if options.iam_instance_profile.is_none()
        && options.key_name.is_none()
        && output_format != "json"
    {
        println!("⚠️  WARNING: No IAM instance profile or SSH key provided.");
        println!("   Training commands will fail without SSM or SSH access.");
        println!("   Recommended: Setup SSM (one-time): ./scripts/setup-ssm-role.sh");
        println!("   Then use: --iam-instance-profile runctl-ssm-profile");
        println!("   Or provide SSH key: --key-name <your-key-name>");
        println!();
    }

    // Validate S3 bucket if using IAM profile (needed for SSM code sync)
    if options.iam_instance_profile.is_some() {
        let has_s3_bucket = config
            .aws
            .as_ref()
            .and_then(|c| c.s3_bucket.as_ref())
            .is_some();
        
        if !has_s3_bucket && output_format != "json" {
            println!("⚠️  WARNING: IAM instance profile provided but S3 bucket not configured.");
            println!("   SSM-based code sync requires an S3 bucket for temporary storage.");
            println!("   To resolve:");
            println!("     1. Add S3 bucket to .runctl.toml:");
            println!("        [aws]");
            println!("        s3_bucket = \"your-bucket-name\"");
            println!();
            println!("     2. Or use SSH instead:");
            println!("        Remove --iam-instance-profile and add --key-name <your-key-name>");
            println!();
            println!("   Note: Training will fail if you try to use --sync-code without S3 bucket.");
            println!();
        } else if has_s3_bucket {
            // Validate S3 bucket exists and is accessible
            if let Some(bucket_name) = config.aws.as_ref().and_then(|c| c.s3_bucket.as_ref()) {
                let s3_client = S3Client::new(aws_config);
                
                // Check if bucket exists and is accessible
                match s3_client
                    .head_bucket()
                    .bucket(bucket_name)
                    .send()
                    .await
                {
                    Ok(_) => {
                        // Bucket exists and is accessible
                        if output_format != "json" {
                            info!("S3 bucket validated: {}", bucket_name);
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("{}", e);
                        if output_format != "json" {
                            println!("⚠️  WARNING: S3 bucket '{}' validation failed: {}", bucket_name, error_msg);
                            println!("   SSM code sync may fail if bucket is not accessible.");
                            println!("   To resolve:");
                            if error_msg.contains("NotFound") || error_msg.contains("NoSuchBucket") {
                                println!("     1. Create the bucket: aws s3 mb s3://{}", bucket_name);
                                println!("     2. Or use a different bucket name");
                            } else if error_msg.contains("AccessDenied") || error_msg.contains("Forbidden") {
                                println!("     1. Check IAM permissions for S3 access");
                                println!("     2. Verify bucket exists: aws s3 ls s3://{}", bucket_name);
                                println!("     3. Check bucket policy allows your IAM user/role");
                            } else {
                                println!("     1. Verify bucket name is correct");
                                println!("     2. Check AWS credentials and permissions");
                                println!("     3. Verify bucket exists: aws s3 ls s3://{}", bucket_name);
                            }
                            println!();
                        } else {
                            // In JSON mode, return error immediately
                            return Err(TrainctlError::Aws(format!(
                                "S3 bucket '{}' validation failed: {}\n\n\
                                SSM code sync requires an accessible S3 bucket.\n\n\
                                To resolve:\n\
                                  1. Verify bucket exists: aws s3 ls s3://{}\n\
                                  2. Check IAM permissions for S3 access\n\
                                  3. Verify bucket name is correct in .runctl.toml",
                                bucket_name, error_msg, bucket_name
                            )));
                        }
                    }
                }
            }
        }
    }

    // Auto-detect AMI if not provided
    let final_ami = if let Some(ami) = &options.ami_id {
        ami.clone()
    } else {
        // Validate instance type format (basic validation - AWS API will validate fully)
        // Instance types follow pattern: [family][generation].[size] (e.g., t3.micro, g4dn.xlarge)
        let instance_type_lower = options.instance_type.to_lowercase();
        let is_valid_format = instance_type_lower.matches('.').count() == 1
            && instance_type_lower.len() >= 5 // Minimum: "t3.x"
            && instance_type_lower.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false);
        
        if !is_valid_format && output_format != "json" {
            warn!(
                "Instance type '{}' may be invalid. Expected format: [family][generation].[size] (e.g., t3.micro)",
                options.instance_type
            );
        }

        // Check if GPU instance (g4dn, p3, p4, etc.)
        let is_gpu = instance_type_lower.starts_with("g")
            || instance_type_lower.starts_with("p")
            || instance_type_lower.contains("gpu");

        if is_gpu {
            // Try to find Deep Learning AMI
            match find_deep_learning_ami(&client, &aws_cfg.region).await {
                Ok(ami) => {
                    println!("   Using Deep Learning AMI: {}", ami);
                    ami
                }
                Err(e) => {
                    println!("WARNING: Could not find Deep Learning AMI: {}", e);
                    println!("   Using default AMI: {}", aws_cfg.default_ami);
                    aws_cfg.default_ami.clone()
                }
            }
        } else {
            aws_cfg.default_ami.clone()
        }
    };

    // Determine root volume size (larger for GPU instances or if specified)
    let root_size = options.root_volume_size.unwrap_or_else(|| {
        if options.instance_type.starts_with("g") || options.instance_type.starts_with("p") {
            50 // GPU instances need more space for CUDA/PyTorch
        } else {
            30 // Default
        }
    });

    // Generate user data script
    let user_data = generate_user_data(&options.project_name, options.data_volume_size.is_some());

    // Try spot instance first if requested
    if options.use_spot {
        let spot_options = CreateSpotInstanceOptions {
            instance_type: options.instance_type.clone(),
            ami_id: final_ami.clone(),
            user_data: user_data.clone(),
            max_price: options.spot_max_price.clone(),
            key_name: options.key_name.clone(),
            security_group: options.security_group.clone(),
            root_volume_size: root_size,
            iam_instance_profile: options.iam_instance_profile.clone(),
        };
        match create_spot_instance(&client, spot_options, output_format).await {
            Ok(instance_id) => {
                if output_format == "json" {
                    let instance_info =
                        get_instance_info_json(&client, &instance_id, &options.instance_type)
                            .await?;
                    println!("{}", serde_json::to_string_pretty(&instance_info)?);
                } else {
                    println!("Created spot instance: {}", instance_id);
                }
                if let Err(e) =
                    tag_instance(&client, &instance_id, &options.project_name, config).await
                {
                    warn!("Failed to tag instance {}: {}", instance_id, e);
                    if output_format != "json" {
                        println!("  Instance created but tagging failed. You can tag manually if needed.");
                    }
                }

                // Auto-attach data volume if requested
                if let Some(data_size) = options.data_volume_size {
                    if output_format != "json" {
                        println!("   Creating and attaching {}GB data volume...", data_size);
                    }
                    if let Err(e) =
                        auto_attach_data_volume(&client, &instance_id, data_size, &aws_cfg.region)
                            .await
                    {
                        if output_format != "json" {
                            println!("WARNING: Failed to attach data volume: {}", e);
                            println!(
                                "   You can attach manually: runctl aws ebs create --size {} --attach",
                                data_size
                            );
                        }
                    }
                }

                // Wait for instance to be ready if requested
                if options.wait {
                    if output_format != "json" {
                        println!("Waiting for instance to be ready...");
                    }
                    if let Err(e) = crate::aws_utils::wait_for_instance_running(
                        &client,
                        &instance_id,
                        Some(aws_config),
                    )
                    .await
                    {
                        warn!("Failed to wait for instance ready: {}", e);
                        if output_format != "json" {
                            println!("WARNING: Instance created but may not be ready yet.");
                            println!("  Check status: runctl aws wait {}", instance_id);
                        }
                    } else if output_format != "json" {
                        println!("Instance ready and SSM connected (if IAM profile configured)");
                    }
                }

                // Register resource with ResourceTracker
                if let Some(tracker) = &config.resource_tracker {
                    // Get instance details for registration
                    let instance_response = client
                        .describe_instances()
                        .instance_ids(&instance_id)
                        .send()
                        .await
                        .map_err(|e| {
                            TrainctlError::Aws(format!("Failed to describe instance: {}", e))
                        })?;

                    if let Some(instance) = crate::aws::helpers::find_instance_in_response(
                        &instance_response,
                        &instance_id,
                    ) {
                        if let Ok(resource_status) =
                            ec2_instance_to_resource_status(instance, &instance_id)
                        {
                            if let Err(e) = tracker.register(resource_status).await {
                                warn!("Failed to register resource in tracker: {}", e);
                            } else {
                                info!(
                                    "Registered spot instance {} with ResourceTracker",
                                    instance_id
                                );
                            }
                        }
                    }
                }

                // Handle structured output formats
                if output_format == "instance-id" {
                    println!("{}", instance_id);
                    return Ok(());
                }

                return Ok(());
            }
            Err(e) if !options.no_fallback => {
                // Calculate cost difference for user awareness
                let spot_cost = crate::utils::get_instance_cost(&options.instance_type) * 0.1; // Assume 90% discount
                let ondemand_cost = crate::utils::get_instance_cost(&options.instance_type);
                let cost_multiplier = (ondemand_cost / spot_cost).round() as u32;

                println!();
                println!("⚠️  WARNING: Spot instance failed: {}", e);
                println!();
                println!("   Cost impact:");
                println!("   - Spot (requested):   ~${:.4}/hour", spot_cost);
                println!("   - On-demand (fallback): ${:.4}/hour", ondemand_cost);
                println!("   - On-demand is ~{}x more expensive", cost_multiplier);
                println!();
                println!("   Falling back to on-demand instance...");
                println!(
                    "   (Use --no-fallback to fail instead, or try different instance type/region)"
                );
                println!();
            }
            Err(e) => {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Spot instance failed and no fallback: {}\n\
                        \n\
                        Suggestions:\n\
                          - Try on-demand instance: remove --spot flag\n\
                          - Check spot price limits: current max price may be too low\n\
                          - Try a different instance type or region\n\
                          - Check AWS spot instance availability in your region",
                        e
                    ),
                    source: None,
                });
            }
        }
    }

    // Create on-demand instance
    let instance_id = create_ondemand_instance(
        &client,
        &options.instance_type,
        &final_ami,
        &user_data,
        options.key_name.as_deref(),
        options.security_group.as_deref(),
        root_size,
        options.iam_instance_profile.as_deref(),
    )
    .await?;

    if output_format == "json" {
        let instance_info =
            get_instance_info_json(&client, &instance_id, &options.instance_type).await?;
        println!("{}", serde_json::to_string_pretty(&instance_info)?);
    } else {
        println!("Created on-demand instance: {}", instance_id);
    }

    if let Err(e) = tag_instance(&client, &instance_id, &options.project_name, config).await {
        warn!("Failed to tag instance {}: {}", instance_id, e);
        if output_format != "json" {
            println!("  Instance created but tagging failed. You can tag manually if needed.");
        }
    }

    // Auto-attach data volume if requested
    if let Some(data_size) = options.data_volume_size {
        if output_format != "json" {
            println!("   Creating and attaching {}GB data volume...", data_size);
        }
        if let Err(e) =
            auto_attach_data_volume(&client, &instance_id, data_size, &aws_cfg.region).await
        {
            if output_format != "json" {
                println!("WARNING: Failed to attach data volume: {}", e);
                println!(
                    "   You can attach manually: runctl aws ebs create --size {} --attach",
                    data_size
                );
            }
        }
    }

    // Wait for instance to be ready if requested
    if options.wait {
        if output_format != "json" {
            println!("Waiting for instance to be ready...");
        }
        if let Err(e) =
            crate::aws_utils::wait_for_instance_running(&client, &instance_id, Some(aws_config))
                .await
        {
            // Even if wait fails, instance was created - warn but don't fail
            warn!("Failed to wait for instance ready: {}", e);
            if output_format != "json" {
                println!("WARNING: Instance created but may not be ready yet.");
                println!("  Check status: runctl aws wait {}", instance_id);
            }
        } else if output_format != "json" {
            // Check if instance actually has IAM profile for more specific message
            let has_iam = if let Ok(instance_response) = client
                .describe_instances()
                .instance_ids(&instance_id)
                .send()
                .await
            {
                crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
                    .and_then(|i| i.iam_instance_profile())
                    .is_some()
            } else {
                false
            };

            if has_iam {
                println!("Instance ready and SSM connected");
            } else {
                println!("Instance ready (SSM not available - use --iam-instance-profile for SSM)");
            }
        }
    }

    // Register resource with ResourceTracker
    if let Some(tracker) = &config.resource_tracker {
        // Get instance details for registration
        let instance_response = client
            .describe_instances()
            .instance_ids(&instance_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

        if let Some(instance) =
            crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
        {
            if let Ok(resource_status) = ec2_instance_to_resource_status(instance, &instance_id) {
                if let Err(e) = tracker.register(resource_status).await {
                    warn!("Failed to register resource in tracker: {}", e);
                } else {
                    info!("Registered instance {} with ResourceTracker", instance_id);
                }
            }
        }
    }

    // Handle structured output formats
    if output_format == "instance-id" {
        println!("{}", instance_id);
        return Ok(());
    }

    Ok(())
}

/// Find latest Deep Learning AMI for GPU instances
async fn find_deep_learning_ami(client: &Ec2Client, _region: &str) -> Result<String> {
    use aws_sdk_ec2::types::Filter;

    // Try multiple Deep Learning AMI patterns
    let patterns = vec![
        "Deep Learning AMI GPU PyTorch * (Amazon Linux 2)*",
        "Deep Learning AMI GPU PyTorch *",
        "Deep Learning AMI (Amazon Linux 2)*",
        "Deep Learning Base AMI (Amazon Linux 2)*",
    ];

    for pattern in patterns {
        let response = client
            .describe_images()
            .owners("amazon")
            .filters(Filter::builder().name("name").values(pattern).build())
            .filters(Filter::builder().name("state").values("available").build())
            .send()
            .await
            .map_err(|e| {
                TrainctlError::Aws(format!("Failed to search for Deep Learning AMI: {}", e))
            })?;

        let images = response.images();
        if !images.is_empty() {
            // Sort by creation date (newest first)
            let mut sorted: Vec<_> = images.iter().collect();
            sorted.sort_by(|a, b| {
                let a_date = a.creation_date().unwrap_or("");
                let b_date = b.creation_date().unwrap_or("");
                b_date.cmp(a_date)
            });

            return Ok(sorted[0]
                .image_id()
                .ok_or_else(|| TrainctlError::Aws("AMI has no image ID".to_string()))?
                .to_string());
        }
    }

    Err(TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "No Deep Learning AMI found with any pattern".to_string(),
        source: None,
    })
}

/// Generate user data script for instance initialization
fn generate_user_data(project_name: &str, _has_data_volume: bool) -> String {
    format!(
        r#"#!/bin/bash
set -e

# Log all output for debugging
exec > >(tee /var/log/user-data.log)
exec 2>&1

echo "Starting instance setup..."

# Detect OS (Ubuntu vs Amazon Linux)
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$ID
else
    OS="unknown"
fi

echo "Detected OS: $OS"

# Update system
if [ "$OS" = "ubuntu" ]; then
    export DEBIAN_FRONTEND=noninteractive
apt-get update -y
    apt-get upgrade -y -qq
    apt-get install -y python3-pip python3-venv git curl build-essential
    USER="ubuntu"
    HOME_DIR="/home/ubuntu"
elif [ "$OS" = "amzn" ] || [ "$OS" = "rhel" ]; then
    yum update -y -q
    # Install Python 3 and pip
    yum install -y python3 python3-pip git curl gcc gcc-c++ make
    # Ensure pip3 is available
    if ! command -v pip3 &> /dev/null; then
        # Try alternative installation methods
        if command -v python3 &> /dev/null; then
            curl -sS https://bootstrap.pypa.io/get-pip.py | python3
        fi
    fi
    USER="ec2-user"
    HOME_DIR="/home/ec2-user"
else
    echo "WARNING: Unknown OS, using defaults"
    USER="ubuntu"
    HOME_DIR="/home/ubuntu"
fi

# Install uv for Python package management
echo "Installing uv..."
curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME_DIR/.local/bin:$HOME_DIR/.cargo/bin:$PATH"
echo 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"' >> $HOME_DIR/.bashrc

# Pre-install common ML libraries (cached for faster training startup)
echo "Pre-installing common ML libraries..."
if command -v uv &> /dev/null; then
    uv pip install --system --quiet numpy pandas || pip3 install --quiet --user numpy pandas
else
    pip3 install --quiet --user numpy pandas || python3 -m pip install --quiet --user numpy pandas
fi

# Create dependency cache directory
mkdir -p /opt/runctl-cache
chmod 777 /opt/runctl-cache
echo "Dependency cache: /opt/runctl-cache"

# Setup data volume if attached
if [ -b /dev/nvme1n1 ] || [ -b /dev/xvdf ]; then
    echo "Setting up data volume..."
    DEVICE=$(lsblk -o NAME,TYPE,SIZE | grep -E '^nvme[0-9]+n1' | grep -v nvme0n1 | awk '{{print $1}}' | head -1)
    if [ -z "$DEVICE" ]; then
        for dev in /dev/xvdf /dev/sdf /dev/nvme1n1; do
            if [ -b "$dev" ]; then
                DEVICE=$(basename $dev)
                break
            fi
        done
    fi
    
    if [ -n "$DEVICE" ]; then
        FULL_DEVICE="/dev/$DEVICE"
        MOUNT_POINT="/mnt/data"
        
        # Format if not already formatted
        if ! blkid $FULL_DEVICE > /dev/null 2>&1; then
            echo "   Formatting volume..."
            mkfs.ext4 -F $FULL_DEVICE
        fi
        
        # Mount
        mkdir -p $MOUNT_POINT
        if ! mountpoint -q $MOUNT_POINT; then
            mount $FULL_DEVICE $MOUNT_POINT
            UUID=$(blkid -s UUID -o value $FULL_DEVICE)
            echo "UUID=$UUID $MOUNT_POINT ext4 defaults,nofail 0 2" >> /etc/fstab
        fi
        
        chown -R $USER:$USER $MOUNT_POINT
        echo "Data volume mounted at $MOUNT_POINT"
    fi
fi

# Create project directory
PROJECT_DIR="$HOME_DIR/{project_name}"
mkdir -p $PROJECT_DIR
chown $USER:$USER $PROJECT_DIR

# Create data directory (use mounted volume if available, else local)
if [ -d /mnt/data ]; then
    DATA_DIR="/mnt/data"
else
    DATA_DIR="$HOME_DIR/data"
fi
mkdir -p $DATA_DIR
chown $USER:$USER $DATA_DIR

# Setup Python environment
export PYTHONPATH=$PROJECT_DIR:$PYTHONPATH
echo "export PYTHONPATH=$PROJECT_DIR:\$PYTHONPATH" >> $HOME_DIR/.bashrc

# Create helper script for training
cat > $HOME_DIR/start_training.sh << 'TRAIN_SCRIPT'
#!/bin/bash
cd $PROJECT_DIR
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
export PYTHONPATH=$PROJECT_DIR:$PYTHONPATH

if command -v uv &> /dev/null; then
    uv run python3 -m training.train_lightning "$@"
else
    python3 -m training.train_lightning "$@"
fi
TRAIN_SCRIPT
chmod +x $HOME_DIR/start_training.sh
chown $USER:$USER $HOME_DIR/start_training.sh

echo "Instance setup complete"
echo "   Project directory: $PROJECT_DIR"
echo "   Data directory: $DATA_DIR"
echo "   To start training: $HOME_DIR/start_training.sh"
"#,
        project_name = project_name
    )
}

/// Auto-attach and setup data volume
async fn auto_attach_data_volume(
    client: &Ec2Client,
    instance_id: &str,
    size_gb: i32,
    _region: &str,
) -> Result<()> {
    // Get instance AZ
    let instance_response = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws("Instance not found".to_string()))?;

    let az = instance
        .placement()
        .and_then(|p| p.availability_zone())
        .ok_or_else(|| TrainctlError::Aws("Instance has no availability zone".to_string()))?;

    // Create volume
    let volume_response = client
        .create_volume()
        .size(size_gb)
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("Name")
                        .value(format!("{}-data", instance_id))
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("CreatedBy")
                        .value("runctl")
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create data volume: {}", e)))?;

    let volume_id = volume_response
        .volume_id()
        .ok_or_else(|| TrainctlError::Aws("Volume ID not in response".to_string()))?;

    // Wait for volume to be available
    println!("   Waiting for volume to be available...");
    let mut attempts = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        attempts += 1;

        let vol_response = client
            .describe_volumes()
            .volume_ids(volume_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

        let vol = vol_response
            .volumes()
            .first()
            .ok_or_else(|| TrainctlError::Aws("Volume not found".to_string()))?;
        let state = vol.state().map(|s| format!("{:?}", s)).unwrap_or_default();

        if state == "available" {
            break;
        }
        if attempts > 30 {
            return Err(TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "Volume creation timed out".to_string(),
                source: None,
            });
        }
    }

    // Attach volume (use /dev/sdf for compatibility)
    client
        .attach_volume()
        .volume_id(volume_id)
        .instance_id(instance_id)
        .device("/dev/sdf")
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to attach volume: {}", e)))?;

    println!(
        "Data volume {} attached (will be auto-mounted by user-data)",
        volume_id
    );

    Ok(())
}

/// Create a spot instance
async fn create_spot_instance(
    client: &Ec2Client,
    options: CreateSpotInstanceOptions,
    output_format: &str,
) -> Result<String> {
    // Base64 encode user data
    let user_data_b64 = base64::engine::general_purpose::STANDARD.encode(&options.user_data);

    // Create spot instance request with launch specification
    let mut spec_builder = aws_sdk_ec2::types::RequestSpotLaunchSpecification::builder()
        .image_id(&options.ami_id)
        .instance_type(Ec2InstanceType::from(options.instance_type.as_str()))
        .user_data(&user_data_b64)
        .ebs_optimized(true); // Enable EBS optimization for better I/O performance

    if let Some(key) = &options.key_name {
        spec_builder = spec_builder.key_name(key);
    }
    if let Some(sg) = &options.security_group {
        spec_builder = spec_builder.security_groups(sg);
    }

    // Configure root volume size (device name depends on AMI - try both common ones)
    // For Ubuntu: /dev/sda1, for Amazon Linux: /dev/xvda
    let block_device = aws_sdk_ec2::types::BlockDeviceMapping::builder()
        .device_name("/dev/sda1") // Ubuntu default
        .ebs(
            aws_sdk_ec2::types::EbsBlockDevice::builder()
                .volume_size(options.root_volume_size)
                .delete_on_termination(true)
                .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
                .build(),
        )
        .build();
    spec_builder = spec_builder.block_device_mappings(block_device);

    // Add IAM instance profile if provided
    if let Some(profile_name) = &options.iam_instance_profile {
        spec_builder = spec_builder.iam_instance_profile(
            aws_sdk_ec2::types::IamInstanceProfileSpecification::builder()
                .name(profile_name)
                .build(),
        );
    }

    let spec = spec_builder.build();

    let mut spot_request = client
        .request_spot_instances()
        .instance_count(1)
        .launch_specification(spec);

    // Set spot price if provided
    if let Some(price) = &options.max_price {
        spot_request = spot_request.spot_price(price);
    } else {
        // Use one-time spot request by default
        spot_request = spot_request.spot_price("0.10"); // Default max price
    }

    let response = spot_request
        .send()
        .await
        .map_err(|e| {
            // Extract more detailed error information from AWS SDK error
            let error_msg = format!("{}", e);
            let mut detailed_msg = format!("Failed to request spot instance: {}", error_msg);

            // Check for common error patterns and provide specific guidance
            if error_msg.contains("InsufficientInstanceCapacity") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Try on-demand instance (remove --spot flag)\n  2. Try a different instance type\n  3. Try a different availability zone or region\n  4. Check AWS spot instance availability");
            } else if error_msg.contains("SpotPrice") || error_msg.contains("price") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Increase --spot-max-price (current may be too low)\n  2. Try on-demand instance (remove --spot flag)\n  3. Check current spot prices: aws ec2 describe-spot-price-history");
            } else if error_msg.contains("InvalidParameter") {
                detailed_msg.push_str("\n\nTo resolve:\n  1. Verify instance type is valid for your region\n  2. Check IAM permissions for spot instance requests\n  3. Verify security group and key pair exist");
            }

            TrainctlError::Aws(detailed_msg)
        })?;

    let spot_request_id = response
        .spot_instance_requests()
        .first()
        .and_then(|req| req.spot_instance_request_id())
        .ok_or_else(|| TrainctlError::Aws("No spot request ID in response".to_string()))?
        .to_string();

    // Wait for spot instance to be fulfilled
    const MAX_ATTEMPTS: u32 = 60; // 5 minutes (60 * 5 seconds)
    const POLL_INTERVAL: Duration = Duration::from_secs(5);
    
    // Use progress bar for non-JSON output
    let pb = if output_format != "json" {
        let pb = ProgressBar::new(MAX_ATTEMPTS as u64);
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .expect("Progress bar template should be valid"),
        );
        pb.set_message(format!("Waiting for spot instance (request: {})...", spot_request_id));
        Some(pb)
    } else {
        None
    };

    info!(
        "Waiting for spot instance to be fulfilled (request ID: {})",
        spot_request_id
    );

    let mut attempts = 0;
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        attempts += 1;
        if let Some(ref p) = pb {
            p.set_position(attempts as u64);
            p.set_message(format!(
                "Waiting for spot instance... (attempt {}/{})",
                attempts, MAX_ATTEMPTS
            ));
        }

        let describe_response = client
            .describe_spot_instance_requests()
            .spot_instance_request_ids(&spot_request_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe spot request: {}", e)))?;

        let request = describe_response
            .spot_instance_requests()
            .first()
            .ok_or_else(|| TrainctlError::Aws("Spot request not found".to_string()))?;

        let state = request.state().and_then(|s| s.as_str().into());

        match state {
            Some("fulfilled") => {
                if let Some(ref p) = pb {
                    p.finish_with_message("Spot instance fulfilled!");
                }
                let instance_id = request
                    .instance_id()
                    .ok_or_else(|| {
                        TrainctlError::Aws("No instance ID in fulfilled request".to_string())
                    })?
                    .to_string();
                return Ok(instance_id);
            }
            Some("open") | Some("active") => {
                // Still waiting
                if attempts >= MAX_ATTEMPTS {
                    if let Some(ref p) = pb {
                        p.finish_with_message("Spot request timed out");
                    }
                    return Err(TrainctlError::CloudProvider {
                        provider: "aws".to_string(),
                        message: format!(
                            "Spot request timed out after {} minutes ({} attempts).\n\n\
                            Spot instances may be unavailable due to:\n\
                              - High demand for this instance type\n\
                              - Current spot price exceeds your max price\n\
                              - Limited capacity in this region/zone\n\n\
                            Suggestions:\n\
                              1. Try on-demand instance (remove --spot flag)\n\
                              2. Try a different instance type\n\
                              3. Try a different region\n\
                              4. Increase --spot-max-price if using it",
                            (MAX_ATTEMPTS as u64 * POLL_INTERVAL.as_secs()) / 60,
                            MAX_ATTEMPTS
                        ),
                        source: None,
                    });
                }
                continue;
            }
            Some("failed") | Some("cancelled") | Some("closed") => {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Spot request {}: {}",
                        spot_request_id,
                        state.unwrap_or("unknown")
                    ),
                    source: None,
                });
            }
            _ => {
                if attempts >= MAX_ATTEMPTS {
                    if let Some(ref p) = pb {
                        p.finish_with_message("Spot request timed out (unknown state)");
                    }
                    return Err(TrainctlError::CloudProvider {
                        provider: "aws".to_string(),
                        message: format!("Spot request in unknown state: {:?}", state),
                        source: None,
                    });
                }
                continue;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_ondemand_instance(
    client: &Ec2Client,
    instance_type: &str,
    ami_id: &str,
    user_data: &str,
    key_name: Option<&str>,
    security_group: Option<&str>,
    root_volume_size: i32,
    iam_instance_profile: Option<&str>,
) -> Result<String> {
    // Base64 encode user data
    let user_data_b64 = base64::engine::general_purpose::STANDARD.encode(user_data);

    let mut run_request = client
        .run_instances()
        .image_id(ami_id)
        .instance_type(Ec2InstanceType::from(instance_type))
        .min_count(1)
        .max_count(1)
        .user_data(&user_data_b64)
        .ebs_optimized(true); // Enable EBS optimization for better I/O performance

    if let Some(key) = key_name {
        run_request = run_request.key_name(key);
    }
    if let Some(sg) = security_group {
        run_request = run_request.security_group_ids(sg);
    }

    // Add IAM instance profile if provided
    if let Some(profile_name) = iam_instance_profile {
        run_request = run_request.iam_instance_profile(
            aws_sdk_ec2::types::IamInstanceProfileSpecification::builder()
                .name(profile_name)
                .build(),
        );
    }

    // Configure root volume size (device name depends on AMI)
    let block_device = aws_sdk_ec2::types::BlockDeviceMapping::builder()
        .device_name("/dev/sda1") // Ubuntu default, works for most AMIs
        .ebs(
            aws_sdk_ec2::types::EbsBlockDevice::builder()
                .volume_size(root_volume_size)
                .delete_on_termination(true)
                .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
                .build(),
        )
        .build();
    run_request = run_request.block_device_mappings(block_device);

    let response = run_request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create instance: {}", e)))?;

    let instance_id = response
        .instances()
        .first()
        .and_then(|inst| inst.instance_id())
        .ok_or_else(|| TrainctlError::Aws("No instance ID in response".to_string()))?
        .to_string();

    Ok(instance_id)
}

/// Tag an instance with Name and runctl metadata
async fn tag_instance(
    client: &Ec2Client,
    instance_id: &str,
    project_name: &str,
    config: &Config,
) -> Result<()> {
    use aws_sdk_ec2::types::Tag;

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let user_id = get_user_id(config);

    // config is used via get_user_id() call above
    let name_tag = format!("runctl-{}-{}-{}", user_id, project_name, &instance_id[..8]);

    client
        .create_tags()
        .resources(instance_id)
        .tags(Tag::builder().key("Name").value(&name_tag).build())
        .tags(
            Tag::builder()
                .key("runctl:created")
                .value(timestamp)
                .build(),
        )
        .tags(
            Tag::builder()
                .key("runctl:project")
                .value(project_name)
                .build(),
        )
        .tags(Tag::builder().key("runctl:user").value(&user_id).build())
        .tags(Tag::builder().key("CreatedBy").value("runctl").build())
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to tag instance: {}", e)))?;

    Ok(())
}

/// Terminate an EC2 instance
///
/// Permanently terminates an instance. This action cannot be undone. The instance
/// and its root volume are deleted (unless root volume has `DeleteOnTermination: false`).
/// Attached EBS volumes are preserved by default.
///
/// # Safety
///
/// This function uses `safe_cleanup()` which applies protection mechanisms:
/// - Time-based protection (instances <5 minutes old require `--force`)
/// - Tag-based protection (instances with `runctl:protected=true` cannot be deleted)
/// - Explicit protection (via `CleanupSafety::protect()`)
///
/// # Arguments
///
/// * `force`: Bypass all safety checks (use with caution)
/// * `output_format`: "json" for structured output, anything else for human-readable
///
/// # Errors
///
/// Returns `TrainctlError::CloudProvider` if:
/// - Instance not found
/// - Instance is protected (unless `force=true`)
/// - AWS API errors
pub async fn terminate_instance(
    instance_id: String,
    force: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
    config: &Config,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    // Check for attached volumes
    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    // Check for attached volumes
    let block_devices = instance.block_device_mappings();
    let has_data_volumes = block_devices.iter().any(|bd| {
        bd.device_name()
            .map(|d| d != "/dev/xvda" && d != "/dev/sda1")
            .unwrap_or(false)
    });

    if has_data_volumes {
        println!(
            "WARNING: Instance {} has attached EBS volumes.",
            instance_id
        );
        println!("Volumes will remain after instance termination.");
        println!(
            "   List volumes: runctl aws ebs list --instance-id {}",
            instance_id
        );
    }

    // Check for running training jobs and resource usage (unless force is used)
    if !force {
        // Check for checkpoints before termination
        // Training metadata retrieval temporarily disabled
        // TODO: Re-enable when lifecycle module is properly integrated
        // This would check for checkpoints before termination

        if let Some(_iam_profile) = instance.iam_instance_profile() {
            // Check for high resource usage (warns but doesn't block)
            match crate::diagnostics::check_high_resource_usage(&ssm_client, &instance_id).await {
                Ok(Some(warnings)) => {
                    println!(
                        "WARNING: High resource usage detected on instance {}:",
                        instance_id
                    );
                    println!("{}", warnings);
                    println!("Consider stopping active processes before termination.");
                    println!("Use --force to override and terminate anyway.");
                }
                Ok(None) => {
                    // No high usage, but still check for training processes
                }
                Err(e) => {
                    println!("WARNING: Could not check resource usage: {}", e);
                }
            }

            // Try SSM to check for training processes (blocks termination)
            let check_training_cmd = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
    else
        echo "TRAINING_STOPPED"
    fi
else
    # Check for common training process names
    if pgrep -f "python.*train\|python.*training\|python.*main.py" > /dev/null; then
        echo "TRAINING_RUNNING:$(pgrep -f 'python.*train\|python.*training\|python.*main.py' | head -1)"
    else
        echo "NO_TRAINING"
    fi
fi
"#;

            // Check for running training jobs using shared SSM utility
            match crate::aws_utils::execute_ssm_command(
                &ssm_client,
                &instance_id,
                check_training_cmd,
            )
            .await
            {
                Ok(output) => {
                    if output.contains("TRAINING_RUNNING") {
                        println!("ERROR: Training job is running on instance {}", instance_id);
                        println!("Termination blocked to prevent data loss.");
                        println!("Please stop the training job first or use --force to override.");
                        return Err(TrainctlError::CloudProvider {
                            provider: "aws".to_string(),
                            message: "Termination blocked: training job is running".to_string(),
                            source: None,
                        });
                    } else {
                        println!("No training jobs detected, proceeding with termination");
                    }
                }
                Err(e) => {
                    println!("WARNING: Could not check for training jobs: {}", e);
                    println!("Proceeding with termination. Use --force to suppress this warning.");
                }
            }
        }
    } else {
        println!("Force termination enabled, skipping safety checks.");
    }

    // Use safe cleanup if ResourceTracker is available
    if let Some(tracker) = &config.resource_tracker {
        let safety = CleanupSafety::new();
        // Clone needed: safe_cleanup needs owned Vec<String> for ResourceId conversion
        let cleanup_result = safe_cleanup(
            vec![instance_id.clone()],
            tracker,
            &safety,
            false, // dry_run
            force,
        )
        .await?;

        if !cleanup_result.deleted.is_empty() {
            info!("Safe cleanup approved termination of {}", instance_id);
        } else if !cleanup_result.skipped.is_empty() {
            let (id, reason) = &cleanup_result.skipped[0];
            if !force {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Termination blocked by safe cleanup: {}\nUse --force to override.",
                        reason
                    ),
                    source: None,
                });
            }
            warn!(
                "Safe cleanup blocked termination of {}: {}, but --force overrides",
                id, reason
            );
        }
    }

    client
        .terminate_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to terminate instance: {}", e)))?;

    // Remove from ResourceTracker after successful termination
    if let Some(tracker) = &config.resource_tracker {
        if let Err(e) = tracker.remove(&instance_id).await {
            warn!("Failed to remove resource from tracker: {}", e);
        } else {
            info!("Removed instance {} from ResourceTracker", instance_id);
        }
    }

    if output_format == "json" {
        let result = TerminateInstanceResult {
            success: true,
            instance_id: instance_id.clone(), // Clone needed: used in message format! below
            state: "terminating".to_string(),
            has_data_volumes,
            message: format!("Instance {} termination requested", instance_id),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Instance termination requested: {}", instance_id);
    }

    Ok(())
}

/// Stop an instance
pub async fn stop_instance(
    instance_id: String,
    force: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
    config: &Config,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    // Check instance state
    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    if state == "stopped" || state == "stopping" {
        println!("Instance {} is already stopped or stopping", instance_id);
        return Ok(());
    }

    if state != "running" {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Instance {} is in state '{}', cannot stop",
                instance_id, state
            ),
            source: None,
        });
    }

    // Graceful shutdown: save checkpoints and stop training cleanly
    if !force {
        if let Some(_iam_profile) = instance.iam_instance_profile() {
            let graceful_stop_cmd = r#"
# Check for training and gracefully stop
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
        # Send SIGTERM for graceful shutdown
        kill -TERM $PID 2>/dev/null || true
        # Wait up to 30 seconds for graceful shutdown
        for i in {1..30}; do
            if ! ps -p $PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                exit 0
            fi
            sleep 1
        done
        # Force kill if still running
        kill -9 $PID 2>/dev/null || true
        echo "TRAINING_FORCE_STOPPED"
    else
        echo "TRAINING_STOPPED"
    fi
else
    # Check for training processes
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training\|python.*main.py" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        echo "TRAINING_RUNNING:$TRAINING_PID"
        # Send SIGTERM for graceful shutdown
        kill -TERM $TRAINING_PID 2>/dev/null || true
        # Wait up to 30 seconds
        for i in {1..30}; do
            if ! ps -p $TRAINING_PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                exit 0
            fi
            sleep 1
        done
        # Force kill if still running
        kill -9 $TRAINING_PID 2>/dev/null || true
        echo "TRAINING_FORCE_STOPPED"
    else
        echo "NO_TRAINING"
    fi
fi
"#;

            match crate::aws_utils::execute_ssm_command(
                &ssm_client,
                &instance_id,
                graceful_stop_cmd,
            )
            .await
            {
                Ok(output) => {
                    if output.contains("TRAINING_RUNNING") {
                        println!(
                            "Training detected on instance {}, attempting graceful shutdown...",
                            instance_id
                        );
                    } else if output.contains("TRAINING_STOPPED_GRACEFULLY") {
                        println!("Training stopped gracefully on instance {}", instance_id);
                    } else if output.contains("TRAINING_FORCE_STOPPED") {
                        println!("WARNING: Training force-stopped on instance {} (graceful shutdown timeout)", instance_id);
                    }
                }
                Err(e) => {
                    println!("WARNING: Could not gracefully stop training: {}", e);
                }
            }
        }
    }

    client
        .stop_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to stop instance: {}", e)))?;

    // Update ResourceTracker
    update_resource_status_in_tracker(&instance_id, &client, config).await;

    if output_format == "json" {
        let result = StopInstanceResult {
            success: true,
            instance_id: instance_id.clone(), // Clone needed: used in message format! and println below
            state: "stopping".to_string(),
            message: format!("Instance {} stop requested", instance_id),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Instance stop requested: {}", instance_id);
        println!(
            "Instance can be restarted with: runctl aws start {}",
            instance_id
        );
    }

    Ok(())
}

/// Start a stopped EC2 instance
///
/// Starts a previously stopped instance. If the instance is already running,
/// returns success with current state. If the instance is in any other state
/// (terminating, terminated, etc.), returns an error.
///
/// # Arguments
///
/// * `wait`: If true, waits for instance to reach "running" state before returning
/// * `output_format`: "json" for structured output, anything else for human-readable
///
/// # Errors
///
/// Returns `TrainctlError::CloudProvider` if:
/// - Instance not found
/// - Instance is not in "stopped" state (cannot start from current state)
/// - AWS API errors
pub async fn start_instance(
    instance_id: String,
    wait: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
    config: &Config,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);

    // Check instance state
    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    if state == "running" {
        if output_format == "json" {
            let result = StartInstanceResult {
                success: true,
                instance_id: instance_id.clone(), // Clone needed: used in message format! below
                state: "running".to_string(),
                public_ip: instance.public_ip_address().map(|s| s.to_string()),
                message: format!("Instance {} is already running", instance_id),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Instance {} is already running", instance_id);
            if let Some(ip) = instance.public_ip_address() {
                println!("  Public IP: {}", ip);
            }
        }
        return Ok(());
    }

    if state != "stopped" {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Instance {} is in state '{}', can only start stopped instances",
                instance_id, state
            ),
            source: None,
        });
    }

    // Start the instance
    client
        .start_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to start instance: {}", e)))?;

    if output_format != "json" {
        println!("Starting instance: {}", instance_id);
    }

    // Wait for instance to be running if requested
    let mut final_ip: Option<String> = None;
    if wait {
        if output_format != "json" {
            println!("  Waiting for instance to be running...");
        }

        let mut attempts = 0;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            attempts += 1;

            let check_response = client
                .describe_instances()
                .instance_ids(&instance_id)
                .send()
                .await
                .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

            let inst = check_response
                .reservations()
                .iter()
                .flat_map(|r| r.instances())
                .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false));

            if let Some(inst) = inst {
                let current_state = inst
                    .state()
                    .and_then(|s| s.name())
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");

                if current_state == "running" {
                    final_ip = inst.public_ip_address().map(|s| s.to_string());
                    break;
                }
            }

            if attempts > 60 {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: "Timeout waiting for instance to start (5 minutes)".to_string(),
                    source: None,
                });
            }
        }
        // Update ResourceTracker after instance is running
        update_resource_status_in_tracker(&instance_id, &client, config).await;
    }

    let final_state = if wait { "running" } else { "pending" };

    if output_format == "json" {
        let result = StartInstanceResult {
            success: true,
            instance_id: instance_id.clone(), // Clone needed: used in message format! and println below
            state: final_state.to_string(),
            public_ip: final_ip.clone(),
            message: format!("Instance {} start requested", instance_id),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Instance {} is now {}", instance_id, final_state);
        if let Some(ip) = final_ip {
            println!("  Public IP: {}", ip);
        } else if !wait {
            println!("  Use --wait to get the public IP once running");
        }
    }

    Ok(())
}

/// Show instance status and training state
pub async fn show_instance_status(
    instance_id: String,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    use aws_sdk_ssm::Client as SsmClient;
    use serde_json::json;

    let ec2_client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    // Get instance details
    let response = ec2_client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&response, &instance_id)
        .ok_or_else(|| TrainctlError::ResourceNotFound {
            resource_type: "instance".to_string(),
            resource_id: instance_id.clone(),
        })?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    let public_ip = instance.public_ip_address().map(|s| s.to_string());
    let private_ip = instance.private_ip_address().map(|s| s.to_string());
    let instance_type = instance
        .instance_type()
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    // Check if SSM is available
    let ssm_available = instance.iam_instance_profile().is_some();

    // Try to get training status if instance is running and SSM is available
    let training_status = if state == "running" && ssm_available {
        // Check for training process
        let check_cmd = "if [ -f ~/training.pid ]; then PID=$(cat ~/training.pid 2>/dev/null) && ps -p $PID > /dev/null 2>&1 && echo 'RUNNING' || echo 'COMPLETE'; else echo 'NO_TRAINING'; fi";
        match crate::aws_utils::execute_ssm_command(&ssm_client, &instance_id, check_cmd).await {
            Ok(output) => {
                let trimmed = output.trim();
                if trimmed == "RUNNING" {
                    Some("running".to_string())
                } else if trimmed == "COMPLETE" {
                    Some("completed".to_string())
                } else {
                    Some("not_started".to_string())
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

    if output_format == "json" {
        let status = json!({
            "success": true,
            "instance_id": instance_id,
            "state": state,
            "instance_type": instance_type,
            "public_ip": public_ip,
            "private_ip": private_ip,
            "ssm_available": ssm_available,
            "training_status": training_status,
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("Instance: {}", instance_id);
        println!("  State: {}", state);
        println!("  Type: {}", instance_type);
        if let Some(ip) = public_ip {
            println!("  Public IP: {}", ip);
        }
        if let Some(ip) = private_ip {
            println!("  Private IP: {}", ip);
        }
        println!(
            "  SSM Available: {}",
            if ssm_available { "Yes" } else { "No" }
        );
        if let Some(status) = training_status {
            println!("  Training Status: {}", status);
        }
    }

    Ok(())
}

/// Wait for instance to be ready
pub async fn wait_for_instance(
    instance_id: String,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    use serde_json::json;

    let client = Ec2Client::new(aws_config);

    if output_format != "json" {
        println!("Waiting for instance {} to be ready...", instance_id);
    }

    match crate::aws_utils::wait_for_instance_running(&client, &instance_id, Some(aws_config)).await
    {
        Ok(_) => {
            if output_format == "json" {
                let result = json!({
                    "success": true,
                    "instance_id": instance_id,
                    "state": "running",
                    "message": "Instance is ready and SSM is connected (if IAM profile configured)"
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Instance {} is ready", instance_id);
            }
            Ok(())
        }
        Err(e) => {
            if output_format == "json" {
                let result = json!({
                    "success": false,
                    "instance_id": instance_id,
                    "error": e.to_string()
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            Err(e)
        }
    }
}
