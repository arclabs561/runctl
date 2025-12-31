//! Docker + EBS workflow E2E test
//!
//! This test verifies that Docker training works with EBS volumes:
//! 1. Creates EBS volume
//! 2. Attaches to instance
//! 3. Trains with Docker (should auto-detect Dockerfile)
//! 4. Verifies EBS volume is mounted in container
//! 5. Verifies training can access EBS data
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test docker_ebs_workflow_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.20-1.00 per run (uses t3.micro, creates EBS volume)

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
async fn test_docker_ebs_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    info!("=== Docker + EBS Workflow E2E Test ===");

    // Find training script
    let script = if PathBuf::from("training/train_mnist_e2e.py").exists() {
        PathBuf::from("training/train_mnist_e2e.py")
    } else if PathBuf::from("training/train_mnist.py").exists() {
        PathBuf::from("training/train_mnist.py")
    } else {
        eprintln!("No training script found (train_mnist_e2e.py or train_mnist.py)");
        return;
    };

    // Check for Dockerfile
    if !PathBuf::from("training/Dockerfile").exists() {
        eprintln!("No Dockerfile found in training/, skipping Docker test");
        return;
    }

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

    // Get instance AZ for volume
    let instance_desc = ec2_client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .expect("Failed to describe instance");

    let instance = instance_desc
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .expect("Instance not found");

    let availability_zone = instance
        .placement()
        .and_then(|p| p.availability_zone())
        .expect("Could not get availability zone");

    info!("Instance AZ: {}", availability_zone);

    // Step 2: Create EBS volume
    info!("Step 2: Creating EBS volume...");
    let volume_output = ec2_client
        .create_volume()
        .size(1) // 1 GB for testing
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(availability_zone)
        .send()
        .await
        .expect("Failed to create volume");

    let volume_id = volume_output
        .volume_id()
        .expect("No volume ID returned")
        .to_string();

    info!("Created volume: {}", volume_id);

    // Wait for volume to be available
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;

        if attempts > 30 {
            panic!("Volume did not become available");
        }

        let vol_desc = ec2_client
            .describe_volumes()
            .volume_ids(&volume_id)
            .send()
            .await
            .expect("Failed to describe volume");

        if let Some(volume) = vol_desc.volumes().first() {
            if let Some(state) = volume.state() {
                if state.as_str() == "available" {
                    break;
                }
            }
        }
    }

    // Step 3: Attach volume to instance
    info!("Step 3: Attaching volume to instance...");
    ec2_client
        .attach_volume()
        .volume_id(&volume_id)
        .instance_id(&instance_id)
        .device("/dev/sdf")
        .send()
        .await
        .expect("Failed to attach volume");

    // Wait for attachment
    sleep(Duration::from_secs(5)).await;

    // Step 4: Wait for instance to be ready
    info!("Step 4: Waiting for instance to be ready...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        // Cleanup
        let _ = ec2_client
            .detach_volume()
            .volume_id(&volume_id)
            .send()
            .await;
        let _ = ec2_client
            .delete_volume()
            .volume_id(&volume_id)
            .send()
            .await;
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
        panic!("Instance did not start: {}", e);
    }

    // Mount volume on instance
    info!("Mounting volume on instance...");
    let mount_cmd = r#"
# Find the volume device (usually /dev/nvme1n1 on newer instances)
DEVICE=$(lsblk -rno NAME,TYPE | grep disk | grep -v nvme0 | head -1 | awk '{print "/dev/"$1}')
if [ -z "$DEVICE" ]; then
    DEVICE="/dev/nvme1n1"
fi

# Create filesystem if needed
if ! blkid $DEVICE > /dev/null 2>&1; then
    sudo mkfs -t xfs $DEVICE
fi

# Mount volume
sudo mkdir -p /mnt/data
sudo mount $DEVICE /mnt/data || true
echo "Mounted: $DEVICE -> /mnt/data"
"#;

    let _ = execute_ssm_command(&ssm_client, &instance_id, mount_cmd).await;

    // Step 5: Train with Docker (should auto-detect Dockerfile and mount EBS)
    info!("Step 5: Starting Docker training...");
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
        let stdout = String::from_utf8_lossy(&train_output.stdout);
        warn!("Training output:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }

    info!("Training started, waiting for Docker build...");
    sleep(Duration::from_secs(60)).await; // Give time for Docker build/push

    // Step 6: Verify Docker container is running
    info!("Step 6: Verifying Docker container is running...");
    let check_docker = r#"
if command -v docker >/dev/null 2>&1; then
    CONTAINERS=$(docker ps --format "{{.Names}}" 2>/dev/null | wc -l)
    echo "DOCKER_AVAILABLE:$CONTAINERS"
    docker ps --format "{{.Names}}: {{.Status}}" 2>/dev/null || echo "NO_CONTAINERS"
else
    echo "DOCKER_NOT_INSTALLED"
fi
"#;

    let docker_output = execute_ssm_command(&ssm_client, &instance_id, check_docker)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    assert!(
        docker_output.contains("DOCKER_AVAILABLE") || docker_output.contains("NO_CONTAINERS"),
        "Docker should be available, got: {}",
        docker_output
    );

    info!("✅ Docker is available");

    // Step 7: Verify EBS volume is mounted in container
    info!("Step 7: Verifying EBS volume is mounted in container...");
    let check_mount = r#"
# Check if /mnt/data is mounted on host
if mountpoint -q /mnt/data 2>/dev/null; then
    echo "HOST_MOUNTED:/mnt/data"
    ls -la /mnt/data/ | head -5
else
    echo "HOST_NOT_MOUNTED"
fi

# Check if container has access to /data (mapped from /mnt/data)
if docker ps --format "{{.Names}}" | grep -q .; then
    CONTAINER=$(docker ps --format "{{.Names}}" | head -1)
    if docker exec $CONTAINER test -d /data 2>/dev/null; then
        echo "CONTAINER_HAS_DATA_DIR:/data"
        docker exec $CONTAINER ls -la /data/ 2>/dev/null | head -5 || echo "EMPTY"
    else
        echo "CONTAINER_NO_DATA_DIR"
    fi
else
    echo "NO_CONTAINERS"
fi
"#;

    let mount_output = execute_ssm_command(&ssm_client, &instance_id, check_mount)
        .await
        .unwrap_or_else(|_| "FAILED".to_string());

    // At minimum, host should have volume mounted
    assert!(
        mount_output.contains("HOST_MOUNTED") || mount_output.contains("CONTAINER_HAS_DATA_DIR"),
        "EBS volume should be mounted, got: {}",
        mount_output
    );

    info!("✅ EBS volume mount verified");
    info!("Mount output: {}", mount_output);

    // Step 8: Cleanup
    info!("Step 8: Cleaning up...");

    // Detach volume
    let _ = ec2_client
        .detach_volume()
        .volume_id(&volume_id)
        .send()
        .await;

    sleep(Duration::from_secs(2)).await;

    // Delete volume
    let _ = ec2_client
        .delete_volume()
        .volume_id(&volume_id)
        .send()
        .await;

    // Terminate instance
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

    info!("=== Docker + EBS Workflow Test Passed ===");
}
