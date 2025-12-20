//! Integration tests for trainctl

use std::fs;
use tempfile::TempDir;
use trainctl::config::{init_config, Config};

#[test]
fn test_config_initialization() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".trainctl.toml");

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
fn test_config_load_and_save() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config.toml");

    // Create default config
    let config = Config::default();
    config.save(&config_path).unwrap();

    // Load it back
    let loaded = Config::load(Some(&config_path)).unwrap();
    assert_eq!(
        loaded.checkpoint.save_interval,
        config.checkpoint.save_interval
    );
    assert_eq!(loaded.checkpoint.keep_last_n, config.checkpoint.keep_last_n);
}

#[test]
fn test_init_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("init_test.toml");

    init_config(&config_path).unwrap();
    assert!(config_path.exists());

    // Verify it's valid TOML
    let config = Config::load(Some(&config_path)).unwrap();
    assert!(config.runpod.is_some());
    assert!(config.aws.is_some());
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
fn test_s3_path_parsing_basic() {
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
    // Test using the actual function from resources module
    use trainctl::resources::estimate_instance_cost;

    assert_eq!(estimate_instance_cost("t3.medium"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.large"), 0.0416);
    assert_eq!(estimate_instance_cost("m5.large"), 0.192);
    assert_eq!(estimate_instance_cost("g4dn.xlarge"), 0.526);
    assert_eq!(estimate_instance_cost("unknown"), 0.1);
}

#[tokio::test]
async fn test_checkpoint_operations() {
    use trainctl::checkpoint::get_checkpoint_paths;

    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();

    // Create test checkpoints
    for i in 1..=5 {
        let checkpoint = checkpoint_dir.join(format!("checkpoint_{}.pt", i));
        fs::write(&checkpoint, b"test checkpoint").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Test listing
    let checkpoints = get_checkpoint_paths(&checkpoint_dir).await.unwrap();
    assert_eq!(checkpoints.len(), 5);

    // Note: cleanup_checkpoints is not exported, so we test listing only
    // Full cleanup tests are in checkpoint module unit tests
}

#[test]
fn test_utils_functions() {
    use chrono::Utc;
    use trainctl::utils::{calculate_accumulated_cost, format_duration, is_old_instance};

    // Test duration formatting
    assert_eq!(format_duration(0), "0s");
    assert_eq!(format_duration(30), "30s");
    assert_eq!(format_duration(90), "1m 30s");
    assert_eq!(format_duration(3665), "1h 1m 5s");

    // Test cost calculation
    let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
    let cost = calculate_accumulated_cost(1.0, Some(one_hour_ago));
    assert!((cost - 1.0).abs() < 0.1); // Allow some time drift

    // Test old instance detection
    let old_time = Utc::now() - chrono::Duration::hours(25);
    assert!(is_old_instance(Some(old_time), 24));

    let recent_time = Utc::now() - chrono::Duration::hours(1);
    assert!(!is_old_instance(Some(recent_time), 24));
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

    // Test edge cases
    let bucket_only = "s3://bucket-name";
    assert!(bucket_only.starts_with("s3://"));

    let nested = "s3://bucket/path/to/deep/file.txt";
    let nested_parts: Vec<&str> = nested[5..].splitn(2, '/').collect();
    assert_eq!(nested_parts[0], "bucket");
    assert_eq!(nested_parts[1], "path/to/deep/file.txt");
}
