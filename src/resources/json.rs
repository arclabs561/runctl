//! JSON serialization functions for resource data
//!
//! Provides functions to convert resource information into JSON format
//! for API responses and data export.

use crate::config::Config;
use crate::error::Result;
use crate::resources::types::{AwsInstance, LocalProcess, ResourceSummary, RunPodPod};
use chrono::Utc;
use serde_json;

/// Get complete resource summary as JSON
pub async fn get_resource_summary_json(config: &Config) -> Result<serde_json::Value> {
    let aws_instances_json = list_aws_instances_json(config).await?;
    let runpod_pods_json = list_runpod_pods_json(config).await?;
    let local_processes_json = list_local_processes_json().await?;

    let aws_instances: Vec<AwsInstance> = aws_instances_json
        .iter()
        .filter_map(|inst| serde_json::from_value(inst.clone()).ok())
        .collect();

    let runpod_pods: Vec<RunPodPod> = runpod_pods_json
        .iter()
        .filter_map(|pod| serde_json::from_value(pod.clone()).ok())
        .collect();

    let local_processes: Vec<LocalProcess> = local_processes_json
        .iter()
        .filter_map(|proc| serde_json::from_value(proc.clone()).ok())
        .collect();

    let total_cost: f64 = aws_instances.iter().map(|i| i.cost_per_hour).sum::<f64>()
        + runpod_pods.iter().map(|p| p.cost_per_hour).sum::<f64>();

    let summary = ResourceSummary {
        aws_instances,
        runpod_pods,
        local_processes,
        total_cost_estimate: total_cost,
        timestamp: Utc::now(),
    };

    Ok(serde_json::to_value(summary)?)
}

/// List AWS instances as JSON
pub async fn list_aws_instances_json(_config: &Config) -> Result<Vec<serde_json::Value>> {
    use crate::error::TrainctlError;
    use aws_config::BehaviorVersion;
    use aws_sdk_ec2::Client as Ec2Client;

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list instances: {}", e)))?;

    let mut instances = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                let instance_type = instance
                    .instance_type()
                    .map(|t| t.as_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let state = instance
                    .state()
                    .and_then(|s| s.name())
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let launch_time = instance.launch_time().map(|lt| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(lt.secs(), 0)
                        .unwrap_or_else(chrono::Utc::now)
                });

                let tags: Vec<(String, String)> = instance
                    .tags()
                    .iter()
                    .filter_map(|tag| {
                        tag.key()
                            .zip(tag.value())
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                    })
                    .collect();

                let cost_per_hour = crate::utils::get_instance_cost(&instance_type);

                let instance_json = serde_json::json!({
                    "instance_id": instance_id,
                    "instance_type": instance_type,
                    "state": state,
                    "launch_time": launch_time.map(|dt| dt.to_rfc3339()),
                    "tags": tags,
                    "cost_per_hour": cost_per_hour,
                });

                instances.push(instance_json);
            }
        }
    }

    Ok(instances)
}

/// List RunPod pods as JSON
pub async fn list_runpod_pods_json(_config: &Config) -> Result<Vec<serde_json::Value>> {
    use crate::error::TrainctlError;
    use std::process::Command;

    let mut pods = Vec::new();

    if which::which("runpodctl").is_err() {
        return Ok(pods);
    }

    let output = Command::new("runpodctl")
        .args(["get", "pod"])
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to run runpodctl: {}",
                e
            )))
        })?;

    if !output.status.success() {
        return Ok(pods);
    }

    // Parse runpodctl output
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        // Skip empty lines, headers, and help text
        if trimmed.is_empty()
            || trimmed.starts_with("NAME")
            || trimmed.starts_with("ID")
            || trimmed.contains("Available Commands")
            || trimmed.contains("Usage:")
            || trimmed.contains("Flags:")
            || trimmed.contains("Use \"runpodctl")
        {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            pods.push(serde_json::json!({
                "id": parts[0],
                "status": parts.get(1).unwrap_or(&""),
                "name": parts.get(2).unwrap_or(&""),
            }));
        }
    }

    Ok(pods)
}

/// List local processes as JSON
pub async fn list_local_processes_json() -> Result<Vec<serde_json::Value>> {
    use sysinfo::System;

    let mut system = System::new_all();
    system.refresh_all();

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        // Filter for Python processes or processes with "train" in name
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains("python") || name.contains("train") {
            let cpu_percent = process.cpu_usage();
            let memory_mb = process.memory() as f32 / 1024.0 / 1024.0;

            let cmd_str: String = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");

            let process_json = serde_json::json!({
                "pid": pid.as_u32(),
                "command": cmd_str,
                "started": None::<String>, // sysinfo doesn't provide start time easily
                "cpu_percent": cpu_percent,
                "memory_mb": memory_mb,
            });

            processes.push(process_json);
        }
    }

    Ok(processes)
}
