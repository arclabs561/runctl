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
    launch_time.and_then(|lt| {
        let now = Utc::now();
        let duration = now.signed_duration_since(lt);
        let total_secs = duration.num_seconds().max(0) as u64;
        Some(format_duration(total_secs))
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

