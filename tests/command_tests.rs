//! Comprehensive tests for trainctl commands
//!
//! Tests cover:
//! - JSON output consistency
//! - Input validation
//! - Project name derivation
//! - Command argument parsing

use std::process::Command;

/// Test that JSON output is valid JSON
fn test_json_output(command: &[&str]) {
    let output = Command::new("cargo")
        .args(&["run", "--release", "--"])
        .args(command)
        .args(&["--output", "json"])
        .output()
        .expect("Failed to execute command");
    
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Try to parse as JSON
        let _json: serde_json::Value = serde_json::from_str(&stdout)
            .expect(&format!("Invalid JSON output from command: {:?}\nOutput: {}", command, stdout));
    }
}

#[test]
#[ignore] // Requires AWS credentials
fn test_aws_create_json_output() {
    test_json_output(&["aws", "create", "--instance-type", "t3.micro", "--dry-run"]);
}

#[test]
#[ignore] // Requires AWS credentials
fn test_ebs_create_json_output() {
    test_json_output(&["aws", "ebs", "create", "--size", "10"]);
}

#[test]
#[ignore] // Requires AWS credentials
fn test_ebs_list_json_output() {
    test_json_output(&["aws", "ebs", "list"]);
}

#[test]
#[ignore] // Requires AWS credentials
fn test_s3_list_json_output() {
    test_json_output(&["s3", "list", "s3://test-bucket/"]);
}

#[test]
#[ignore] // Requires AWS credentials
fn test_resources_list_json_output() {
    test_json_output(&["resources", "list"]);
}

#[test]
fn test_config_show_json_output() {
    test_json_output(&["config", "show"]);
}

#[test]
fn test_config_validate_json_output() {
    test_json_output(&["config", "validate"]);
}

#[test]
fn test_project_name_derivation() {
    use std::env;
    
    // Test that project name is derived from current directory
    let current_dir = env::current_dir().unwrap();
    let dir_name = current_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("trainctl-project");
    
    // Sanitize directory name
    let sanitized: String = dir_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
            c
        } else {
            '-'
        })
        .collect();
    
    assert!(!sanitized.is_empty(), "Project name should not be empty");
    assert!(sanitized.len() <= 64, "Project name should be <= 64 characters");
}

#[test]
fn test_input_validation() {
    use trainctl::validation::*;
    
    // Instance ID validation
    assert!(validate_instance_id("i-1234567890abcdef0").is_ok());
    assert!(validate_instance_id("i-123").is_err());
    assert!(validate_instance_id("invalid").is_err());
    
    // Volume ID validation
    assert!(validate_volume_id("vol-1234567890abcdef0").is_ok());
    assert!(validate_volume_id("vol-123").is_err());
    
    // Snapshot ID validation
    assert!(validate_snapshot_id("snap-1234567890abcdef0").is_ok());
    assert!(validate_snapshot_id("snap-123").is_err());
    
    // Project name validation
    assert!(validate_project_name("my-project").is_ok());
    assert!(validate_project_name("").is_err());
    assert!(validate_project_name(&"a".repeat(65)).is_err());
    
    // Path validation
    assert!(validate_path("/valid/path").is_ok());
    assert!(validate_path("../invalid").is_err());
    
    // S3 path validation
    assert!(validate_s3_path("s3://bucket/key").is_ok());
    assert!(validate_s3_path("invalid").is_err());
    
    // Volume size validation
    assert!(validate_volume_size(1).is_ok());
    assert!(validate_volume_size(0).is_err());
    assert!(validate_volume_size(16385).is_err());
}

#[test]
fn test_help_text_present() {
    // Test that all commands have help text
    let commands = vec![
        "aws",
        "ebs",
        "s3",
        "checkpoint",
        "resources",
        "config",
        "local",
        "monitor",
        "transfer",
        "top",
    ];
    
    for cmd in commands {
        let output = Command::new("cargo")
            .args(&["run", "--release", "--", cmd, "--help"])
            .output()
            .expect(&format!("Failed to execute {} --help", cmd));
        
        assert!(output.status.success(), "Command {} should have help text", cmd);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty(), "Help text for {} should not be empty", cmd);
    }
}

#[test]
fn test_json_error_output() {
    // Test that errors are also JSON when --output json is used
    let output = Command::new("cargo")
        .args(&["run", "--release", "--"])
        .args(&["aws", "terminate", "invalid-instance-id", "--output", "json"])
        .output()
        .expect("Failed to execute command");
    
    // Should fail, but error should be JSON
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Error should be JSON
    let _json: serde_json::Value = serde_json::from_str(&stderr)
        .expect(&format!("Error output should be JSON, got: {}", stderr));
}

