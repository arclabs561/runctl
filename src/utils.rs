use anyhow::{Context, Result};
use std::path::Path;
use chrono::{DateTime, Utc};

pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))?;
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
        let runtime_str = runtime.unwrap();
        assert!(runtime_str.contains("1h") || runtime_str.contains("1m"));
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
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_subdir");
        
        assert!(ensure_dir(&test_dir).is_ok());
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());
    }
}

