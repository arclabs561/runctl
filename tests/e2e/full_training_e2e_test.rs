//! Complete end-to-end training test
//!
//! This test verifies the full training workflow:
//! 1. Create AWS instance
//! 2. Sync code (including test training script)
//! 3. Start training
//! 4. Monitor training progress
//! 5. Verify checkpoints created
//! 6. Cleanup
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test full_training_e2e_test --features e2e -- --ignored
//!
//! Cost: ~$0.50-2.00 per run (creates real instance, runs training)

use std::env;
use std::path::PathBuf;
use std::time::Duration;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
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
                    // Wait a bit more for SSM to be ready
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
    use aws_sdk_ssm::types::InstanceIdList;
    
    let command_id = ssm_client
        .send_command()
        .instance_ids(instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command])
        .send()
        .await?
        .command()
        .ok_or("No command in response")?
        .command_id()
        .ok_or("No command ID")?
        .to_string();
    
    // Wait for command to complete
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;
        
        if attempts > 60 {
            return Err("Command timeout".into());
        }
        
        let output = ssm_client
            .get_command_invocation()
            .command_id(&command_id)
            .instance_id(instance_id)
            .send()
            .await?;
        
        if let Some(status) = output.status() {
            match status {
                aws_sdk_ssm::types::CommandInvocationStatus::Success => {
                    return Ok(output
                        .standard_output_content()
                        .unwrap_or("")
                        .to_string());
                }
                aws_sdk_ssm::types::CommandInvocationStatus::Failed
                | aws_sdk_ssm::types::CommandInvocationStatus::Cancelled
                | aws_sdk_ssm::types::CommandInvocationStatus::TimedOut => {
                    let stderr = output.standard_error_content().unwrap_or("");
                    return Err(format!(
                        "Command failed: {} - {}",
                        status.as_str(),
                        stderr
                    )
                    .into());
                }
                _ => {
                    // Still running, continue waiting
                }
            }
        }
    }
}

/// Check if file exists on instance
async fn file_exists_on_instance(
    ssm_client: &SsmClient,
    instance_id: &str,
    file_path: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let command = format!("test -f {} && echo EXISTS || echo NOT_FOUND", file_path);
    let output = execute_ssm_command(ssm_client, instance_id, &command).await?;
    Ok(output.contains("EXISTS"))
}

/// Get file content from instance
async fn get_file_content(
    ssm_client: &SsmClient,
    instance_id: &str,
    file_path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let command = format!("cat {}", file_path);
    execute_ssm_command(ssm_client, instance_id, &command).await
}

#[tokio::test]
#[ignore]
async fn test_full_training_workflow() {
    if !should_run_e2e() {
        println!("Skipping E2E test (set TRAINCTL_E2E=1 to run)");
        return;
    }

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    // Use smallest/cheapest instance for testing
    let instance_type = "t3.micro"; // Free tier eligible
    let project_name = format!("e2e-test-{}", std::process::id());

    info!("Starting full training workflow E2E test");
    info!("Instance type: {}", instance_type);
    info!("Project name: {}", project_name);

    let instance_id = match create_test_instance(&ec2_client, instance_type, &project_name).await {
        Ok(id) => {
            info!("Created test instance: {}", id);
            id
        }
        Err(e) => {
            panic!("Failed to create test instance: {}", e);
        }
    };

    // Cleanup on panic
    let cleanup = || async {
        warn!("Cleaning up test instance: {}", instance_id);
        let _ = ec2_client
            .terminate_instances()
            .instance_ids(&instance_id)
            .send()
            .await;
    };

    // Wait for instance to be running
    info!("Waiting for instance to be running...");
    if let Err(e) = wait_for_instance_running(&ec2_client, &instance_id, 300).await {
        cleanup().await;
        panic!("Instance did not start: {}", e);
    }

    // Step 1: Verify code sync (we'll use trainctl CLI for this)
    info!("Step 1: Verifying code sync capability...");
    // Note: Actual code sync would be tested via trainctl CLI, not directly here
    // This test verifies the infrastructure is ready

    // Step 2: Create test training script on instance
    info!("Step 2: Creating test training script on instance...");
    let test_script = r#"#!/usr/bin/env python3
import os
import json
import time
from pathlib import Path

def train_model(epochs=3, checkpoint_dir="checkpoints"):
    os.makedirs(checkpoint_dir, exist_ok=True)
    
    for epoch in range(epochs):
        loss = 1.0 / (epoch + 1)
        checkpoint = {
            "epoch": epoch + 1,
            "loss": loss,
            "timestamp": time.time()
        }
        
        checkpoint_path = f"{checkpoint_dir}/checkpoint_epoch_{epoch+1}.json"
        with open(checkpoint_path, "w") as f:
            json.dump(checkpoint, f)
        
        print(f"Epoch {epoch+1}/{epochs}: loss={loss:.4f}")
        time.sleep(1)
    
    # Final checkpoint
    final_checkpoint = {
        "epoch": epochs,
        "loss": 1.0 / epochs,
        "status": "completed"
    }
    
    with open(f"{checkpoint_dir}/final_checkpoint.json", "w") as f:
        json.dump(final_checkpoint, f)
    
    with open("training_complete.txt", "w") as f:
        f.write("Training completed successfully\n")
    
    print("Training completed!")

if __name__ == "__main__":
    train_model(epochs=3)
"#;

    let script_path = "/tmp/test_training.py";
    let create_script_cmd = format!(
        "cat > {} << 'EOF'\n{}\nEOF\nchmod +x {}",
        script_path, test_script, script_path
    );

    if let Err(e) = execute_ssm_command(&ssm_client, &instance_id, &create_script_cmd).await {
        cleanup().await;
        panic!("Failed to create test script: {}", e);
    }

    // Verify script exists
    if !file_exists_on_instance(&ssm_client, &instance_id, script_path).await.unwrap_or(false) {
        cleanup().await;
        panic!("Test script was not created on instance");
    }

    // Step 3: Run training
    info!("Step 3: Running training...");
    let train_cmd = format!("cd /tmp && python3 {}", script_path);
    let train_output = match execute_ssm_command(&ssm_client, &instance_id, &train_cmd).await {
        Ok(output) => {
            info!("Training output: {}", output);
            output
        }
        Err(e) => {
            cleanup().await;
            panic!("Training failed: {}", e);
        }
    };

    // Verify training output
    assert!(
        train_output.contains("Training completed"),
        "Training did not complete successfully"
    );

    // Step 4: Verify checkpoints created
    info!("Step 4: Verifying checkpoints...");
    let checkpoint_dir = "/tmp/checkpoints";
    let check_checkpoints_cmd = format!("ls -la {} 2>/dev/null || echo 'NO_CHECKPOINTS'", checkpoint_dir);
    let checkpoint_list = execute_ssm_command(&ssm_client, &instance_id, &check_checkpoints_cmd)
        .await
        .unwrap_or_default();

    assert!(
        !checkpoint_list.contains("NO_CHECKPOINTS"),
        "Checkpoints directory was not created"
    );

    // Verify final checkpoint exists
    let final_checkpoint_path = format!("{}/final_checkpoint.json", checkpoint_dir);
    if !file_exists_on_instance(&ssm_client, &instance_id, &final_checkpoint_path)
        .await
        .unwrap_or(false)
    {
        cleanup().await;
        panic!("Final checkpoint was not created");
    }

    // Verify training complete marker
    if !file_exists_on_instance(&ssm_client, &instance_id, "/tmp/training_complete.txt")
        .await
        .unwrap_or(false)
    {
        cleanup().await;
        panic!("Training complete marker was not created");
    }

    info!("✅ Full training workflow test passed!");

    // Cleanup
    cleanup().await;
    info!("Test instance terminated");
}

/// Create a test instance
async fn create_test_instance(
    client: &Ec2Client,
    instance_type: &str,
    project_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use aws_sdk_ec2::types::InstanceType as Ec2InstanceType;

    // Get default AMI (Amazon Linux 2023)
    let ami_id = "ami-0c55b159cbfafe1f0"; // Amazon Linux 2023 in us-east-1

    let response = client
        .run_instances()
        .image_id(ami_id)
        .instance_type(Ec2InstanceType::from(instance_type))
        .min_count(1)
        .max_count(1)
        .ebs_optimized(true)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Instance)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("Name")
                        .value(format!("{}-test", project_name))
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("trainctl:created")
                        .value("true")
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("trainctl:project")
                        .value(project_name)
                        .build(),
                )
                .build(),
        )
        .send()
        .await?;

    let instance_id = response
        .instances()
        .first()
        .and_then(|i| i.instance_id())
        .ok_or("No instance in response")?;

    Ok(instance_id.to_string())
}

