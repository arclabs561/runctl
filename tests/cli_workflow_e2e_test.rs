//! End-to-end tests for CLI command workflows
//!
//! Tests complete workflows using the actual CLI (via `cargo run`).
//! These tests verify that commands work together correctly and produce
//! expected output formats.
//!
//! Run with: `TRAINCTL_E2E=1 cargo test --test cli_workflow_e2e_test --features e2e -- --ignored`
//!
//! Most tests use `--dry-run` to avoid creating actual AWS resources.

use std::env;
use std::process::Command;
use tempfile::TempDir;

// Import test utilities from e2e subdirectory
#[path = "e2e/test_utils.rs"]
mod test_utils;
use test_utils::*;

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

/// Test config initialization workflow
#[tokio::test]
#[ignore]
async fn test_config_init_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join(".runctl.toml");

    // Test: Initialize config
    let output = run_runctl_command(&["init", "--output", config_path.to_str().unwrap()])
        .expect("Failed to run init command");

    assert_command_success(&output, &["init"]);
    assert!(config_path.exists(), "Config file should be created");

    // Test: Show config
    let output = run_runctl_command(&["config", "show"])
        .expect("Failed to run config show");

    assert_command_success(&output, &["config", "show"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_output_contains(&stdout, "aws");
    assert_output_contains(&stdout, "checkpoint");
}

/// Test config validation
#[tokio::test]
#[ignore]
async fn test_config_validation() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Validate default config
    let output = run_runctl_command(&["config", "validate"])
        .expect("Failed to run config validate");

    assert_command_success(&output, &["config", "validate"]);

    // Test: Validate with JSON output
    let json = run_runctl_json(&["config", "validate"])
        .expect("Failed to run config validate with JSON");

    verify_json_structure(&json, &["success"]).expect("Invalid JSON structure");
}

/// Test checkpoint operations workflow (no AWS required)
#[tokio::test]
#[ignore]
async fn test_checkpoint_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    std::fs::create_dir_all(&checkpoint_dir).expect("Failed to create checkpoint dir");

    // Create test checkpoints
    for i in 1..=3 {
        let checkpoint_path = checkpoint_dir.join(format!("checkpoint_epoch_{}.pt", i));
        std::fs::write(&checkpoint_path, format!("checkpoint data {}", i))
            .expect("Failed to write checkpoint");
    }

    // Test: List checkpoints
    let output = run_runctl_command(&[
        "checkpoint",
        "list",
        checkpoint_dir.to_str().unwrap(),
    ])
    .expect("Failed to run checkpoint list");

    assert_command_success(&output, &["checkpoint", "list"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_output_contains(&stdout, "checkpoint_epoch_1.pt");
    assert_output_contains(&stdout, "checkpoint_epoch_2.pt");
    assert_output_contains(&stdout, "checkpoint_epoch_3.pt");

    // Test: List checkpoints with JSON output
    let json = run_runctl_json(&[
        "checkpoint",
        "list",
        checkpoint_dir.to_str().unwrap(),
    ])
    .expect("Failed to run checkpoint list with JSON");

    verify_json_structure(&json, &["success"]).expect("Invalid JSON structure");
}

/// Test local training workflow (no AWS required)
#[tokio::test]
#[ignore]
async fn test_local_training_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let script_path = create_test_training_script(temp_dir.path());

    // Test: Run local training
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--release", "--", "local", script_path.to_str().unwrap()]);
    cmd.env("CHECKPOINT_DIR", temp_dir.path().join("checkpoints").to_str().unwrap());
    let output = cmd.output().expect("Failed to run local training");

    // Training might fail if Python/uv not available, but command should execute
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check that command was attempted (either success or meaningful error)
    assert!(
        output.status.success() || stderr.contains("python") || stderr.contains("uv"),
        "Command should either succeed or fail with Python-related error\nStdout: {}\nStderr: {}",
        stdout,
        stderr
    );
}

/// Test AWS create command with dry-run
#[tokio::test]
#[ignore]
async fn test_aws_create_dry_run() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Create instance with dry-run (doesn't actually create)
    let output = run_runctl_command(&[
        "aws",
        "create",
        "t3.micro",
        "--dry-run",
    ])
    .expect("Failed to run aws create");

    // Dry-run should succeed (even without AWS credentials in some cases)
    // or fail with a clear error message
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either succeeds or fails with AWS-related error (credentials, permissions, etc.)
    assert!(
        output.status.success()
            || stderr.contains("AWS")
            || stderr.contains("credentials")
            || stderr.contains("permission"),
        "Command should succeed or fail with AWS-related error\nStdout: {}\nStderr: {}",
        stdout,
        stderr
    );
}

/// Test AWS create command JSON output
#[tokio::test]
#[ignore]
async fn test_aws_create_json_output() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Create instance with JSON output
    let json = run_runctl_json(&[
        "aws",
        "create",
        "t3.micro",
        "--dry-run",
    ]);

    match json {
        Ok(json_value) => {
            // If command succeeded, verify JSON structure
            verify_json_structure(&json_value, &["success"]).expect("Invalid JSON structure");
        }
        Err(_) => {
            // If command failed (e.g., no AWS credentials), that's acceptable
            // The test verifies that JSON parsing works when command succeeds
        }
    }
}

/// Test resources list command
#[tokio::test]
#[ignore]
async fn test_resources_list() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: List resources
    let output = run_runctl_command(&["resources", "list"])
        .expect("Failed to run resources list");

    // Should succeed even if no resources exist
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success()
            || stderr.contains("AWS")
            || stderr.contains("credentials"),
        "Command should succeed or fail with AWS-related error\nStdout: {}\nStderr: {}",
        stdout,
        stderr
    );
}

/// Test resources list with JSON output
#[tokio::test]
#[ignore]
async fn test_resources_list_json() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let json = run_runctl_json(&["resources", "list"]);

    match json {
        Ok(json_value) => {
            verify_json_structure(&json_value, &["success"]).expect("Invalid JSON structure");
        }
        Err(_) => {
            // Acceptable if AWS credentials not available
        }
    }
}

/// Test status command
#[tokio::test]
#[ignore]
async fn test_status_command() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Status command
    let output = run_runctl_command(&["status"])
        .expect("Failed to run status command");

    // Status should work even without resources
    assert_command_success(&output, &["status"]);
}

/// Test status command with JSON output
#[tokio::test]
#[ignore]
async fn test_status_json_output() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let json = run_runctl_json(&["status"])
        .expect("Failed to run status with JSON");

    verify_json_structure(&json, &["success"]).expect("Invalid JSON structure");
}

/// Test help text for all major commands
#[tokio::test]
#[ignore]
async fn test_help_text_completeness() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let commands = vec![
        vec!["aws", "--help"],
        vec!["aws", "create", "--help"],
        vec!["checkpoint", "--help"],
        vec!["config", "--help"],
        vec!["resources", "--help"],
        vec!["s3", "--help"],
        vec!["local", "--help"],
        vec!["monitor", "--help"],
        vec!["transfer", "--help"],
        vec!["status", "--help"],
        vec!["top", "--help"],
    ];

    for cmd in commands {
        let output = run_runctl_command(&cmd)
            .expect(&format!("Failed to run {:?}", cmd));

        assert_command_success(&output, &cmd);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.is_empty(),
            "Help text should not be empty for {:?}",
            cmd
        );
    }
}

/// Test error handling for invalid commands
#[tokio::test]
#[ignore]
async fn test_error_handling() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Invalid instance ID
    let output = run_runctl_command(&["aws", "terminate", "invalid-instance-id"])
        .expect("Failed to run invalid command");

    // Should fail with validation error
    assert_command_failure(&output, Some("invalid"));

    // Test: Invalid S3 path
    let output = run_runctl_command(&["s3", "list", "invalid-path"])
        .expect("Failed to run invalid S3 command");

    assert_command_failure(&output, Some("s3://"));
}

/// Test error handling with JSON output
#[tokio::test]
#[ignore]
async fn test_error_json_output() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    // Test: Invalid command with JSON output
    let output = run_runctl_command(&[
        "aws",
        "terminate",
        "invalid-instance-id",
        "--output",
        "json",
    ])
    .expect("Failed to run invalid command");

    // Should fail, but error should be JSON
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for JSON in either stdout or stderr
    let combined = format!("{}\n{}", stdout, stderr);
    if let Some(json_start) = combined.find('{') {
        let json_str = &combined[json_start..];
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            // Verify it has error structure
            assert!(
                json.get("error").is_some() || json.get("success") == Some(&serde_json::json!(false)),
                "Error JSON should have 'error' or 'success: false' field"
            );
        }
    }
}

/// Test complete workflow: config init → checkpoint list → status
#[tokio::test]
#[ignore]
async fn test_complete_workflow() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Step 1: Initialize config
    let config_path = temp_dir.path().join(".runctl.toml");
    let output = run_runctl_command(&["init", "--output", config_path.to_str().unwrap()])
        .expect("Failed to init config");
    assert_command_success(&output, &["init"]);

    // Step 2: Create checkpoints
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    std::fs::create_dir_all(&checkpoint_dir).expect("Failed to create checkpoint dir");
    for i in 1..=2 {
        let checkpoint_path = checkpoint_dir.join(format!("checkpoint_{}.pt", i));
        std::fs::write(&checkpoint_path, "test data").expect("Failed to write checkpoint");
    }

    // Step 3: List checkpoints
    let output = run_runctl_command(&["checkpoint", "list", checkpoint_dir.to_str().unwrap()])
        .expect("Failed to list checkpoints");
    assert_command_success(&output, &["checkpoint", "list"]);

    // Step 4: Check status
    let output = run_runctl_command(&["status"])
        .expect("Failed to check status");
    assert_command_success(&output, &["status"]);
}
