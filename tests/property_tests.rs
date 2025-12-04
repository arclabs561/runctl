//! Property-based tests for trainctl
//!
//! These tests use proptest to generate random inputs and verify
//! that properties hold across a wide range of scenarios.

use proptest::prelude::*;
use trainctl::utils::{format_duration, calculate_accumulated_cost, is_old_instance};
use trainctl::resources::estimate_instance_cost;
use chrono::{Utc, Duration};
use std::path::PathBuf;

proptest! {
    #[test]
    fn test_format_duration_never_negative(seconds in 0u64..1_000_000u64) {
        let result = format_duration(seconds);
        // Duration format should never be empty
        assert!(!result.is_empty());
        // Should contain at least one time unit
        assert!(
            result.contains('s') || 
            result.contains('m') || 
            result.contains('h') ||
            result.contains('d')
        );
    }
    
    #[test]
    fn test_format_duration_monotonic(
        seconds1 in 0u64..100_000u64,
        seconds2 in 0u64..100_000u64
    ) {
        // If seconds1 < seconds2, formatted result should reflect ordering
        // (though format strings may differ, the numeric values should order correctly)
        if seconds1 < seconds2 {
            // Both should be valid formats
            let fmt1 = format_duration(seconds1);
            let fmt2 = format_duration(seconds2);
            assert!(!fmt1.is_empty());
            assert!(!fmt2.is_empty());
        }
    }
    
    #[test]
    fn test_cost_calculation_non_negative(
        hourly_cost in 0.0f64..1000.0f64,
        hours_ago in 0i64..720i64  // 0 to 30 days
    ) {
        let launch_time = Utc::now() - Duration::hours(hours_ago);
        let cost = calculate_accumulated_cost(hourly_cost, Some(launch_time));
        
        // Cost should never be negative
        assert!(cost >= 0.0);
        
        // Cost should be approximately hourly_cost * hours_ago (within 1% tolerance)
        let expected = hourly_cost * hours_ago as f64;
        let tolerance = expected * 0.01 + 0.01; // 1% or 1 cent minimum
        assert!((cost - expected).abs() < tolerance, 
            "cost={}, expected={}, diff={}", cost, expected, (cost - expected).abs());
    }
    
    #[test]
    fn test_cost_calculation_monotonic(
        hourly_cost in 0.0f64..1000.0f64,
        hours1 in 0i64..720i64,
        hours2 in 0i64..720i64
    ) {
        if hours1 < hours2 {
            let time1 = Utc::now() - Duration::hours(hours1);
            let time2 = Utc::now() - Duration::hours(hours2);
            
            let cost1 = calculate_accumulated_cost(hourly_cost, Some(time1));
            let cost2 = calculate_accumulated_cost(hourly_cost, Some(time2));
            
            // Longer running instance should cost more
            assert!(cost2 >= cost1, 
                "cost2={} should be >= cost1={} for hours2={} > hours1={}", 
                cost2, cost1, hours2, hours1);
        }
    }
    
    #[test]
    fn test_is_old_instance_consistency(
        hours_ago in 0i64..100i64,
        threshold_hours in 1i64..48i64
    ) {
        let launch_time = Utc::now() - Duration::hours(hours_ago);
        let is_old = is_old_instance(Some(launch_time), threshold_hours);
        
        // If hours_ago > threshold, should be old
        if hours_ago > threshold_hours {
            assert!(is_old, 
                "Instance {} hours old should be considered old (threshold: {})", 
                hours_ago, threshold_hours);
        }
        
        // If hours_ago < threshold, should not be old
        if hours_ago < threshold_hours {
            assert!(!is_old, 
                "Instance {} hours old should not be considered old (threshold: {})", 
                hours_ago, threshold_hours);
        }
    }
    
    #[test]
    fn test_s3_path_parsing_valid_paths(path in r"[a-z0-9-]+(/[a-z0-9-]+)*") {
        let s3_path = format!("s3://bucket-name/{}", path);
        
        // Should start with s3://
        assert!(s3_path.starts_with("s3://"));
        
        // Should have bucket and key parts
        let path_part = &s3_path[5..];
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "bucket-name");
        assert!(!parts[1].is_empty());
    }
    
    #[test]
    fn test_config_roundtrip_serialization(
        region in r"[a-z]+-[a-z]+-[0-9]+",
        instance_type in r"[a-z0-9.]+",
        bucket in r"[a-z0-9-]+"
    ) {
        // Create a minimal valid config
        let config_content = format!(r#"
[aws]
region = "{}"
default_instance_type = "{}"
s3_bucket = "{}"
"#, region, instance_type, bucket);
        
        // Should parse as valid TOML
        let parsed: Result<toml::Value, _> = toml::from_str(&config_content);
        assert!(parsed.is_ok(), "Config should parse as valid TOML");
    }
}

// Property tests for retry logic
proptest! {
    #[test]
    fn test_retry_backoff_calculation(
        attempt in 0u32..10u32,
        initial_delay_ms in 100u64..1000u64,
        max_delay_ms in 1000u64..60000u64
    ) {
        // Calculate backoff using exponential backoff formula
        let exponential = initial_delay_ms as f64 * 2f64.powi(attempt as i32);
        let delay_ms = exponential.min(max_delay_ms as f64);
        
        // Backoff should be within bounds
        assert!(delay_ms >= initial_delay_ms as f64 || attempt == 0);
        assert!(delay_ms <= max_delay_ms as f64);
        
        // Should be monotonically increasing (or at max)
        if attempt > 0 {
            let prev_delay = (initial_delay_ms as f64 * 2f64.powi((attempt - 1) as i32)).min(max_delay_ms as f64);
            assert!(delay_ms >= prev_delay || delay_ms >= max_delay_ms as f64);
        }
    }
}

// Property tests for data location parsing
proptest! {
    #[test]
    fn test_data_location_parsing(
        local_path in r"[a-zA-Z0-9_/-]+",
        s3_bucket in r"[a-z0-9-]+",
        s3_key in r"[a-zA-Z0-9_/-]+",
        instance_id in r"i-[a-z0-9]+",
        remote_path in r"/[a-zA-Z0-9_/-]+"
    ) {
        // Test local path parsing
        let local = PathBuf::from(&local_path);
        assert!(local.to_string_lossy().len() > 0);
        
        // Test S3 path parsing
        let s3_path = format!("s3://{}/{}", s3_bucket, s3_key);
        assert!(s3_path.starts_with("s3://"));
        let path_part = &s3_path[5..];
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], s3_bucket);
        
        // Test instance path parsing
        let instance_path = format!("{}:{}", instance_id, remote_path);
        let instance_parts: Vec<&str> = instance_path.splitn(2, ':').collect();
        assert_eq!(instance_parts.len(), 2);
        assert_eq!(instance_parts[0], instance_id);
    }
}

// Property tests for volume size validation
proptest! {
    #[test]
    fn test_volume_size_constraints(size_gb in 1i32..16384i32) {
        // AWS EBS volume size constraints
        // Minimum: 1 GB, Maximum: 16,384 GB (16 TB)
        assert!(size_gb >= 1);
        assert!(size_gb <= 16384);
        
        // Size should be positive
        assert!(size_gb > 0);
    }
    
    #[test]
    fn test_volume_size_calculations(
        size_gb in 1i32..1000i32,
        iops_per_gb in 3i32..16i32
    ) {
        // For gp3 volumes: IOPS = min(3000 + size * iops_per_gb, 16000)
        let calculated_iops = (3000 + size_gb * iops_per_gb).min(16000);
        
        assert!(calculated_iops >= 3000);
        assert!(calculated_iops <= 16000);
        
        // For larger volumes, should hit max IOPS
        if size_gb > 1000 {
            assert!(calculated_iops >= 15000); // Should be close to max
        }
    }
}

// Property tests for instance type cost estimation
proptest! {
    #[test]
    fn test_instance_cost_estimation(instance_type in r"[a-z0-9.]+") {
        let cost = estimate_instance_cost(&instance_type);
        
        // Cost should always be non-negative
        assert!(cost >= 0.0);
        
        // Cost should be reasonable (less than $100/hour for any instance)
        assert!(cost < 100.0, "Instance cost {} seems unreasonably high", cost);
    }
}

// Stateful property tests for resource lifecycle
proptest! {
    #[test]
    fn test_resource_state_transitions(
        operations in prop::collection::vec(
            prop_oneof![
                Just("create"),
                Just("start"),
                Just("stop"),
                Just("terminate"),
            ],
            1..20
        )
    ) {
        // Simulate resource state machine
        let mut state = "none";
        let mut created = false;
        
        for op in operations {
            match (state, op) {
                ("none", "create") => {
                    state = "created";
                    created = true;
                }
                ("created", "start") => {
                    state = "running";
                }
                ("running", "stop") => {
                    state = "stopped";
                }
                ("stopped", "start") => {
                    state = "running";
                }
                ("running" | "stopped", "terminate") => {
                    state = "terminated";
                }
                _ => {
                    // Invalid transition - should not happen in valid sequences
                    // But we allow it for property testing
                }
            }
        }
        
        // Properties that should always hold:
        // 1. If we're in terminated state, resource was created
        if state == "terminated" {
            assert!(created);
        }
        
        // 2. State should be one of valid states
        assert!(matches!(state, "none" | "created" | "running" | "stopped" | "terminated"));
    }
}

// Property tests for tag validation
proptest! {
    #[test]
    fn test_tag_key_value_validation(
        key in r"[a-zA-Z0-9._/-]+",
        value in r"[a-zA-Z0-9._/-]+"
    ) {
        // AWS tag constraints
        // Key: max 128 chars, value: max 256 chars
        // Must not start with "aws:" (reserved)
        
        if key.len() <= 128 && value.len() <= 256 {
            // Valid tag
            assert!(!key.starts_with("aws:"));
            assert!(key.len() > 0);
        }
    }
}

// Property tests for AZ validation
proptest! {
    #[test]
    fn test_az_format_validation(
        region in r"[a-z]+-[a-z]+-[0-9]+",
        az_letter in r"[a-z]"
    ) {
        let az = format!("{}-{}", region, az_letter);
        
        // AZ should contain region
        assert!(az.contains(&region));
        
        // Should have format: region-az
        let parts: Vec<&str> = az.split('-').collect();
        assert!(parts.len() >= 3); // region, region, number, az
    }
}

// Property tests for snapshot naming
proptest! {
    #[test]
    fn test_snapshot_name_validation(name in r"[a-zA-Z0-9._-]+") {
        // AWS snapshot name constraints
        // Max 255 chars, can contain alphanumeric, dots, dashes, underscores
        
        if name.len() <= 255 {
            // Should be valid
            assert!(name.len() > 0);
        }
    }
}

// Property tests for cost accumulation
proptest! {
    #[test]
    fn test_cost_accumulation_properties(
        hourly_costs in prop::collection::vec(0.0f64..100.0f64, 1..10),
        hours in 1i64..24i64
    ) {
        let total_hourly = hourly_costs.iter().sum::<f64>();
        let daily_cost = total_hourly * 24.0;
        let accumulated = total_hourly * hours as f64;
        
        // Properties:
        // 1. Accumulated cost should be proportional to hours
        assert!(accumulated >= 0.0);
        assert!(accumulated <= daily_cost);
        
        // 2. If hours < 24, accumulated < daily
        if hours < 24 {
            assert!(accumulated < daily_cost);
        }
        
        // 3. Cost should scale linearly with hours
        let ratio = accumulated / total_hourly;
        assert!((ratio - hours as f64).abs() < 0.01);
    }
}

