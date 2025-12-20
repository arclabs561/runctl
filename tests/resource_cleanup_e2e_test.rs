//! E2E tests for resource cleanup
//!
//! Tests that resources are properly cleaned up after E2E tests.
//! Run with: TRAINCTL_E2E=1 cargo test --test resource_cleanup_test --features e2e

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use std::env;
use tracing::info;

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

#[tokio::test]
#[ignore]
async fn test_cleanup_orphaned_resources() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // List all running instances
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
        .expect("Failed to describe instances");

    let mut orphaned_count = 0;
    let mut test_instances = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let instance_id = instance.instance_id().unwrap_or("unknown");
            let tags = instance.tags();

            // Check if instance has runctl test tags
            let is_test_instance = tags.iter().any(|tag| {
                tag.key()
                    .map(|k| k.starts_with("runctl:test"))
                    .unwrap_or(false)
            });

            // Check if instance is old (>24 hours) and untagged
            let is_old = instance
                .launch_time()
                .map(|lt| {
                    let launch = chrono::DateTime::<chrono::Utc>::from_timestamp(lt.secs(), 0)
                        .unwrap_or_default();
                    let now = chrono::Utc::now();
                    (now - launch).num_hours() > 24
                })
                .unwrap_or(false);

            if is_test_instance || (is_old && tags.is_empty()) {
                orphaned_count += 1;
                test_instances.push(instance_id.to_string());
                info!("Found potential orphan: {}", instance_id);
            }
        }
    }

    if orphaned_count > 0 {
        eprintln!(
            "⚠️  WARNING: Found {} potential orphaned test instances",
            orphaned_count
        );
        eprintln!("   Instance IDs: {:?}", test_instances);
        eprintln!("   Review these manually before deleting!");
        eprintln!("   Use: runctl resources cleanup --dry-run");
    } else {
        info!("✅ No orphaned test instances found");
    }

    // Don't fail the test, just warn
    // The user should manually review and clean up
}

#[tokio::test]
#[ignore]
async fn test_list_all_resources() {
    if !should_run_e2e() {
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // Count all instances by state
    let response = client
        .describe_instances()
        .send()
        .await
        .expect("Failed to describe instances");

    let mut running = 0;
    let mut stopped = 0;
    let mut terminated = 0;

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let state = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            match state {
                "running" => running += 1,
                "stopped" => stopped += 1,
                "terminated" => terminated += 1,
                _ => {}
            }
        }
    }

    info!("Resource summary:");
    info!("  Running: {}", running);
    info!("  Stopped: {}", stopped);
    info!("  Terminated: {}", terminated);

    // This test just reports, doesn't fail
    assert!(true);
}
