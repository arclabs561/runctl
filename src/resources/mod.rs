//! Resource management module
//!
//! Provides unified resource listing, management, and reporting across
//! multiple platforms (AWS, RunPod, local).

mod aws;
mod cleanup;
mod export;
mod json;
mod local;
mod runpod;
mod summary;
mod types;
pub mod utils;  // Public for re-export
mod watch;

// Types are used internally via `types::` path.
// External consumers can access types via `crate::resources::types::TypeName` if needed.

// Re-export utility functions
pub use utils::estimate_instance_cost;

use crate::config::Config;
use crate::error::Result;
use clap::Subcommand;

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
        /// Filter by project name (from runctl:project tag)
        #[arg(long)]
        project: Option<String>,
        /// Filter by user (from runctl:user tag)
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
                watch::list_resources_watch(
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
                let list_options = types::ListResourcesOptions {
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
                if let Some(export_format) = &list_options.export {
                    export::export_resources(
                        config,
                        &list_options.platform,
                        export_format,
                        list_options.export_file.as_deref(),
                    )
                    .await
                } else {
                    aws::list_resources(list_options, config).await
                }
            }
        }
        ResourceCommands::Summary => summary::show_summary(config, output_format).await,
        ResourceCommands::Cleanup { dry_run, force } => {
            cleanup::cleanup_zombies(dry_run, force, config).await
        }
        ResourceCommands::StopAll {
            dry_run,
            force,
            platform,
        } => cleanup::stop_all_instances(dry_run, force, platform, config).await,
        ResourceCommands::Insights => summary::show_insights(config, output_format).await,
    }
}

pub async fn show_quick_status(
    detailed: bool,
    config: &Config,
    output_format: &str,
) -> Result<()> {
    use crate::checkpoint;
    use console::Style;

    if output_format == "json" {
        let summary = json::get_resource_summary_json(config).await?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    if !detailed {
        // Quick 1-2 line summary
        let aws_instances_json = json::list_aws_instances_json(config).await?;
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

        // Try to use ResourceTracker for cost data if available
        let (hourly_cost, total_cost) = if let Some(tracker) = &config.resource_tracker {
            let tracked = tracker.get_running().await;
            let hourly: f64 = tracked.iter().map(|r| r.status.cost_per_hour).sum();
            let total: f64 = tracked.iter().map(|r| r.accumulated_cost).sum();
            (hourly, total)
        } else {
            // Fallback to calculating from JSON data
            let hourly: f64 = running
                .iter()
                .filter_map(|inst| {
                    inst.get("instance_type")
                        .and_then(|t| t.as_str())
                        .map(crate::utils::get_instance_cost)
                })
                .sum();

            let total: f64 = running
                .iter()
                .filter_map(|inst| {
                    let hourly = inst
                        .get("instance_type")
                        .and_then(|t| t.as_str())
                        .map(crate::utils::get_instance_cost)?;
                    let hours = inst
                        .get("launch_time")
                        .and_then(|lt| lt.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| {
                            let runtime = chrono::Utc::now()
                                .signed_duration_since(dt.with_timezone(&chrono::Utc));
                            runtime.num_hours().max(0) as f64
                        })
                        .unwrap_or(0.0);
                    Some(hourly * hours)
                })
                .sum();
            (hourly, total)
        };

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
    println!("{}", header_style.apply_to("runctl Status"));
    println!("{}", header_style.apply_to("=".repeat(80)));

    // Quick resource summary
    println!("\nRESOURCES:");
    summary::show_summary(config, "text").await?;

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

