use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod aws;
mod aws_utils;
mod checkpoint;
mod config;
mod data_transfer;
mod diagnostics;
mod ebs;
mod local;
mod monitor;
mod provider;
mod providers;
mod resources;
mod runpod;
mod s3;
mod training;
mod utils;

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
    Local {
        /// Training script path
        script: PathBuf,
        /// Additional arguments to pass to script
        #[arg(last = true)]
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
    Monitor {
        /// Training log path
        log: Option<PathBuf>,
        /// Checkpoint directory
        checkpoint: Option<PathBuf>,
        /// Follow mode (like tail -f)
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
    /// Initialize training configuration
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
    /// Data transfer operations (local ↔ S3 ↔ training instances)
    Transfer {
        /// Source location (local path, s3://bucket/key, or instance:path)
        source: String,
        /// Destination location (local path, s3://bucket/key, or instance:path)
        destination: String,
        /// Number of parallel transfers
        #[arg(long)]
        parallel: Option<usize>,
        /// Enable compression
        #[arg(long)]
        compress: bool,
        /// Verify checksums
        #[arg(long, default_value_t = true)]
        verify: bool,
        /// Resume interrupted transfers
        #[arg(long, default_value_t = true)]
        resume: bool,
    },
}


#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging - suppress INFO by default, only show warnings and errors
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")  // Changed from "info" to "warn" to suppress INFO logs
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Load config
    let config = Config::load(cli.config.as_deref())?;

    // Execute command
    match cli.command {
        Commands::Local { script, args } => {
            local::train(script, args, &config).await?;
        }
        Commands::Runpod { subcommand } => {
            runpod::handle_command(subcommand, &config).await?;
        }
        Commands::Aws { subcommand } => {
            aws::handle_command(subcommand, &config).await?;
        }
        Commands::Monitor { log, checkpoint, follow } => {
            monitor::monitor(log, checkpoint, follow).await?;
        }
        Commands::Checkpoint { subcommand } => {
            checkpoint::handle_command(subcommand).await?;
        }
        Commands::S3 { subcommand } => {
            s3::handle_command(subcommand, &config).await?;
        }
        Commands::Resources { subcommand } => {
            resources::handle_command(subcommand, &config, &cli.output).await?;
        }
        Commands::Init { output } => {
            config::init_config(&output)?;
        }
        Commands::Status { detailed } => {
            resources::show_quick_status(detailed, &config, &cli.output).await?;
        }
        Commands::Transfer { source, destination, parallel, compress, verify, resume } => {
            data_transfer::handle_transfer(source, destination, parallel, compress, verify, resume, &config).await?;
        }
    }

    Ok(())
}

