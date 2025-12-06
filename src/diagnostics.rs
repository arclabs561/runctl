//! Diagnostic and resource monitoring utilities
//!
//! Provides visibility into process resource usage, system metrics,
//! and diagnostic insights for instances and pods.

use crate::error::Result;
use aws_sdk_ssm::Client as SsmClient;
use serde::{Deserialize, Serialize};
use crate::aws_utils::execute_ssm_command;

/// Thresholds for high resource usage warnings
const HIGH_CPU_THRESHOLD_PERCENT: f64 = 80.0;
const HIGH_MEMORY_THRESHOLD_PERCENT: f64 = 80.0;
const HIGH_GPU_UTILIZATION_THRESHOLD_PERCENT: f64 = 80.0;
const HIGH_GPU_MEMORY_THRESHOLD_PERCENT: f64 = 80.0;
const ACTIVE_PROCESS_CPU_THRESHOLD_PERCENT: f64 = 10.0;
const ACTIVE_PROCESS_MEMORY_THRESHOLD_MB: f64 = 1000.0;

/// Resource usage information for an instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub instance_id: String,
    pub cpu_percent: f64,
    pub memory_total_gb: f64,
    pub memory_used_gb: f64,
    pub memory_percent: f64,
    pub disk_usage: Vec<DiskUsage>,
    pub gpu_info: Option<GpuInfo>,
    pub top_processes: Vec<ProcessInfo>,
    pub network_stats: Option<NetworkStats>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub filesystem: String,
    pub size_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub percent_used: f64,
    pub mount_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub gpu_count: usize,
    pub gpus: Vec<GpuDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDetail {
    pub index: usize,
    pub name: String,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub memory_percent: f64,
    pub utilization_percent: f64,
    pub temperature_c: Option<u32>,
    pub power_draw_w: Option<f64>,
    pub processes: Vec<GpuProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProcess {
    pub pid: u32,
    pub name: String,
    pub memory_used_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub command: String,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub memory_percent: f64,
    pub runtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

/// Check resource usage on an instance via SSM
pub async fn get_instance_resource_usage(
    ssm_client: &SsmClient,
    instance_id: &str,
) -> Result<ResourceUsage> {
    // Collect system metrics via SSM
    let metrics_cmd = r#"
#!/bin/bash
set -e

# CPU usage (1-minute average)
CPU=$(top -bn1 | grep "Cpu(s)" | sed "s/.*, *\([0-9.]*\)%* id.*/\1/" | awk '{print 100 - $1}')

# Memory usage
MEM_INFO=$(free -g)
MEM_TOTAL=$(echo "$MEM_INFO" | grep Mem: | awk '{print $2}')
MEM_USED=$(echo "$MEM_INFO" | grep Mem: | awk '{print $3}')
MEM_AVAIL=$(echo "$MEM_INFO" | grep Mem: | awk '{print $7}')
MEM_PERCENT=$(echo "$MEM_INFO" | grep Mem: | awk '{printf "%.1f", ($3/$2)*100}')

# Disk usage
DF_OUTPUT=$(df -h | grep -E '^/dev/|^tmpfs' | awk '{print $1","$2","$3","$4","$5","$6}' | tr '\n' '|')

# Top processes by CPU
TOP_CPU=$(ps aux --sort=-%cpu | head -n 11 | tail -n 10 | awk '{printf "%s:%s:%s:%.1f:%.1f:%.1f:", $2, $1, $11, $3, $4, $10}' | tr '\n' '|')

# Top processes by memory
TOP_MEM=$(ps aux --sort=-%mem | head -n 11 | tail -n 10 | awk '{printf "%s:%s:%s:%.1f:%.1f:%.1f:", $2, $1, $11, $3, $4, $10}' | tr '\n' '|')

# GPU info (if nvidia-smi available)
GPU_INFO=""
if command -v nvidia-smi &> /dev/null; then
    GPU_COUNT=$(nvidia-smi --list-gpus | wc -l)
    GPU_INFO="${GPU_COUNT}|"
    for i in $(seq 0 $((GPU_COUNT-1))); do
        GPU_DETAIL=$(nvidia-smi --id=$i --query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu,temperature.gpu,power.draw --format=csv,noheader,nounits 2>/dev/null || echo "N/A,N/A,N/A,N/A,N/A,N/A,N/A")
        GPU_INFO="${GPU_INFO}${GPU_DETAIL}|"
    done
else
    GPU_INFO="0|"
fi

# Network stats
NET_STATS=$(cat /proc/net/dev | grep -E 'eth0|ens5' | awk '{print $2","$10","$3","$11}' || echo "0,0,0,0")

# Output JSON-like structure
echo "CPU:$CPU|MEM_TOTAL:$MEM_TOTAL|MEM_USED:$MEM_USED|MEM_AVAIL:$MEM_AVAIL|MEM_PERCENT:$MEM_PERCENT|DF:$DF_OUTPUT|TOP_CPU:$TOP_CPU|TOP_MEM:$TOP_MEM|GPU:$GPU_INFO|NET:$NET_STATS"
"#;

    let output = execute_ssm_command(ssm_client, instance_id, metrics_cmd).await?;
    
    // Parse output
    parse_resource_usage_output(instance_id, &output)
}

fn parse_resource_usage_output(instance_id: &str, output: &str) -> Result<ResourceUsage> {
    let mut cpu_percent = 0.0;
    let mut memory_total_gb = 0.0;
    let mut memory_used_gb = 0.0;
    let mut memory_percent = 0.0;
    let mut disk_usage = Vec::new();
    let mut top_processes = Vec::new();
    let mut gpu_info = None;
    let mut network_stats = None;

    // Parse output line by line
    // Helper to parse a metric value with fallback
    // Returns 0.0 on parse failure (logged separately if needed)
    let parse_metric = |prefix: &str, data: &str| -> f64 {
        data.strip_prefix(prefix)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_else(|| {
                tracing::debug!("Failed to parse metric with prefix '{}' from: {}", prefix, data);
                0.0
            })
    };
    
    for line in output.lines() {
        if line.starts_with("CPU:") {
            let parts: Vec<&str> = line.split('|').collect();
            for part in parts {
                if part.starts_with("CPU:") {
                    cpu_percent = parse_metric("CPU:", part);
                } else if part.starts_with("MEM_TOTAL:") {
                    memory_total_gb = parse_metric("MEM_TOTAL:", part);
                } else if part.starts_with("MEM_USED:") {
                    memory_used_gb = parse_metric("MEM_USED:", part);
                } else if part.starts_with("MEM_PERCENT:") {
                    memory_percent = parse_metric("MEM_PERCENT:", part);
                } else if part.starts_with("DF:") {
                    let df_data = part.strip_prefix("DF:").unwrap_or("");
                    disk_usage = parse_disk_usage(df_data);
                } else if part.starts_with("TOP_CPU:") {
                    let top_data = part.strip_prefix("TOP_CPU:").unwrap_or("");
                    top_processes = parse_top_processes(top_data);
                } else if part.starts_with("GPU:") {
                    let gpu_data = part.strip_prefix("GPU:").unwrap_or("");
                    gpu_info = parse_gpu_info(gpu_data);
                } else if part.starts_with("NET:") {
                    let net_data = part.strip_prefix("NET:").unwrap_or("");
                    network_stats = parse_network_stats(net_data);
                }
            }
        }
    }

    Ok(ResourceUsage {
        instance_id: instance_id.to_string(),
        cpu_percent,
        memory_total_gb,
        memory_used_gb,
        memory_percent,
        disk_usage,
        gpu_info,
        top_processes,
        network_stats,
        timestamp: chrono::Utc::now(),
    })
}

fn parse_disk_usage(data: &str) -> Vec<DiskUsage> {
    let mut disks = Vec::new();
    for entry in data.split('|') {
        if entry.is_empty() {
            continue;
        }
        let parts: Vec<&str> = entry.split(',').collect();
        if parts.len() >= 6 {
            if let (Ok(size), Ok(used), Ok(avail), Ok(percent)) = (
                parse_size_gb(parts[1]),
                parse_size_gb(parts[2]),
                parse_size_gb(parts[3]),
                parts[4].strip_suffix('%').unwrap_or("0").parse::<f64>(),
            ) {
                disks.push(DiskUsage {
                    filesystem: parts[0].to_string(),
                    size_gb: size,
                    used_gb: used,
                    available_gb: avail,
                    percent_used: percent,
                    mount_point: parts[5].to_string(),
                });
            }
        }
    }
    disks
}

fn parse_size_gb(size_str: &str) -> Result<f64> {
    let size_str = size_str.trim();
    if size_str.ends_with('G') {
        Ok(size_str.strip_suffix('G').unwrap_or("0").parse().unwrap_or(0.0))
    } else if size_str.ends_with('M') {
        Ok(size_str.strip_suffix('M').unwrap_or("0").parse().unwrap_or(0.0) / 1024.0)
    } else if size_str.ends_with('T') {
        Ok(size_str.strip_suffix('T').unwrap_or("0").parse().unwrap_or(0.0) * 1024.0)
    } else {
        Ok(size_str.parse().unwrap_or(0.0))
    }
}

fn parse_top_processes(data: &str) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();
    for entry in data.split('|') {
        if entry.is_empty() {
            continue;
        }
        let parts: Vec<&str> = entry.split(':').collect();
        // Explicitly check bounds before accessing
        if parts.len() >= 6 {
            // Use get() for safe access (first() is more idiomatic than get(0))
            let pid_str = parts.first().copied().unwrap_or("0");
            let user = parts.get(1).copied().unwrap_or("unknown");
            let command = parts.get(2).copied().unwrap_or("");
            let cpu_str = parts.get(3).copied().unwrap_or("0");
            let mem_str = parts.get(4).copied().unwrap_or("0");
            let runtime_str = parts.get(5).copied().unwrap_or("0");
            
            if let (Ok(pid), Ok(cpu), Ok(mem), Ok(runtime)) = (
                pid_str.parse::<u32>(),
                cpu_str.parse::<f64>(),
                mem_str.parse::<f64>(),
                runtime_str.parse::<f64>(),
            ) {
                processes.push(ProcessInfo {
                    pid,
                    user: user.to_string(),
                    command: command.to_string(),
                    cpu_percent: cpu,
                    memory_mb: mem,
                    memory_percent: mem,
                    runtime: format!("{:.1}s", runtime),
                });
            }
        }
    }
    processes
}

fn parse_gpu_info(data: &str) -> Option<GpuInfo> {
    let parts: Vec<&str> = data.split('|').collect();
    if parts.is_empty() || parts[0] == "0" {
        return None;
    }

    let gpu_count: usize = parts[0].parse().unwrap_or(0);
    if gpu_count == 0 {
        return None;
    }

    let mut gpus = Vec::new();
    for (idx, part) in parts.iter().skip(1).enumerate() {
        if part.is_empty() {
            continue;
        }
        let fields: Vec<&str> = part.split(',').collect();
        if fields.len() >= 7 {
            if let (Ok(mem_total), Ok(mem_used), Ok(util), Ok(temp), Ok(power)) = (
                fields[1].trim().parse::<u64>(),
                fields[2].trim().parse::<u64>(),
                fields[4].trim().parse::<f64>(),
                fields[5].trim().parse::<u32>(),
                fields[6].trim().parse::<f64>(),
            ) {
                let mem_percent = if mem_total > 0 {
                    (mem_used as f64 / mem_total as f64) * 100.0
                } else {
                    0.0
                };

                gpus.push(GpuDetail {
                    index: idx,
                    name: fields[0].trim().to_string(),
                    memory_total_mb: mem_total,
                    memory_used_mb: mem_used,
                    memory_percent: mem_percent,
                    utilization_percent: util,
                    temperature_c: Some(temp),
                    power_draw_w: Some(power),
                    processes: Vec::new(), // Would need separate nvidia-smi query
                });
            }
        }
    }

    if gpus.is_empty() {
        None
    } else {
        Some(GpuInfo {
            gpu_count,
            gpus,
        })
    }
}

fn parse_network_stats(data: &str) -> Option<NetworkStats> {
    let parts: Vec<&str> = data.split(',').collect();
    // Explicitly check bounds before accessing
    if parts.len() >= 4 {
        // Use get() for safe access (first() is more idiomatic than get(0))
        let rx_bytes_str = parts.first().copied().unwrap_or("0");
        let tx_bytes_str = parts.get(1).copied().unwrap_or("0");
        let rx_packets_str = parts.get(2).copied().unwrap_or("0");
        let tx_packets_str = parts.get(3).copied().unwrap_or("0");
        
        if let (Ok(rx_bytes), Ok(tx_bytes), Ok(rx_packets), Ok(tx_packets)) = (
            rx_bytes_str.parse::<u64>(),
            tx_bytes_str.parse::<u64>(),
            rx_packets_str.parse::<u64>(),
            tx_packets_str.parse::<u64>(),
        ) {
            return Some(NetworkStats {
                rx_bytes,
                tx_bytes,
                rx_packets,
                tx_packets,
            });
        }
    }
    None
}

/// Check if instance has high resource usage (warn before termination)
pub async fn check_high_resource_usage(
    ssm_client: &SsmClient,
    instance_id: &str,
) -> Result<Option<String>> {
    let usage = get_instance_resource_usage(ssm_client, instance_id).await?;
    
    let mut warnings = Vec::new();
    
    // Check CPU usage
    if usage.cpu_percent > HIGH_CPU_THRESHOLD_PERCENT {
        warnings.push(format!("High CPU usage: {:.1}%", usage.cpu_percent));
    }
    
    // Check memory usage
    if usage.memory_percent > HIGH_MEMORY_THRESHOLD_PERCENT {
        warnings.push(format!("High memory usage: {:.1}% ({:.1}GB/{:.1}GB)", 
            usage.memory_percent, usage.memory_used_gb, usage.memory_total_gb));
    }
    
    // Check GPU usage
    if let Some(ref gpu) = usage.gpu_info {
        for gpu_detail in &gpu.gpus {
            if gpu_detail.utilization_percent > HIGH_GPU_UTILIZATION_THRESHOLD_PERCENT {
                warnings.push(format!("GPU {} high utilization: {:.1}%", 
                    gpu_detail.index, gpu_detail.utilization_percent));
            }
            if gpu_detail.memory_percent > HIGH_GPU_MEMORY_THRESHOLD_PERCENT {
                warnings.push(format!("GPU {} high memory usage: {:.1}%", 
                    gpu_detail.index, gpu_detail.memory_percent));
            }
        }
    }
    
    // Check for active training processes
    let training_processes: Vec<_> = usage.top_processes
        .iter()
        .filter(|p| {
            p.command.contains("python") && (
                p.command.contains("train") ||
                p.command.contains("training") ||
                p.command.contains("main.py") ||
                p.cpu_percent > ACTIVE_PROCESS_CPU_THRESHOLD_PERCENT || 
                p.memory_mb > ACTIVE_PROCESS_MEMORY_THRESHOLD_MB
            )
        })
        .collect();
    
    if !training_processes.is_empty() {
        warnings.push(format!("Active training processes detected: {} process(es)", 
            training_processes.len()));
        for proc in training_processes.iter().take(3) {
            warnings.push(format!("  - PID {}: {} (CPU: {:.1}%, Mem: {:.1}MB)", 
                proc.pid, proc.command, proc.cpu_percent, proc.memory_mb));
        }
    }
    
    if warnings.is_empty() {
        Ok(None)
    } else {
        Ok(Some(warnings.join("\n")))
    }
}

