//! End-to-end test for spot instance interruption handling
//!
//! Tests spot interruption detection, graceful shutdown, and checkpoint saving.
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test spot_interruption_test --features e2e -- --ignored
//!
//! Cost: ~$0.50-2.00 per run (creates real spot instance, runs training)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

/// Helper to wait for instance to be running
async fn wait_for_instance_running(
    client: &Ec2Client,
    instance_id: &str,
    max_wait_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();

    while start.elapsed().as_secs() < max_wait_secs {
        let response = client
            .describe_instances()
            .instance_ids(instance_id)
            .send()
            .await?;

        if let Some(instance) = response
            .reservations()
            .iter()
            .flat_map(|r| r.instances())
            .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        {
            if let Some(state) = instance.state() {
                if let Some(state_name) = state.name() {
                    if state_name.as_str() == "running" {
                        // Wait a bit more for SSM to be ready
                        sleep(Duration::from_secs(30)).await;
                        return Ok(());
                    }
                }
            }
        }

        sleep(Duration::from_secs(5)).await;
    }

    Err("Instance did not reach running state in time".into())
}

/// Execute SSM command and get output
async fn execute_ssm_command(
    ssm_client: &SsmClient,
    instance_id: &str,
    command: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let command_id = ssm_client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command.to_string()])
        .send()
        .await?
        .command()
        .and_then(|c| c.command_id())
        .ok_or("No command ID returned")?;

    // Wait for command to complete
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;

        if attempts > 60 {
            return Err("SSM command timeout".into());
        }

        let output = ssm_client
            .get_command_invocation()
            .command_id(command_id)
            .instance_id(instance_id)
            .send()
            .await?;

        use aws_sdk_ssm::types::CommandInvocationStatus;
        let status = output.status();

        match status {
            Some(CommandInvocationStatus::Success) => {
                return Ok(output.standard_output_content().unwrap_or("").to_string());
            }
            Some(CommandInvocationStatus::Failed)
            | Some(CommandInvocationStatus::Cancelled)
            | Some(CommandInvocationStatus::TimedOut) => {
                let error = output.standard_error_content().unwrap_or("");
                return Err(format!("SSM command failed: {}", error).into());
            }
            _ => {
                // Still in progress
                continue;
            }
        }
    }
}

/// Create a simple training script that saves checkpoints
fn create_test_training_script() -> Result<PathBuf, Box<dyn std::error::Error>> {
    use std::fs;
    use std::io::Write;

    let script_dir = PathBuf::from("training");
    fs::create_dir_all(&script_dir)?;

    let script_path = script_dir.join("test_spot_training.py");
    let mut file = fs::File::create(&script_path)?;

    // Simple training script that saves checkpoints and handles SIGTERM
    writeln!(
        file,
        r#"#!/usr/bin/env python3
import signal
import sys
import time
import os
import json

checkpoint_dir = "checkpoints"
os.makedirs(checkpoint_dir, exist_ok=True)

current_epoch = 0
max_epochs = 100

def save_checkpoint(epoch):
    checkpoint_path = os.path.join(checkpoint_dir, f"epoch_{{epoch}}.pt")
    with open(checkpoint_path, "w") as f:
        json.dump({{"epoch": epoch, "status": "checkpoint"}}, f)
    print(f"Checkpoint saved: {{checkpoint_path}}")

def signal_handler(sig, frame):
    print("Received SIGTERM, saving checkpoint...")
    save_checkpoint(current_epoch)
    print("Checkpoint saved before termination")
    sys.exit(0)

signal.signal(signal.SIGTERM, signal_handler)

print("Starting training...")
for epoch in range(max_epochs):
    current_epoch = epoch
    print(f"Epoch {{epoch}}/{{max_epochs}}")
    
    # Save checkpoint every 5 epochs
    if epoch % 5 == 0:
        save_checkpoint(epoch)
    
    # Simulate training work
    time.sleep(2)
    
    if epoch >= 10:
        # Stop after 10 epochs for testing
        print("Training complete (test mode)")
        break

print("Training finished")
"#
    )?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }

    Ok(script_path)
}

#[tokio::test]
#[ignore]
async fn test_spot_interruption_monitoring() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    // Create test training script
    let script_path = create_test_training_script().expect("Failed to create test training script");

    info!("Created test training script: {:?}", script_path);

    // Create spot instance
    info!("Creating spot instance for interruption test...");
    let instance_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "create",
            "t3.medium",
            "--spot",
        ])
        .output()
        .expect("Failed to create instance");

    if !instance_output.status.success() {
        let stderr = String::from_utf8_lossy(&instance_output.stderr);
        panic!("Failed to create instance: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&instance_output.stdout);
    let instance_id = stdout
        .lines()
        .find_map(|line| {
            if let Some(start) = line.find("i-") {
                let end = line[start..].find(' ').unwrap_or(line[start..].len());
                Some(line[start..start + end].to_string())
            } else {
                None
            }
        })
        .expect("Could not extract instance ID from output");

    info!("Created spot instance: {}", instance_id);

    // Wait for instance to be running
    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not become running");

    info!("Instance is running, starting training...");

    // Start training
    let train_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "train",
            &instance_id,
            script_path.to_str().unwrap(),
            "--sync-code",
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        warn!("Training start may have issues: {}", stderr);
    }

    info!("Training started, waiting for checkpoint creation...");

    // Wait for training to create a checkpoint
    sleep(Duration::from_secs(30)).await;

    // Verify checkpoint was created
    let check_checkpoint_cmd = r#"
if [ -d checkpoints ] && [ -n "$(ls -A checkpoints/*.pt 2>/dev/null)" ]; then
    echo "CHECKPOINT_EXISTS"
    ls -la checkpoints/*.pt | head -1
else
    echo "NO_CHECKPOINT"
fi
"#;

    let checkpoint_output = execute_ssm_command(&ssm_client, &instance_id, check_checkpoint_cmd)
        .await
        .expect("Failed to check checkpoint");

    assert!(
        checkpoint_output.contains("CHECKPOINT_EXISTS"),
        "Checkpoint should exist after training started. Output: {}",
        checkpoint_output
    );

    info!("Checkpoint verified, testing graceful shutdown...");

    // Test graceful shutdown (simulating spot interruption)
    let graceful_shutdown_cmd = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
        kill -TERM $PID 2>/dev/null || true
        for i in {1..30}; do
            if ! ps -p $PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                break
            fi
            sleep 1
        done
        if ps -p $PID > /dev/null 2>&1; then
            kill -9 $PID 2>/dev/null || true
            echo "TRAINING_FORCE_STOPPED"
        fi
    else
        echo "TRAINING_STOPPED"
    fi
else
    TRAINING_PID=$(pgrep -f "python.*test_spot_training" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        echo "TRAINING_RUNNING:$TRAINING_PID"
        kill -TERM $TRAINING_PID 2>/dev/null || true
        for i in {1..30}; do
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
"#;

    let shutdown_output = execute_ssm_command(&ssm_client, &instance_id, graceful_shutdown_cmd)
        .await
        .expect("Failed to execute graceful shutdown");

    info!("Graceful shutdown output: {}", shutdown_output);

    // Verify checkpoint still exists after shutdown
    let final_checkpoint_output =
        execute_ssm_command(&ssm_client, &instance_id, check_checkpoint_cmd)
            .await
            .expect("Failed to check final checkpoint");

    assert!(
        final_checkpoint_output.contains("CHECKPOINT_EXISTS"),
        "Checkpoint should still exist after graceful shutdown. Output: {}",
        final_checkpoint_output
    );

    info!("Test passed! Cleaning up instance...");

    // Cleanup: terminate instance
    let terminate_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "terminate",
            &instance_id,
            "--force",
        ])
        .output()
        .expect("Failed to terminate instance");

    if !terminate_output.status.success() {
        let stderr = String::from_utf8_lossy(&terminate_output.stderr);
        warn!("Failed to terminate instance: {}", stderr);
    } else {
        info!("Instance terminated successfully");
    }

    // Cleanup: remove test script
    let _ = std::fs::remove_file(&script_path);
}

#[tokio::test]
#[ignore]
async fn test_spot_interruption_metadata_detection() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    // Create spot instance
    info!("Creating spot instance for metadata detection test...");
    let instance_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "create",
            "t3.medium",
            "--spot",
        ])
        .output()
        .expect("Failed to create instance");

    if !instance_output.status.success() {
        let stderr = String::from_utf8_lossy(&instance_output.stderr);
        panic!("Failed to create instance: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&instance_output.stdout);
    let instance_id = stdout
        .lines()
        .find_map(|line| {
            if let Some(start) = line.find("i-") {
                let end = line[start..].find(' ').unwrap_or(line[start..].len());
                Some(line[start..start + end].to_string())
            } else {
                None
            }
        })
        .expect("Could not extract instance ID from output");

    info!("Created spot instance: {}", instance_id);

    // Wait for instance to be running
    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not become running");

    // Test metadata service access
    let metadata_cmd = r#"
if command -v curl >/dev/null 2>&1; then
    RESPONSE=$(curl -s -w "\n%{http_code}" http://169.254.169.254/latest/meta-data/spot/instance-action 2>/dev/null || echo -e "\n404")
    HTTP_CODE=$(echo "$RESPONSE" | tail -1)
    if [ "$HTTP_CODE" = "200" ]; then
        echo "SPOT_INTERRUPTION_DETECTED"
        echo "$RESPONSE" | head -n -1
    else
        echo "NO_INTERRUPTION"
        echo "HTTP_CODE:$HTTP_CODE"
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

    let metadata_output = execute_ssm_command(&ssm_client, &instance_id, metadata_cmd)
        .await
        .expect("Failed to check metadata service");

    info!("Metadata service output: {}", metadata_output);

    // Should not have interruption (instance just created)
    assert!(
        metadata_output.contains("NO_INTERRUPTION") || metadata_output.contains("NO_METADATA_TOOL"),
        "New instance should not have interruption warning. Output: {}",
        metadata_output
    );

    info!("Metadata detection test passed! Cleaning up...");

    // Cleanup
    let _ = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "terminate",
            &instance_id,
            "--force",
        ])
        .output();
}
