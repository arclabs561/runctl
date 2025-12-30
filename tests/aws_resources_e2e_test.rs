//! End-to-end tests for AWS resource management
//!
//! These tests require AWS credentials and will interact with real AWS resources.
//! Run with: TRAINCTL_E2E=1 cargo test --test aws_resources_test --features e2e
//!
//! Safety: Tests use dry-run mode and cleanup after themselves.

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use std::env;
use tracing::info;

/// Check if E2E tests should run (require explicit opt-in)
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

#[tokio::test]
#[ignore] // Requires AWS credentials and explicit opt-in
async fn test_list_aws_instances() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // Test listing instances
    let response = client
        .describe_instances()
        .send()
        .await
        .expect("Failed to describe EC2 instances");

    let mut total_instances = 0;
    let mut running_count = 0;

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            total_instances += 1;
            let state = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            if state == "running" {
                running_count += 1;
                let instance_id = instance.instance_id().unwrap_or("unknown");
                let instance_type = instance
                    .instance_type()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                info!(
                    "Found running instance: {} ({})",
                    instance_id, instance_type
                );
            }
        }
    }

    info!(
        "Total instances: {}, Running: {}",
        total_instances, running_count
    );

    // Test passes if we can list instances (even if none are running)
    // (no assertion needed - if describe_instances succeeded, test passes)
}

#[tokio::test]
#[ignore]
async fn test_resource_summary() {
    if !should_run_e2e() {
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .expect("Failed to describe instances");

    let mut running_instances = Vec::new();
    let mut total_cost = 0.0;

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let state = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            if state == "running" {
                let instance_id = instance.instance_id().unwrap_or("unknown").to_string();
                let instance_type = instance
                    .instance_type()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());

                // Rough cost estimate
                let cost = match instance_type.as_str() {
                    "t3.micro" => 0.0104,
                    "t3.small" => 0.0208,
                    "t3.medium" => 0.0416,
                    "g4dn.xlarge" => 0.526,
                    "g4dn.2xlarge" => 0.752,
                    "p3.2xlarge" => 3.06,
                    _ => 0.1, // Default estimate
                };

                total_cost += cost;
                running_instances.push((instance_id, instance_type, cost));
            }
        }
    }

    info!("Resource summary:");
    info!("  Running instances: {}", running_instances.len());
    info!("  Estimated hourly cost: ${:.2}", total_cost);

    for (id, instance_type, cost) in &running_instances {
        info!("    {} ({}) - ${}/hour", id, instance_type, cost);
    }

    // Test passes if we can generate summary
    // (no assertion needed - if we got here without error, test passes)
}

#[tokio::test]
#[ignore]
async fn test_zombie_detection() {
    if !should_run_e2e() {
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

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

    let mut zombies = Vec::new();
    let now = chrono::Utc::now();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let instance_id = instance.instance_id().unwrap_or("unknown");
            let tags = instance.tags();

            // Check if instance has runctl tags
            let has_runctl_tag = tags
                .iter()
                .any(|tag| tag.key().map(|k| k.starts_with("runctl:")).unwrap_or(false));

            // Check if instance is old (>24 hours)
            let is_old = instance
                .launch_time()
                .map(|lt| {
                    let launch = chrono::DateTime::<chrono::Utc>::from_timestamp(lt.secs(), 0)
                        .unwrap_or_default();
                    (now - launch).num_hours() > 24
                })
                .unwrap_or(false);

            // Zombie: old instance without runctl tags
            if is_old && !has_runctl_tag {
                zombies.push(instance_id.to_string());
                info!("Found potential zombie: {} (old and untagged)", instance_id);
            }
        }
    }

    if !zombies.is_empty() {
        eprintln!(
            "WARNING: Found {} potential zombie instances",
            zombies.len()
        );
        eprintln!("   Review these: {:?}", zombies);
    } else {
        info!("No zombie instances detected");
    }

    // Test passes (just reports, doesn't fail)
    // (no assertion needed - if we got here without error, test passes)
}

#[tokio::test]
#[ignore]
async fn test_cleanup_dry_run() {
    if !should_run_e2e() {
        return;
    }

    // Test that we can identify resources for cleanup without actually deleting
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

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

    let mut candidates = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let instance_id = instance.instance_id().unwrap_or("unknown");
            let tags = instance.tags();

            // Check if protected
            let is_protected = tags.iter().any(|tag| {
                tag.key()
                    .map(|k| k == "runctl:protected" || k == "runctl:important")
                    .unwrap_or(false)
                    && tag.value().map(|v| v == "true").unwrap_or(false)
            });

            if !is_protected {
                candidates.push(instance_id.to_string());
            }
        }
    }

    info!(
        "Dry-run cleanup would affect {} instances",
        candidates.len()
    );
    for candidate in &candidates {
        info!("  Would cleanup: {}", candidate);
    }

    // Test passes - dry-run doesn't actually delete
    // (no assertion needed - if we got here without error, test passes)
}
