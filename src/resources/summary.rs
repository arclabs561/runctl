//! Resource summary and insights

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::resources::json;
use crate::resources::types::{AwsInstance, ResourceSummary};
use crate::resources::utils::estimate_instance_cost;
use crate::utils::calculate_accumulated_cost;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use chrono::Utc;
use console::style;
use std::collections::HashMap;
use tracing::info;

use super::aws;

/// Show resource summary
pub async fn show_summary(config: &Config, output_format: &str) -> Result<()> {
    if output_format == "json" {
        let summary = json::get_resource_summary_json(config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    // Sync ResourceTracker with current AWS state if available
    if let Some(tracker) = &config.resource_tracker {
        let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = Ec2Client::new(&aws_config);
        if let Err(e) = aws::sync_resource_tracker_with_aws(&client, tracker).await {
            info!("Failed to sync ResourceTracker: {}", e);
        }
        // Refresh all costs before generating summary
        tracker.refresh_costs().await;
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

                    // Use ResourceTracker cost if available, otherwise calculate
                    let cost = if let Some(tracker) = &config.resource_tracker {
                        if let Some(tracked) = tracker.get_by_id(&instance_id).await {
                            tracked.status.cost_per_hour
                        } else {
                            estimate_instance_cost(&instance_type)
                        }
                    } else {
                        estimate_instance_cost(&instance_type)
                    };
                    summary.total_cost_estimate += cost;

                    summary.aws_instances.push(AwsInstance {
                        instance_id,
                        instance_type,
                        state,
                        launch_time: instance
                            .launch_time()
                            .and_then(|t| chrono::DateTime::from_timestamp(t.secs(), 0)),
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
    // Use ResourceTracker if available for more accurate accumulated costs
    let mut total_accumulated = if let Some(tracker) = &config.resource_tracker {
        tracker.get_total_cost().await
    } else {
        0.0
    };
    let mut type_breakdown: HashMap<String, (usize, f64, f64)> = HashMap::new();

    for inst in &summary.aws_instances {
        // Use ResourceTracker accumulated cost if available
        let accumulated = if let Some(tracker) = &config.resource_tracker {
            if let Some(tracked) = tracker.get_by_id(&inst.instance_id).await {
                tracked.accumulated_cost
            } else {
                // Fallback to calculation
                if let Some(lt) = inst.launch_time {
                    calculate_accumulated_cost(inst.cost_per_hour, Some(lt))
                } else {
                    0.0
                }
            }
        } else {
            // No tracker, calculate
            if let Some(lt) = inst.launch_time {
                calculate_accumulated_cost(inst.cost_per_hour, Some(lt))
            } else {
                0.0
            }
        };

        // Only add to total if not using tracker (tracker already has total)
        if config.resource_tracker.is_none() {
            total_accumulated += accumulated;
        }

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
        style("").red().bold()
    } else if summary.total_cost_estimate > hourly_threshold / 2.0 {
        style("").yellow().bold()
    } else {
        style("").yellow()
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
        println!("   Run 'runctl resources cleanup --dry-run' to identify cleanup candidates.");
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

/// Show resource insights
pub async fn show_insights(config: &Config, output_format: &str) -> Result<()> {
    if output_format == "json" {
        // For JSON, return structured insights
        let summary = json::get_resource_summary_json(config).await?;
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
                        .and_then(|t| chrono::DateTime::from_timestamp(t.secs(), 0))
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

    println!("\nRecommendations:");

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

    println!("\nActions:");
    println!("  runctl resources list --detailed    # See all resources");
    println!("  runctl resources cleanup --dry-run  # Preview cleanup");
    println!("  runctl resources cleanup --force    # Cleanup zombies");

    Ok(())
}
