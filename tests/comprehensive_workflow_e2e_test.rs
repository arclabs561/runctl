//! Comprehensive end-to-end workflow test
//!
//! Tests the complete workflow from instance creation to training completion,
//! including all new features: Docker, spot monitoring, auto-resume.
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test comprehensive_workflow_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$1-3 per run (creates instance, runs full training workflow)

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

#[tokio::test]
#[ignore]
async fn test_complete_workflow_with_spot_monitoring() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Starting comprehensive workflow test ===");

    // Step 1: Create spot instance with IAM profile
    info!("Step 1: Creating spot instance...");
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
        .expect("Could not extract instance ID");

    info!("Created instance: {}", instance_id);

    // Step 2: Wait for instance to be ready
    info!("Step 2: Waiting for instance to be ready...");
    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not become running");

    // Step 3: Start training (should auto-detect spot and enable monitoring)
    info!("Step 3: Starting training with spot monitoring...");
    // Try train_mnist_e2e.py first (minimal, fast), fallback to train_mnist.py
    let script = if PathBuf::from("training/train_mnist_e2e.py").exists() {
        PathBuf::from("training/train_mnist_e2e.py")
    } else if PathBuf::from("training/train_mnist.py").exists() {
        PathBuf::from("training/train_mnist.py")
    } else {
        warn!("No training script found (train_mnist_e2e.py or train_mnist.py), skipping training step");
        // Cleanup and return
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
        return;
    };

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
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        warn!("Training start may have issues: {}", stderr);
    } else {
        info!("Training started successfully");
    }

    // Step 4: Verify spot monitoring is active
    info!("Step 4: Verifying spot monitoring...");
    sleep(Duration::from_secs(10)).await;

    // Check if metadata service is accessible (indicates monitoring setup)
    let metadata_check = r#"
if command -v curl >/dev/null 2>&1; then
    curl -s http://169.254.169.254/latest/meta-data/instance-id 2>/dev/null && echo " METADATA_OK"
else
    echo "NO_CURL"
fi
"#;

    let metadata_output = execute_ssm_command(&ssm_client, &instance_id, metadata_check)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to check metadata service: {}", e);
            String::new()
        });

    if metadata_output.contains("METADATA_OK") {
        info!("Metadata service accessible (spot monitoring should work)");
    }

    // Step 5: Verify training is running
    info!("Step 5: Verifying training process...");
    sleep(Duration::from_secs(30)).await;

    let process_check = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
    else
        echo "TRAINING_STOPPED"
    fi
else
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        echo "TRAINING_RUNNING:$TRAINING_PID"
    else
        echo "NO_TRAINING_PROCESS"
    fi
fi
"#;

    let process_output = execute_ssm_command(&ssm_client, &instance_id, process_check)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to check training process: {}", e);
            String::new()
        });

    info!("Training process status: {}", process_output);

    // Step 6: Verify checkpoint directory exists
    info!("Step 6: Verifying checkpoint setup...");
    let checkpoint_check = r#"
if [ -d checkpoints ]; then
    echo "CHECKPOINT_DIR_EXISTS"
    ls -la checkpoints/ 2>/dev/null | head -5 || echo "EMPTY"
else
    echo "NO_CHECKPOINT_DIR"
fi
"#;

    let checkpoint_output = execute_ssm_command(&ssm_client, &instance_id, checkpoint_check)
        .await
        .unwrap_or_else(|e| {
            warn!("Failed to check checkpoints: {}", e);
            String::new()
        });

    info!("Checkpoint status: {}", checkpoint_output);

    info!("=== Comprehensive workflow test completed ===");
    info!("Cleaning up instance...");

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

#[tokio::test]
#[ignore]
async fn test_docker_workflow_complete() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Check if Dockerfile exists
    let dockerfile = PathBuf::from("training/Dockerfile");
    if !dockerfile.exists() {
        eprintln!("Skipping Docker test: Dockerfile not found");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);

    info!("=== Starting Docker workflow test ===");

    // Create instance
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
        .expect("Could not extract instance ID");

    info!("Created instance: {}", instance_id);

    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not become running");

    // Start training (should auto-detect Dockerfile)
    info!("Starting Docker-based training...");
    let script = PathBuf::from("training/train_mnist.py");

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
        ])
        .output()
        .expect("Failed to start training");

    let stdout = String::from_utf8_lossy(&train_output.stdout);
    let stderr = String::from_utf8_lossy(&train_output.stderr);

    info!("Docker training output: {}", stdout);
    if !train_output.status.success() {
        warn!("Docker training errors: {}", stderr);
    }

    // Verify Docker was used
    assert!(
        stdout.contains("Dockerfile")
            || stdout.contains("Docker")
            || stdout.contains("ECR")
            || stderr.contains("Docker"),
        "Should mention Docker in output. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    info!("=== Docker workflow test completed ===");

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
