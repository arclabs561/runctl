use crate::checkpoint;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::utils::{calculate_accumulated_cost, format_runtime, is_old_instance};
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use chrono::{DateTime, TimeZone, Utc};
use clap::Subcommand;
use comfy_table::{Cell, Table};
use console::{style, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use sysinfo::{Pid, System};

#[derive(Subcommand, Clone)]
pub enum ResourceCommands {
    /// List all running resources (AWS, RunPod, local)
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        /// Filter by platform (aws, runpod, local, all)
        #[arg(long, default_value = "all")]
        platform: String,
        /// Filter by project name (from trainctl:project tag)
        #[arg(long)]
        project: Option<String>,
        /// Filter by user (from trainctl:user tag)
        #[arg(long)]
        user: Option<String>,
        /// Output format (table, compact, detailed)
        #[arg(long, default_value = "compact")]
        format: String,
        /// Filter by state (running, stopped, terminated, all)
        #[arg(long, default_value = "running")]
        filter: String,
        /// Sort by field (cost, age, type, state)
        #[arg(long)]
        sort: Option<String>,
        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
        /// Show terminated instances (default: hidden)
        #[arg(long)]
        show_terminated: bool,
        /// Watch mode (auto-refresh, like tail -f)
        #[arg(short, long)]
        watch: bool,
        /// Refresh interval for watch mode (seconds)
        #[arg(long, default_value = "5")]
        interval: u64,
        /// Export format (csv, html, json)
        #[arg(long)]
        export: Option<String>,
        /// Export output file
        #[arg(long)]
        export_file: Option<String>,
    },
    /// Show resource summary and costs
    Summary,
    /// Cleanup zombie/orphaned resources
    Cleanup {
        /// Dry run (don't actually delete)
        #[arg(long)]
        dry_run: bool,
        /// Force cleanup (skip confirmation)
        #[arg(short, long)]
        force: bool,
    },
    /// Stop all running instances (pause for cost savings)
    StopAll {
        /// Dry run (show what would be stopped)
        #[arg(long)]
        dry_run: bool,
        /// Force stop (skip confirmation)
        #[arg(short, long)]
        force: bool,
        /// Platform to stop (aws, runpod, all)
        #[arg(long, default_value = "all")]
        platform: String,
    },
    /// Show resource insights and recommendations
    Insights,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub aws_instances: Vec<AwsInstance>,
    pub runpod_pods: Vec<RunPodPod>,
    pub local_processes: Vec<LocalProcess>,
    pub total_cost_estimate: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AwsInstance {
    pub instance_id: String,
    pub instance_type: String,
    pub state: String,
    pub launch_time: Option<DateTime<Utc>>,
    pub tags: Vec<(String, String)>,
    pub cost_per_hour: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunPodPod {
    pub pod_id: String,
    pub name: String,
    pub status: String,
    pub gpu_type: String,
    pub created_at: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalProcess {
    pub pid: u32,
    pub command: String,
    pub started: Option<DateTime<Utc>>,
    pub cpu_percent: f32,
    pub memory_mb: f32,
}

pub async fn handle_command(
    cmd: ResourceCommands,
    config: &Config,
    output_format: &str,
) -> Result<()> {
    match cmd {
        ResourceCommands::List {
            detailed,
            platform,
            format,
            filter,
            sort,
            limit,
            show_terminated,
            watch,
            interval,
            export,
            export_file,
            project,
            user,
        } => {
            if watch {
                list_resources_watch(
                    config,
                    &platform,
                    &filter,
                    sort.as_deref(),
                    interval,
                    project.as_deref(),
                    user.as_deref(),
                )
                .await
            } else {
                let list_options = ListResourcesOptions {
                    detailed,
                    platform: platform.clone(),
                    output_format: output_format.to_string(),
                    format: format.clone(),
                    filter: filter.clone(),
                    sort: sort.clone(),
                    limit,
                    show_terminated,
                    export: export.clone(),
                    export_file: export_file.clone(),
                    project_filter: project.clone(),
                    user_filter: user.clone(),
                };
                list_resources(list_options, config).await
            }
        }
        ResourceCommands::Summary => show_summary(config, output_format).await,
        ResourceCommands::Cleanup { dry_run, force } => {
            cleanup_zombies(dry_run, force, config).await
        }
        ResourceCommands::StopAll {
            dry_run,
            force,
            platform,
        } => stop_all_instances(dry_run, force, platform, config).await,
        ResourceCommands::Insights => show_insights(config, output_format).await,
    }
}

pub async fn show_quick_status(detailed: bool, config: &Config, output_format: &str) -> Result<()> {
    if output_format == "json" {
        let summary = get_resource_summary_json(config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    if !detailed {
        // Quick 1-2 line summary
        let aws_instances_json = list_aws_instances_json(config).await?;
        let running: Vec<_> = aws_instances_json
            .iter()
            .filter(|inst| {
                inst.get("state")
                    .and_then(|s| s.as_str())
                    .map(|s| s == "running")
                    .unwrap_or(false)
            })
            .collect();

        let running_count = running.len();
        let hourly_cost: f64 = running
            .iter()
            .filter_map(|inst| {
                inst.get("instance_type")
                    .and_then(|t| t.as_str())
                    .map(|t| crate::utils::get_instance_cost(t))
            })
            .sum();

        let total_cost: f64 = running
            .iter()
            .filter_map(|inst| {
                let hourly = inst
                    .get("instance_type")
                    .and_then(|t| t.as_str())
                    .map(|t| crate::utils::get_instance_cost(t))?;
                let hours = inst
                    .get("launch_time")
                    .and_then(|lt| lt.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| {
                        let runtime = Utc::now().signed_duration_since(dt.with_timezone(&Utc));
                        runtime.num_hours().max(0) as f64
                    })
                    .unwrap_or(0.0);
                Some(hourly * hours)
            })
            .sum();

        println!(
            "{} instances running, ${:.2}/hr, ${:.2} total",
            running_count, hourly_cost, total_cost
        );

        if running_count > 0 {
            println!("{} training jobs active", running_count);
        }

        return Ok(());
    }

    // Detailed output with better formatting
    let header_style = Style::new().bold().cyan();
    println!("{}", header_style.apply_to("=".repeat(80)));
    println!("{}", header_style.apply_to("trainctl Status"));
    println!("{}", header_style.apply_to("=".repeat(80)));

    // Quick resource summary
    println!("\nRESOURCES:");
    show_summary(config, "text").await?;

    // Recent checkpoints
    println!("\nRECENT CHECKPOINTS:");
    if let Some(checkpoint_dir) = config.local.as_ref().map(|c| c.checkpoint_dir.as_path()) {
        if checkpoint_dir.exists() {
            let checkpoints = checkpoint::get_checkpoint_paths(checkpoint_dir).await?;
            let recent: Vec<_> = checkpoints.into_iter().take(5).collect();
            if recent.is_empty() {
                println!("  No checkpoints found");
            } else {
                for cp in recent {
                    println!("  {}", cp.display());
                }
            }
        } else {
            println!(
                "  Checkpoint directory not found: {}",
                checkpoint_dir.display()
            );
        }
    } else {
        println!("  No checkpoint directory configured");
    }

    Ok(())
}

async fn get_resource_summary_json(config: &Config) -> Result<serde_json::Value> {
    let aws_instances = list_aws_instances_json(config).await?;
    let runpod_pods = list_runpod_pods_json(config).await?;
    let local_processes = list_local_processes_json().await?;

    Ok(serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "aws": {
            "instances": aws_instances,
        },
        "runpod": {
            "pods": runpod_pods,
        },
        "local": {
            "processes": local_processes,
        },
    }))
}

async fn list_aws_instances_json(_config: &Config) -> Result<Vec<serde_json::Value>> {
    let mut instances = Vec::new();
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list EC2 instances: {}", e)))?;

    let reservations = response.reservations();
    for reservation in reservations {
        for instance in reservation.instances() {
            let state_str = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "unknown".to_string());
            let instance_type_str = instance
                .instance_type()
                .map(|t| format!("{}", t))
                .unwrap_or_else(|| "unknown".to_string());
            let launch_time = instance
                .launch_time()
                .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single())
                .map(|dt| dt.to_rfc3339());

            let cost_per_hour = estimate_instance_cost(&instance_type_str);

            let mut tags_json = serde_json::Map::new();
            let tags = instance.tags();
            for tag in tags {
                if let (Some(key), Some(value)) = (tag.key(), tag.value()) {
                    tags_json.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
            }

            let is_spot = instance.spot_instance_request_id().is_some();
            let spot_request_id = instance.spot_instance_request_id().map(|s| s.to_string());
            let public_ip = instance.public_ip_address().map(|s| s.to_string());
            let private_ip = instance.private_ip_address().map(|s| s.to_string());
            let launch_time_dt = instance
                .launch_time()
                .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single());
            let accumulated_cost = if state_str == "running" {
                calculate_accumulated_cost(cost_per_hour, launch_time_dt)
            } else {
                0.0
            };
            let runtime = format_runtime(launch_time_dt);
            let is_old = is_old_instance(launch_time_dt, 24);

            instances.push(serde_json::json!({
                "instance_id": instance.instance_id().unwrap_or("unknown"),
                "instance_type": instance_type_str,
                "state": state_str,
                "launch_time": launch_time,
                "runtime": runtime,
                "cost_per_hour": cost_per_hour,
                "accumulated_cost": accumulated_cost,
                "is_spot": is_spot,
                "spot_request_id": spot_request_id,
                "public_ip": public_ip,
                "private_ip": private_ip,
                "is_old": is_old && state_str == "running",
                "tags": tags_json,
            }));
        }
    }

    Ok(instances)
}

async fn list_runpod_pods_json(_config: &Config) -> Result<Vec<serde_json::Value>> {
    let mut pods = Vec::new();

    if which::which("runpodctl").is_err() {
        return Ok(pods);
    }

    let output = Command::new("runpodctl")
        .args(["get", "pod"])
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to run runpodctl: {}", e),
            ))
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

async fn list_local_processes_json() -> Result<Vec<serde_json::Value>> {
    let mut processes = Vec::new();

    // Use sysinfo for native Rust process listing
    let mut system = System::new_all();
    system.refresh_all();

    let current_pid = Pid::from_u32(std::process::id());

    for (pid, process) in system.processes() {
        // Skip current process
        if *pid == current_pid {
            continue;
        }

        let cmd: Vec<String> = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let cmd_str = cmd.join(" ");

        // Filter out system processes and trainctl itself
        if cmd_str.contains("trainctl")
            || cmd_str.contains("ps aux")
            || cmd_str.contains("runpodctl")
            || cmd_str.contains("ripgrep")
            || cmd_str.contains("mypy")
            || cmd_str.contains("lsp_server")
            || cmd_str.contains("Cursor.app")
        {
            continue;
        }

        // Look for actual training scripts
        let is_training = (cmd_str.contains(".py")
            && (cmd_str.contains("train")
                || cmd_str.contains("epoch")
                || cmd_str.contains("training")))
            || (cmd_str.contains("python")
                && cmd_str.contains(".py")
                && (cmd_str.contains("train") || cmd_str.contains("epoch")));

        if is_training {
            let cpu_usage = process.cpu_usage();
            let memory_mb = process.memory() as f64 / 1024.0 / 1024.0;

            processes.push(serde_json::json!({
                "pid": pid.as_u32(),
                "cpu_percent": cpu_usage,
                "memory_mb": memory_mb,
                "command": cmd_str,
            }));
        }
    }

    Ok(processes)
}

/// Options for listing resources
#[derive(Debug, Clone)]
struct ListResourcesOptions {
    detailed: bool,
    platform: String,
    output_format: String,
    format: String,
    filter: String,
    sort: Option<String>,
    limit: Option<usize>,
    show_terminated: bool,
    export: Option<String>,
    export_file: Option<String>,
    project_filter: Option<String>,
    user_filter: Option<String>,
}

async fn list_resources(options: ListResourcesOptions, config: &Config) -> Result<()> {
    if options.output_format == "json" {
        let summary = get_resource_summary_json(config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    // Handle exports
    if let Some(export_format) = &options.export {
        return export_resources(
            config,
            &options.platform,
            export_format,
            options.export_file.as_deref(),
        )
        .await;
    }

    println!("{}", "=".repeat(80));
    println!("RESOURCE OVERVIEW");
    println!("{}", "=".repeat(80));

    if options.platform == "all" || options.platform == "aws" {
        let aws_options = ListAwsInstancesOptions {
            detailed: options.detailed,
            format: options.format.clone(),
            filter: options.filter.clone(),
            sort: options.sort.clone(),
            limit: options.limit,
            show_terminated: options.show_terminated,
            project_filter: options.project_filter.clone(),
            user_filter: options.user_filter.clone(),
        };
        list_aws_instances(aws_options, config).await?;
    }

    if options.platform == "all" || options.platform == "runpod" {
        list_runpod_pods(options.detailed, config).await?;
    }

    if options.platform == "all" || options.platform == "local" {
        list_local_processes(options.detailed).await?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct InstanceInfo {
    id: String,
    instance_type: String,
    state: String,
    launch_time: Option<DateTime<Utc>>,
    cost_per_hour: f64,
    accumulated_cost: f64,
    runtime: Option<String>,
    is_spot: bool,
    _spot_request_id: Option<String>,
    public_ip: Option<String>,
    private_ip: Option<String>,
    tags: Vec<(String, String)>,
    is_old: bool,
}

/// Options for listing AWS instances
#[derive(Debug, Clone)]
struct ListAwsInstancesOptions {
    detailed: bool,
    format: String,
    filter: String,
    sort: Option<String>,
    limit: Option<usize>,
    show_terminated: bool,
    project_filter: Option<String>,
    user_filter: Option<String>,
}

async fn list_aws_instances(options: ListAwsInstancesOptions, _config: &Config) -> Result<()> {
    let _section_style = Style::new().bold().yellow();
    println!("\nAWS EC2 INSTANCES:");
    println!("{}", "-".repeat(80));

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list EC2 instances: {}", e)))?;

    // Collect all instance info
    let mut instances: Vec<InstanceInfo> = Vec::new();
    let mut total_instances = 0;
    let mut running_instances = 0;
    let mut total_hourly_cost = 0.0;
    let mut total_accumulated_cost = 0.0;
    let mut old_instances = 0;

    let reservations = response.reservations();
    for reservation in reservations {
        for instance in reservation.instances() {
            total_instances += 1;
            let state_str = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "unknown".to_string());

            if state_str == "running" {
                running_instances += 1;
            }

            let instance_id = instance.instance_id().unwrap_or("unknown").to_string();
            let instance_type_str = instance
                .instance_type()
                .map(|t| format!("{}", t))
                .unwrap_or_else(|| "unknown".to_string());
            let launch_time = instance
                .launch_time()
                .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single());

            let cost_per_hour = estimate_instance_cost(&instance_type_str);
            let accumulated_cost = if state_str == "running" {
                calculate_accumulated_cost(cost_per_hour, launch_time)
            } else {
                0.0
            };

            if state_str == "running" {
                total_hourly_cost += cost_per_hour;
                total_accumulated_cost += accumulated_cost;
            }

            // Check if spot instance
            let is_spot = instance.spot_instance_request_id().is_some();
            let spot_request_id = instance.spot_instance_request_id().map(|s| s.to_string());

            // Get IP addresses
            let public_ip = instance.public_ip_address().map(|s| s.to_string());
            let private_ip = instance.private_ip_address().map(|s| s.to_string());

            // Get tags
            let mut tags = Vec::new();
            for tag in instance.tags() {
                if let (Some(key), Some(value)) = (tag.key(), tag.value()) {
                    tags.push((key.to_string(), value.to_string()));
                }
            }

            // Check if old instance
            let is_old = is_old_instance(launch_time, 24);
            if is_old && state_str == "running" {
                old_instances += 1;
            }

            instances.push(InstanceInfo {
                id: instance_id,
                instance_type: instance_type_str,
                state: state_str,
                launch_time,
                cost_per_hour,
                accumulated_cost,
                runtime: format_runtime(launch_time),
                is_spot,
                _spot_request_id: spot_request_id,
                public_ip,
                private_ip,
                tags,
                is_old,
            });
        }
    }

    // Apply filtering
    let mut filtered_instances: Vec<&InstanceInfo> = instances.iter().collect();

    // Filter by project
    if let Some(project) = &options.project_filter {
        filtered_instances.retain(|inst| {
            inst.tags
                .iter()
                .any(|(k, v)| k == "trainctl:project" && v == project)
        });
    }

    // Filter by user
    if let Some(user) = &options.user_filter {
        filtered_instances.retain(|inst| {
            inst.tags
                .iter()
                .any(|(k, v)| k == "trainctl:user" && v == user)
        });
    }

    // Filter by state
    if options.filter != "all" {
        filtered_instances.retain(|inst| {
            if options.filter == "running" {
                inst.state == "running"
            } else if options.filter == "stopped" {
                inst.state == "stopped"
            } else if options.filter == "terminated" {
                inst.state == "terminated"
            } else {
                true
            }
        });
    }

    // Hide terminated by default unless explicitly requested
    if !options.show_terminated && options.filter == "running" {
        filtered_instances.retain(|inst| inst.state != "terminated");
    }

    // Apply sorting
    if let Some(sort_field) = &options.sort {
        match sort_field.as_str() {
            "cost" | "cost_per_hour" => {
                filtered_instances.sort_by(|a, b| {
                    b.cost_per_hour
                        .partial_cmp(&a.cost_per_hour)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            "age" | "runtime" => {
                filtered_instances.sort_by(|a, b| {
                    let a_runtime = a.launch_time.map(|t| t.timestamp()).unwrap_or(0);
                    let b_runtime = b.launch_time.map(|t| t.timestamp()).unwrap_or(0);
                    a_runtime.cmp(&b_runtime) // Oldest first
                });
            }
            "type" | "instance_type" => {
                filtered_instances.sort_by(|a, b| a.instance_type.cmp(&b.instance_type));
            }
            "state" => {
                filtered_instances.sort_by(|a, b| a.state.cmp(&b.state));
            }
            "accumulated" | "total" => {
                filtered_instances.sort_by(|a, b| {
                    b.accumulated_cost
                        .partial_cmp(&a.accumulated_cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            _ => {}
        }
    }

    // Apply limit
    if let Some(limit_val) = options.limit {
        filtered_instances.truncate(limit_val);
    }

    // Table format
    if options.format == "table" {
        return display_table_format(&filtered_instances, options.detailed).await;
    }

    // Group by instance type for better display (compact format)
    let mut grouped: HashMap<String, Vec<&InstanceInfo>> = HashMap::new();
    for inst in &filtered_instances {
        grouped
            .entry(inst.instance_type.clone())
            .or_default()
            .push(inst);
    }

    // Sort instance types by cost
    let mut type_keys: Vec<_> = grouped.keys().collect();
    type_keys.sort();

    // Display grouped by type
    for instance_type in type_keys {
        let type_instances = &grouped[instance_type];
        let running_count = type_instances
            .iter()
            .filter(|i| i.state == "running")
            .count();
        let type_hourly_cost: f64 = type_instances
            .iter()
            .filter(|i| i.state == "running")
            .map(|i| i.cost_per_hour)
            .sum();
        let type_accumulated: f64 = type_instances
            .iter()
            .filter(|i| i.state == "running")
            .map(|i| i.accumulated_cost)
            .sum();

        if options.detailed {
            println!(
                "\n  {} ({} running, ${:.4}/hr, ${:.2} total)",
                style(instance_type).bold().cyan(),
                running_count,
                type_hourly_cost,
                type_accumulated
            );
        } else {
            println!(
                "\n  {} ({} running, ${:.4}/hr)",
                style(instance_type).bold().cyan(),
                running_count,
                type_hourly_cost
            );
        }

        for inst in type_instances {
            let state_style = match inst.state.as_str() {
                "running" => Style::new().green(),
                "stopped" => Style::new().yellow(),
                "terminated" => Style::new().red(),
                _ => Style::new(),
            };

            let spot_indicator = if inst.is_spot {
                style(" [SPOT]").yellow()
            } else {
                style(" [ON-DEMAND]").dim()
            };

            let old_warning_str = if inst.is_old && inst.state == "running" {
                " >24h"
            } else {
                ""
            };

            if options.detailed {
                let old_warning_display = if !old_warning_str.is_empty() {
                    style(old_warning_str).red().bold()
                } else {
                    style("")
                };
                println!(
                    "    {}  {}  {}  {}  {}",
                    inst.id,
                    state_style.apply_to(&inst.state),
                    spot_indicator,
                    inst.runtime
                        .as_ref()
                        .map(|r| format!("runtime: {}", r))
                        .unwrap_or_else(|| "N/A".to_string()),
                    old_warning_display
                );

                if let Some(public_ip) = &inst.public_ip {
                    println!("      {} {}", style("Public IP:").dim(), public_ip);
                }
                if let Some(private_ip) = &inst.private_ip {
                    println!("      {} {}", style("Private IP:").dim(), private_ip);
                }

                let cost_style = if inst.cost_per_hour > 5.0 {
                    Style::new().red()
                } else {
                    Style::new()
                };
                println!(
                    "      {} {}  {} {}",
                    style("Cost/hour:").dim(),
                    cost_style.apply_to(format!("${:.4}", inst.cost_per_hour)),
                    style("Total:").dim(),
                    style(format!("${:.2}", inst.accumulated_cost)).yellow()
                );

                if !inst.tags.is_empty() {
                    let tag_str: String = inst
                        .tags
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("      {} {}", style("Tags:").dim(), style(tag_str).cyan());
                }
            } else {
                let runtime_str = if inst.state == "running" {
                    inst.runtime
                        .as_deref()
                        .map(|r| format!("({})", r))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let cost_str = if inst.state == "running" {
                    format!(
                        "${:.4}/hr (${:.2} total)",
                        inst.cost_per_hour, inst.accumulated_cost
                    )
                } else {
                    format!("${:.4}/hr", inst.cost_per_hour)
                };

                // Build IP display for running instances
                let ip_display = if inst.state == "running" {
                    let mut parts = Vec::new();
                    if let Some(public) = &inst.public_ip {
                        parts.push(format!("pub:{}", public));
                    }
                    if let Some(private) = &inst.private_ip {
                        parts.push(format!("priv:{}", private));
                    }
                    if !parts.is_empty() {
                        format!(" [{}]", parts.join(", "))
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let old_warning_display = if !old_warning_str.is_empty() {
                    style(old_warning_str).red().bold()
                } else {
                    style("")
                };
                println!(
                    "    {}  {}  {}  {}  {}{}  {}",
                    inst.id,
                    state_style.apply_to(&inst.state),
                    spot_indicator,
                    runtime_str,
                    cost_str,
                    ip_display,
                    old_warning_display
                );

                // Show key tags in summary (Name, project, trainctl tags)
                if !inst.tags.is_empty() {
                    let key_tags: Vec<String> = inst
                        .tags
                        .iter()
                        .filter(|(k, _)| {
                            k == "Name"
                                || k == "trainctl:project"
                                || k == "trainctl:created"
                                || k == "CreatedBy"
                        })
                        .take(3)
                        .map(|(k, v)| {
                            // Clean up tag keys for display
                            let display_key = if k == "trainctl:project" {
                                "project"
                            } else if k == "trainctl:created" {
                                "created"
                            } else {
                                k
                            };
                            format!("{}={}", display_key, v)
                        })
                        .collect();
                    if !key_tags.is_empty() {
                        println!("      {}", style(key_tags.join(", ")).cyan());
                    }
                }
            }
        }
    }

    // Summary
    println!("\n{}", "─".repeat(80));
    let total_style = Style::new().bold();
    let running_style = if running_instances > 0 {
        Style::new().green()
    } else {
        Style::new()
    };
    println!(
        "  {} {} instances ({} running)",
        total_style.apply_to("Total:"),
        total_instances,
        running_style.apply_to(running_instances)
    );

    if running_instances > 0 {
        let cost_style = if total_hourly_cost > 10.0 {
            Style::new().red().bold()
        } else {
            Style::new().yellow()
        };
        println!(
            "  {} {}  {} {}",
            style("Hourly cost:").dim(),
            cost_style.apply_to(format!("${:.2}/hour", total_hourly_cost)),
            style("Accumulated:").dim(),
            style(format!("${:.2}", total_accumulated_cost)).yellow()
        );

        // Project daily/weekly costs
        let daily_cost = total_hourly_cost * 24.0;
        let weekly_cost = daily_cost * 7.0;
        println!(
            "  {} {}  {} {}",
            style("Daily projection:").dim(),
            style(format!("${:.2}", daily_cost)).yellow(),
            style("Weekly:").dim(),
            style(format!("${:.2}", weekly_cost)).yellow()
        );
    }

    if old_instances > 0 {
        println!(
            "  {} {} instance(s) running >24h - consider terminating",
            style("!").red().bold(),
            old_instances
        );
    }

    Ok(())
}

async fn list_runpod_pods(detailed: bool, _config: &Config) -> Result<()> {
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
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to execute runpodctl: {}", e),
            ))
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

async fn list_local_processes(detailed: bool) -> Result<()> {
    println!("\n💻 Local Training Processes:");
    println!("{}", "-".repeat(80));

    // Use sysinfo for native Rust process listing
    let mut system = System::new_all();
    system.refresh_all();

    let current_pid = Pid::from_u32(std::process::id());
    let mut found = false;

    for (pid, process) in system.processes() {
        // Skip current process
        if *pid == current_pid {
            continue;
        }

        let cmd: Vec<String> = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let cmd_str = cmd.join(" ");

        // Filter out system processes
        if cmd_str.contains("trainctl")
            || cmd_str.contains("ps aux")
            || cmd_str.contains("runpodctl")
            || cmd_str.contains("ripgrep")
            || cmd_str.contains("mypy")
            || cmd_str.contains("lsp_server")
            || cmd_str.contains("Cursor.app")
        {
            continue;
        }

        // Look for actual training scripts: Python scripts with train/epoch keywords, or .py files
        let is_training = (cmd_str.contains(".py")
            && (cmd_str.contains("train")
                || cmd_str.contains("epoch")
                || cmd_str.contains("training")))
            || (cmd_str.contains("python")
                && cmd_str.contains(".py")
                && (cmd_str.contains("train") || cmd_str.contains("epoch")));

        if is_training {
            found = true;
            let cpu_usage = process.cpu_usage();
            let memory_mb = process.memory() as f64 / 1024.0 / 1024.0;
            if detailed {
                println!(
                    "  PID: {}  CPU: {:.1}%  MEM: {:.1}MB  CMD: {}",
                    pid.as_u32(),
                    cpu_usage,
                    memory_mb,
                    cmd_str
                );
            } else {
                println!("  PID: {}  CMD: {}", pid.as_u32(), cmd_str);
            }
        }
    }

    if !found {
        println!("  No training processes found");
    }

    Ok(())
}

async fn show_summary(_config: &Config, output_format: &str) -> Result<()> {
    if output_format == "json" {
        let summary = get_resource_summary_json(_config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    let mut summary = ResourceSummary {
        aws_instances: Vec::new(),
        runpod_pods: Vec::new(),
        local_processes: Vec::new(),
        total_cost_estimate: 0.0,
        timestamp: Utc::now(),
    };

    // Collect AWS instances
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    if let Ok(response) = client.describe_instances().send().await {
        let reservations = response.reservations();
        for reservation in reservations {
            let instances = reservation.instances();
            for instance in instances {
                let state = instance
                    .state()
                    .and_then(|s| s.name())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| String::from("unknown"));

                if state == "running" {
                    let instance_id = instance.instance_id().unwrap_or("unknown").to_string();
                    let instance_type = instance
                        .instance_type()
                        .map(|t| format!("{}", t))
                        .unwrap_or_else(|| "unknown".to_string());
                    let cost = estimate_instance_cost(&instance_type);
                    summary.total_cost_estimate += cost;

                    summary.aws_instances.push(AwsInstance {
                        instance_id,
                        instance_type,
                        state,
                        launch_time: instance
                            .launch_time()
                            .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single()),
                        tags: Vec::new(),
                        cost_per_hour: cost,
                    });
                }
            }
        }
    }

    println!("{}", "=".repeat(80));
    println!("Resource Summary");
    println!("{}", "=".repeat(80));
    println!(
        "Timestamp: {}",
        summary.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!();
    println!("AWS Instances: {} running", summary.aws_instances.len());
    println!("RunPod Pods: {}", summary.runpod_pods.len());
    println!("Local Processes: {}", summary.local_processes.len());
    println!();

    // Calculate accumulated costs and breakdowns
    let mut total_accumulated = 0.0;
    let mut type_breakdown: HashMap<String, (usize, f64, f64)> = HashMap::new();

    for inst in &summary.aws_instances {
        let accumulated = if let Some(lt) = inst.launch_time {
            calculate_accumulated_cost(inst.cost_per_hour, Some(lt))
        } else {
            0.0
        };
        total_accumulated += accumulated;

        let entry = type_breakdown
            .entry(inst.instance_type.clone())
            .or_insert((0, 0.0, 0.0));
        entry.0 += 1;
        entry.1 += inst.cost_per_hour;
        entry.2 += accumulated;
    }

    // Cost threshold warnings
    let hourly_threshold = 50.0; // Warn if > $50/hour
    let daily_threshold = 100.0; // Warn if > $100/day
    let accumulated_threshold = 500.0; // Warn if > $500 accumulated

    let _cost_style = if summary.total_cost_estimate > hourly_threshold {
        Style::new().red().bold()
    } else if summary.total_cost_estimate > hourly_threshold / 2.0 {
        Style::new().yellow().bold()
    } else {
        Style::new().yellow()
    };

    println!("COST:");
    println!("  hourly:     ${:.2}/hour", summary.total_cost_estimate);
    println!("  accumulated: ${:.2}", total_accumulated);

    let daily_cost = summary.total_cost_estimate * 24.0;
    let weekly_cost = daily_cost * 7.0;
    println!("  daily:      ${:.2}", daily_cost);
    println!("  weekly:     ${:.2}", weekly_cost);

    // Cost warnings
    if summary.total_cost_estimate > hourly_threshold {
        println!();
        println!(
            "{} {}",
            style("WARNING:").red().bold(),
            style(format!(
                "Hourly cost (${:.2}/hr) exceeds threshold (${}/hr)",
                summary.total_cost_estimate, hourly_threshold
            ))
            .red()
            .bold()
        );
        println!("   Consider terminating unused instances or using spot instances.");
    } else if summary.total_cost_estimate > hourly_threshold / 2.0 {
        println!();
        println!(
            "{} {}",
            style("NOTE:").yellow(),
            style(format!(
                "Hourly cost (${:.2}/hr) is approaching threshold (${}/hr)",
                summary.total_cost_estimate, hourly_threshold
            ))
            .yellow()
        );
    }

    if daily_cost > daily_threshold {
        println!();
        println!(
            "{} {}",
            style("WARNING:").red().bold(),
            style(format!(
                "Daily projection (${:.2}/day) exceeds threshold (${}/day)",
                daily_cost, daily_threshold
            ))
            .red()
            .bold()
        );
    }

    if total_accumulated > accumulated_threshold {
        println!();
        println!(
            "{} {}",
            style("WARNING:").red().bold(),
            style(format!(
                "Accumulated cost (${:.2}) exceeds threshold (${})",
                total_accumulated, accumulated_threshold
            ))
            .red()
            .bold()
        );
        println!("   Run 'trainctl resources cleanup --dry-run' to identify cleanup candidates.");
    }

    println!();

    if !type_breakdown.is_empty() {
        println!("Cost Breakdown by Instance Type:");
        let mut type_keys: Vec<_> = type_breakdown.keys().collect();
        type_keys.sort();
        for instance_type in type_keys {
            let (count, hourly, accumulated) = &type_breakdown[instance_type];
            println!(
                "  {}: {} instance(s), ${:.4}/hr, ${:.2} total",
                style(instance_type).cyan(),
                count,
                hourly,
                accumulated
            );
        }
        println!();
    }

    if !summary.aws_instances.is_empty() {
        println!("Running AWS Instances:");
        for inst in &summary.aws_instances {
            let accumulated = if let Some(lt) = inst.launch_time {
                calculate_accumulated_cost(inst.cost_per_hour, Some(lt))
            } else {
                0.0
            };
            println!(
                "  {} ({}) - ${:.4}/hr (${:.2} total)",
                inst.instance_id, inst.instance_type, inst.cost_per_hour, accumulated
            );
        }
    }

    Ok(())
}

async fn cleanup_zombies(dry_run: bool, force: bool, _config: &Config) -> Result<()> {
    println!("{}", "=".repeat(80));
    println!("Zombie Resource Cleanup");
    println!("{}", "=".repeat(80));

    if dry_run {
        println!("[DRY RUN MODE - No resources will be deleted]");
    }

    // Find orphaned AWS instances (running > 24 hours without tags)
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list instances: {}", e)))?;

    let mut zombies = Vec::new();
    let mut protected_instances = Vec::new();
    let cutoff = Utc::now() - chrono::Duration::hours(24);

    let reservations = response.reservations();
    for reservation in reservations {
        let instances = reservation.instances();
        for instance in instances {
            let state_str = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "unknown".to_string());
            if state_str != "running" {
                continue;
            }

            let instance_id = instance.instance_id().unwrap_or("unknown").to_string();

            // Check if protected
            let is_protected = instance.tags().iter().any(|t| {
                t.key()
                    .map(|k| {
                        k == "trainctl:protected"
                            || k == "trainctl:important"
                            || k == "trainctl:persistent"
                    })
                    .unwrap_or(false)
                    && t.value().map(|v| v == "true").unwrap_or(false)
            });

            if is_protected {
                protected_instances.push(instance_id.clone());
                continue;
            }

            let launch_time = instance
                .launch_time()
                .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single());

            if let Some(lt) = launch_time {
                if lt < cutoff {
                    // Check if it has trainctl tags
                    let has_trainctl_tag = instance
                        .tags()
                        .iter()
                        .any(|t| t.key().map(|k| k.contains("trainctl")).unwrap_or(false));

                    if !has_trainctl_tag {
                        zombies.push(instance_id);
                    }
                }
            }
        }
    }

    // Also check for orphaned volumes (available, no trainctl tags, not persistent)
    let volumes_response = client
        .describe_volumes()
        .filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("status")
                .values("available")
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list volumes: {}", e)))?;

    let mut orphaned_volumes = Vec::new();
    for volume in volumes_response.volumes() {
        let volume_id = volume.volume_id().unwrap_or("unknown").to_string();

        // Skip persistent volumes
        let is_persistent = volume.tags().iter().any(|t| {
            t.key().map(|k| k == "trainctl:persistent").unwrap_or(false)
                && t.value().map(|v| v == "true").unwrap_or(false)
        });

        if is_persistent {
            continue;
        }

        // Check if has trainctl tags (if not, might be orphaned)
        let has_trainctl_tag = volume
            .tags()
            .iter()
            .any(|t| t.key().map(|k| k.contains("trainctl")).unwrap_or(false));

        if !has_trainctl_tag {
            orphaned_volumes.push(volume_id);
        }
    }

    if zombies.is_empty() && orphaned_volumes.is_empty() {
        println!("No zombie resources found");
        if !protected_instances.is_empty() {
            println!(
                "   ({} protected instance(s) skipped)",
                protected_instances.len()
            );
        }
        return Ok(());
    }

    if !zombies.is_empty() {
        println!("\nFound {} potential zombie instance(s):", zombies.len());
        for id in &zombies {
            println!("  - {}", id);
        }
    }

    if !orphaned_volumes.is_empty() {
        println!(
            "\nFound {} orphaned volume(s) (available, not persistent):",
            orphaned_volumes.len()
        );
        for id in &orphaned_volumes {
            println!("  - {}", id);
        }
    }

    if !protected_instances.is_empty() {
        println!(
            "\nSkipped {} protected instance(s):",
            protected_instances.len()
        );
        for id in &protected_instances {
            println!("  - {} (protected)", id);
        }
    }

    if dry_run {
        println!(
            "\n[DRY RUN] Would terminate {} instance(s) and delete {} volume(s)",
            zombies.len(),
            orphaned_volumes.len()
        );
        return Ok(());
    }

    if !force {
        println!("\nWARNING: This will terminate {} instance(s) and delete {} volume(s). Continue? (y/N): ", 
            zombies.len(), orphaned_volumes.len());
        // In real implementation, would read from stdin
        println!("  (Use --force to skip confirmation)");
        return Ok(());
    }

    // Terminate zombie instances
    for id in &zombies {
        client
            .terminate_instances()
            .instance_ids(id.as_str())
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to terminate {}: {}", id, e)))?;
        println!("  Terminated {}", id);
    }

    // Delete orphaned volumes
    for id in &orphaned_volumes {
        match client.delete_volume().volume_id(id.as_str()).send().await {
            Ok(_) => println!("  Deleted volume {}", id),
            Err(e) => println!("  Failed to delete volume {}: {}", id, e),
        }
    }

    println!("\nCleanup complete");
    Ok(())
}

async fn show_insights(_config: &Config, output_format: &str) -> Result<()> {
    if output_format == "json" {
        // For JSON, return structured insights
        let summary = get_resource_summary_json(_config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    println!("{}", "=".repeat(80));
    println!("Resource Insights & Recommendations");
    println!("{}", "=".repeat(80));

    // Analyze resources and provide recommendations
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    let response = client
        .describe_instances()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to list instances: {}", e)))?;

    let mut running = 0;
    let mut stopped = 0;
    let mut total_cost = 0.0;
    let mut old_instances = 0;

    let reservations = response.reservations();
    for reservation in reservations {
        let instances = reservation.instances();
        for instance in instances {
            let state_str = instance
                .state()
                .and_then(|s| s.name())
                .map(|s| format!("{}", s))
                .unwrap_or_else(|| "unknown".to_string());
            match state_str.as_str() {
                "running" => {
                    running += 1;
                    let instance_type_str = instance
                        .instance_type()
                        .map(|t| format!("{}", t))
                        .unwrap_or_else(|| "unknown".to_string());
                    total_cost += estimate_instance_cost(&instance_type_str);

                    // Check age
                    if let Some(lt) = instance
                        .launch_time()
                        .and_then(|t| Utc.timestamp_opt(t.secs(), 0).single())
                    {
                        if lt < Utc::now() - chrono::Duration::hours(24) {
                            old_instances += 1;
                        }
                    }
                }
                "stopped" => stopped += 1,
                _ => {}
            }
        }
    }

    println!("\nCURRENT STATE:");
    println!("  Running instances: {}", running);
    println!("  Stopped instances: {}", stopped);
    println!("  Estimated hourly cost: ${:.2}", total_cost);

    println!("\n💡 Recommendations:");

    if old_instances > 0 {
        println!(
            "WARNING: {} instance(s) running > 24 hours - consider terminating",
            old_instances
        );
    }

    if stopped > 0 {
        println!(
            "{} stopped instance(s) - terminate to avoid storage costs",
            stopped
        );
    }

    if total_cost > 10.0 {
        println!(
            "  WARNING: High hourly cost (${:.2}/hr) - review instance types",
            total_cost
        );
    }

    if running == 0 {
        println!("No running instances");
    }

    println!("\n🔧 Actions:");
    println!("  trainctl resources list --detailed    # See all resources");
    println!("  trainctl resources cleanup --dry-run  # Preview cleanup");
    println!("  trainctl resources cleanup --force    # Cleanup zombies");

    Ok(())
}

// Table format display
async fn display_table_format(instances: &[&InstanceInfo], detailed: bool) -> Result<()> {
    let mut table = Table::new();
    // Table uses default styling

    if detailed {
        table.set_header(vec![
            "Instance ID",
            "State",
            "Type",
            "Runtime",
            "Spot",
            "Cost/hr",
            "Total",
            "Public IP",
            "Tags",
        ]);

        for inst in instances {
            let state_cell = match inst.state.as_str() {
                "running" => Cell::new(&inst.state).fg(comfy_table::Color::Green),
                "stopped" => Cell::new(&inst.state).fg(comfy_table::Color::Yellow),
                "terminated" => Cell::new(&inst.state).fg(comfy_table::Color::Red),
                _ => Cell::new(&inst.state),
            };

            let runtime = inst.runtime.as_deref().unwrap_or("N/A");
            let spot = if inst.is_spot { "SPOT" } else { "ON-DEMAND" };
            let public_ip = inst.public_ip.as_deref().unwrap_or("-");
            // Prioritize Name tag, then show other key tags
            let name_tag = inst
                .tags
                .iter()
                .find(|(k, _)| k == "Name")
                .map(|(_, v)| v.as_str())
                .unwrap_or("-");
            let other_tags: String = inst
                .tags
                .iter()
                .filter(|(k, _)| k != "Name")
                .take(1)
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            let tags = if other_tags.is_empty() {
                name_tag.to_string()
            } else {
                format!("{} ({})", name_tag, other_tags)
            };

            table.add_row(vec![
                Cell::new(&inst.id),
                state_cell,
                Cell::new(&inst.instance_type),
                Cell::new(runtime),
                Cell::new(spot),
                Cell::new(format!("${:.4}", inst.cost_per_hour)),
                Cell::new(format!("${:.2}", inst.accumulated_cost)),
                Cell::new(public_ip),
                Cell::new(&tags),
            ]);
        }
    } else {
        table.set_header(vec![
            "Name", "ID", "State", "Type", "Runtime", "Cost/hr", "Total", "IP",
        ]);

        for inst in instances {
            if inst.state != "running" {
                continue; // Skip non-running in compact view
            }

            let state_cell = Cell::new(&inst.state).fg(comfy_table::Color::Green);
            let runtime = inst.runtime.as_deref().unwrap_or("N/A");
            let ip = inst.public_ip.as_deref().unwrap_or("-");

            // Extract Name tag or use instance ID prefix
            let name = inst
                .tags
                .iter()
                .find(|(k, _)| k == "Name")
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| &inst.id[..12]);

            table.add_row(vec![
                Cell::new(name),
                Cell::new(&inst.id),
                state_cell,
                Cell::new(&inst.instance_type),
                Cell::new(runtime),
                Cell::new(format!("${:.4}", inst.cost_per_hour)),
                Cell::new(format!("${:.2}", inst.accumulated_cost)),
                Cell::new(ip),
            ]);
        }
    }

    println!("{}", table);
    Ok(())
}

// Interactive mode removed - use watch mode or flags instead

// Watch mode - auto-refresh
async fn list_resources_watch(
    config: &Config,
    platform: &str,
    filter: &str,
    sort: Option<&str>,
    interval: u64,
    project_filter: Option<&str>,
    user_filter: Option<&str>,
) -> Result<()> {
    use std::io::{self, Write};

    loop {
        // Clear screen (ANSI escape code)
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush()?;

        println!("WATCH: refreshing every {}s | [Ctrl+C] to stop", interval);
        println!(
            "Last update: {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        let list_options = ListResourcesOptions {
            detailed: false,
            platform: platform.to_string(),
            output_format: "text".to_string(),
            format: "table".to_string(),
            filter: filter.to_string(),
            sort: sort.map(|s| s.to_string()),
            limit: None,
            show_terminated: false,
            export: None,
            export_file: None,
            project_filter: project_filter.map(|s| s.to_string()),
            user_filter: user_filter.map(|s| s.to_string()),
        };
        list_resources(list_options, config).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

// Export resources
async fn export_resources(
    config: &Config,
    _platform: &str,
    format: &str,
    file: Option<&str>,
) -> Result<()> {
    let summary = get_resource_summary_json(config).await?;

    match format {
        "csv" => {
            let csv = generate_csv(&summary)?;
            if let Some(path) = file {
                std::fs::write(path, csv)?;
                println!("Exported to {}", path);
            } else {
                print!("{}", csv);
            }
        }
        "html" => {
            let html = generate_html(&summary)?;
            if let Some(path) = file {
                std::fs::write(path, html)?;
                println!("Exported to {}", path);
            } else {
                print!("{}", html);
            }
        }
        _ => {
            return Err(TrainctlError::Validation {
                field: "format".to_string(),
                reason: format!("Unsupported export format: {}. Use 'csv' or 'html'", format),
            });
        }
    }

    Ok(())
}

fn generate_csv(summary: &serde_json::Value) -> Result<String> {
    let mut csv = String::from(
        "Instance ID,Type,State,Cost/hr,Accumulated,Public IP,Private IP,Is Spot,Runtime\n",
    );

    if let Some(aws) = summary.get("aws") {
        if let Some(instances) = aws.get("instances").and_then(|v| v.as_array()) {
            for inst in instances {
                let id = inst
                    .get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let inst_type = inst
                    .get("instance_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let state = inst.get("state").and_then(|v| v.as_str()).unwrap_or("");
                let cost = inst
                    .get("cost_per_hour")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let total = inst
                    .get("accumulated_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let public_ip = inst.get("public_ip").and_then(|v| v.as_str()).unwrap_or("");
                let private_ip = inst
                    .get("private_ip")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let is_spot = inst
                    .get("is_spot")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let runtime = inst.get("runtime").and_then(|v| v.as_str()).unwrap_or("");

                csv.push_str(&format!(
                    "{},{},{},{:.4},{:.2},{},{},{},{}\n",
                    id, inst_type, state, cost, total, public_ip, private_ip, is_spot, runtime
                ));
            }
        }
    }

    Ok(csv)
}

fn generate_html(summary: &serde_json::Value) -> Result<String> {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>trainctl Resource Report</title>
    <style>
        body { font-family: monospace; margin: 20px; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #4CAF50; color: white; }
        tr:nth-child(even) { background-color: #f2f2f2; }
        .running { color: green; }
        .stopped { color: orange; }
        .terminated { color: red; }
    </style>
</head>
<body>
    <h1>Resource Report</h1>
    <p>Generated: "#,
    );

    html.push_str(&Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string());
    html.push_str(
        r#"</p>
    <table>
        <tr>
            <th>Instance ID</th>
            <th>Type</th>
            <th>State</th>
            <th>Cost/hr</th>
            <th>Total</th>
            <th>Public IP</th>
            <th>Runtime</th>
        </tr>"#,
    );

    if let Some(aws) = summary.get("aws") {
        if let Some(instances) = aws.get("instances").and_then(|v| v.as_array()) {
            for inst in instances {
                let id = inst
                    .get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let inst_type = inst
                    .get("instance_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let state = inst.get("state").and_then(|v| v.as_str()).unwrap_or("");
                let state_class = match state {
                    "running" => "running",
                    "stopped" => "stopped",
                    "terminated" => "terminated",
                    _ => "",
                };
                let cost = inst
                    .get("cost_per_hour")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let total = inst
                    .get("accumulated_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let public_ip = inst.get("public_ip").and_then(|v| v.as_str()).unwrap_or("");
                let runtime = inst.get("runtime").and_then(|v| v.as_str()).unwrap_or("");

                html.push_str(&format!(
                    r#"<tr>
            <td>{}</td>
            <td>{}</td>
            <td class="{}">{}</td>
            <td>${:.4}</td>
            <td>${:.2}</td>
            <td>{}</td>
            <td>{}</td>
        </tr>"#,
                    id, inst_type, state_class, state, cost, total, public_ip, runtime
                ));
            }
        }
    }

    html.push_str(
        r#"
    </table>
</body>
</html>"#,
    );

    Ok(html)
}

async fn stop_all_instances(
    dry_run: bool,
    force: bool,
    _platform: String,
    _config: &Config,
) -> Result<()> {
    use std::io::{self, Write};

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // Get all running instances
    let response = client
        .describe_instances()
        .filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("instance-state-name")
                .values("running")
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instances: {}", e)))?;

    let mut instance_ids = Vec::new();
    let mut instance_info = Vec::new();

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                let instance_type = instance
                    .instance_type()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                let cost_per_hour = estimate_instance_cost(&instance_type);

                instance_ids.push(instance_id.to_string());
                instance_info.push((instance_id.to_string(), instance_type, cost_per_hour));
            }
        }
    }

    if instance_ids.is_empty() {
        println!("No running instances found");
        return Ok(());
    }

    // Calculate cost savings
    let total_hourly_cost: f64 = instance_info.iter().map(|(_, _, cost)| cost).sum();
    let nightly_savings = total_hourly_cost * 8.0; // Assume 8 hours of sleep

    println!("RUNNING INSTANCES:");
    println!("{}", "-".repeat(80));
    for (id, inst_type, cost) in &instance_info {
        println!("  {}  {}  ${:.4}/hr", id, inst_type, cost);
    }
    println!("{}", "-".repeat(80));
    println!("Total: {} instance(s)", instance_ids.len());
    println!("Hourly cost: ${:.2}/hr", total_hourly_cost);
    println!("Estimated savings (8h): ${:.2}", nightly_savings);
    println!();

    if dry_run {
        println!("DRY RUN: Would stop {} instance(s)", instance_ids.len());
        return Ok(());
    }

    if !force {
        print!("Stop all {} instance(s)? (y/N): ", instance_ids.len());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            println!("Cancelled");
            return Ok(());
        }
    }

    println!("Stopping instances gracefully...");
    let ssm_client = aws_sdk_ssm::Client::new(&aws_config);

    for instance_id in &instance_ids {
        // First, gracefully stop any training jobs
        let graceful_stop_cmd = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        kill -TERM $PID 2>/dev/null || true
        for i in {1..30}; do
            if ! ps -p $PID > /dev/null 2>&1; then break; fi
            sleep 1
        done
        kill -9 $PID 2>/dev/null || true
    fi
else
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training\|python.*main.py" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        kill -TERM $TRAINING_PID 2>/dev/null || true
        for i in {1..30}; do
            if ! ps -p $TRAINING_PID > /dev/null 2>&1; then break; fi
            sleep 1
        done
        kill -9 $TRAINING_PID 2>/dev/null || true
    fi
fi
"#;

        // Try graceful shutdown (ignore errors - instance might not have SSM)
        let _ = crate::aws_utils::execute_ssm_command(&ssm_client, instance_id, graceful_stop_cmd)
            .await;

        // Then stop the instance
        match client
            .stop_instances()
            .instance_ids(instance_id)
            .send()
            .await
        {
            Ok(_) => {
                println!("  Stop requested: {}", instance_id);
            }
            Err(e) => {
                eprintln!("  ERROR: Failed to stop {}: {}", instance_id, e);
            }
        }
    }

    println!();
    println!("Stop requested for {} instance(s)", instance_ids.len());
    println!("Instances can be restarted later");

    Ok(())
}

pub fn estimate_instance_cost(instance_type: &str) -> f64 {
    // Simplified cost estimation (would use AWS Pricing API in production)
    // These are approximate on-demand prices per hour
    match instance_type {
        t if t.starts_with("t3.") => 0.0416, // t3.medium ~$0.0416/hr
        t if t.starts_with("t4g.") => 0.0336,
        t if t.starts_with("m5.") => 0.192,
        t if t.starts_with("c5.") => 0.17,
        t if t.starts_with("g4dn.") => 0.526, // GPU instance
        t if t.starts_with("p3.") => 3.06,    // GPU instance
        _ => 0.1,                             // Default estimate
    }
}
