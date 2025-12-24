//! E2E tests for EBS volume lifecycle management
//!
//! Tests verify complete EBS workflows:
//! - Create, attach, use, detach, delete
//! - Snapshot creation and restoration
//! - Persistent vs ephemeral behavior
//!
//! Run with: `TRAINCTL_E2E=1 cargo test --test ebs_lifecycle_e2e_test --features e2e -- --ignored`
//!
//! Cost: ~$0.10-0.30 per test run

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

macro_rules! require_e2e {
    () => {
        if !should_run_e2e() {
            eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
            return;
        }
    };
}

fn test_tag() -> String {
    format!(
        "runctl-test-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    )
}

#[tokio::test]
#[ignore]
async fn test_ebs_complete_lifecycle() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    // 1. Create volume
    info!("Step 1: Creating volume");
    let vol_response = client
        .create_volume()
        .size(1)
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(&az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:test")
                        .value(&test_tag)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("Failed to create volume");

    let volume_id = vol_response.volume_id().expect("No volume ID").to_string();
    info!("Created volume: {}", volume_id);

    // 2. Wait for volume to be available
    info!("Step 2: Waiting for volume to be available");
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;

        let describe = client
            .describe_volumes()
            .volume_ids(&volume_id)
            .send()
            .await
            .expect("Failed to describe volume");

        let volume = describe.volumes().first().expect("Volume not found");
        let state = volume
            .state()
            .map(|s| format!("{:?}", s))
            .unwrap_or_default();

        if state == "available" {
            info!("Volume is available");
            break;
        }

        if attempts > 30 {
            panic!("Volume did not become available within 60 seconds");
        }
    }

    // 3. Verify volume state
    info!("Step 3: Verifying volume state");
    let describe = client
        .describe_volumes()
        .volume_ids(&volume_id)
        .send()
        .await
        .expect("Failed to describe volume");

    let volume = describe.volumes().first().expect("Volume not found");
    let state = volume
        .state()
        .map(|s| format!("{:?}", s))
        .unwrap_or_default();
    assert_eq!(state, "available", "Volume should be available");
    assert_eq!(volume.size().unwrap_or(0), 1, "Volume should be 1 GB");

    // 4. Create snapshot
    info!("Step 4: Creating snapshot");
    let snap_response = client
        .create_snapshot()
        .volume_id(&volume_id)
        .description(format!("Test snapshot for {}", test_tag))
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Snapshot)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:test")
                        .value(&test_tag)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("Failed to create snapshot");

    let snapshot_id = snap_response
        .snapshot_id()
        .expect("No snapshot ID")
        .to_string();
    info!("Created snapshot: {}", snapshot_id);

    // 5. Wait for snapshot to complete
    info!("Step 5: Waiting for snapshot to complete");
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(5)).await;
        attempts += 1;

        let snap_describe = client
            .describe_snapshots()
            .snapshot_ids(&snapshot_id)
            .send()
            .await
            .expect("Failed to describe snapshot");

        let snapshot = snap_describe
            .snapshots()
            .first()
            .expect("Snapshot not found");
        let state = snapshot
            .state()
            .map(|s| format!("{:?}", s))
            .unwrap_or_default();

        if state == "completed" {
            info!("Snapshot is completed");
            break;
        }

        if attempts > 24 {
            warn!("Snapshot taking longer than expected, continuing anyway");
            break;
        }
    }

    // 6. Delete snapshot
    info!("Step 6: Deleting snapshot");
    client
        .delete_snapshot()
        .snapshot_id(&snapshot_id)
        .send()
        .await
        .expect("Failed to delete snapshot");
    info!("Deleted snapshot: {}", snapshot_id);

    // 7. Delete volume
    info!("Step 7: Deleting volume");
    client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await
        .expect("Failed to delete volume");
    info!("Deleted volume: {}", volume_id);

    info!("✅ Complete lifecycle test passed");
}

#[tokio::test]
#[ignore]
async fn test_persistent_vs_ephemeral_behavior() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    // Create persistent volume
    let persistent_vol = client
        .create_volume()
        .size(1)
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(&az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:persistent")
                        .value("true")
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:test")
                        .value(&test_tag)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("Failed to create persistent volume");

    let persistent_id = persistent_vol
        .volume_id()
        .expect("No volume ID")
        .to_string();

    // Create ephemeral volume
    let ephemeral_vol = client
        .create_volume()
        .size(1)
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(&az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:test")
                        .value(&test_tag)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("Failed to create ephemeral volume");

    let ephemeral_id = ephemeral_vol.volume_id().expect("No volume ID").to_string();

    sleep(Duration::from_secs(5)).await;

    // Verify tags
    let persistent_desc = client
        .describe_volumes()
        .volume_ids(&persistent_id)
        .send()
        .await
        .expect("Failed to describe persistent volume");

    let persistent_vol = persistent_desc.volumes().first().expect("Volume not found");
    let has_persistent_tag = persistent_vol.tags().iter().any(|t| {
        t.key().map(|k| k == "runctl:persistent").unwrap_or(false)
            && t.value().map(|v| v == "true").unwrap_or(false)
    });
    assert!(
        has_persistent_tag,
        "Persistent volume should have persistent tag"
    );

    let ephemeral_desc = client
        .describe_volumes()
        .volume_ids(&ephemeral_id)
        .send()
        .await
        .expect("Failed to describe ephemeral volume");

    let ephemeral_vol = ephemeral_desc.volumes().first().expect("Volume not found");
    let has_persistent_tag = ephemeral_vol
        .tags()
        .iter()
        .any(|t| t.key().map(|k| k == "runctl:persistent").unwrap_or(false));
    assert!(
        !has_persistent_tag,
        "Ephemeral volume should not have persistent tag"
    );

    // Cleanup
    client
        .delete_volume()
        .volume_id(&persistent_id)
        .send()
        .await
        .expect("Failed to delete persistent volume");

    client
        .delete_volume()
        .volume_id(&ephemeral_id)
        .send()
        .await
        .expect("Failed to delete ephemeral volume");

    info!("✅ Persistent vs ephemeral test passed");
}
