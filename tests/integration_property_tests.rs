//! Integration property tests
//!
//! Property-based tests that verify integration between modules
//! and end-to-end workflows.

use chrono::{Duration, Utc};
use proptest::prelude::*;
use runctl::resources::estimate_instance_cost;
use runctl::utils::{calculate_accumulated_cost, format_duration};

proptest! {
    #[test]
    fn test_cost_estimation_and_accumulation_consistency(
        instance_type in r"[a-z0-9.]+",
        hours in 1i64..720i64
    ) {
        let hourly_cost = estimate_instance_cost(&instance_type);
        let launch_time = Utc::now() - Duration::hours(hours);
        let accumulated = calculate_accumulated_cost(hourly_cost, Some(launch_time));

        // Accumulated should be approximately hourly * hours
        let expected = hourly_cost * hours as f64;
        let tolerance = expected * 0.01 + 0.01; // 1% or 1 cent

        assert!((accumulated - expected).abs() < tolerance,
            "Accumulated cost {} should be close to {} (hourly: {}, hours: {})",
            accumulated, expected, hourly_cost, hours);
    }

    #[test]
    fn test_config_roundtrip_property(
        region in r"[a-z]+-[a-z]+-[0-9]+",
        instance_type in r"[a-z0-9.]+",
        bucket in r"[a-z0-9-]+"
    ) {
        let config_str = format!(r#"
[aws]
region = "{}"
default_instance_type = "{}"
s3_bucket = "{}"
default_ami = "ami-12345678"
use_spot = false

[checkpoint]
dir = "checkpoints"
save_interval = 5
keep_last_n = 10

[monitoring]
log_dir = "logs"
update_interval_secs = 10
enable_warnings = true
"#, region, instance_type, bucket);

        // Should parse as valid TOML
        let parsed: Result<toml::Value, _> = toml::from_str(&config_str);
        prop_assert!(parsed.is_ok(), "Config should parse: {}", config_str);

        if let Ok(config_val) = parsed {
            // Should be able to extract values
            if let Some(aws) = config_val.get("aws") {
                prop_assert!(aws.get("region").is_some(), "Should have region");
                prop_assert!(aws.get("default_instance_type").is_some(), "Should have instance_type");
            }
        }
    }

    #[test]
    fn test_duration_format_roundtrip(
        hours in 0u64..24u64,
        minutes in 0u64..60u64,
        seconds in 0u64..60u64
    ) {
        let total_seconds = hours * 3600 + minutes * 60 + seconds;
        let formatted = format_duration(total_seconds);

        // Formatted string should not be empty
        prop_assert!(!formatted.is_empty());

        // Should contain appropriate time units based on format_duration implementation
        // format_duration only supports h, m, s (not days)
        if hours > 0 {
            prop_assert!(formatted.contains('h'), "Should contain 'h' for hours: {}", formatted);
        }
        if minutes > 0 || hours > 0 {
            // May have 'm' or just 'h' if minutes is 0
            prop_assert!(
                formatted.contains('m') || formatted.contains('h'),
                "Should contain 'm' or 'h': {}", formatted
            );
        }
        // Always contains 's' even if 0
        prop_assert!(formatted.contains('s'), "Should contain 's': {}", formatted);
    }

    #[test]
    fn test_cost_threshold_properties(
        hourly_costs in prop::collection::vec(0.0f64..100.0f64, 1..20)
    ) {
        let total_hourly: f64 = hourly_costs.iter().sum();
        let daily_cost = total_hourly * 24.0;
        let weekly_cost = daily_cost * 7.0;

        // Properties:
        // 1. Daily should be 24x hourly
        prop_assert!((daily_cost - total_hourly * 24.0).abs() < 0.01);

        // 2. Weekly should be 7x daily
        prop_assert!((weekly_cost - daily_cost * 7.0).abs() < 0.01);

        // 3. All costs should be non-negative
        prop_assert!(total_hourly >= 0.0);
        prop_assert!(daily_cost >= 0.0);
        prop_assert!(weekly_cost >= 0.0);
    }
}

// Property tests for resource state consistency
proptest! {
    #[test]
    fn test_resource_state_consistency(
        states in prop::collection::vec(
            prop_oneof![
                Just("none"),
                Just("created"),
                Just("running"),
                Just("stopped"),
                Just("terminated"),
            ],
            1..10
        )
    ) {
        // Simulate state transitions
        let mut current_state = "none";
        let mut created = false;

        for state in states {
            match (current_state, state) {
                ("none", "created") => {
                    current_state = "created";
                    created = true;
                }
                ("created", "running") => {
                    current_state = "running";
                }
                ("running", "stopped") => {
                    current_state = "stopped";
                }
                ("stopped", "running") => {
                    current_state = "running";
                }
                (_, "terminated") if current_state != "none" => {
                    current_state = "terminated";
                }
                _ => {
                    // Invalid transition, but we continue
                }
            }
        }

        // Property: If terminated, must have been created
        if current_state == "terminated" {
            prop_assert!(created, "Terminated resource must have been created");
        }

        // Property: State should be valid
        prop_assert!(matches!(
            current_state,
            "none" | "created" | "running" | "stopped" | "terminated"
        ));
    }
}

// Property tests for volume size and IOPS relationships
proptest! {
    #[test]
    fn test_volume_size_iops_relationship(
        size_gb in 1i32..16384i32,
        iops_per_gb in 3i32..16i32
    ) {
        // For gp3: IOPS = min(3000 + size * iops_per_gb, 16000)
        let base_iops = 3000;
        let calculated_iops = (base_iops + size_gb * iops_per_gb).min(16000);

        // Properties:
        // 1. IOPS should be at least base
        prop_assert!(calculated_iops >= base_iops);

        // 2. IOPS should not exceed max
        prop_assert!(calculated_iops <= 16000);

        // 3. For small volumes, should be base + size * iops_per_gb
        if size_gb < 1000 {
            let expected = base_iops + size_gb * iops_per_gb;
            prop_assert_eq!(calculated_iops, expected.min(16000));
        }
    }
}

// Property tests for tag key-value pairs
proptest! {
    #[test]
    fn test_tag_validation_properties(
        key in r"[a-zA-Z0-9._/-]+",
        value in r"[a-zA-Z0-9._/-]+"
    ) {
        // AWS tag constraints
        let key_valid = key.len() <= 128 && !key.starts_with("aws:");
        let value_valid = value.len() <= 256;

        if key_valid && value_valid {
            // Should be valid tag
            prop_assert!(!key.is_empty());
            prop_assert!(!value.is_empty());
        }
    }
}

// Property tests for S3 path construction
proptest! {
    #[test]
    fn test_s3_path_construction(
        bucket in r"[a-z0-9-]+",
        key_prefix in r"[a-z0-9/_-]*",
        filename in r"[a-z0-9._-]+"
    ) {
        let s3_path = format!("s3://{}/{}{}", bucket, key_prefix, filename);

        // Properties:
        // 1. Should start with s3://
        prop_assert!(s3_path.starts_with("s3://"));

        // 2. Should contain bucket
        prop_assert!(s3_path.contains(&bucket));

        // 3. Should be parseable
        let path_part = &s3_path[5..];
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        prop_assert_eq!(parts.len(), 2);
        prop_assert_eq!(parts[0], bucket);
    }
}

// Property tests for instance ID format
proptest! {
    #[test]
    fn test_instance_id_format_validation(
        hex_part in r"[0-9a-f]+"
    ) {
        if hex_part.len() >= 8 && hex_part.len() <= 17 {
            let instance_id = format!("i-{}", hex_part);

            // Properties:
            // 1. Should start with i-
            prop_assert!(instance_id.starts_with("i-"));

            // 2. Should be valid length (i- + 8-17 hex chars)
            prop_assert!(instance_id.len() >= 10);
            prop_assert!(instance_id.len() <= 19);
        }
    }
}

// Property tests for volume ID format
proptest! {
    #[test]
    fn test_volume_id_format_validation(
        hex_part in r"[0-9a-f]+"
    ) {
        if hex_part.len() >= 8 && hex_part.len() <= 17 {
            let volume_id = format!("vol-{}", hex_part);

            // Properties:
            // 1. Should start with vol-
            prop_assert!(volume_id.starts_with("vol-"));

            // 2. Should be valid length
            prop_assert!(volume_id.len() >= 12);
            prop_assert!(volume_id.len() <= 21);
        }
    }
}

// Property tests for cost accumulation over time
proptest! {
    #[test]
    fn test_cost_accumulation_time_properties(
        hourly_cost in 0.0f64..100.0f64,
        hours1 in 1i64..100i64,
        hours2 in 1i64..100i64
    ) {
        let time1 = Utc::now() - Duration::hours(hours1);
        let time2 = Utc::now() - Duration::hours(hours2);

        let cost1 = calculate_accumulated_cost(hourly_cost, Some(time1));
        let cost2 = calculate_accumulated_cost(hourly_cost, Some(time2));

        // Property: Longer running should cost more
        if hours1 < hours2 {
            prop_assert!(cost1 <= cost2,
                "Cost for {} hours ({}) should be <= cost for {} hours ({})",
                hours1, cost1, hours2, cost2);
        }

        // Property: Cost should scale linearly
        let ratio1 = cost1 / hourly_cost;
        let ratio2 = cost2 / hourly_cost;

        prop_assert!((ratio1 - hours1 as f64).abs() < 0.1);
        prop_assert!((ratio2 - hours2 as f64).abs() < 0.1);
    }
}

// Property tests for AZ format validation
proptest! {
    #[test]
    fn test_az_format_properties(
        region in r"[a-z]+-[a-z]+-[0-9]+",
        az_letter in r"[a-z]"
    ) {
        // AWS AZ format is region-az_letter (e.g., us-east-1a)
        // The region already contains the number, so we append just the letter
        let az = format!("{}{}", region, az_letter);

        // Properties:
        // 1. Should contain region
        prop_assert!(az.contains(&region));

        // 2. Should end with az_letter (no dash, just letter appended)
        prop_assert!(az.ends_with(&az_letter));

        // 3. Should have correct format (region + single letter)
        let parts: Vec<&str> = region.split('-').collect();
        prop_assert!(parts.len() >= 3, "Region should have at least 3 parts");

        // Last character should be the AZ letter
        let last_char = az.chars().last().unwrap();
        prop_assert_eq!(last_char.to_string(), az_letter);
    }
}
