//! RunPod pod listing

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use std::process::Command;

/// List RunPod pods
pub async fn list_runpod_pods(detailed: bool, _config: &Config) -> Result<()> {
    println!("\nRUNPOD PODS:");
    println!("{}", "-".repeat(80));

    // Check for runpodctl
    if which::which("runpodctl").is_err() {
        println!("WARNING: runpodctl not found. Install from: https://github.com/runpod/runpodctl");
        return Ok(());
    }

    let output = Command::new("runpodctl")
        .args(["get", "pod"])
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to execute runpodctl: {}",
                e
            )))
        })?;

    if !output.status.success() {
        // Check if it's just no pods or an actual error
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // If stderr contains help text, it means the command structure might be wrong
        if stderr.contains("Available Commands") || stdout.contains("Available Commands") {
            // Try alternative: maybe it's just empty
            println!("  No pods found");
            return Ok(());
        }

        println!("WARNING: Failed to list pods: {}", stderr);
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.is_empty() || (lines.len() == 1 && lines[0].trim().is_empty()) {
        println!("  No pods found");
        return Ok(());
    }

    // Filter out header lines and help text
    let mut found_pods = false;
    for line in lines.iter() {
        let trimmed = line.trim();
        // Skip empty lines, headers, and help text
        if trimmed.is_empty()
            || trimmed.starts_with("ID") && trimmed.contains("NAME")  // Header row
            || trimmed.starts_with("NAME") && !trimmed.contains("ID")  // Alternative header
            || trimmed.contains("Available Commands")
            || trimmed.contains("Usage:")
            || trimmed.contains("Flags:")
            || trimmed.contains("Use \"runpodctl")
            || trimmed == "---"
        {
            continue;
        }

        // Check if this looks like a pod row (has ID-like pattern)
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        // Pod IDs are typically alphanumeric, 10+ chars
        let first_part = parts[0];
        if first_part.len() >= 10 && first_part.chars().all(|c| c.is_alphanumeric() || c == '-') {
            found_pods = true;
            if detailed {
                println!("  {}", line);
            } else {
                // Extract pod ID, name, and status
                // Format: ID NAME GPU IMAGE STATUS
                let pod_id = parts[0];
                let pod_name = parts.get(1).unwrap_or(&"");
                let pod_status = parts.last().unwrap_or(&"");
                println!("  {}  {}  {}", pod_id, pod_name, pod_status);
            }
        }
    }

    if !found_pods {
        println!("  No pods found");
    }

    Ok(())
}

