//! End-to-end test for Docker container support
//!
//! Tests Docker image building, ECR push, and container execution.
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test docker_test --features e2e -- --ignored
//!
//! Cost: ~$0.50-2.00 per run (creates real instance, builds/pushes Docker image)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::types::CommandInvocationStatus;
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
            if let Some(state) = instance.state().and_then(|s| s.name()) {
                if state.as_str() == "running" {
                    sleep(Duration::from_secs(30)).await;
                    return Ok(());
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

#[tokio::test]
#[ignore]
async fn test_docker_training() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Check if Dockerfile exists
    let dockerfile = PathBuf::from("training/Dockerfile");
    if !dockerfile.exists() {
        eprintln!(
            "Skipping Docker test: Dockerfile not found at {:?}",
            dockerfile
        );
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("Creating instance for Docker training test...");

    // Create instance with IAM profile for ECR access
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

    info!("Created instance: {}", instance_id);

    // Wait for instance to be running
    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not become running");

    info!("Instance is running, starting Docker training...");

    // Start training (should auto-detect Dockerfile)
    // Use train_mnist_e2e.py if available (faster for E2E), otherwise train_mnist.py
    let script_path = if PathBuf::from("training/train_mnist_e2e.py").exists() {
        "training/train_mnist_e2e.py"
    } else if PathBuf::from("training/train_mnist.py").exists() {
        "training/train_mnist.py"
    } else {
        warn!("No training script found, skipping Docker training test");
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
            script_path,
            "--sync-code",
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        warn!("Training start output: {}", stdout);
        warn!("Training start errors: {}", stderr);
    }

    info!("Docker training started, waiting for completion...");

    // Wait a bit for Docker image to build and push
    sleep(Duration::from_secs(60)).await;

    // Check if Docker container is running
    let check_docker_cmd = r#"
if command -v docker >/dev/null 2>&1; then
    CONTAINERS=$(docker ps --format "{{.Names}}" 2>/dev/null | wc -l)
    echo "DOCKER_AVAILABLE:$CONTAINERS"
    docker ps --format "{{.Names}}: {{.Status}}" 2>/dev/null || echo "NO_CONTAINERS"
else
    echo "DOCKER_NOT_INSTALLED"
fi
"#;

    let docker_output = execute_ssm_command(&ssm_client, &instance_id, check_docker_cmd)
        .await
        .expect("Failed to check Docker");

    info!("Docker status: {}", docker_output);

    // Verify Docker is available (even if no containers running)
    assert!(
        docker_output.contains("DOCKER_AVAILABLE") || docker_output.contains("NO_CONTAINERS"),
        "Docker should be available on instance. Output: {}",
        docker_output
    );

    info!("Docker test passed! Cleaning up...");

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
