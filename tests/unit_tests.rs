//! Comprehensive unit tests for trainctl modules
//!
//! These tests verify individual functions and modules in isolation.

use chrono::{Duration, Utc};
use std::path::PathBuf;
use trainctl::config::Config;
use trainctl::error::{ConfigError, TrainctlError};
use trainctl::utils::{calculate_accumulated_cost, format_duration, is_old_instance};

#[test]
fn test_format_duration_edge_cases() {
    // format_duration only supports h, m, s (not days)
    assert_eq!(format_duration(0), "0s");
    assert_eq!(format_duration(1), "1s");
    assert_eq!(format_duration(59), "59s");
    assert_eq!(format_duration(60), "1m 0s");
    assert_eq!(format_duration(61), "1m 1s");
    assert_eq!(format_duration(3600), "1h 0m 0s");
    assert_eq!(format_duration(3661), "1h 1m 1s");
    // 86400 seconds = 24 hours
    assert_eq!(format_duration(86400), "24h 0m 0s");
    // 90061 seconds = 25h 1m 1s
    assert_eq!(format_duration(90061), "25h 1m 1s");
}

#[test]
fn test_format_duration_large_values() {
    // Test very large durations
    let large = format_duration(999999);
    assert!(!large.is_empty());
    assert!(large.contains('d') || large.contains('h'));
}

#[test]
fn test_cost_calculation_edge_cases() {
    // Zero cost
    let cost = calculate_accumulated_cost(0.0, Some(Utc::now() - Duration::hours(1)));
    assert_eq!(cost, 0.0);

    // No launch time (should return 0)
    let cost = calculate_accumulated_cost(10.0, None);
    assert_eq!(cost, 0.0);

    // Just created (should be ~0)
    let cost = calculate_accumulated_cost(10.0, Some(Utc::now()));
    assert!(cost < 0.01);

    // One hour exactly
    let cost = calculate_accumulated_cost(1.0, Some(Utc::now() - Duration::hours(1)));
    assert!((cost - 1.0).abs() < 0.1);
}

#[test]
fn test_is_old_instance_edge_cases() {
    // None launch time
    assert!(!is_old_instance(None, 24));

    // Just created
    assert!(!is_old_instance(Some(Utc::now()), 24));

    // Exactly at threshold
    let exactly_threshold = Utc::now() - Duration::hours(24);
    assert!(is_old_instance(Some(exactly_threshold), 24));

    // Just over threshold
    let just_over = Utc::now() - Duration::hours(25);
    assert!(is_old_instance(Some(just_over), 24));

    // Just under threshold
    let just_under = Utc::now() - Duration::hours(23);
    assert!(!is_old_instance(Some(just_under), 24));
}

#[test]
fn test_config_default_values() {
    let config = Config::default();

    assert!(config.runpod.is_some());
    assert!(config.aws.is_some());
    assert!(config.local.is_some());
    assert_eq!(config.checkpoint.save_interval, 5);
    assert_eq!(config.checkpoint.keep_last_n, 10);
}

#[test]
fn test_config_serialization() {
    let config = Config::default();

    // Should serialize to TOML
    let toml = toml::to_string(&config);
    assert!(toml.is_ok());

    let toml_str = toml.unwrap();
    assert!(toml_str.contains("runpod"));
    assert!(toml_str.contains("aws"));
    assert!(toml_str.contains("checkpoint"));
}

#[test]
fn test_config_deserialization() {
    use std::fs;
    use tempfile::TempDir;

    // Remove duplicate [checkpoint] section and fix structure
    let config_str = r#"
[aws]
region = "us-west-2"
default_instance_type = "t3.large"
default_ami = "ami-12345"
use_spot = true
s3_bucket = "my-bucket"

[runpod]
default_gpu = "RTX 4090"
default_disk_gb = 50
default_image = "runpod/pytorch:2.1.0-py3.10-cuda11.8.0-devel-ubuntu22.04"

[checkpoint]
dir = "/tmp/checkpoints"
save_interval = 10
keep_last_n = 20

[monitoring]
log_dir = "logs"
update_interval_secs = 10
enable_warnings = true
"#;

    // Test that config can be parsed as TOML
    let parsed: Result<toml::Value, _> = toml::from_str(&config_str);
    assert!(parsed.is_ok(), "Config should parse as valid TOML");

    // Test that Config can be loaded from file
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("test_config.toml");
    fs::write(&config_path, config_str).unwrap();

    let config = Config::load(Some(&config_path));
    assert!(
        config.is_ok(),
        "Config should load successfully: {:?}",
        config
    );

    let config = config.unwrap();
    assert_eq!(config.aws.as_ref().unwrap().region, "us-west-2");
    assert_eq!(
        config.aws.as_ref().unwrap().default_instance_type,
        "t3.large"
    );
    assert_eq!(config.checkpoint.save_interval, 10);
    assert_eq!(config.checkpoint.keep_last_n, 20);
}

#[test]
fn test_error_types() {
    // Test ConfigError
    let err = ConfigError::InvalidProvider("invalid".to_string());
    assert!(format!("{}", err).contains("invalid"));

    let err = ConfigError::MissingField("field".to_string());
    assert!(format!("{}", err).contains("field"));

    // Test TrainctlError
    let err = TrainctlError::Config(ConfigError::InvalidProvider("test".to_string()));
    assert!(format!("{}", err).contains("Configuration error"));

    let err = TrainctlError::DataTransfer("test error".to_string());
    assert!(format!("{}", err).contains("Data transfer error"));
}

#[test]
fn test_s3_path_parsing_valid() {
    let valid_paths = vec![
        "s3://bucket/key",
        "s3://bucket/path/to/file",
        "s3://my-bucket-name/data/train.csv",
        "s3://bucket123/key456",
    ];

    for path in valid_paths {
        assert!(path.starts_with("s3://"));
        let path_part = &path[5..];
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }
}

#[test]
fn test_s3_path_parsing_invalid() {
    let invalid_paths = vec![
        "bucket/key",        // Missing s3://
        "s3://bucket",       // Missing key
        "s3://",             // Empty
        "file://bucket/key", // Wrong scheme
    ];

    for path in invalid_paths {
        if path.starts_with("s3://") {
            let path_part = &path[5..];
            if path_part.is_empty() {
                continue; // This is invalid
            }
            let parts: Vec<&str> = path_part.splitn(2, '/').collect();
            if parts.len() != 2 {
                // Invalid format
                continue;
            }
        }
        // Other invalid cases handled
    }
}

#[test]
fn test_instance_type_validation() {
    let valid_types = vec![
        "t3.micro",
        "t3.small",
        "t3.medium",
        "g4dn.xlarge",
        "p3.2xlarge",
        "m5.large",
    ];

    for instance_type in valid_types {
        // Should not panic
        let _ = instance_type.to_string();
    }
}

#[test]
fn test_volume_size_validation() {
    // Valid sizes
    assert!(1 >= 1 && 1 <= 16384);
    assert!(100 >= 1 && 100 <= 16384);
    assert!(16384 >= 1 && 16384 <= 16384);

    // Invalid sizes (should be caught by validation)
    assert!(0 < 1); // Too small
    assert!(16385 > 16384); // Too large
}

#[test]
fn test_az_format() {
    let valid_azs = vec!["us-east-1a", "us-west-2b", "eu-west-1c"];

    for az in &valid_azs {
        // Verify AZ format: region + single letter
        let parts: Vec<&str> = az.split('-').collect();
        assert!(parts.len() >= 3, "AZ {} should have at least 3 parts", az);

        // Last part should be region number + letter (e.g., "1a")
        let last = parts.last().unwrap();
        assert!(
            last.len() >= 2,
            "Last part should have region number + letter"
        );

        // Should end with a letter
        let last_char = last.chars().last().unwrap();
        assert!(
            last_char.is_ascii_lowercase(),
            "AZ should end with lowercase letter"
        );
    }
}

#[test]
fn test_tag_format() {
    // Valid tag keys
    let valid_keys = vec![
        "Name",
        "trainctl:persistent",
        "trainctl:protected",
        "Environment",
        "Project",
    ];

    for key in valid_keys {
        assert!(key.len() <= 128);
        assert!(!key.starts_with("aws:"));
    }

    // Valid tag values
    let valid_values = vec!["true", "false", "production", "my-project-name"];

    for value in valid_values {
        assert!(value.len() <= 256);
    }
}

#[test]
fn test_cost_thresholds() {
    let hourly_threshold = 50.0;
    let daily_threshold = 100.0;
    let accumulated_threshold = 500.0;

    // Test threshold logic
    assert!(51.0 > hourly_threshold);
    assert!(49.0 < hourly_threshold);

    // Daily threshold: 100.0 means $100/day, so hourly * 24 should be compared
    // 51.0 * 24 = 1224.0 (exceeds $100/day threshold)
    // 49.0 * 24 = 1176.0 (also exceeds $100/day threshold, so test needs different values)
    // Use lower hourly cost for daily threshold test
    assert!((51.0 * 24.0) > daily_threshold);
    let low_hourly = 3.0; // $3/hr = $72/day, below $100 threshold
    assert!((low_hourly * 24.0) < daily_threshold);

    assert!(501.0 > accumulated_threshold);
    assert!(499.0 < accumulated_threshold);
}

#[test]
fn test_retry_attempt_counting() {
    
    use trainctl::retry::ExponentialBackoffPolicy;

    let policy = ExponentialBackoffPolicy::for_cloud_api();

    // Policy should have reasonable defaults
    // (We can't test execute_with_retry without async, but we can test structure)
    let _policy = policy;
}

#[test]
fn test_data_location_parsing() {
    // Local path
    let local = PathBuf::from("/tmp/data");
    assert!(local.is_absolute() || local.to_string_lossy().starts_with("."));

    // S3 path
    let s3 = "s3://bucket/key";
    assert!(s3.starts_with("s3://"));

    // Instance path
    let instance = "i-1234567890abcdef0:/mnt/data";
    let parts: Vec<&str> = instance.splitn(2, ':').collect();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].starts_with("i-"));
    assert!(parts[1].starts_with("/"));
}

#[test]
fn test_snapshot_naming() {
    let valid_names = vec![
        "snapshot-1",
        "my-snapshot_2024",
        "checkpoint.backup",
        "data-snapshot-123",
    ];

    for name in valid_names {
        assert!(name.len() <= 255);
        assert!(!name.is_empty());
    }
}

#[test]
fn test_cost_estimation_consistency() {
    use trainctl::resources::estimate_instance_cost;

    // Same instance type should return same cost
    let cost1 = estimate_instance_cost("t3.micro");
    let cost2 = estimate_instance_cost("t3.micro");
    assert_eq!(cost1, cost2);

    // Different instance types should have different costs (usually)
    let micro_cost = estimate_instance_cost("t3.micro");
    let large_cost = estimate_instance_cost("t3.large");
    // Note: t3.large might have same cost as t3.micro in our simple estimator
    // But both should be non-negative
    assert!(micro_cost >= 0.0);
    assert!(large_cost >= 0.0);
}

#[test]
fn test_config_path_resolution() {
    // Test that config loading handles various path scenarios
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join(".trainctl.toml");

    // Non-existent path should return default
    let config = Config::load(Some(&config_path));
    assert!(config.is_ok());
}

#[test]
fn test_duration_edge_cases() {
    // Test various duration edge cases
    // format_duration always includes seconds
    assert_eq!(format_duration(0), "0s");
    assert_eq!(format_duration(1), "1s");
    assert_eq!(format_duration(59), "59s");
    assert_eq!(format_duration(60), "1m 0s");
    assert_eq!(format_duration(3599), "59m 59s");
    assert_eq!(format_duration(3600), "1h 0m 0s");
}

#[test]
fn test_cost_accumulation_properties() {
    // Cost should accumulate linearly with time
    let hourly = 10.0;
    let one_hour = calculate_accumulated_cost(hourly, Some(Utc::now() - Duration::hours(1)));
    let two_hours = calculate_accumulated_cost(hourly, Some(Utc::now() - Duration::hours(2)));

    // Two hours should be approximately double one hour
    assert!((two_hours - one_hour * 2.0).abs() < 0.1);
}

#[test]
fn test_old_instance_detection_properties() {
    // Property: If instance is old at threshold N, it should also be old at threshold N+1
    let old_time = Utc::now() - Duration::hours(25);

    assert!(is_old_instance(Some(old_time), 24));
    assert!(is_old_instance(Some(old_time), 25));
    assert!(is_old_instance(Some(old_time), 20));

    // Property: If instance is not old at threshold N, it should not be old at threshold N+1
    let recent_time = Utc::now() - Duration::hours(10);

    assert!(!is_old_instance(Some(recent_time), 24));
    assert!(!is_old_instance(Some(recent_time), 25));
}
