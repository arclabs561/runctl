//! Cleanup operations for resources

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::resources::utils::estimate_instance_cost;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use chrono::Utc;
use std::io::{self, Write};

/// Cleanup zombie/orphaned resources
pub async fn cleanup_zombies(dry_run: bool, force: bool, _config: &Config) -> Result<()> {
    println!("{}", "=".repeat(80));
    println!("Zombie Resource Cleanup");
    println!("{}", "=".repeat(80));

    if dry_run {
        println!("[DRY RUN MODE - No resources will be deleted]");
    }

    // Find orphaned AWS instances (running > 24 hours without tags)
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list instances: {}", e)))?;

    let mut zombies = Vec::new();
    let mut protected_instances = Vec::new();
    let cutoff = Utc::now() - chrono::Duration::hours(24);

    let reservations = response.reservations();
    for reservation in reservations {
        let instances = reservation.instances();
        for instance in instances {
            let state_str = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "unknown".to_string());
            if state_str != "running" {
                continue;
            }

            let instance_id = instance.instance_id().unwrap_or("unknown").to_string();

            // Check if protected
            let is_protected = instance.tags().iter().any(|t| {
                t.key()
                    .map(|k| {
                        k == "runctl:protected"
                            || k == "runctl:important"
                            || k == "runctl:persistent"
                    })
                    .unwrap_or(false)
                    && t.value().map(|v| v == "true").unwrap_or(false)
            });

            if is_protected {
                protected_instances.push(instance_id.clone());
                continue;
            }

            let launch_time = instance
                .launch_time()
                .and_then(|t| chrono::DateTime::from_timestamp(t.secs(), 0));

            if let Some(lt) = launch_time {
                if lt < cutoff {
                    // Check if it has runctl tags
                    let has_runctl_tag = instance
                        .tags()
                        .iter()
                        .any(|t| t.key().map(|k| k.contains("runctl")).unwrap_or(false));

                    if !has_runctl_tag {
                        zombies.push(instance_id);
                    }
                }
            }
        }
    }

    // Also check for orphaned volumes (available, no runctl tags, not persistent)
    let volumes_response = client
        .describe_volumes()
        .filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("status")
                .values("available")
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list volumes: {}", e)))?;

    let mut orphaned_volumes = Vec::new();
    for volume in volumes_response.volumes() {
        let volume_id = volume.volume_id().unwrap_or("unknown").to_string();

        // Skip persistent volumes
        let is_persistent = volume.tags().iter().any(|t| {
            t.key().map(|k| k == "runctl:persistent").unwrap_or(false)
                && t.value().map(|v| v == "true").unwrap_or(false)
        });

        if is_persistent {
            continue;
        }

        // Check if has runctl tags (if not, might be orphaned)
        let has_runctl_tag = volume
            .tags()
            .iter()
            .any(|t| t.key().map(|k| k.contains("runctl")).unwrap_or(false));

        if !has_runctl_tag {
            orphaned_volumes.push(volume_id);
        }
    }

    if zombies.is_empty() && orphaned_volumes.is_empty() {
        println!("No zombie resources found");
        if !protected_instances.is_empty() {
            println!(
                "   ({} protected instance(s) skipped)",
                protected_instances.len()
            );
        }
        return Ok(());
    }

    if !zombies.is_empty() {
        println!("\nFound {} potential zombie instance(s):", zombies.len());
        for id in &zombies {
            println!("  - {}", id);
        }
    }

    if !orphaned_volumes.is_empty() {
        println!(
            "\nFound {} orphaned volume(s) (available, not persistent):",
            orphaned_volumes.len()
        );
        for id in &orphaned_volumes {
            println!("  - {}", id);
        }
    }

    if !protected_instances.is_empty() {
        println!(
            "\nSkipped {} protected instance(s):",
            protected_instances.len()
        );
        for id in &protected_instances {
            println!("  - {} (protected)", id);
        }
    }

    if dry_run {
        println!(
            "\n[DRY RUN] Would terminate {} instance(s) and delete {} volume(s)",
            zombies.len(),
            orphaned_volumes.len()
        );
        return Ok(());
    }

    if !force {
        println!(
            "\nWARNING: This will terminate {} instance(s) and delete {} volume(s). Continue? (y/N): ",
            zombies.len(), orphaned_volumes.len()
        );
        // In real implementation, would read from stdin
        println!("  (Use --force to skip confirmation)");
        return Ok(());
    }

    // Terminate zombie instances
    for id in &zombies {
        client
            .terminate_instances()
            .instance_ids(id.as_str())
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to terminate {}: {}", id, e)))?;
        println!("  Terminated {}", id);
    }

    // Delete orphaned volumes
    for id in &orphaned_volumes {
        match client.delete_volume().volume_id(id.as_str()).send().await {
            Ok(_) => println!("  Deleted volume {}", id),
            Err(e) => println!("  Failed to delete volume {}: {}", id, e),
        }
    }

    println!("\nCleanup complete");
    Ok(())
}

/// Stop all running instances
pub async fn stop_all_instances(
    dry_run: bool,
    force: bool,
    _platform: String,
    _config: &Config,
) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // Get all running instances
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

    let mut instance_ids = Vec::new();
    let mut instance_info = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                let instance_type = instance
                    .instance_type()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                let cost_per_hour = estimate_instance_cost(&instance_type);

                instance_ids.push(instance_id.to_string());
                instance_info.push((instance_id.to_string(), instance_type, cost_per_hour));
            }
        }
    }

    if instance_ids.is_empty() {
        println!("No running instances found");
        return Ok(());
    }

    // Calculate cost savings
    let total_hourly_cost: f64 = instance_info.iter().map(|(_, _, cost)| cost).sum();
    let nightly_savings = total_hourly_cost * 8.0; // Assume 8 hours of sleep

    println!("RUNNING INSTANCES:");
    println!("{}", "-".repeat(80));
    for (id, inst_type, cost) in &instance_info {
        println!("  {}  {}  ${:.4}/hr", id, inst_type, cost);
    }
    println!("{}", "-".repeat(80));
    println!("Total: {} instance(s)", instance_ids.len());
    println!("Hourly cost: ${:.2}/hr", total_hourly_cost);
    println!("Estimated savings (8h): ${:.2}", nightly_savings);
    println!();

    if dry_run {
        println!("DRY RUN: Would stop {} instance(s)", instance_ids.len());
        return Ok(());
    }

    if !force {
        print!("Stop all {} instance(s)? (y/N): ", instance_ids.len());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            println!("Cancelled");
            return Ok(());
        }
    }

    println!("Stopping instances gracefully...");
    let ssm_client = aws_sdk_ssm::Client::new(&aws_config);

    for instance_id in &instance_ids {
        // First, gracefully stop any training jobs
        let graceful_stop_cmd = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        kill -TERM $PID 2>/dev/null || true
        for i in {1..30}; do
            if ! ps -p $PID > /dev/null 2>&1; then break; fi
            sleep 1
        done
        kill -9 $PID 2>/dev/null || true
    fi
else
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training\|python.*main.py" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        kill -TERM $TRAINING_PID 2>/dev/null || true
        for i in {1..30}; do
            if ! ps -p $TRAINING_PID > /dev/null 2>&1; then break; fi
            sleep 1
        done
        kill -9 $TRAINING_PID 2>/dev/null || true
    fi
fi
"#;

        // Try graceful shutdown (ignore errors - instance might not have SSM)
        let _ = crate::aws_utils::execute_ssm_command(&ssm_client, instance_id, graceful_stop_cmd)
            .await;

        // Then stop the instance
        match client
            .stop_instances()
            .instance_ids(instance_id)
            .send()
            .await
        {
            Ok(_) => {
                println!("  Stop requested: {}", instance_id);
            }
            Err(e) => {
                eprintln!("  ERROR: Failed to stop {}: {}", instance_id, e);
            }
        }
    }

    println!();
    println!("Stop requested for {} instance(s)", instance_ids.len());
    println!("Instances can be restarted later");

    Ok(())
}
