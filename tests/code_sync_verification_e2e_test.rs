//! Code sync verification E2E test
//!
//! This test verifies that code sync actually transfers files correctly:
//! 1. Creates test files locally
//! 2. Syncs code to instance via runctl
//! 3. Verifies files exist on instance
//! 4. Verifies exclusions work (.git, checkpoints, etc.)
//! 5. Verifies file contents match
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test code_sync_verification_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.10-0.30 per run (uses t3.micro, minimal time)

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
    use aws_sdk_ssm::types::CommandStatus;

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
            .command_id(&command_id)
            .instance_id(instance_id)
            .send()
            .await?;

        let status = output.status();

        match status {
            Some(CommandStatus::Success) => {
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
async fn test_code_sync_transfers_files() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Code Sync Verification E2E Test ===");

    // Create temporary test directory
    let test_dir = PathBuf::from("test_sync_e2e");
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    // Create test files
    let test_file1 = test_dir.join("test_file1.py");
    let test_file2 = test_dir.join("test_file2.txt");
    let test_subdir = test_dir.join("subdir");
    fs::create_dir_all(&test_subdir).expect("Failed to create subdir");
    let test_file3 = test_subdir.join("test_file3.py");

    fs::write(&test_file1, "print('File 1')\n").expect("Failed to write test file");
    fs::write(&test_file2, "Test content 2\n").expect("Failed to write test file");
    fs::write(&test_file3, "print('File 3')\n").expect("Failed to write test file");

    // Create files that should be excluded
    let git_dir = test_dir.join(".git");
    fs::create_dir_all(&git_dir).expect("Failed to create .git");
    fs::write(git_dir.join("config"), "excluded\n").expect("Failed to write .git/config");

    let checkpoint_dir = test_dir.join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).expect("Failed to create checkpoints");
    fs::write(checkpoint_dir.join("checkpoint.pt"), "excluded\n")
        .expect("Failed to write checkpoint");

    // Create a training script
    let train_script = test_dir.join("train.py");
    fs::write(&train_script, "#!/usr/bin/env python3\nprint('Training')\n")
        .expect("Failed to write train script");

    // Create requirements.txt
    let requirements = test_dir.join("requirements.txt");
    fs::write(&requirements, "# Test requirements\n").expect("Failed to write requirements.txt");

    info!("Created test files in: {:?}", test_dir);

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
        // Cleanup
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

    // Step 3: Sync code via runctl
    info!("Step 3: Syncing code via runctl...");
    let sync_output = std::process::Command::new("cargo")
        .args([
            "run",
            "--release",
            "--",
            "aws",
            "train",
            &instance_id,
            train_script.to_str().unwrap(),
            "--sync-code",
            "--include-patterns",
            "*.py,*.txt,requirements.txt",
        ])
        .output()
        .expect("Failed to sync code");

    if !sync_output.status.success() {
        let stderr = String::from_utf8_lossy(&sync_output.stderr);
        let stdout = String::from_utf8_lossy(&sync_output.stdout);
        cleanup().await;
        panic!("Code sync failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    info!("Code sync completed");

    // Step 4: Verify files exist on instance
    info!("Step 4: Verifying files exist on instance...");

    // Get project directory (runctl uses project name or default)
    let project_dir = "/home/ec2-user/test_sync_e2e";

    // Check test_file1.py
    let check_file1 = format!(
        "test -f {}/test_file1.py && echo EXISTS || echo MISSING",
        project_dir
    );
    let file1_status = execute_ssm_command(&ssm_client, &instance_id, &check_file1)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        file1_status.contains("EXISTS"),
        "test_file1.py should exist on instance, got: {}",
        file1_status
    );

    // Check test_file2.txt
    let check_file2 = format!(
        "test -f {}/test_file2.txt && echo EXISTS || echo MISSING",
        project_dir
    );
    let file2_status = execute_ssm_command(&ssm_client, &instance_id, &check_file2)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        file2_status.contains("EXISTS"),
        "test_file2.txt should exist on instance, got: {}",
        file2_status
    );

    // Check subdir/test_file3.py
    let check_file3 = format!(
        "test -f {}/subdir/test_file3.py && echo EXISTS || echo MISSING",
        project_dir
    );
    let file3_status = execute_ssm_command(&ssm_client, &instance_id, &check_file3)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        file3_status.contains("EXISTS"),
        "subdir/test_file3.py should exist on instance, got: {}",
        file3_status
    );

    // Check requirements.txt
    let check_req = format!(
        "test -f {}/requirements.txt && echo EXISTS || echo MISSING",
        project_dir
    );
    let req_status = execute_ssm_command(&ssm_client, &instance_id, &check_req)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        req_status.contains("EXISTS"),
        "requirements.txt should exist on instance, got: {}",
        req_status
    );

    info!("✅ All expected files exist on instance");

    // Step 5: Verify file contents match
    info!("Step 5: Verifying file contents...");

    let check_content1 = format!("cat {}/test_file1.py", project_dir);
    let content1 = execute_ssm_command(&ssm_client, &instance_id, &check_content1)
        .await
        .unwrap_or_default();

    assert!(
        content1.contains("File 1"),
        "test_file1.py content should match, got: {}",
        content1
    );

    info!("✅ File contents match");

    // Step 6: Verify exclusions work
    info!("Step 6: Verifying exclusions work...");

    // .git should NOT exist
    let check_git = format!(
        "test -d {}/.git && echo EXISTS || echo MISSING",
        project_dir
    );
    let git_status = execute_ssm_command(&ssm_client, &instance_id, &check_git)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        git_status.contains("MISSING"),
        ".git directory should be excluded, but got: {}",
        git_status
    );

    // checkpoints should NOT exist
    let check_checkpoints = format!(
        "test -d {}/checkpoints && echo EXISTS || echo MISSING",
        project_dir
    );
    let checkpoint_status = execute_ssm_command(&ssm_client, &instance_id, &check_checkpoints)
        .await
        .unwrap_or_else(|_| "MISSING".to_string());

    assert!(
        checkpoint_status.contains("MISSING"),
        "checkpoints directory should be excluded, but got: {}",
        checkpoint_status
    );

    info!("✅ Exclusions work correctly");

    // Step 7: Cleanup
    info!("Step 7: Cleaning up...");
    cleanup().await;

    info!("=== Code Sync Verification Test Passed ===");
}
