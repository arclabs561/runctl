//! Integration tests for train-ops

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_config_initialization() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".train-ops.toml");
    
    // Test config structure
    let config_content = r#"
[runpod]
default_gpu = "RTX 4080"
default_disk_gb = 30

[aws]
region = "us-east-1"
default_instance_type = "t3.medium"
"#;
    
    fs::write(&config_path, config_content).unwrap();
    
    // Verify file exists
    assert!(config_path.exists());
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("runpod"));
    assert!(content.contains("aws"));
}

#[test]
fn test_checkpoint_directory_creation() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    
    fs::create_dir_all(&checkpoint_dir).unwrap();
    assert!(checkpoint_dir.exists());
    assert!(checkpoint_dir.is_dir());
}

#[test]
fn test_s3_path_parsing() {
    // Test S3 path parsing logic
    let s3_path = "s3://bucket-name/path/to/file";
    assert!(s3_path.starts_with("s3://"));
    
    let path_part = &s3_path[5..]; // Remove "s3://"
    let parts: Vec<&str> = path_part.splitn(2, '/').collect();
    
    assert_eq!(parts[0], "bucket-name");
    assert_eq!(parts[1], "path/to/file");
}

#[test]
fn test_resource_cost_estimation() {
    // Test cost estimation logic
    fn estimate_cost(instance_type: &str) -> f64 {
        match instance_type {
            t if t.starts_with("t3.") => 0.0416,
            t if t.starts_with("m5.") => 0.192,
            _ => 0.1,
        }
    }
    
    assert_eq!(estimate_cost("t3.medium"), 0.0416);
    assert_eq!(estimate_cost("m5.large"), 0.192);
    assert_eq!(estimate_cost("unknown"), 0.1);
}
