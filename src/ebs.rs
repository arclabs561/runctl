//! EBS Volume Management for AWS
//!
//! Provides commands for creating, managing, and optimizing EBS volumes
//! for training workloads, especially with spot instances.

use crate::aws_utils::{
    execute_ssm_command, wait_for_instance_running, wait_for_volume_attachment,
    wait_for_volume_detached,
};
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_config::BehaviorVersion;
use aws_sdk_ec2::types::VolumeType;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Clone)]
pub enum EbsCommands {
    /// Create a new EBS volume
    Create {
        /// Volume size in GB
        #[arg(long)]
        size: i32,
        /// Volume type (gp3, gp2, io2, st1, sc1)
        #[arg(long, default_value = "gp3")]
        volume_type: String,
        /// Availability zone (required)
        #[arg(long)]
        availability_zone: Option<String>,
        /// IOPS (for gp3/io2)
        #[arg(long)]
        iops: Option<i32>,
        /// Throughput in MB/s (for gp3)
        #[arg(long)]
        throughput: Option<i32>,
        /// Volume name tag
        #[arg(long)]
        name: Option<String>,
        /// Enable encryption
        #[arg(long)]
        encrypted: bool,
        /// Mark as persistent (survives cleanup, protected from deletion)
        #[arg(long)]
        persistent: bool,
        /// Pre-warm from S3 path
        #[arg(long)]
        pre_warm: Option<String>,
    },
    /// List EBS volumes
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        /// Filter by name tag
        #[arg(long)]
        name: Option<String>,
    },
    /// Attach volume to instance
    Attach {
        /// Volume ID
        volume_id: String,
        /// Instance ID
        #[arg(long)]
        instance_id: String,
        /// Device name (e.g., /dev/sdf)
        #[arg(long, default_value = "/dev/sdf")]
        device: String,
    },
    /// Detach volume from instance
    Detach {
        /// Volume ID
        volume_id: String,
        /// Force detach (if instance is stopped)
        #[arg(long)]
        force: bool,
    },
    /// Delete EBS volume
    Delete {
        /// Volume ID
        volume_id: String,
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
    /// Pre-warm volume with data from S3
    PreWarm {
        /// Volume ID
        volume_id: String,
        /// S3 source path
        s3_source: String,
        /// Mount point on instance
        #[arg(long, default_value = "/mnt/data")]
        mount_point: String,
        /// Instance ID to use for pre-warming (creates temporary if not provided)
        #[arg(long)]
        instance_id: Option<String>,
    },
    /// Create snapshot of volume
    Snapshot {
        /// Volume ID
        volume_id: String,
        /// Snapshot description
        #[arg(long)]
        description: Option<String>,
        /// Snapshot name tag
        #[arg(long)]
        name: Option<String>,
    },
    /// List snapshots
    SnapshotList {
        /// Filter by volume ID
        #[arg(long)]
        volume_id: Option<String>,
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    /// Restore volume from snapshot
    Restore {
        /// Snapshot ID
        snapshot_id: String,
        /// Volume size in GB (defaults to snapshot size)
        #[arg(long)]
        size: Option<i32>,
        /// Volume type
        #[arg(long, default_value = "gp3")]
        volume_type: String,
        /// Availability zone (required)
        #[arg(long)]
        availability_zone: Option<String>,
        /// Volume name tag
        #[arg(long)]
        name: Option<String>,
    },
}

pub async fn handle_command(cmd: EbsCommands, config: &Config, _output_format: &str) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    match cmd {
        EbsCommands::Create {
            size,
            volume_type,
            availability_zone,
            iops,
            throughput,
            name,
            encrypted,
            persistent,
            pre_warm,
        } => {
            create_volume(
                size,
                volume_type,
                availability_zone,
                iops,
                throughput,
                name,
                encrypted,
                persistent,
                pre_warm,
                config,
                &client,
                &ssm_client,
            )
            .await
        }
        EbsCommands::List { detailed, name } => list_volumes(detailed, name, &client).await,
        EbsCommands::Attach {
            volume_id,
            instance_id,
            device,
        } => attach_volume(volume_id, instance_id, device, &client).await,
        EbsCommands::Detach { volume_id, force } => detach_volume(volume_id, force, &client).await,
        EbsCommands::Delete { volume_id, force } => delete_volume(volume_id, force, &client).await,
        EbsCommands::PreWarm {
            volume_id,
            s3_source,
            mount_point,
            instance_id,
        } => {
            pre_warm_volume(
                volume_id,
                s3_source,
                mount_point,
                instance_id,
                config,
                &client,
                &ssm_client,
            )
            .await
        }
        EbsCommands::Snapshot {
            volume_id,
            description,
            name,
        } => create_snapshot(volume_id, description, name, &client).await,
        EbsCommands::SnapshotList {
            volume_id,
            detailed,
        } => list_snapshots(volume_id, detailed, &client).await,
        EbsCommands::Restore {
            snapshot_id,
            size,
            volume_type,
            availability_zone,
            name,
        } => {
            restore_from_snapshot(
                snapshot_id,
                size,
                volume_type,
                availability_zone,
                name,
                &client,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_volume(
    size: i32,
    volume_type: String,
    availability_zone: Option<String>,
    iops: Option<i32>,
    throughput: Option<i32>,
    name: Option<String>,
    encrypted: bool,
    persistent: bool,
    pre_warm: Option<String>,
    config: &Config,
    client: &Ec2Client,
    ssm_client: &SsmClient,
) -> Result<()> {
    let aws_cfg = config
        .aws
        .as_ref()
        .ok_or_else(|| TrainctlError::Aws("AWS config not found".to_string()))?;

    // Get availability zone if not provided
    let az = if let Some(az) = availability_zone {
        az
    } else {
        // Get default AZ from region
        // For now, use a default - in production, would query available AZs
        format!("{}-1a", aws_cfg.region)
    };

    let vol_type = match volume_type.as_str() {
        "gp3" => VolumeType::Gp3,
        "gp2" => VolumeType::Gp2,
        "io2" => VolumeType::Io2,
        "st1" => VolumeType::St1,
        "sc1" => VolumeType::Sc1,
        _ => {
            return Err(TrainctlError::Validation {
                field: "volume_type".to_string(),
                reason: format!(
                    "Invalid volume type: {}. Use: gp3, gp2, io2, st1, sc1",
                    volume_type
                ),
            })
        }
    };

    info!(
        "Creating EBS volume: size={}GB, type={}, az={}",
        size, volume_type, az
    );

    // Check if volume with same name already exists
    if let Some(name_val) = &name {
        let response = client
            .describe_volumes()
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("tag:Name")
                    .values(name_val)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                TrainctlError::Aws(format!("Failed to check for existing volumes: {}", e))
            })?;

        if !response.volumes().is_empty() {
            let existing_id = response.volumes()[0].volume_id().unwrap_or("unknown");
            return Err(TrainctlError::ResourceExists {
                resource_type: "volume".to_string(),
                resource_id: format!(
                    "Volume with name '{}' already exists: {}",
                    name_val, existing_id
                ),
            });
        }
    }

    // Check volume type for IOPS/throughput before moving vol_type
    let needs_iops = matches!(vol_type, VolumeType::Gp3 | VolumeType::Io2);
    let needs_throughput = matches!(vol_type, VolumeType::Gp3);

    let mut request = client
        .create_volume()
        .size(size)
        .volume_type(vol_type)
        .availability_zone(&az)
        .encrypted(encrypted);

    // Add IOPS for gp3/io2
    if needs_iops {
        if let Some(iops_val) = iops {
            request = request.iops(iops_val);
        }
    }

    // Add throughput for gp3
    if needs_throughput {
        if let Some(throughput_val) = throughput {
            request = request.throughput(throughput_val);
        }
    }

    // Add tags
    let mut tags = vec![("CreatedBy".to_string(), "runctl".to_string())];
    if let Some(name_val) = &name {
        tags.push(("Name".to_string(), name_val.clone()));
    }
    // Mark as persistent if requested (protects from cleanup)
    if persistent {
        tags.push(("runctl:persistent".to_string(), "true".to_string()));
        tags.push(("runctl:protected".to_string(), "true".to_string()));
    }

    let tag_spec = aws_sdk_ec2::types::TagSpecification::builder()
        .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
        .set_tags(Some(
            tags.into_iter()
                .map(|(k, v)| aws_sdk_ec2::types::Tag::builder().key(k).value(v).build())
                .collect(),
        ))
        .build();

    request = request.tag_specifications(tag_spec);

    // Send request (retry logic can be added when ebs is moved to library)
    let response = request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create EBS volume: {}", e)))?;

    let volume_id = response
        .volume_id()
        .ok_or_else(|| TrainctlError::Aws("Volume ID not in response".to_string()))?;

    println!("Created EBS volume: {}", volume_id);
    println!("   Size: {} GB", size);
    println!("   Type: {}", volume_type);
    println!("   AZ: {}", az);
    println!(
        "   State: {}",
        response
            .state()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "unknown".to_string())
    );
    if persistent {
        println!("   Persistent: Protected from cleanup");
    }

    // Pre-warm if requested
    if let Some(s3_path) = pre_warm {
        println!("   Pre-warming from {}...", s3_path);
        pre_warm_volume(
            volume_id.to_string(),
            s3_path,
            "/mnt/data".to_string(),
            None,
            config,
            client,
            ssm_client,
        )
        .await?;
    }

    Ok(())
}

async fn list_volumes(
    detailed: bool,
    name_filter: Option<String>,
    client: &Ec2Client,
) -> Result<()> {
    let response = client
        .describe_volumes()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list EBS volumes: {}", e)))?;

    let volumes = response.volumes();

    if detailed {
        println!(
            "{:<20} {:<10} {:<8} {:<12} {:<15} {:<10} {:<3} {:<20}",
            "Volume ID", "Size (GB)", "Type", "State", "AZ", "Attached", "Persist", "Name"
        );
        println!("{}", "-".repeat(120));
    } else {
        println!(
            "{:<20} {:<10} {:<8} {:<12} {:<15} {:<3}",
            "Volume ID", "Size (GB)", "Type", "State", "Attached", "Persist"
        );
        println!("{}", "-".repeat(75));
    }

    for volume in volumes {
        // Filter by name if specified
        if let Some(ref name_filter_val) = name_filter {
            let volume_name = volume
                .tags()
                .iter()
                .find(|t| t.key().map(|k| k == "Name").unwrap_or(false))
                .and_then(|t| t.value());

            if volume_name != Some(name_filter_val) {
                continue;
            }
        }

        let volume_id = volume.volume_id().unwrap_or("unknown");
        let size = volume.size().unwrap_or(0);
        let vol_type = volume
            .volume_type()
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "unknown".to_string());
        let state = volume
            .state()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "unknown".to_string());
        let az = volume.availability_zone().unwrap_or("unknown");

        let attached_to = volume
            .attachments()
            .first()
            .and_then(|a| a.instance_id())
            .unwrap_or("-");

        // Check if persistent
        let is_persistent = volume.tags().iter().any(|t| {
            t.key().map(|k| k == "runctl:persistent").unwrap_or(false)
                && t.value().map(|v| v == "true").unwrap_or(false)
        });
        let persistent_marker = if is_persistent { "YES" } else { "" };

        if detailed {
            let name = volume
                .tags()
                .iter()
                .find(|t| t.key().map(|k| k == "Name").unwrap_or(false))
                .and_then(|t| t.value())
                .unwrap_or("-");

            println!(
                "{:<20} {:<10} {:<8} {:<12} {:<15} {:<10} {:<3} {:<20}",
                volume_id, size, vol_type, state, az, attached_to, persistent_marker, name
            );
        } else {
            println!(
                "{:<20} {:<10} {:<8} {:<12} {:<15} {:<3}",
                volume_id, size, vol_type, state, attached_to, persistent_marker
            );
        }
    }

    Ok(())
}

async fn attach_volume(
    volume_id: String,
    instance_id: String,
    device: String,
    client: &Ec2Client,
) -> Result<()> {
    info!(
        "Attaching volume {} to instance {} at {}",
        volume_id, instance_id, device
    );

    // Validate AZ match before attempting attachment
    let volume_response = client
        .describe_volumes()
        .volume_ids(&volume_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

    let volume = volume_response
        .volumes()
        .first()
        .ok_or_else(|| TrainctlError::Aws("Volume not found".to_string()))?;

    let volume_az = volume
        .availability_zone()
        .ok_or_else(|| TrainctlError::Aws("Volume has no availability zone".to_string()))?;

    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    let instance_az = instance
        .placement()
        .and_then(|p| p.availability_zone())
        .ok_or_else(|| TrainctlError::Aws("Instance has no availability zone".to_string()))?;

    if volume_az != instance_az {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Availability zone mismatch: Volume {} is in {}, but instance {} is in {}.\n\
                 EBS volumes must be in the same AZ as the instance.\n\
                 Create a new volume in {} or use instance in {}.",
                volume_id, volume_az, instance_id, instance_az, instance_az, volume_az
            ),
            source: None,
        });
    }

    // Check if volume is already attached
    if !volume.attachments().is_empty() {
        let attached_to = volume
            .attachments()
            .first()
            .and_then(|a| a.instance_id())
            .unwrap_or("unknown");
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Volume {} is already attached to instance {}.\n\
                 Detach it first or use a different volume.",
                volume_id, attached_to
            ),
            source: None,
        });
    }

    client
        .attach_volume()
        .volume_id(&volume_id)
        .instance_id(&instance_id)
        .device(&device)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to attach volume: {}", e)))?;

    println!(
        "Attached volume {} to instance {} at {}",
        volume_id, instance_id, device
    );
    println!("   Note: You may need to mount the volume on the instance:");
    println!("   sudo mkfs -t xfs {}  # First time only", device);
    println!("   sudo mkdir -p /mnt/data");
    println!("   sudo mount {} /mnt/data", device);

    Ok(())
}

async fn detach_volume(volume_id: String, force: bool, client: &Ec2Client) -> Result<()> {
    info!("Detaching volume {}", volume_id);

    let mut request = client.detach_volume().volume_id(&volume_id);

    if force {
        request = request.force(true);
    }

    request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to detach volume: {}", e)))?;

    println!("Detached volume {}", volume_id);
    Ok(())
}

async fn delete_volume(volume_id: String, force: bool, client: &Ec2Client) -> Result<()> {
    // Check volume details
    let response = client
        .describe_volumes()
        .volume_ids(&volume_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

    let volume = response
        .volumes()
        .first()
        .ok_or_else(|| TrainctlError::Aws("Volume not found".to_string()))?;

    // Check if volume is persistent/protected
    let is_persistent = volume.tags().iter().any(|tag| {
        tag.key()
            .map(|k| k == "runctl:persistent" || k == "runctl:protected")
            .unwrap_or(false)
            && tag.value().map(|v| v == "true").unwrap_or(false)
    });

    if is_persistent && !force {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Volume {} is marked as persistent and protected from deletion.\n\
                 Use --force to override (not recommended for persistent storage).",
                volume_id
            ),
            source: None,
        });
    }

    if !force {
        // Check if volume is attached
        if !volume.attachments().is_empty() {
            return Err(TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "Volume is attached. Use --force to detach and delete, or detach first."
                    .to_string(),
                source: None,
            });
        }

        // Check for snapshots
        let snapshots_response = client
            .describe_snapshots()
            .filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name("volume-id")
                    .values(&volume_id)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to check for snapshots: {}", e)))?;

        let snapshot_count = snapshots_response.snapshots().len();
        if snapshot_count > 0 {
            println!(
                "WARNING: Volume {} has {} snapshot(s).",
                volume_id, snapshot_count
            );
            println!("   Snapshots will remain after volume deletion.");
            println!(
                "   List snapshots: runctl aws ebs snapshot-list --volume-id {}",
                volume_id
            );
        }
    }

    info!("Deleting volume {}", volume_id);

    client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to delete volume: {}", e)))?;

    println!("Deleted volume {}", volume_id);
    Ok(())
}

async fn pre_warm_volume(
    volume_id: String,
    s3_source: String,
    mount_point: String,
    instance_id: Option<String>,
    config: &Config,
    client: &Ec2Client,
    ssm_client: &SsmClient,
) -> Result<()> {
    info!("Pre-warming volume {} from {}", volume_id, s3_source);

    let aws_cfg = config
        .aws
        .as_ref()
        .ok_or_else(|| TrainctlError::Aws("AWS config not found".to_string()))?;

    // Get volume details (especially AZ)
    let volume_response = client
        .describe_volumes()
        .volume_ids(&volume_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

    let volume = volume_response
        .volumes()
        .first()
        .ok_or_else(|| TrainctlError::Aws("Volume not found".to_string()))?;

    let availability_zone = volume
        .availability_zone()
        .ok_or_else(|| TrainctlError::Aws("Volume has no availability zone".to_string()))?;

    let final_mount_point = if mount_point.is_empty() {
        "/mnt/data".to_string()
    } else {
        mount_point
    };

    // Use provided instance or create temporary one
    let temp_instance_id = if let Some(ref inst_id) = instance_id {
        // Verify instance is in same AZ
        let inst_response = client
            .describe_instances()
            .instance_ids(inst_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

        let instance = inst_response
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .find(|i| i.instance_id().map(|id| id == inst_id).unwrap_or(false))
            .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", inst_id)))?;

        let inst_az = instance
            .placement()
            .and_then(|p| p.availability_zone())
            .ok_or_else(|| TrainctlError::Aws("Instance has no availability zone".to_string()))?;

        if inst_az != availability_zone {
            return Err(TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: format!(
                    "Instance {} is in AZ {}, but volume is in AZ {}. They must be in the same AZ.",
                    inst_id, inst_az, availability_zone
                ),
                source: None,
            });
        }

        println!("   Using existing instance: {}", inst_id);
        inst_id.clone()
    } else {
        // Create temporary instance for pre-warming
        println!("   Creating temporary instance for pre-warming...");
        let temp_instance = create_temp_prewarm_instance(
            client,
            availability_zone,
            &aws_cfg.default_ami,
            &aws_cfg.region,
        )
        .await?;

        println!("Created temporary instance: {}", temp_instance);

        // Wait for instance to be running
        wait_for_instance_running(client, &temp_instance).await?;

        temp_instance
    };

    // Attach volume
    println!(
        "   Attaching volume {} to instance {}...",
        volume_id, temp_instance_id
    );
    let device_name = "/dev/sdf";
    client
        .attach_volume()
        .volume_id(&volume_id)
        .instance_id(&temp_instance_id)
        .device(device_name)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to attach volume: {}", e)))?;

    // Wait for attachment to complete
    wait_for_volume_attachment(client, &volume_id, &temp_instance_id).await?;
    println!("Volume attached");

    // Mount and sync via SSM
    println!("   Mounting volume and syncing data from S3...");
    let mount_and_sync_cmd = format!(
        r#"
set -e
# Detect device (could be /dev/sdf, /dev/nvme1n1, etc.)
DEVICE=""
for dev in /dev/nvme1n1 /dev/xvdf /dev/sdf; do
    if [ -b "$dev" ] && [ "$(lsblk -o MOUNTPOINT -n $dev)" = "" ]; then
        DEVICE="$dev"
        break
    fi
done

if [ -z "$DEVICE" ]; then
    echo "ERROR: Could not find unmounted device"
    exit 1
fi

echo "Using device: $DEVICE"

# Format if needed
if ! blkid $DEVICE > /dev/null 2>&1; then
    echo "Formatting device..."
    sudo mkfs -t xfs $DEVICE
fi

# Create mount point
sudo mkdir -p {mount}
sudo mount $DEVICE {mount}

# Sync from S3 (use s5cmd if available, fallback to aws s3)
if command -v s5cmd &> /dev/null; then
    echo "Using s5cmd for fast transfer..."
    s5cmd cp --recursive {s3} {mount}/
else
    echo "Using aws s3 for transfer..."
    aws s3 sync {s3} {mount}/
fi

# Verify sync
echo "Sync complete. Data size:"
du -sh {mount}
"#,
        mount = final_mount_point,
        s3 = s3_source
    );

    execute_ssm_command(ssm_client, &temp_instance_id, &mount_and_sync_cmd).await?;
    println!("Data synced to volume");

    // Detach volume
    println!("   Detaching volume...");
    client
        .detach_volume()
        .volume_id(&volume_id)
        .instance_id(&temp_instance_id)
        .force(false)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to detach volume: {}", e)))?;

    wait_for_volume_detached(client, &volume_id).await?;
    println!("Volume detached");

    // Terminate temporary instance if we created it
    if instance_id.is_none() {
        println!("   Terminating temporary instance...");
        client
            .terminate_instances()
            .instance_ids(&temp_instance_id)
            .send()
            .await
            .map_err(|e| {
                TrainctlError::Aws(format!("Failed to terminate temporary instance: {}", e))
            })?;
        println!("Temporary instance termination requested");
    }

    println!("Volume {} pre-warmed from {}", volume_id, s3_source);
    println!("   Mount point: {}", final_mount_point);
    println!("   Ready to attach to training instances");

    Ok(())
}

/// Create a temporary instance for pre-warming
async fn create_temp_prewarm_instance(
    client: &Ec2Client,
    availability_zone: &str,
    ami_id: &str,
    _region: &str,
) -> Result<String> {
    use aws_sdk_ec2::types::InstanceType as Ec2InstanceType;

    // Use small, cheap instance type for pre-warming
    let instance_type = "t3.micro";

    let response = client
        .run_instances()
        .image_id(ami_id)
        .instance_type(Ec2InstanceType::from(instance_type))
        .min_count(1)
        .max_count(1)
        .placement(
            aws_sdk_ec2::types::Placement::builder()
                .availability_zone(availability_zone)
                .build(),
        )
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Instance)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("Name")
                        .value("runctl-prewarm-temp")
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:temp")
                        .value("true")
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create temporary instance: {}", e)))?;

    let instance_id = response
        .instances()
        .first()
        .and_then(|inst| inst.instance_id())
        .ok_or_else(|| TrainctlError::Aws("No instance ID in response".to_string()))?
        .to_string();

    Ok(instance_id)
}

async fn create_snapshot(
    volume_id: String,
    description: Option<String>,
    name: Option<String>,
    client: &Ec2Client,
) -> Result<()> {
    let desc = description.unwrap_or_else(|| "runctl snapshot".to_string());

    info!("Creating snapshot of volume {}", volume_id);

    let request = client
        .create_snapshot()
        .volume_id(&volume_id)
        .description(&desc);

    let response = request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create snapshot: {}", e)))?;

    let snapshot_id = response
        .snapshot_id()
        .ok_or_else(|| TrainctlError::Aws("Snapshot ID not in response".to_string()))?;

    // Add name tag if provided
    if let Some(name_val) = name {
        client
            .create_tags()
            .resources(snapshot_id)
            .tags(
                aws_sdk_ec2::types::Tag::builder()
                    .key("Name")
                    .value(&name_val)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to tag snapshot: {}", e)))?;
    }

    println!("Created snapshot: {}", snapshot_id);
    println!("   Volume: {}", volume_id);
    println!("   Description: {}", desc);

    Ok(())
}

async fn list_snapshots(
    volume_id_filter: Option<String>,
    detailed: bool,
    client: &Ec2Client,
) -> Result<()> {
    let mut request = client.describe_snapshots();

    // Filter by volume if specified
    if let Some(vol_id) = volume_id_filter {
        request = request.filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("volume-id")
                .values(&vol_id)
                .build(),
        );
    }

    // Only show our snapshots (by default)
    request = request.owner_ids("self");

    let response = request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list snapshots: {}", e)))?;

    let snapshots = response.snapshots();

    if detailed {
        println!(
            "{:<25} {:<20} {:<10} {:<15} {:<20} {:<30}",
            "Snapshot ID", "Volume ID", "Size (GB)", "State", "Start Time", "Description"
        );
        println!("{}", "-".repeat(130));
    } else {
        println!(
            "{:<25} {:<20} {:<10} {:<15}",
            "Snapshot ID", "Volume ID", "Size (GB)", "State"
        );
        println!("{}", "-".repeat(75));
    }

    for snapshot in snapshots {
        let snap_id = snapshot.snapshot_id().unwrap_or("unknown");
        let vol_id = snapshot.volume_id().unwrap_or("unknown");
        let size = snapshot.volume_size().unwrap_or(0);
        let state = snapshot
            .state()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "unknown".to_string());

        if detailed {
            let start_time = snapshot
                .start_time()
                .and_then(|t| t.to_millis().ok())
                .and_then(|ms| chrono::DateTime::from_timestamp(ms / 1000, 0))
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "-".to_string());
            let desc = snapshot.description().unwrap_or("-");

            println!(
                "{:<25} {:<20} {:<10} {:<15} {:<20} {:<30}",
                snap_id, vol_id, size, state, start_time, desc
            );
        } else {
            println!("{:<25} {:<20} {:<10} {:<15}", snap_id, vol_id, size, state);
        }
    }

    Ok(())
}

async fn restore_from_snapshot(
    snapshot_id: String,
    size: Option<i32>,
    volume_type: String,
    availability_zone: Option<String>,
    name: Option<String>,
    client: &Ec2Client,
) -> Result<()> {
    // Get snapshot info
    let response = client
        .describe_snapshots()
        .snapshot_ids(&snapshot_id)
        .owner_ids("self")
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe snapshot: {}", e)))?;

    let snapshot = response
        .snapshots()
        .first()
        .ok_or_else(|| TrainctlError::Aws(format!("Snapshot not found: {}", snapshot_id)))?;

    let vol_size = size.unwrap_or(snapshot.volume_size().unwrap_or(100));
    let az = availability_zone
        .or_else(|| snapshot.availability_zone().map(|s| s.to_string()))
        .ok_or_else(|| TrainctlError::Aws("Availability zone required".to_string()))?;

    let vol_type = match volume_type.as_str() {
        "gp3" => VolumeType::Gp3,
        "gp2" => VolumeType::Gp2,
        "io2" => VolumeType::Io2,
        "st1" => VolumeType::St1,
        "sc1" => VolumeType::Sc1,
        _ => {
            return Err(TrainctlError::Validation {
                field: "volume_type".to_string(),
                reason: format!("Invalid volume type: {}", volume_type),
            })
        }
    };

    info!(
        "Restoring volume from snapshot {} (size={}GB, type={}, az={})",
        snapshot_id, vol_size, volume_type, az
    );

    let mut request = client
        .create_volume()
        .snapshot_id(&snapshot_id)
        .size(vol_size)
        .volume_type(vol_type)
        .availability_zone(&az);

    // Add tags
    let mut tags = vec![
        ("CreatedBy".to_string(), "runctl".to_string()),
        ("RestoredFrom".to_string(), snapshot_id.clone()),
    ];
    if let Some(name_val) = &name {
        tags.push(("Name".to_string(), name_val.clone()));
    }

    let tag_spec = aws_sdk_ec2::types::TagSpecification::builder()
        .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
        .set_tags(Some(
            tags.into_iter()
                .map(|(k, v)| aws_sdk_ec2::types::Tag::builder().key(k).value(v).build())
                .collect(),
        ))
        .build();

    request = request.tag_specifications(tag_spec);

    let response = request.send().await.map_err(|e| {
        TrainctlError::Aws(format!("Failed to restore volume from snapshot: {}", e))
    })?;

    let volume_id = response
        .volume_id()
        .ok_or_else(|| TrainctlError::Aws("Volume ID not in response".to_string()))?;

    println!(
        "Restored volume {} from snapshot {}",
        volume_id, snapshot_id
    );
    println!("   Size: {} GB", vol_size);
    println!("   Type: {}", volume_type);
    println!("   AZ: {}", az);

    Ok(())
}
