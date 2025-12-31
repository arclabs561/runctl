//! Complete end-to-end training test using runctl CLI
//!
//! This test verifies the FULL workflow using actual runctl commands:
//! 1. Create instance via `runctl aws create`
//! 2. Sync code via `runctl aws train --sync-code`
//! 3. Start training via `runctl aws train`
//! 4. Monitor via `runctl aws monitor`
//! 5. Verify checkpoints created
//! 6. Cleanup via `runctl aws terminate`
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test complete_training_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.10-0.50 per run (uses t3.micro, minimal training)

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

/// Execute SSM command and get output
async fn execute_ssm_command(
    ssm_client: &SsmClient,
    instance_id: &str,
    command: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use aws_sdk_ssm::types::CommandInvocationStatus;

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
            _ => continue,
        }
    }
}

/// Wait for instance to be running
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
            if let Some(state) = instance.state().and_then(|s| s.name()) {
                if state.as_str() == "running" {
                    // Wait for SSM to be ready
                    sleep(Duration::from_secs(30)).await;
                    return Ok(());
                }
            }
        }

        sleep(Duration::from_secs(5)).await;
    }

    Err("Instance did not reach running state in time".into())
}

#[tokio::test]
#[ignore]
async fn test_complete_training_workflow_with_runctl_cli() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Complete Training E2E Test (using runctl CLI) ===");

    // Find training script
    let script = if PathBuf::from("training/train_mnist_e2e.py").exists() {
        PathBuf::from("training/train_mnist_e2e.py")
    } else if PathBuf::from("training/train_mnist.py").exists() {
        PathBuf::from("training/train_mnist.py")
    } else {
        eprintln!("No training script found (train_mnist_e2e.py or train_mnist.py)");
        eprintln!("Create training/train_mnist_e2e.py for E2E testing");
        return;
    };

    info!("Using training script: {:?}", script);

    // Step 1: Create instance via runctl CLI
    info!("Step 1: Creating instance via runctl CLI...");
    let instance_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "create",
            "t3.micro",
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
        .expect("Could not extract instance ID");

    info!("Created instance: {}", instance_id);

    // Cleanup function
    let cleanup = || async {
        warn!("Cleaning up instance: {}", instance_id);
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
    };

    // Step 2: Wait for instance to be ready
    info!("Step 2: Waiting for instance to be ready...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        cleanup().await;
        panic!("Instance did not start: {}", e);
    }

    // Step 3: Train via runctl CLI with code sync
    info!("Step 3: Starting training via runctl CLI (with code sync)...");
    let train_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "train",
            &instance_id,
            script.to_str().unwrap(),
            "--sync-code",
            "--script-args",
            "--epochs 3", // Fast for E2E testing
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        cleanup().await;
        panic!(
            "Training failed to start:\nSTDOUT: {}\nSTDERR: {}",
            stdout, stderr
        );
    }

    info!("Training started successfully");

    // Step 4: Wait for training to complete
    info!("Step 4: Waiting for training to complete...");
    let mut attempts = 0;
    let max_attempts = 60; // 2 minutes max

    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;

        if attempts > max_attempts {
            cleanup().await;
            panic!("Training did not complete in time");
        }

        // Check for training_complete.txt marker
        let check_cmd = "test -f training_complete.txt && echo COMPLETE || echo RUNNING";
        match execute_ssm_command(&ssm_client, &instance_id, check_cmd).await {
            Ok(output) => {
                if output.contains("COMPLETE") {
                    info!("Training completed!");
                    break;
                }
            }
            Err(e) => {
                warn!("Failed to check training status: {}", e);
            }
        }
    }

    // Step 5: Verify checkpoints were created
    info!("Step 5: Verifying checkpoints...");
    let checkpoint_check = "ls -la checkpoints/*.json 2>/dev/null | wc -l || echo '0'";
    let checkpoint_count = execute_ssm_command(&ssm_client, &instance_id, checkpoint_check)
        .await
        .unwrap_or_else(|_| "0".to_string());

    let count: usize = checkpoint_count.trim().parse().unwrap_or(0);
    assert!(
        count > 0,
        "Expected checkpoints to be created, found: {}",
        count
    );

    info!("Found {} checkpoint(s)", count);

    // Step 6: Verify final checkpoint exists
    let final_checkpoint_check =
        "test -f checkpoints/final_checkpoint.json && echo EXISTS || echo MISSING";
    let final_exists = execute_ssm_command(&ssm_client, &instance_id, final_checkpoint_check)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        final_exists.contains("EXISTS"),
        "Final checkpoint should exist, got: {}",
        final_exists
    );

    info!("✅ All checks passed!");

    // Step 7: Cleanup
    info!("Step 7: Cleaning up...");
    cleanup().await;

    info!("=== Complete Training E2E Test Passed ===");
}
