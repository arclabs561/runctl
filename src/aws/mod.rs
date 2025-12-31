//! AWS EC2 operations module
//!
//! This module provides a unified interface for managing EC2 instances for ML training.
//!
//! ## Module Organization
//!
//! The AWS module is organized into focused submodules:
//! - `instance`: Instance lifecycle (create, start, stop, terminate)
//! - `training`: Training operations (train_on_instance, sync_code)
//! - `processes`: Process monitoring (show_processes)
//! - `helpers`: Utility functions (status conversion, user/project detection)
//! - `types`: Shared type definitions (options structs)
//!
//! ## Design Philosophy
//!
//! This module uses direct AWS SDK calls rather than the provider trait abstraction.
//! This provides:
//! - Direct control over AWS-specific features (spot pricing, EBS volumes)
//! - Easier debugging (no abstraction layer)
//! - Better performance (no trait dispatch overhead)
//!
//! See `docs/PROVIDER_TRAIT_DECISION.md` for rationale on why provider trait
//! is defined but not used here.
//!
//! ## Safety Features
//!
//! - Instance creation blocked if >50 instances running (prevents accidental mass creation)
//! - Warning shown if >10 instances running
//! - Spot instance fallback to on-demand (unless `--no-fallback`)
//! - Automatic Deep Learning AMI detection for GPU instances

mod helpers;
mod instance;
mod processes;
mod ssm_sync;
mod training;
mod types;

// Re-export helpers that are used by other modules (pub(crate) for crate-internal use)
pub(crate) use helpers::ec2_instance_to_resource_status;
pub use helpers::get_project_name;
pub use instance::{
    create_instance, create_instance_and_get_id, start_instance, stop_instance, terminate_instance,
};
// show_instance_status and wait_for_instance are used via instance:: prefix, no need to import
pub use processes::show_processes;
pub use training::{monitor_instance, train_on_instance};
pub use types::{CreateInstanceOptions, TrainInstanceOptions};

use crate::config::Config;
use crate::error::Result;
use aws_config::BehaviorVersion;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Clone)]
pub enum AwsCommands {
    /// Create EC2 instance for training
    ///
    /// Creates a new EC2 instance with optional spot pricing and data volumes.
    /// Automatically detects Deep Learning AMI for GPU instance types.
    ///
    /// Examples:
    ///   runctl aws create t3.medium
    ///   runctl aws create g4dn.xlarge --spot
    ///   runctl aws create p3.2xlarge --spot --data-volume-size 500
    #[command(alias = "new", alias = "spawn")]
    Create {
        /// EC2 instance type (e.g., t3.medium, g4dn.xlarge, p3.2xlarge)
        ///
        /// Common types:
        ///   - CPU: t3.medium, t3.large, m5.xlarge
        ///   - GPU: g4dn.xlarge, p3.2xlarge, p4d.24xlarge
        #[arg(value_name = "INSTANCE_TYPE")]
        instance_type: String,

        /// Request spot instance (cheaper, can be interrupted)
        ///
        /// Spot instances are up to 90% cheaper but can be terminated by AWS.
        /// Use for fault-tolerant workloads. Falls back to on-demand unless --no-fallback is set.
        #[arg(long)]
        spot: bool,

        /// Maximum spot price per hour (e.g., 0.10)
        ///
        /// If not set, uses the current on-demand price as maximum.
        /// Set lower to save money, but may reduce availability.
        #[arg(long, value_name = "PRICE")]
        spot_max_price: Option<String>,

        /// Don't fall back to on-demand if spot request fails
        ///
        /// By default, if spot instance creation fails, the command will
        /// automatically try on-demand. Use this flag to fail instead.
        #[arg(long)]
        no_fallback: bool,

        /// SSH key pair name (for EC2 Key Pairs)
        #[arg(long, value_name = "KEY_NAME")]
        key_name: Option<String>,

        /// Security group ID or name
        #[arg(long, value_name = "SECURITY_GROUP")]
        security_group: Option<String>,

        /// AMI ID (auto-detects Deep Learning AMI for GPU instances if not provided)
        #[arg(long, value_name = "AMI_ID")]
        ami_id: Option<String>,

        /// Root volume size in GB (default: 30, increased for GPU instances)
        #[arg(long, value_name = "SIZE_GB")]
        root_volume_size: Option<i32>,

        /// Auto-attach EBS volume for data/cache (size in GB)
        ///
        /// Creates and attaches an additional EBS volume for datasets, checkpoints, etc.
        /// The volume persists after instance termination unless explicitly deleted.
        #[arg(long, value_name = "SIZE_GB")]
        data_volume_size: Option<i32>,

        /// Project directory name (default: current directory name)
        ///
        /// Used for tagging and organizing instances. Defaults to the current
        /// directory name. Use to group related instances together.
        #[arg(long, value_name = "NAME")]
        project_name: Option<String>,

        /// IAM instance profile name for SSM access
        ///
        /// Enables Systems Manager (SSM) for secure command execution without SSH keys.
        /// Requires an IAM instance profile with AmazonSSMManagedInstanceCore policy.
        /// If not provided, instances will use SSH (requires --key-name).
        ///
        /// Example: runctl aws create t3.micro --iam-instance-profile runctl-ssm-profile
        #[arg(long, value_name = "PROFILE_NAME")]
        iam_instance_profile: Option<String>,

        /// Wait for instance to be ready before returning
        ///
        /// Blocks until instance is running and SSM is connected (if IAM profile provided).
        /// Without this flag, instance creation returns immediately after launch.
        #[arg(long)]
        wait: bool,
    },
    /// Start training job on an EC2 instance
    ///
    /// Uploads training script and dependencies, then starts training in the background.
    /// Training runs in a detached process and can be monitored with 'runctl aws monitor'.
    ///
    /// Examples:
    ///   runctl aws train i-1234567890abcdef0 training/train.py
    ///   runctl aws train i-1234567890abcdef0 training/train.py -- --epochs 50 --batch-size 32
    #[command(alias = "run")]
    Train {
        /// EC2 instance ID (e.g., i-1234567890abcdef0)
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,

        /// Training script path (Python script)
        #[arg(value_name = "SCRIPT")]
        script: PathBuf,

        /// S3 path for training data (s3://bucket/path)
        ///
        /// If provided, data will be downloaded before training starts.
        #[arg(long, value_name = "S3_PATH")]
        data_s3: Option<String>,

        /// S3 path for output/checkpoints (s3://bucket/path)
        ///
        /// If provided, checkpoints will be uploaded to S3 after training.
        #[arg(long, value_name = "S3_PATH")]
        _output_s3: Option<String>,

        /// Sync code before training (default: true)
        ///
        /// Uploads project code to the instance before starting training.
        /// Set to false if code is already present on the instance.
        #[arg(long, default_value = "true")]
        sync_code: bool,

        /// Include patterns even if gitignored (e.g., data/, datasets/)
        ///
        /// Useful for syncing data directories that are typically gitignored.
        /// Can be specified multiple times. Patterns are matched against file paths.
        ///
        /// Example: --include-pattern data/ --include-pattern datasets/
        #[arg(long, value_name = "PATTERN")]
        include_pattern: Vec<String>,

        /// Project directory name (default: current directory name)
        #[arg(long, value_name = "NAME")]
        project_name: Option<String>,

        /// Additional arguments to pass to training script
        ///
        /// Use '--' to separate runctl args from script args:
        ///   runctl aws train i-123 -- --epochs 50 --batch-size 32
        #[arg(last = true, value_name = "ARGS")]
        script_args: Vec<String>,

        /// Wait for training to complete before returning
        ///
        /// Blocks until training completes (checks for completion markers, checkpoints, or process status).
        /// Without this flag, training starts in background and command returns immediately.
        #[arg(long)]
        wait: bool,
    },
    /// Monitor training progress on an instance
    ///
    /// Shows training logs and checkpoint progress. Use --follow for continuous updates.
    ///
    /// Examples:
    ///   runctl aws monitor i-1234567890abcdef0
    ///   runctl aws monitor i-1234567890abcdef0 --follow
    #[command(alias = "watch", alias = "logs")]
    Monitor {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,

        /// Follow mode (continuous updates, like tail -f)
        #[arg(short, long)]
        follow: bool,
    },

    /// Stop an instance (preserves data, can be restarted)
    ///
    /// Stops the instance gracefully, preserving all data on attached volumes.
    /// The instance can be restarted later with 'start'. Use 'terminate' to permanently delete.
    ///
    /// Examples:
    ///   runctl aws stop i-1234567890abcdef0
    ///   runctl aws stop i-1234567890abcdef0 --force
    #[command(alias = "pause")]
    Stop {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,

        /// Force stop, bypassing safety checks
        ///
        /// Skips checks for running training jobs. Use with caution.
        #[arg(long)]
        force: bool,
    },

    /// Start a stopped instance
    ///
    /// Starts a previously stopped instance. The instance will retain its
    /// instance ID, attached volumes, and configuration. Note that the public
    /// IP address may change unless you're using an Elastic IP.
    ///
    /// Examples:
    ///   runctl aws start i-1234567890abcdef0
    ///   runctl aws start i-1234567890abcdef0 --wait
    #[command(alias = "resume")]
    Start {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,

        /// Wait for instance to be running before returning
        #[arg(long)]
        wait: bool,
    },

    /// Terminate an instance (permanently deletes, data on volumes preserved)
    ///
    /// Permanently terminates the instance. Attached EBS volumes are preserved
    /// unless they're set to delete on termination. Blocks if training is running
    /// unless --force is used.
    ///
    /// Examples:
    ///   runctl aws terminate i-1234567890abcdef0
    ///   runctl aws terminate i-1234567890abcdef0 --force
    #[command(alias = "destroy", alias = "rm", alias = "delete")]
    Terminate {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,

        /// Force termination, bypassing safety checks (e.g., running training jobs)
        ///
        /// WARNING: This will terminate even if training is actively running.
        /// Use only if you're certain you want to lose in-progress work.
        #[arg(long)]
        force: bool,
    },
    /// Show processes and resource usage on an instance
    ///
    /// Displays running processes and resource usage (CPU, memory, disk, GPU) on an instance.
    /// Use --watch for continuous monitoring. Use --detailed for full process information.
    ///
    /// Examples:
    ///   runctl aws processes i-1234567890abcdef0
    ///   runctl aws processes i-1234567890abcdef0 --watch --detailed
    Processes {
        /// EC2 instance ID (e.g., i-1234567890abcdef0)
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,
        /// Show detailed process information
        #[arg(short, long)]
        detailed: bool,
        /// Watch mode (auto-refresh)
        #[arg(short, long)]
        watch: bool,
        /// Refresh interval for watch mode (seconds)
        #[arg(long, default_value = "2")]
        interval: u64,
    },
    /// EBS volume management
    Ebs {
        #[command(subcommand)]
        subcommand: crate::ebs::EbsCommands,
    },
    /// Show instance status and training state
    ///
    /// Displays current instance state, training status, and resource usage.
    ///
    /// Examples:
    ///   runctl aws status i-1234567890abcdef0
    ///   runctl aws status i-1234567890abcdef0 --json
    Status {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,
    },
    /// Wait for instance to be ready
    ///
    /// Blocks until instance is running and SSM is connected (if IAM profile configured).
    ///
    /// Examples:
    ///   runctl aws wait i-1234567890abcdef0
    Wait {
        /// EC2 instance ID
        #[arg(value_name = "INSTANCE_ID")]
        instance_id: String,
    },
}

pub async fn handle_command(cmd: AwsCommands, config: &Config, output_format: &str) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    match cmd {
        AwsCommands::Create {
            instance_type,
            spot,
            spot_max_price,
            no_fallback,
            key_name,
            security_group,
            ami_id,
            root_volume_size,
            data_volume_size,
            project_name,
            iam_instance_profile,
            wait,
        } => {
            let final_project_name = helpers::get_project_name(project_name, config);
            crate::validation::validate_project_name(&final_project_name)?;
            let options = CreateInstanceOptions {
                instance_type,
                use_spot: spot,
                spot_max_price,
                no_fallback,
                key_name,
                security_group,
                ami_id,
                root_volume_size,
                data_volume_size,
                project_name: final_project_name,
                iam_instance_profile,
                wait,
            };
            create_instance(options, config, &aws_config, output_format).await
        }
        AwsCommands::Train {
            instance_id,
            script,
            data_s3,
            _output_s3,
            sync_code,
            include_pattern,
            project_name,
            script_args,
            wait,
        } => {
            crate::validation::validate_instance_id(&instance_id)?;
            let final_project_name = helpers::get_project_name(project_name, config);
            let options = TrainInstanceOptions {
                instance_id,
                script,
                data_s3,
                output_s3: _output_s3,
                sync_code,
                include_patterns: include_pattern,
                project_name: final_project_name,
                script_args,
                wait,
            };
            train_on_instance(options, config, &aws_config, output_format).await
        }
        AwsCommands::Status { instance_id } => {
            crate::validation::validate_instance_id(&instance_id)?;
            instance::show_instance_status(instance_id, &aws_config, output_format).await
        }
        AwsCommands::Wait { instance_id } => {
            crate::validation::validate_instance_id(&instance_id)?;
            instance::wait_for_instance(instance_id, &aws_config, output_format).await
        }
        AwsCommands::Monitor {
            instance_id,
            follow,
        } => {
            crate::validation::validate_instance_id(&instance_id)?;
            monitor_instance(instance_id, follow, &aws_config, output_format).await
        }
        AwsCommands::Stop { instance_id, force } => {
            crate::validation::validate_instance_id(&instance_id)?;
            stop_instance(instance_id, force, &aws_config, output_format, config).await
        }
        AwsCommands::Start { instance_id, wait } => {
            crate::validation::validate_instance_id(&instance_id)?;
            start_instance(instance_id, wait, &aws_config, output_format, config).await
        }
        AwsCommands::Terminate { instance_id, force } => {
            crate::validation::validate_instance_id(&instance_id)?;
            terminate_instance(instance_id, force, &aws_config, output_format, config).await
        }
        AwsCommands::Processes {
            instance_id,
            detailed,
            watch,
            interval,
        } => {
            crate::validation::validate_instance_id(&instance_id)?;
            show_processes(
                instance_id,
                detailed,
                watch,
                interval,
                &aws_config,
                output_format,
            )
            .await
        }
        AwsCommands::Ebs { subcommand } => {
            crate::ebs::handle_command(subcommand, config, output_format).await
        }
    }
}
