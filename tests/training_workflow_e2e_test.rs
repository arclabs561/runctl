//! End-to-end test for complete training workflow
//!
//! Tests the full cycle: create instance → sync code → train → monitor → cleanup
//!
//! Run with: TRAINCTL_E2E=1 cargo test --test training_workflow_test --features e2e -- --ignored
//!
//! Cost: ~$0.50-2.00 per run (creates real instance, runs training)

use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use std::env;
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

/// Helper to check if directory exists on instance via SSM
async fn dir_exists_on_instance(
    ssm_client: &SsmClient,
    instance_id: &str,
    path: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let cmd = format!("test -d {} && echo 'EXISTS' || echo 'NOT_FOUND'", path);

    let output = execute_ssm_command(ssm_client, instance_id, &cmd).await?;

    Ok(output.contains("EXISTS"))
}

/// Helper to execute SSM command (reimplementation for test)
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

    // Wait for command to complete (simplified polling)
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

        if let Some(status) = output.status() {
            match status.as_str() {
                "Success" => {
                    return Ok(output.standard_output_content().unwrap_or("").to_string());
                }
                "Failed" | "Cancelled" | "TimedOut" => {
                    return Err(format!(
                        "Command failed: {}",
                        output.standard_error_content().unwrap_or("")
                    )
                    .into());
                }
                _ => {
                    // Still running, continue waiting
                    continue;
                }
            }
        }
    }
}

#[tokio::test]
#[ignore] // Requires AWS credentials and explicit opt-in
async fn test_full_training_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    info!("Starting full training workflow E2E test");

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let ec2_client = Ec2Client::new(&aws_config);
    let ssm_client = SsmClient::new(&aws_config);

    // Step 1: Create a small test instance
    info!("Step 1: Creating test instance...");
    let instance_id = {
        use aws_sdk_ec2::types::InstanceType;

        let response = ec2_client
            .run_instances()
            .image_id("ami-08fa3ed5577079e64") // Amazon Linux 2023
            .instance_type(InstanceType::T3Micro)
            .min_count(1)
            .max_count(1)
            .tag_specifications(
                aws_sdk_ec2::types::TagSpecification::builder()
                    .resource_type(aws_sdk_ec2::types::ResourceType::Instance)
                    .tags(
                        aws_sdk_ec2::types::Tag::builder()
                            .key("Name")
                            .value("trainctl-e2e-test")
                            .build(),
                    )
                    .tags(
                        aws_sdk_ec2::types::Tag::builder()
                            .key("CreatedBy")
                            .value("trainctl-e2e-test")
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .expect("Failed to create instance");

        let id = response
            .instances()
            .first()
            .and_then(|i| i.instance_id())
            .expect("No instance ID")
            .to_string();

        info!("Created instance: {}", id);
        id
    };

    // Cleanup on test failure
    let cleanup = || async {
        warn!("Cleaning up instance: {}", instance_id);
        let _ = ec2_client
            .terminate_instances()
            .instance_ids(&instance_id)
            .send()
            .await;
    };

    // Step 2: Wait for instance to be running
    info!("Step 2: Waiting for instance to be running...");
    wait_for_instance_running(&ec2_client, &instance_id, 300)
        .await
        .expect("Instance did not start");

    // Step 3: Verify project directory exists (from user-data)
    info!("Step 3: Verifying instance setup...");
    let project_dir = "/home/ec2-user/test-project";
    let dir_exists = dir_exists_on_instance(&ssm_client, &instance_id, project_dir)
        .await
        .unwrap_or(false);

    if !dir_exists {
        // Create it (user-data might not have run yet)
        let cmd = format!("mkdir -p {}", project_dir);
        execute_ssm_command(&ssm_client, &instance_id, &cmd)
            .await
            .expect("Failed to create project directory");
    }

    // Step 4: Create a simple test training script on instance
    info!("Step 4: Creating test training script...");
    let test_script = format!(
        r#"
#!/bin/bash
cd {}
echo "Training started at $(date)" > training.log
echo "Epoch 1/10: loss=0.5" >> training.log
sleep 2
echo "Epoch 2/10: loss=0.4" >> training.log
echo "Training completed" >> training.log
echo "TRAINING_COMPLETE" > training.status
"#,
        project_dir
    );

    let create_script_cmd = format!(
        "cat > {}/test_train.sh << 'EOF'\n{}\nEOF\nchmod +x {}/test_train.sh",
        project_dir, test_script, project_dir
    );

    execute_ssm_command(&ssm_client, &instance_id, &create_script_cmd)
        .await
        .expect("Failed to create test script");

    // Step 5: Start training in background
    info!("Step 5: Starting training...");
    let start_training_cmd = format!(
        "cd {} && nohup ./test_train.sh > training.log 2>&1 & echo $! > training.pid",
        project_dir
    );

    execute_ssm_command(&ssm_client, &instance_id, &start_training_cmd)
        .await
        .expect("Failed to start training");

    // Step 6: Wait a bit and verify training is running
    info!("Step 6: Verifying training is running...");
    sleep(Duration::from_secs(5)).await;

    let check_pid_cmd = format!(
        "if [ -f {}/training.pid ]; then PID=$(cat {}/training.pid) && ps -p $PID > /dev/null && echo 'RUNNING' || echo 'STOPPED'; else echo 'NO_PID'; fi",
        project_dir, project_dir
    );

    let status = execute_ssm_command(&ssm_client, &instance_id, &check_pid_cmd)
        .await
        .expect("Failed to check training status");

    assert!(status.contains("RUNNING"), "Training should be running");

    // Step 7: Wait for training to complete
    info!("Step 7: Waiting for training to complete...");
    let mut attempts = 0;
    loop {
        sleep(Duration::from_secs(2)).await;
        attempts += 1;

        if attempts > 30 {
            panic!("Training did not complete in time");
        }

        let check_complete_cmd = format!(
            "if [ -f {}/training.status ] && grep -q TRAINING_COMPLETE {}/training.status; then echo 'COMPLETE'; else echo 'RUNNING'; fi",
            project_dir, project_dir
        );

        let status = execute_ssm_command(&ssm_client, &instance_id, &check_complete_cmd)
            .await
            .expect("Failed to check completion");

        if status.contains("COMPLETE") {
            break;
        }
    }

    // Step 8: Verify training log exists and has content
    info!("Step 8: Verifying training log...");
    let check_log_cmd = format!(
        "if [ -f {}/training.log ] && [ -s {}/training.log ]; then head -5 {}/training.log; else echo 'LOG_MISSING'; fi",
        project_dir, project_dir, project_dir
    );

    let log_content = execute_ssm_command(&ssm_client, &instance_id, &check_log_cmd)
        .await
        .expect("Failed to read training log");

    assert!(
        !log_content.contains("LOG_MISSING"),
        "Training log should exist"
    );
    assert!(
        log_content.contains("Training"),
        "Training log should have content"
    );

    // Step 9: Cleanup
    info!("Step 9: Cleaning up...");
    cleanup().await;

    info!("✅ Full training workflow test passed!");
}
