//! Test utilities for E2E tests
//!
//! Provides common helpers for writing end-to-end tests that interact with
//! the runctl CLI and AWS resources.

use std::env;
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;

/// Check if E2E tests should run
///
/// E2E tests require explicit opt-in via `TRAINCTL_E2E=1` environment variable.
/// This prevents accidental execution of expensive tests.
pub fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

/// Helper macro to skip test if E2E not enabled
///
/// Usage:
/// ```rust,no_run
/// #[tokio::test]
/// #[ignore]
/// async fn test_something() {
///     require_e2e!();
///     // test code
/// }
/// ```
#[macro_export]
macro_rules! require_e2e {
    () => {
        if !$crate::e2e::test_utils::should_run_e2e() {
            eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
            return;
        }
    };
}

/// Execute a runctl CLI command and return the output
///
/// # Arguments
///
/// * `args` - Command arguments (e.g., `["aws", "create", "--dry-run"]`)
/// * `env` - Optional environment variables to set
///
/// # Returns
///
/// The command output (stdout, stderr, status)
pub fn run_runctl_command(
    args: &[&str],
    env: Option<&[(&str, &str)]>,
) -> Result<Output, std::io::Error> {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--release", "--"]);
    cmd.args(args);

    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
    }

    cmd.output()
}

/// Execute a runctl CLI command with JSON output
///
/// Parses the JSON output and returns the parsed value.
/// Fails if the command fails or output is not valid JSON.
pub fn run_runctl_json(
    args: &[&str],
    env: Option<&[(&str, &str)]>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut cmd_args = args.to_vec();
    cmd_args.push("--output");
    cmd_args.push("json");

    let output = run_runctl_command(&cmd_args, env)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)?;
    Ok(json)
}

/// Create a temporary directory for test artifacts
///
/// Returns a `TempDir` that will be automatically cleaned up when dropped.
pub fn create_test_temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

/// Wait for a condition to become true, with timeout
///
/// # Arguments
///
/// * `timeout` - Maximum time to wait
/// * `interval` - Check interval
/// * `condition` - Closure that returns true when condition is met
///
/// # Returns
///
/// `true` if condition became true, `false` if timeout
pub async fn wait_for_condition<F>(timeout: Duration, interval: Duration, mut condition: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(interval).await;
    }

    false
}

/// Extract instance ID from command output
///
/// Looks for patterns like "i-1234567890abcdef0" in the output.
pub fn extract_instance_id(output: &str) -> Option<String> {
    // Simple pattern matching without regex dependency
    for line in output.lines() {
        if let Some(start) = line.find("i-") {
            let rest = &line[start..];
            if rest.len() >= 19 {
                let id = &rest[..19];
                if id.chars().all(|c| c.is_alphanumeric() || c == '-') {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

/// Extract volume ID from command output
///
/// Looks for patterns like "vol-1234567890abcdef0" in the output.
pub fn extract_volume_id(output: &str) -> Option<String> {
    // Simple pattern matching without regex dependency
    for line in output.lines() {
        if let Some(start) = line.find("vol-") {
            let rest = &line[start..];
            if rest.len() >= 21 {
                let id = &rest[..21];
                if id.chars().all(|c| c.is_alphanumeric() || c == '-') {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

/// Verify JSON output structure
///
/// Checks that JSON output has expected fields and types.
pub fn verify_json_structure(
    json: &serde_json::Value,
    expected_fields: &[&str],
) -> Result<(), String> {
    if !json.is_object() {
        return Err("JSON should be an object".to_string());
    }

    let obj = json.as_object().unwrap();
    for field in expected_fields {
        if !obj.contains_key(*field) {
            return Err(format!("Missing field: {}", field));
        }
    }

    Ok(())
}

/// Create a test Python training script
///
/// Creates a minimal Python script that simulates training with checkpoint saving.
pub fn create_test_training_script(dir: &std::path::Path) -> std::path::PathBuf {
    use std::fs;
    let script_path = dir.join("test_train.py");

    let script_content = r#"#!/usr/bin/env python3
"""Test training script for E2E tests"""
import time
import os
import sys

# Create checkpoint directory
checkpoint_dir = os.environ.get("CHECKPOINT_DIR", "./checkpoints")
os.makedirs(checkpoint_dir, exist_ok=True)

# Simulate training with checkpoint saving
for epoch in range(1, 4):
    print(f"Epoch {epoch}/3: Training...")
    time.sleep(1)
    
    # Save checkpoint
    checkpoint_path = os.path.join(checkpoint_dir, f"checkpoint_epoch_{epoch}.pt")
    with open(checkpoint_path, "w") as f:
        f.write(f"checkpoint data for epoch {epoch}")
    print(f"Saved checkpoint: {checkpoint_path}")

print("Training complete!")
sys.exit(0)
"#;

    fs::write(&script_path, script_content).expect("Failed to write test script");
    fs::set_permissions(
        &script_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .ok(); // Ignore error on non-Unix systems

    script_path
}

/// Create a test config file
///
/// Creates a minimal `.runctl.toml` config file for testing.
pub fn create_test_config(dir: &std::path::Path) -> std::path::PathBuf {
    use std::fs;
    let config_path = dir.join(".runctl.toml");

    let config_content = r#"[aws]
region = "us-east-1"
default_instance_type = "t3.micro"

[checkpoint]
dir = "checkpoints"
keep_last_n = 5

[monitoring]
log_dir = "logs"
"#;

    fs::write(&config_path, config_content).expect("Failed to write test config");
    config_path
}

/// Assert command output contains expected text
pub fn assert_output_contains(output: &str, expected: &str) {
    assert!(
        output.contains(expected),
        "Expected output to contain '{}', but got: {}",
        expected,
        output
    );
}

/// Assert command succeeded
pub fn assert_command_success(output: &Output, command: &[&str]) {
    assert!(
        output.status.success(),
        "Command {:?} failed with status: {}\nStdout: {}\nStderr: {}",
        command,
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Assert command failed with expected error
pub fn assert_command_failure(output: &Output, expected_error: Option<&str>) {
    assert!(
        !output.status.success(),
        "Expected command to fail, but it succeeded"
    );

    if let Some(expected) = expected_error {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stderr.contains(expected) || stdout.contains(expected),
            "Expected error message '{}' not found in output\nStdout: {}\nStderr: {}",
            expected,
            stdout,
            stderr
        );
    }
}
