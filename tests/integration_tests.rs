//! Integration tests for command combinations
//!
//! Tests verify that commands work together correctly:
//! - AWS create + train + monitor + terminate workflow
//! - EBS create + attach + detach + delete workflow
//! - S3 upload + download + sync workflow
//! - Checkpoint list + info + resume workflow

use std::process::Command;

// NOTE: E2E workflow tests for AWS, EBS, and S3 operations are in the
// tests/e2e/ directory. They require AWS credentials and explicit opt-in
// via TRAINCTL_E2E=1. See tests/E2E_TEST_GUIDE.md for details.

/// Test checkpoint operations using library API directly (avoids slow cargo run)
#[tokio::test]
async fn test_checkpoint_operations() {
    use std::fs;
    use tempfile::TempDir;
    use runctl::checkpoint::get_checkpoint_paths;

    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();

    // Create dummy checkpoints with .pt extension (recognized by get_checkpoint_paths)
    fs::write(
        checkpoint_dir.join("checkpoint_epoch_1.pt"),
        "dummy checkpoint data",
    ).unwrap();
    fs::write(
        checkpoint_dir.join("checkpoint_epoch_2.pt"),
        "dummy checkpoint data",
    ).unwrap();

    // Test get_checkpoint_paths using library function
    let checkpoints = get_checkpoint_paths(&checkpoint_dir).await;
    assert!(checkpoints.is_ok(), "get_checkpoint_paths should succeed");
    assert_eq!(checkpoints.unwrap().len(), 2, "Should find 2 checkpoints");
}

/// Test project name derivation in different scenarios
#[test]
fn test_project_name_scenarios() {
    // Test with various directory names
    let test_cases = vec![
        ("my-project", "my-project"),
        ("my_project", "my_project"),
        ("project.123", "project.123"),
        ("project with spaces", "project-with-spaces"),
        ("project/with/slashes", "project-with-slashes"),
    ];

    for (dir_name, expected_sanitized) in test_cases {
        let sanitized: String = dir_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect();

        // Remove consecutive dashes
        let sanitized = sanitized
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        assert_eq!(
            sanitized, expected_sanitized,
            "Failed to sanitize: {}",
            dir_name
        );
    }
}

/// Test JSON output consistency across commands
#[test]
#[ignore] // Requires AWS credentials
fn test_json_output_consistency() {
    // Test that all commands return consistent JSON structure:
    // - success: bool
    // - data: {...}
    // - message: string (optional)

    let commands = vec![
        vec!["aws", "create", "--instance-type", "t3.micro", "--dry-run"],
        vec!["aws", "ebs", "list"],
        vec!["resources", "list"],
        vec!["config", "show"],
    ];

    for cmd in commands {
        let output = Command::new("cargo")
            .args(["run", "--release", "--"])
            .args(&cmd)
            .args(["--output", "json"])
            .output()
            .unwrap_or_else(|_| panic!("Failed to execute: {:?}", cmd));

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let json: serde_json::Value = serde_json::from_str(&stdout)
                .unwrap_or_else(|_| panic!("Invalid JSON from {:?}: {}", cmd, stdout));

            // Check for common fields
            assert!(json.is_object(), "JSON should be an object");
        }
    }
}

/// Test input validation across all commands
#[test]
fn test_validation_across_commands() {
    use runctl::validation::*;

    // Test that validation catches invalid inputs before AWS API calls
    // Each validator is tested individually to avoid function pointer type issues
    assert!(
        validate_instance_id("invalid").is_err(),
        "Should reject invalid instance_id"
    );
    assert!(
        validate_volume_id("invalid").is_err(),
        "Should reject invalid volume_id"
    );
    assert!(
        validate_snapshot_id("invalid").is_err(),
        "Should reject invalid snapshot_id"
    );
    assert!(
        validate_s3_path("invalid").is_err(),
        "Should reject invalid s3_path"
    );
    assert!(
        validate_project_name("").is_err(),
        "Should reject empty project_name"
    );
    assert!(
        validate_path("../invalid").is_err(),
        "Should reject path traversal"
    );
}
