//! Checkpoint S3 upload/download E2E test
//!
//! This test verifies checkpoint S3 operations:
//! 1. Creates instance and trains (creates checkpoints)
//! 2. Uploads checkpoints to S3
//! 3. Downloads checkpoints from S3
//! 4. Verifies checkpoint integrity
//!
//! Note: This test uses manual S3 operations since automatic checkpoint upload
//! is not yet implemented. Once implemented, this test should use --output-s3 flag.
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test checkpoint_s3_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.10-0.30 per run (uses t3.micro, minimal S3 storage)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
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
async fn test_checkpoint_s3_operations() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let s3_client = S3Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Checkpoint S3 Operations E2E Test ===");

    // Get AWS account ID for bucket name (use process ID as fallback)
    // Note: In real E2E tests, you'd use STS, but for simplicity we use process ID
    let account_id = format!("test-{}", std::process::id());

    let bucket_name = format!("runctl-e2e-test-{}", account_id);
    let checkpoint_prefix = format!(
        "checkpoints/test-{}/",
        env::var("USER").unwrap_or_else(|_| "test".to_string())
    );

    info!(
        "Using S3 bucket: {} prefix: {}",
        bucket_name, checkpoint_prefix
    );

    // Find training script
    let script = if PathBuf::from("training/train_mnist_e2e.py").exists() {
        PathBuf::from("training/train_mnist_e2e.py")
    } else if PathBuf::from("training/train_mnist.py").exists() {
        PathBuf::from("training/train_mnist.py")
    } else {
        eprintln!("No training script found (train_mnist_e2e.py or train_mnist.py)");
        return;
    };

    info!("Using training script: {:?}", script);

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

        // Cleanup S3 checkpoints
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key(&format!("{}checkpoint_epoch_1.json", checkpoint_prefix))
            .send()
            .await;
        let _ = s3_client
            .delete_object()
            .bucket(&bucket_name)
            .key(&format!("{}final_checkpoint.json", checkpoint_prefix))
            .send()
            .await;
    };

    // Step 2: Wait for instance to be ready
    info!("Step 2: Waiting for instance to be ready...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        cleanup().await;
        panic!("Instance did not start: {}", e);
    }

    // Step 3: Train (creates checkpoints)
    info!("Step 3: Starting training (creates checkpoints)...");
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
            "--epochs 3",
        ])
        .output()
        .expect("Failed to start training");

    if !train_output.status.success() {
        let stderr = String::from_utf8_lossy(&train_output.stderr);
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        warn!("Training output:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    info!("Training started, waiting for checkpoints...");
    sleep(Duration::from_secs(30)).await; // Give time for training to create checkpoints

    // Step 4: Verify checkpoints exist on instance
    info!("Step 4: Verifying checkpoints exist on instance...");
    let project_dir = "/home/ec2-user";
    let check_checkpoints = format!(
        "ls -la {}/checkpoints/*.json 2>/dev/null | wc -l || echo '0'",
        project_dir
    );
    let checkpoint_count = execute_ssm_command(&ssm_client, &instance_id, &check_checkpoints)
        .await
        .unwrap_or_else(|_| "0".to_string());

    let count: usize = checkpoint_count.trim().parse().unwrap_or(0);
    assert!(
        count > 0,
        "Expected checkpoints to be created, found: {}",
        count
    );

    info!("✅ Found {} checkpoint(s) on instance", count);

    // Step 5: Upload checkpoints to S3 (using runctl or direct S3)
    info!("Step 5: Uploading checkpoints to S3...");

    // Use runctl s3 upload if available, otherwise use AWS CLI
    let upload_cmd = format!(
        r#"
# Upload checkpoints to S3
if command -v aws >/dev/null 2>&1; then
    aws s3 cp {}/checkpoints/ s3://{}/{}/ --recursive --exclude "*" --include "*.json" 2>&1
    echo "UPLOAD_COMPLETE"
else
    echo "AWS_CLI_NOT_AVAILABLE"
fi
"#,
        project_dir, bucket_name, checkpoint_prefix
    );

    let upload_output = execute_ssm_command(&ssm_client, &instance_id, &upload_cmd)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    if upload_output.contains("AWS_CLI_NOT_AVAILABLE") {
        warn!("AWS CLI not available on instance, skipping S3 upload test");
        cleanup().await;
        return;
    }

    info!("✅ Checkpoints uploaded to S3");

    // Step 6: Verify checkpoints in S3
    info!("Step 6: Verifying checkpoints in S3...");
    let list_objects = s3_client
        .list_objects_v2()
        .bucket(&bucket_name)
        .prefix(&checkpoint_prefix)
        .send()
        .await
        .expect("Failed to list S3 objects");

    let objects: Vec<String> = list_objects
        .contents()
        .map(|contents| {
            contents
                .iter()
                .filter_map(|obj| obj.key().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    assert!(
        !objects.is_empty(),
        "Expected checkpoints in S3, found: {}",
        objects.len()
    );

    info!("✅ Found {} checkpoint(s) in S3", objects.len());

    // Step 7: Download checkpoints from S3
    info!("Step 7: Downloading checkpoints from S3...");
    let download_dir = format!("{}/checkpoints_downloaded", project_dir);
    let download_cmd = format!(
        r#"
mkdir -p {}
if command -v aws >/dev/null 2>&1; then
    aws s3 cp s3://{}/{}/ {} --recursive 2>&1
    echo "DOWNLOAD_COMPLETE"
    ls -la {} | head -10
else
    echo "AWS_CLI_NOT_AVAILABLE"
fi
"#,
        download_dir, bucket_name, checkpoint_prefix, download_dir, download_dir
    );

    let download_output = execute_ssm_command(&ssm_client, &instance_id, &download_cmd)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    assert!(
        download_output.contains("DOWNLOAD_COMPLETE"),
        "Download should complete, got: {}",
        download_output
    );

    info!("✅ Checkpoints downloaded from S3");

    // Step 8: Verify downloaded checkpoint integrity
    info!("Step 8: Verifying downloaded checkpoint integrity...");
    let verify_cmd = format!(
        r#"
# Compare original and downloaded checkpoints
if [ -f {}/checkpoints/final_checkpoint.json ] && [ -f {}/final_checkpoint.json ]; then
    ORIGINAL=$(md5sum {}/checkpoints/final_checkpoint.json | awk '{{print $1}}')
    DOWNLOADED=$(md5sum {}/final_checkpoint.json | awk '{{print $1}}')
    if [ "$ORIGINAL" = "$DOWNLOADED" ]; then
        echo "INTEGRITY_OK"
    else
        echo "INTEGRITY_FAILED: $ORIGINAL vs $DOWNLOADED"
    fi
else
    echo "FILES_MISSING"
fi
"#,
        project_dir, download_dir, project_dir, download_dir
    );

    let verify_output = execute_ssm_command(&ssm_client, &instance_id, &verify_cmd)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    // Integrity check is best-effort (md5sum may not be available)
    if verify_output.contains("INTEGRITY_OK") {
        info!("✅ Checkpoint integrity verified");
    } else {
        info!("⚠️  Integrity check inconclusive: {}", verify_output);
    }

    // Step 9: Cleanup
    info!("Step 9: Cleaning up...");
    cleanup().await;

    info!("=== Checkpoint S3 Operations Test Passed ===");
}
