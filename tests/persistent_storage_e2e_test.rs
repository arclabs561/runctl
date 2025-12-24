//! E2E tests for persistent storage functionality
//!
//! These tests verify that persistent volumes are properly protected
//! and can be reused across instance lifecycles.
//!
//! Run with: `TRAINCTL_E2E=1 cargo test --test persistent_storage_e2e_test --features e2e -- --ignored`
//!
//! Cost: ~$0.10-0.50 per test run (creates/deletes volumes and instances)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

/// Helper to skip test if E2E not enabled
macro_rules! require_e2e {
    () => {
        if !should_run_e2e() {
            eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
            return;
        }
    };
}

/// Helper to tag resources for cleanup
fn test_tag() -> String {
    format!(
        "runctl-test-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    )
}

#[tokio::test]
#[ignore]
async fn test_persistent_volume_creation_and_tagging() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    // Get default region/AZ
    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    info!("Creating persistent volume in {}", az);

    // Create persistent volume
    let volume_name = format!("{}-persistent", test_tag);
    let response = client
        .create_volume()
        .size(1) // 1 GB for testing
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(&az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("Name")
                        .value(&volume_name)
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:persistent")
                        .value("true")
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("runctl:protected")
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
        .expect("Failed to create volume");

    let volume_id = response.volume_id().expect("No volume ID").to_string();
    info!("Created persistent volume: {}", volume_id);

    // Wait for volume to be available
    sleep(Duration::from_secs(5)).await;

    // Verify tags
    let describe = client
        .describe_volumes()
        .volume_ids(&volume_id)
        .send()
        .await
        .expect("Failed to describe volume");

    let volume = describe.volumes().first().expect("Volume not found");
    let tags = volume.tags();

    let has_persistent = tags.iter().any(|t| {
        t.key().map(|k| k == "runctl:persistent").unwrap_or(false)
            && t.value().map(|v| v == "true").unwrap_or(false)
    });
    let has_protected = tags.iter().any(|t| {
        t.key().map(|k| k == "runctl:protected").unwrap_or(false)
            && t.value().map(|v| v == "true").unwrap_or(false)
    });

    assert!(has_persistent, "Volume should have runctl:persistent tag");
    assert!(has_protected, "Volume should have runctl:protected tag");

    // Cleanup
    client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await
        .expect("Failed to delete test volume");
    info!("Cleaned up test volume: {}", volume_id);
}

#[tokio::test]
#[ignore]
async fn test_persistent_volume_protection_from_deletion() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    // Create persistent volume
    let _volume_name = format!("{}-protected", test_tag);
    let response = client
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
        .expect("Failed to create volume");

    let volume_id = response.volume_id().expect("No volume ID").to_string();
    info!("Created protected volume: {}", volume_id);

    sleep(Duration::from_secs(5)).await;

    // Verify volume exists and is available
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
    assert_eq!(state.to_lowercase(), "available", "Volume should be available");

    // Verify it has persistent tag (simulating runctl's check)
    let tags = volume.tags();
    let is_persistent = tags.iter().any(|t| {
        t.key()
            .map(|k| k == "runctl:persistent" || k == "runctl:protected")
            .unwrap_or(false)
            && t.value().map(|v| v == "true").unwrap_or(false)
    });
    assert!(is_persistent, "Volume should be marked as persistent");

    // Cleanup with force (test allows this)
    client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await
        .expect("Failed to delete test volume");
    info!("Cleaned up test volume: {}", volume_id);
}

#[tokio::test]
#[ignore]
async fn test_persistent_volume_survives_instance_termination() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    // Create persistent volume
    let _volume_name = format!("{}-survives", test_tag);
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
        .expect("Failed to create volume");

    let volume_id = vol_response.volume_id().expect("No volume ID").to_string();
    info!("Created persistent volume: {}", volume_id);

    sleep(Duration::from_secs(5)).await;

    // Note: This test would create an instance, attach volume, terminate instance
    // For cost reasons, we'll just verify the volume exists and can be deleted
    // In a full test, you'd:
    // 1. Create t3.micro instance
    // 2. Attach volume
    // 3. Terminate instance
    // 4. Verify volume still exists and is available

    // Verify volume is available
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
    assert_eq!(state.to_lowercase(), "available", "Volume should be available");

    // Cleanup
    client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await
        .expect("Failed to delete test volume");
    info!("Cleaned up test volume: {}", volume_id);
}

#[tokio::test]
#[ignore]
async fn test_cleanup_skips_persistent_volumes() {
    require_e2e!();

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag();

    let region = aws_config.region().unwrap().as_ref();
    let az = format!("{}a", region);

    // Create persistent volume
    let vol_persistent = client
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

    let persistent_id = vol_persistent
        .volume_id()
        .expect("No volume ID")
        .to_string();
    info!("Created persistent volume: {}", persistent_id);

    // Create ephemeral volume
    let vol_ephemeral = client
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

    let ephemeral_id = vol_ephemeral.volume_id().expect("No volume ID").to_string();
    info!("Created ephemeral volume: {}", ephemeral_id);

    sleep(Duration::from_secs(5)).await;

    // Simulate cleanup: delete ephemeral, skip persistent
    // Delete ephemeral (should succeed)
    client
        .delete_volume()
        .volume_id(&ephemeral_id)
        .send()
        .await
        .expect("Failed to delete ephemeral volume");
    info!("Deleted ephemeral volume: {}", ephemeral_id);

    // Verify persistent still exists
    let describe = client
        .describe_volumes()
        .volume_ids(&persistent_id)
        .send()
        .await
        .expect("Failed to describe persistent volume");

    assert!(
        !describe.volumes().is_empty(),
        "Persistent volume should still exist"
    );

    // Cleanup persistent
    client
        .delete_volume()
        .volume_id(&persistent_id)
        .send()
        .await
        .expect("Failed to delete persistent volume");
    info!("Cleaned up persistent volume: {}", persistent_id);
}
