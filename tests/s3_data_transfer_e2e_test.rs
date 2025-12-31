//! S3 data transfer E2E test
//!
//! This test verifies that --data-s3 works end-to-end:
//! 1. Uploads test data to S3
//! 2. Creates instance
//! 3. Trains with --data-s3 flag
//! 4. Verifies data is accessible on instance
//! 5. Verifies training can access data
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test s3_data_transfer_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.10-0.50 per run (uses t3.micro, minimal S3 storage)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
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
    let command_id = ssm_client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command.to_string()])
        .send()
        .await?
        .command()
        .and_then(|c| c.command_id())
        .ok_or("No command ID returned")?
        .to_string();

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
async fn test_s3_data_transfer() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let s3_client = S3Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== S3 Data Transfer E2E Test ===");

    // Get AWS account ID for bucket name (use process ID as fallback)
    // Note: In real E2E tests, you'd use STS, but for simplicity we use process ID
    let account_id = format!("test-{}", std::process::id());

    let bucket_name = format!("runctl-e2e-test-{}", account_id);
    let s3_prefix = format!("s3://{}/test-data/", bucket_name);

    info!("Using S3 path: {}", s3_prefix);

    // Create test data locally
    let test_dir = PathBuf::from("test_s3_e2e");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    let data_dir = test_dir.join("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // Create test data files
    fs::write(data_dir.join("train.csv"), "x,y\n1,2\n3,4\n5,6\n")
        .expect("Failed to write train.csv");
    fs::write(data_dir.join("test.csv"), "x,y\n7,8\n9,10\n").expect("Failed to write test.csv");
    fs::write(
        data_dir.join("metadata.json"),
        r#"{"version": "1.0", "samples": 3}"#,
    )
    .expect("Failed to write metadata.json");

    // Create training script that uses the data
    let train_script = test_dir.join("train.py");
    fs::write(
        &train_script,
        r#"#!/usr/bin/env python3
"""Test script that verifies S3 data is accessible."""
import os
import json
from pathlib import Path

data_dir = Path("data")
print(f"Checking data directory: {data_dir.absolute()}")

if not data_dir.exists():
    print(f"❌ Data directory does not exist: {data_dir}")
    exit(1)

files = list(data_dir.glob("*.csv")) + list(data_dir.glob("*.json"))
print(f"Found {len(files)} data files:")

for f in files:
    print(f"  - {f.name} ({f.stat().st_size} bytes)")

if len(files) < 3:
    print(f"❌ Expected at least 3 files, found {len(files)}")
    exit(1)

# Verify metadata.json
metadata_path = data_dir / "metadata.json"
if metadata_path.exists():
    with open(metadata_path) as f:
        metadata = json.load(f)
    print(f"✅ Metadata loaded: {metadata}")

print("✅ All data files accessible!")
"#,
    )
    .expect("Failed to write train script");

    info!("Created test data in: {:?}", test_dir);

    // Step 1: Create S3 bucket (if it doesn't exist)
    info!("Step 1: Creating/verifying S3 bucket...");
    let region = aws_config
        .region()
        .map(|r| r.as_ref())
        .unwrap_or("us-east-1");

    // Try to create bucket (may fail if exists, that's OK)
    let _ = s3_client.create_bucket().bucket(&bucket_name).send().await;

    // Step 2: Upload test data to S3
    info!("Step 2: Uploading test data to S3...");

    for entry in fs::read_dir(&data_dir).expect("Failed to read data directory") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        if path.is_file() {
            let key = format!("test-data/{}", path.file_name().unwrap().to_str().unwrap());
            let body = fs::read(&path).expect("Failed to read file");

            s3_client
                .put_object()
                .bucket(&bucket_name)
                .key(&key)
                .body(body.into())
                .send()
                .await
                .expect(&format!("Failed to upload {}", key));

            info!("Uploaded: {}", key);
        }
    }

    info!("✅ Test data uploaded to S3");

    // Step 3: Create instance
    info!("Step 3: Creating instance...");
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
        // Cleanup S3
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key("test-data/train.csv")
            .send()
            .await;
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

        // Cleanup S3 objects
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key("test-data/train.csv")
            .send()
            .await;
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key("test-data/test.csv")
            .send()
            .await;
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key("test-data/metadata.json")
            .send()
            .await;

        let _ = fs::remove_dir_all(&test_dir);
    };

    // Step 4: Wait for instance to be ready
    info!("Step 4: Waiting for instance to be ready...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        cleanup().await;
        panic!("Instance did not start: {}", e);
    }

    // Step 5: Train with --data-s3 flag
    info!("Step 5: Starting training with --data-s3 flag...");
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
            "--data-s3",
            &s3_prefix,
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        cleanup().await;
        panic!("Training failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    info!("Training started, waiting for data download...");
    sleep(Duration::from_secs(30)).await; // Give time for S3 download

    // Step 6: Verify data is on instance
    info!("Step 6: Verifying data is on instance...");
    let project_dir = "/home/ec2-user/test_s3_e2e";

    // Check data directory exists
    let check_data_dir = format!(
        "test -d {}/data && echo EXISTS || echo MISSING",
        project_dir
    );
    let data_dir_status = execute_ssm_command(&ssm_client, &instance_id, &check_data_dir)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        data_dir_status.contains("EXISTS"),
        "Data directory should exist on instance, got: {}",
        data_dir_status
    );

    // Check train.csv exists
    let check_train = format!(
        "test -f {}/data/train.csv && echo EXISTS || echo MISSING",
        project_dir
    );
    let train_status = execute_ssm_command(&ssm_client, &instance_id, &check_train)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        train_status.contains("EXISTS"),
        "train.csv should exist on instance, got: {}",
        train_status
    );

    // Check metadata.json exists
    let check_metadata = format!(
        "test -f {}/data/metadata.json && echo EXISTS || echo MISSING",
        project_dir
    );
    let metadata_status = execute_ssm_command(&ssm_client, &instance_id, &check_metadata)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        metadata_status.contains("EXISTS"),
        "metadata.json should exist on instance, got: {}",
        metadata_status
    );

    info!("✅ All data files exist on instance");

    // Step 7: Verify training can access data
    info!("Step 7: Verifying training can access data...");
    let run_script = format!("cd {} && python3 train.py 2>&1", project_dir);
    let script_output = execute_ssm_command(&ssm_client, &instance_id, &run_script)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    assert!(
        script_output.contains("✅ All data files accessible!"),
        "Training script should access data successfully, got: {}",
        script_output
    );

    info!("✅ Training can access S3 data");
    info!("Script output: {}", script_output);

    // Step 8: Cleanup
    info!("Step 8: Cleaning up...");
    cleanup().await;

    info!("=== S3 Data Transfer Test Passed ===");
}
