use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod aws;
mod aws_utils;
mod checkpoint;
mod config;
mod dashboard;
mod data_transfer;
mod diagnostics;
mod ebs;
mod ebs_optimization;
mod error;
mod local;
mod monitor;
mod provider;
mod providers;
mod resources;
mod runpod;
mod s3;
mod ssh_sync;
mod training;
mod utils;
mod validation;

use crate::config::Config;

#[derive(Parser)]
#[command(name = "trainctl")]
#[command(
    about = "Modern training orchestration CLI for ML workloads",
    long_about = "trainctl is a unified CLI for managing ML training across multiple platforms.\n\nSupports:\n  - Local training (CPU/GPU)\n  - AWS EC2 (spot and on-demand instances)\n  - RunPod (GPU pods)\n\nFeatures:\n  - Checkpoint management and resumption\n  - Cost tracking and optimization\n  - Real-time monitoring\n  - Multi-platform resource management"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format (text, json)
    #[arg(long, global = true, default_value = "text")]
    output: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Train on local machine
    ///
    /// Executes a training script locally on your machine. Automatically detects
    /// Python scripts and uses `uv` if available, otherwise falls back to `python3`.
    ///
    /// Examples:
    ///   trainctl local train.py
    ///   trainctl local train.py -- --epochs 50 --batch-size 32
    ///   trainctl local scripts/train_model.py -- --lr 0.001
    Local {
        /// Training script path (Python script or executable)
        #[arg(value_name = "SCRIPT")]
        script: PathBuf,
        /// Additional arguments to pass to script
        ///
        /// Use '--' to separate trainctl args from script args:
        ///   trainctl local train.py -- --epochs 50
        #[arg(last = true, value_name = "ARGS")]
        args: Vec<String>,
    },
    /// Train on RunPod
    Runpod {
        #[command(subcommand)]
        subcommand: runpod::RunpodCommands,
    },
    /// Train on AWS EC2
    Aws {
        #[command(subcommand)]
        subcommand: aws::AwsCommands,
    },
    /// Monitor training progress
    ///
    /// Monitors training logs and checkpoint updates. Use --follow for continuous
    /// updates (like tail -f). Can monitor both logs and checkpoints simultaneously.
    ///
    /// Examples:
    ///   trainctl monitor --log training.log
    ///   trainctl monitor --checkpoint ./checkpoints/ --follow
    ///   trainctl monitor --log training.log --checkpoint ./checkpoints/ --follow
    Monitor {
        /// Training log file path to monitor
        #[arg(long, value_name = "LOG_PATH")]
        log: Option<PathBuf>,
        /// Checkpoint directory to monitor
        #[arg(long, value_name = "CHECKPOINT_DIR")]
        checkpoint: Option<PathBuf>,
        /// Follow mode (continuous updates, like tail -f)
        ///
        /// Continuously monitors for new log entries and checkpoint updates.
        #[arg(short, long)]
        follow: bool,
    },
    /// Manage checkpoints
    Checkpoint {
        #[command(subcommand)]
        subcommand: checkpoint::CheckpointCommands,
    },
    /// S3 operations (upload, download, sync, cleanup)
    S3 {
        #[command(subcommand)]
        subcommand: s3::S3Commands,
    },
    /// Review and manage resources (AWS, RunPod, local)
    Resources {
        #[command(subcommand)]
        subcommand: resources::ResourceCommands,
    },
    /// Manage configuration
    ///
    /// View, set, and validate configuration settings. Use 'init' to create a new config file.
    ///
    /// Examples:
    ///   trainctl config show
    ///   trainctl config set aws.region us-west-2
    ///   trainctl config validate
    Config {
        #[command(subcommand)]
        subcommand: config::ConfigCommands,
    },
    /// Initialize training configuration
    ///
    /// Creates a new configuration file with default values. The config file can be
    /// placed in the current directory (.trainctl.toml) or in the user config directory.
    ///
    /// Examples:
    ///   trainctl init
    ///   trainctl init --output ~/.config/trainctl/config.toml
    Init {
        /// Output path for config file
        #[arg(short, long, default_value = ".trainctl.toml")]
        output: PathBuf,
    },
    /// Quick status overview (resources summary + recent checkpoints)
    Status {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    /// Interactive top-like dashboard (ratatui) - shows resources, costs, and processes
    ///
    /// Real-time monitoring of instances, processes, and costs. Similar to 'top' command
    /// but for cloud training resources. Press 'q' to quit.
    ///
    /// Examples:
    ///   trainctl top
    ///   trainctl top --interval 2
    Top {
        /// Update interval in seconds
        #[arg(short, long, default_value_t = 5)]
        interval: u64,
    },
    /// Data transfer operations (local ↔ S3 ↔ training instances)
    ///
    /// Transfers data between local storage, S3, and training instances.
    /// Supports parallel transfers, compression, and resumable operations.
    ///
    /// Note: For S3 operations, consider using 'trainctl s3' commands for more
    /// features and better performance.
    ///
    /// Examples:
    ///   trainctl transfer ./data/ s3://bucket/data/
    ///   trainctl transfer s3://bucket/checkpoints/ ./checkpoints/ --parallel 10
    ///   trainctl transfer instance:i-123:/mnt/data ./local_data/
    Transfer {
        /// Source location (local path, s3://bucket/key, or instance:path)
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Destination location (local path, s3://bucket/key, or instance:path)
        #[arg(value_name = "DESTINATION")]
        destination: String,
        /// Number of parallel transfers (default: 10)
        #[arg(long, value_name = "COUNT")]
        parallel: Option<usize>,
        /// Enable compression during transfer
        #[arg(long)]
        compress: bool,
        /// Verify checksums after transfer (default: true)
        #[arg(long, default_value_t = true)]
        verify: bool,
        /// Resume interrupted transfers (default: true)
        #[arg(long, default_value_t = true)]
        resume: bool,
    },
    /// Execute a training script or command (generic executor)
    ///
    /// Executes a command with trainctl environment setup. Useful for running
    /// training scripts that expect trainctl environment variables.
    ///
    /// Note: For local Python training, consider using 'trainctl local' instead.
    ///
    /// Examples:
    ///   trainctl exec train --multi-dataset
    ///   trainctl exec evaluate --model-path ./models/
    Exec {
        /// Command to execute (script name or command)
        #[arg(value_name = "COMMAND")]
        command: String,
        /// Additional arguments to pass to the command
        ///
        /// Use '--' to separate trainctl args from command args:
        ///   trainctl exec train -- --epochs 50
        #[arg(last = true, value_name = "ARGS")]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging - suppress INFO by default, only show warnings and errors
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn") // Changed from "info" to "warn" to suppress INFO logs
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Load config
    let config = Config::load(cli.config.as_deref())?;

    // Execute command with error handling for JSON output
    let result: anyhow::Result<()> = match cli.command {
        Commands::Local { script, args } => local::train(script, args, &config)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Runpod { subcommand } => runpod::handle_command(subcommand, &config)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Aws { subcommand } => aws::handle_command(subcommand, &config, &cli.output)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Monitor {
            log,
            checkpoint,
            follow,
        } => monitor::monitor(log, checkpoint, follow)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Checkpoint { subcommand } => checkpoint::handle_command(subcommand, &cli.output)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Config { subcommand } => {
            config::handle_command(subcommand, cli.config.as_deref(), &cli.output)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
        Commands::S3 { subcommand } => s3::handle_command(subcommand, &config, &cli.output)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Resources { subcommand } => {
            resources::handle_command(subcommand, &config, &cli.output)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
        Commands::Init { output } => {
            config::init_config(&output).map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok(())
        }
        Commands::Status { detailed } => {
            resources::show_quick_status(detailed, &config, &cli.output)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
        Commands::Top { interval } => dashboard::run_dashboard(&config, interval)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Transfer {
            source,
            destination,
            parallel,
            compress,
            verify,
            resume,
        } => data_transfer::handle_transfer(
            source,
            destination,
            parallel,
            compress,
            verify,
            resume,
            &config,
        )
        .await
        .map_err(|e| anyhow::anyhow!("{}", e)),
        Commands::Exec { command, args } => {
            // Exec command - run arbitrary command with trainctl environment
            // For now, treat as local training with the command as script
            let script = PathBuf::from(&command);
            local::train(script, args, &config)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
    };

    // Handle errors with JSON format if requested
    if let Err(e) = result {
        if cli.output == "json" {
            use serde_json::json;
            let error_json = json!({
                "success": false,
                "error": {
                    "message": format!("{}", e),
                    "source": e.source().map(|s| format!("{}", s)).unwrap_or_else(|| "unknown".to_string()),
                }
            });
            eprintln!("{}", serde_json::to_string_pretty(&error_json)?);
            std::process::exit(1);
        } else {
            return Err(e);
        }
    }

    Ok(())
}
