use crate::error::{Result, TrainctlError};
use chrono::{DateTime, Utc};
use std::path::Path;

pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create directory {}: {}", path.display(), e),
            ))
        })?;
    }
    Ok(())
}

pub fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

pub fn format_runtime(launch_time: Option<DateTime<Utc>>) -> Option<String> {
    launch_time.map(|lt| {
        let now = Utc::now();
        let duration = now.signed_duration_since(lt);
        let total_secs = duration.num_seconds().max(0) as u64;
        format_duration(total_secs)
    })
}

pub fn calculate_accumulated_cost(cost_per_hour: f64, launch_time: Option<DateTime<Utc>>) -> f64 {
    if let Some(lt) = launch_time {
        let now = Utc::now();
        let duration = now.signed_duration_since(lt);
        let hours = duration.num_seconds().max(0) as f64 / 3600.0;
        cost_per_hour * hours
    } else {
        0.0
    }
}

pub fn is_old_instance(launch_time: Option<DateTime<Utc>>, hours_threshold: i64) -> bool {
    if let Some(lt) = launch_time {
        let now = Utc::now();
        let duration = now.signed_duration_since(lt);
        duration.num_hours() >= hours_threshold
    } else {
        false
    }
}

/// Get hourly cost for an instance type (approximate)
/// Updated for 2024-2025 pricing (may vary by region)
pub fn get_instance_cost(instance_type: &str) -> f64 {
    match instance_type {
        // Burstable CPU instances (T3/T4)
        "t3.micro" => 0.0104,
        "t3.small" => 0.0208,
        "t3.medium" => 0.0416,
        "t3.large" => 0.0832,
        "t3.xlarge" => 0.1664,
        "t4g.micro" => 0.0084, // ARM-based, cheaper
        "t4g.small" => 0.0168,
        "t4g.medium" => 0.0336,
        "t4g.large" => 0.0672,

        // General purpose CPU instances
        "m5.large" => 0.096,
        "m5.xlarge" => 0.192,
        "m5.2xlarge" => 0.384,
        "m5.4xlarge" => 0.768,
        "m6i.large" => 0.096, // Latest generation Intel
        "m6i.xlarge" => 0.192,
        "m6i.2xlarge" => 0.384,
        "m6i.4xlarge" => 0.768,
        "m7i.large" => 0.108, // Latest generation Intel
        "m7i.xlarge" => 0.216,
        "m7i.2xlarge" => 0.432,
        "m7i.4xlarge" => 0.864,

        // Compute optimized
        "c5.large" => 0.085,
        "c5.xlarge" => 0.17,
        "c5.2xlarge" => 0.34,
        "c5.4xlarge" => 0.68,
        "c6i.large" => 0.085,
        "c6i.xlarge" => 0.17,
        "c6i.2xlarge" => 0.34,
        "c6i.4xlarge" => 0.68,

        // GPU instances - Entry level
        "g4dn.xlarge" => 0.526,   // 1x T4 GPU
        "g4dn.2xlarge" => 0.752,  // 1x T4 GPU
        "g4dn.4xlarge" => 1.204,  // 1x T4 GPU
        "g4dn.8xlarge" => 2.176,  // 1x T4 GPU
        "g4dn.12xlarge" => 3.912, // 4x T4 GPU
        "g4dn.16xlarge" => 4.352, // 1x T4 GPU

        // GPU instances - General purpose
        "g5.xlarge" => 1.006,    // 1x A10G GPU
        "g5.2xlarge" => 1.212,   // 1x A10G GPU
        "g5.4xlarge" => 1.624,   // 1x A10G GPU
        "g5.8xlarge" => 2.448,   // 1x A10G GPU
        "g5.12xlarge" => 3.672,  // 4x A10G GPU
        "g5.16xlarge" => 4.896,  // 1x A10G GPU
        "g5.24xlarge" => 7.344,  // 4x A10G GPU
        "g5.48xlarge" => 14.688, // 8x A10G GPU

        // GPU instances - High performance
        "p3.2xlarge" => 3.06,    // 1x V100 GPU (legacy)
        "p3.8xlarge" => 12.24,   // 4x V100 GPU
        "p3.16xlarge" => 24.48,  // 8x V100 GPU
        "p4d.24xlarge" => 32.77, // 8x A100 GPU
        "p5.48xlarge" => 98.32,  // 8x H100 GPU (latest)

        // Trn2 instances (AWS Trainium2) - Best price-performance
        "trn2.2xlarge" => 1.34,   // 1x Trainium2 chip
        "trn2.32xlarge" => 21.44, // 16x Trainium2 chips
        "trn2.48xlarge" => 32.16, // 24x Trainium2 chips

        // Default fallback - estimate based on instance type pattern
        _ => {
            if instance_type.starts_with("t3.") || instance_type.starts_with("t4g.") {
                0.05
            } else if instance_type.starts_with("m5.")
                || instance_type.starts_with("m6i.")
                || instance_type.starts_with("m7i.")
                || instance_type.starts_with("c5.")
                || instance_type.starts_with("c6i.")
            {
                0.2
            } else if instance_type.starts_with("g4dn.") {
                0.8
            } else if instance_type.starts_with("g5.") {
                2.0
            } else if instance_type.starts_with("p3.") {
                5.0
            } else if instance_type.starts_with("p4") {
                30.0
            } else if instance_type.starts_with("p5.") {
                50.0
            } else if instance_type.starts_with("trn2.") {
                10.0 // Trn2 offers 30-40% better price-performance
            } else {
                0.1 // Conservative default
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3665), "1h 1m 5s");
        assert_eq!(format_duration(7200), "2h 0m 0s");
    }

    #[test]
    fn test_format_runtime() {
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(3665);
        let runtime = format_runtime(Some(past));
        assert!(runtime.is_some());
        if let Some(runtime_str) = runtime {
            assert!(runtime_str.contains("1h") || runtime_str.contains("1m"));
        }
    }

    #[test]
    fn test_format_runtime_none() {
        assert_eq!(format_runtime(None), None);
    }

    #[test]
    fn test_calculate_accumulated_cost() {
        let now = Utc::now();
        let one_hour_ago = now - chrono::Duration::hours(1);
        let cost = calculate_accumulated_cost(1.0, Some(one_hour_ago));
        assert!((cost - 1.0).abs() < 0.01); // Allow small time drift
    }

    #[test]
    fn test_calculate_accumulated_cost_no_launch_time() {
        assert_eq!(calculate_accumulated_cost(1.0, None), 0.0);
    }

    #[test]
    fn test_is_old_instance() {
        let now = Utc::now();
        let old_time = now - chrono::Duration::hours(25);
        assert!(is_old_instance(Some(old_time), 24));

        let recent_time = now - chrono::Duration::hours(1);
        assert!(!is_old_instance(Some(recent_time), 24));
    }

    #[test]
    fn test_is_old_instance_no_launch_time() {
        assert!(!is_old_instance(None, 24));
    }

    #[test]
    fn test_ensure_dir() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_dir = temp_dir.path().join("test_subdir");

        assert!(ensure_dir(&test_dir).is_ok());
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());
    }
}
