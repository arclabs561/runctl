//! Process monitoring and resource usage display
//!
//! Shows running processes, resource usage (CPU, memory, disk, GPU), and system statistics
//! on EC2 instances, similar to top/htop/nvidia-smi.

use crate::aws::types::{
    DiskUsage, GpuDetailJson, GpuInfoJson, ProcessInfo, ProcessListResult, ProcessResourceUsage,
};
use crate::diagnostics::get_instance_resource_usage;
use crate::error::Result;
use aws_sdk_ssm::Client as SsmClient;
use std::io::{self, Write};

/// Show processes and resource usage on an instance
pub async fn show_processes(
    instance_id: String,
    detailed: bool,
    watch: bool,
    interval: u64,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let ssm_client = SsmClient::new(aws_config);

    let display_usage = |usage: &crate::diagnostics::ResourceUsage| -> Result<()> {
        if output_format == "json" {
            // JSON output
            let disk_usage: Vec<DiskUsage> = usage
                .disk_usage
                .iter()
                .map(|d| DiskUsage {
                    filesystem: d.filesystem.clone(),
                    size_gb: d.size_gb,
                    used_gb: d.used_gb,
                    available_gb: d.available_gb,
                    percent_used: d.percent_used,
                    mount_point: d.mount_point.clone(),
                })
                .collect();

            let gpu_info = usage.gpu_info.as_ref().map(|g| GpuInfoJson {
                gpus: g
                    .gpus
                    .iter()
                    .map(|gd| GpuDetailJson {
                        index: gd.index,
                        name: gd.name.clone(),
                        memory_used_mb: gd.memory_used_mb,
                        memory_total_mb: gd.memory_total_mb,
                        memory_percent: gd.memory_percent,
                        utilization_percent: gd.utilization_percent,
                        temperature_c: gd.temperature_c,
                        power_draw_w: gd.power_draw_w,
                    })
                    .collect(),
            });

            let processes: Vec<ProcessInfo> = usage
                .top_processes
                .iter()
                .map(|p| ProcessInfo {
                    pid: p.pid,
                    user: p.user.clone(),
                    command: p.command.clone(),
                    cpu_percent: p.cpu_percent,
                    memory_mb: p.memory_mb,
                    memory_percent: p.memory_percent,
                    runtime: p.runtime.clone(),
                })
                .collect();

            let resource_usage = ProcessResourceUsage {
                cpu_percent: usage.cpu_percent,
                memory_used_gb: usage.memory_used_gb,
                memory_total_gb: usage.memory_total_gb,
                memory_percent: usage.memory_percent,
                disk_usage,
                gpu_info,
            };

            let result = ProcessListResult {
                success: true,
                instance_id: usage.instance_id.clone(),
                timestamp: usage.timestamp.to_rfc3339(),
                resource_usage,
                processes,
            };

            if watch {
                // JSONL format for watch mode (one JSON object per line)
                println!("{}", serde_json::to_string(&result)?);
            } else {
                // Pretty JSON for single output
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
            return Ok(());
        }

        // Text output (existing code)
        // Clear screen in watch mode
        if watch {
            print!("\x1B[2J\x1B[1;1H");
            io::stdout().flush()?;
        }

        // Header - like top/htop
        println!(
            "INSTANCE: {} | UPDATED: {}",
            usage.instance_id,
            usage.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!("{}", "=".repeat(80));

        // System overview - like top
        println!("SYSTEM:");
        println!("  cpu: {:5.1}%", usage.cpu_percent);
        println!(
            "  mem: {:5.1}GB / {:5.1}GB ({:5.1}%)",
            usage.memory_used_gb, usage.memory_total_gb, usage.memory_percent
        );

        // GPU info - minimal, like nvidia-smi
        if let Some(ref gpu) = usage.gpu_info {
            println!("\nGPU:");
            for gpu_detail in &gpu.gpus {
                println!("  [{}] {}", gpu_detail.index, gpu_detail.name);
                println!(
                    "       mem: {:5.1}GB / {:5.1}GB ({:5.1}%) | util: {:5.1}%",
                    gpu_detail.memory_used_mb as f64 / 1024.0,
                    gpu_detail.memory_total_mb as f64 / 1024.0,
                    gpu_detail.memory_percent,
                    gpu_detail.utilization_percent
                );
                if let Some(temp) = gpu_detail.temperature_c {
                    print!(" | temp: {}C", temp);
                }
                if let Some(power) = gpu_detail.power_draw_w {
                    print!(" | power: {:.1}W", power);
                }
                println!();
            }
        }

        // Disk usage - like df -h
        if !usage.disk_usage.is_empty() {
            println!("\nFILESYSTEM:");
            println!(
                "{:<20} {:>8} {:>8} {:>8} {:>6} MOUNTED",
                "FILESYSTEM", "SIZE", "USED", "AVAIL", "USE%"
            );
            println!("{}", "-".repeat(80));
            for disk in &usage.disk_usage {
                let use_str = format!("{:>5.1}%", disk.percent_used);
                println!(
                    "{:<20} {:>7.1}G {:>7.1}G {:>7.1}G {:>6} {}",
                    disk.filesystem,
                    disk.size_gb,
                    disk.used_gb,
                    disk.available_gb,
                    use_str,
                    disk.mount_point
                );
            }
        }

        // Top processes - like top/ps
        if !usage.top_processes.is_empty() {
            println!("\nPROCESSES:");
            if detailed {
                println!(
                    "{:<8} {:<12} {:<40} {:>6} {:>10} {:>6} {:>10}",
                    "PID", "USER", "COMMAND", "CPU%", "MEM(MB)", "MEM%", "RUNTIME"
                );
                println!("{}", "-".repeat(100));
                for proc in &usage.top_processes {
                    let cmd_display = if proc.command.len() > 38 {
                        format!("{}...", &proc.command[..35])
                    } else {
                        format!("{:<38}", proc.command)
                    };
                    println!(
                        "{:<8} {:<12} {:<40} {:>6.1} {:>10.1} {:>6.1} {:>10}",
                        proc.pid,
                        proc.user,
                        cmd_display,
                        proc.cpu_percent,
                        proc.memory_mb,
                        proc.memory_percent,
                        proc.runtime
                    );
                }
            } else {
                println!(
                    "{:<8} {:<50} {:>6} {:>10}",
                    "PID", "COMMAND", "CPU%", "MEM(MB)"
                );
                println!("{}", "-".repeat(80));
                for proc in usage.top_processes.iter().take(10) {
                    let cmd_display = if proc.command.len() > 48 {
                        format!("{}...", &proc.command[..45])
                    } else {
                        proc.command.clone()
                    };
                    println!(
                        "{:<8} {:<50} {:>6.1} {:>10.1}",
                        proc.pid, cmd_display, proc.cpu_percent, proc.memory_mb
                    );
                }
            }
        }

        // Network stats - like ifconfig/ip
        if let Some(ref net) = usage.network_stats {
            println!("\nNETWORK:");
            println!(
                "  rx: {:>12.2} GB ({:>12} packets)",
                net.rx_bytes as f64 / 1_000_000_000.0,
                net.rx_packets
            );
            println!(
                "  tx: {:>12.2} GB ({:>12} packets)",
                net.tx_bytes as f64 / 1_000_000_000.0,
                net.tx_packets
            );
        }

        if watch {
            println!("\n{}", "-".repeat(80));
            println!("refresh: {}s | [Ctrl+C] to stop", interval);
        }

        Ok(())
    };

    if watch {
        loop {
            match get_instance_resource_usage(&ssm_client, &instance_id).await {
                Ok(usage) => {
                    display_usage(&usage)?;
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                }
                Err(e) => {
                    eprintln!("ERROR: Failed to get resource usage: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                }
            }
        }
    } else {
        let usage = get_instance_resource_usage(&ssm_client, &instance_id).await?;
        display_usage(&usage)?;
    }

    Ok(())
}
