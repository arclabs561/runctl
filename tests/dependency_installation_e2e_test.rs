//! Dependency installation verification E2E test
//!
//! This test verifies that requirements.txt is installed correctly:
//! 1. Creates test project with requirements.txt
//! 2. Syncs code to instance
//! 3. Verifies dependencies are installed
//! 4. Verifies training can import packages
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test dependency_installation_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.10-0.30 per run (uses t3.micro, minimal dependencies)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use std::env;
use std::fs;
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

    let command_vec = vec![command.to_string()];
    let response = ssm_client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", command_vec)
        .send()
        .await?;
    let command_id = response
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
            if let Some(state) = instance.state() {
                if let Some(state_name) = state.name() {
                    if state_name.as_str() == "running" {
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

#[tokio::test]
#[ignore]
async fn test_dependency_installation() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Dependency Installation Verification E2E Test ===");

    // Create temporary test directory
    let test_dir = PathBuf::from("test_deps_e2e");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    // Create requirements.txt with a simple, fast-installing package
    let requirements = test_dir.join("requirements.txt");
    fs::write(&requirements, "requests>=2.25.0\n").expect("Failed to write requirements.txt");

    // Create training script that imports the package
    let train_script = test_dir.join("train.py");
    fs::write(
        &train_script,
        r#"#!/usr/bin/env python3
"""Test script that verifies dependencies are installed."""
import sys

try:
    import requests
    print("✅ requests imported successfully")
    print(f"requests version: {requests.__version__}")
    sys.exit(0)
except ImportError as e:
    print(f"❌ Failed to import requests: {e}")
    sys.exit(1)
"#,
    )
    .expect("Failed to write train script");

    info!("Created test project in: {:?}", test_dir);

    // Step 1: Create instance
    info!("Step 1: Creating instance...");
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
        let _ = fs::remove_dir_all(&test_dir);
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
        let _ = fs::remove_dir_all(&test_dir);
    };

    // Step 2: Wait for instance to be ready
    info!("Step 2: Waiting for instance to be ready...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        cleanup().await;
        panic!("Instance did not start: {}", e);
    }

    // Step 3: Train with code sync (should install dependencies)
    info!("Step 3: Starting training with code sync (should install dependencies)...");
    let train_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "train",
            &instance_id,
            train_script.to_str().unwrap(),
            "--sync-code",
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        cleanup().await;
        panic!("Training failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    info!("Training started, waiting for completion...");

    // Step 4: Wait for training to complete and check output
    info!("Step 4: Waiting for training to complete...");
    sleep(Duration::from_secs(30)).await; // Give time for dependency installation and training

    // Check if requests is installed
    let project_dir = "/home/ec2-user/test_deps_e2e";
    let check_requests =
        "python3 -c 'import requests; print(\"INSTALLED:\", requests.__version__)' 2>&1";
    let requests_status = execute_ssm_command(&ssm_client, &instance_id, &check_requests)
        .await
        .unwrap_or_else(|_| "NOT_INSTALLED".to_string());

    assert!(
        requests_status.contains("INSTALLED:"),
        "requests should be installed, got: {}",
        requests_status
    );

    info!("✅ requests is installed: {}", requests_status);

    // Step 5: Verify training script can import requests
    info!("Step 5: Verifying training script can import requests...");
    let run_script = format!("cd {} && python3 train.py 2>&1", project_dir);
    let script_output = execute_ssm_command(&ssm_client, &instance_id, &run_script)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    assert!(
        script_output.contains("✅ requests imported successfully"),
        "Training script should import requests successfully, got: {}",
        script_output
    );

    info!("✅ Training script can import requests");
    info!("Script output: {}", script_output);

    // Step 6: Cleanup
    info!("Step 6: Cleaning up...");
    cleanup().await;

    info!("=== Dependency Installation Test Passed ===");
}
