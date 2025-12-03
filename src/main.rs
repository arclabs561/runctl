use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod aws;
mod checkpoint;
mod config;
mod local;
mod monitor;
mod resources;
mod runpod;
mod s3;
mod training;
mod utils;

use crate::config::Config;

#[derive(Parser)]
#[command(name = "trainctl")]
#[command(about = "Modern training orchestration CLI for ML workloads", long_about = None)]
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
    }

    Ok(())
}

