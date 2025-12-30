//! AWS resource listing and management

use crate::aws::ec2_instance_to_resource_status;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::resource_tracking::ResourceTracker;
use crate::retry::{ExponentialBackoffPolicy, RetryPolicy};
use crate::utils::{format_runtime, is_old_instance};
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use comfy_table::{Cell, Table};
use console::{style, Style};
use std::collections::HashMap;
use tracing::info;

use super::export;
use super::json;
use super::local;
use super::runpod;
use super::types::{InstanceInfo, ListAwsInstancesOptions, ListResourcesOptions};

/// Sync ResourceTracker with current AWS state
///
/// This function queries AWS for all EC2 instances and updates the ResourceTracker
/// to match the current state. It's useful for:
/// - Initializing the tracker with existing resources
/// - Periodically syncing tracker state with actual AWS state
/// - Recovering from tracker state inconsistencies
///
/// # Arguments
/// * `client` - AWS EC2 client
/// * `tracker` - ResourceTracker to sync
///
/// # Errors
/// Returns an error if AWS API calls fail or if resource conversion fails.
pub(crate) async fn sync_resource_tracker_with_aws(
    client: &Ec2Client,
    tracker: &ResourceTracker,
) -> Result<()> {
    // Use retry logic for describe_instances
    let response = ExponentialBackoffPolicy::for_cloud_api()
        .execute_with_retry(|| async {
            client
                .describe_instances()
                .send()
                .await
                .map_err(|e| TrainctlError::Aws(format!("Failed to list EC2 instances: {}", e)))
        })
        .await?;

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                // Use helper function from aws module to avoid duplication
                let instance_id_str = instance_id.to_string();
                match ec2_instance_to_resource_status(instance, &instance_id_str) {
                    Ok(resource_status) => {
                        let instance_id_string = instance_id_str.clone();
                        if tracker.exists(&instance_id_string).await {
                            // Use update_state to update the resource state
                            if let Err(e) = tracker
                                .update_state(&instance_id_string, resource_status.state)
                                .await
                            {
                                info!("Failed to update resource state: {}", e);
                            } else {
                                info!("Synced resource {} to ResourceTracker", instance_id);
                            }
                        } else if let Err(e) = tracker.register(resource_status).await {
                            // Ignore errors for resources that already exist (race condition)
                            if !e.to_string().contains("already exists") {
                                info!("Failed to sync resource {}: {}", instance_id, e);
                            }
                        } else {
                            info!("Synced resource {} to ResourceTracker", instance_id);
                        }
                    }
                    Err(e) => {
                        info!(
                            "Failed to convert instance {} to ResourceStatus: {}",
                            instance_id, e
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// List resources across platforms
pub async fn list_resources(options: ListResourcesOptions, config: &Config) -> Result<()> {
    if options.output_format == "json" {
        let summary = json::get_resource_summary_json(config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    // Handle exports
    if let Some(export_format) = &options.export {
        return export::export_resources(
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
        runpod::list_runpod_pods(options.detailed, config).await?;
    }

    if options.platform == "all" || options.platform == "local" {
        local::list_local_processes(options.detailed).await?;
    }

    Ok(())
}

/// List AWS EC2 instances
async fn list_aws_instances(options: ListAwsInstancesOptions, config: &Config) -> Result<()> {
    let _section_style = Style::new().bold().yellow();
    println!("\nAWS EC2 INSTANCES:");
    println!("{}", "-".repeat(80));

    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);

    // Sync ResourceTracker with current AWS state if available
    if let Some(tracker) = &config.resource_tracker {
        if let Err(e) = sync_resource_tracker_with_aws(&client, tracker).await {
            info!("Failed to sync ResourceTracker: {}", e);
        }
    }

    // Use retry logic for describe_instances
    let response = ExponentialBackoffPolicy::for_cloud_api()
        .execute_with_retry(|| async {
            client
                .describe_instances()
                .send()
                .await
                .map_err(|e| TrainctlError::Aws(format!("Failed to list EC2 instances: {}", e)))
        })
        .await?;

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
                .and_then(|t| chrono::DateTime::from_timestamp(t.secs(), 0));

            // Try to get cost from ResourceTracker if available, otherwise calculate
            let (cost_per_hour, accumulated_cost) = crate::utils::get_instance_cost_with_tracker(
                config.resource_tracker.as_deref(),
                &instance_id,
                &instance_type_str,
                launch_time,
                state_str == "running",
            )
            .await;

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
                .any(|(k, v)| k == "runctl:project" && v == project)
        });
    }

    // Filter by user
    if let Some(user) = &options.user_filter {
        filtered_instances.retain(|inst| {
            inst.tags
                .iter()
                .any(|(k, v)| k == "runctl:user" && v == user)
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

                // Show key tags in summary (Name, project, runctl tags)
                if !inst.tags.is_empty() {
                    let key_tags: Vec<String> = inst
                        .tags
                        .iter()
                        .filter(|(k, _)| {
                            k == "Name"
                                || k == "runctl:project"
                                || k == "runctl:created"
                                || k == "CreatedBy"
                        })
                        .take(3)
                        .map(|(k, v)| {
                            // Clean up tag keys for display
                            let display_key = if k == "runctl:project" {
                                "project"
                            } else if k == "runctl:created" {
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

/// Display instances in table format
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
                .unwrap_or_else(|| &inst.id[..12.min(inst.id.len())]);

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
