use crate::aws_utils::{count_running_instances, execute_ssm_command};
use crate::config::Config;
use crate::diagnostics::check_high_resource_usage;
use crate::error::{Result, TrainctlError};
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use base64::Engine;
use chrono::Utc;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Serialize, Deserialize)]
struct InstanceInfo {
    success: bool,
    instance_id: String,
    instance_type: String,
    public_ip: Option<String>,
    private_ip: Option<String>,
    state: String,
    cost_per_hour: f64,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct TrainingInfo {
    success: bool,
    method: String,
    instance_id: String,
    log_path: String,
    monitor_command: String,
}

#[derive(Serialize, Deserialize)]
struct StopInstanceResult {
    success: bool,
    instance_id: String,
    state: String,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct TerminateInstanceResult {
    success: bool,
    instance_id: String,
    state: String,
    has_data_volumes: bool,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct ProcessListResult {
    success: bool,
    instance_id: String,
    timestamp: String,
    resource_usage: ProcessResourceUsage,
    processes: Vec<ProcessInfo>,
}

#[derive(Serialize, Deserialize)]
struct ProcessResourceUsage {
    cpu_percent: f64,
    memory_used_gb: f64,
    memory_total_gb: f64,
    memory_percent: f64,
    disk_usage: Vec<DiskUsage>,
    gpu_info: Option<GpuInfoJson>,
}

#[derive(Serialize, Deserialize)]
struct DiskUsage {
    filesystem: String,
    size_gb: f64,
    used_gb: f64,
    available_gb: f64,
    percent_used: f64,
    mount_point: String,
}

#[derive(Serialize, Deserialize)]
struct GpuInfoJson {
    gpus: Vec<GpuDetailJson>,
}

#[derive(Serialize, Deserialize)]
struct GpuDetailJson {
    index: usize,
    name: String,
    memory_used_mb: u64,
    memory_total_mb: u64,
    memory_percent: f64,
    utilization_percent: f64,
    temperature_c: Option<u32>,
    power_draw_w: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct ProcessInfo {
    pid: u32,
    user: String,
    command: String,
    cpu_percent: f64,
    memory_mb: f64,
    memory_percent: f64,
    runtime: String,
}

async fn get_instance_info_json(
    client: &Ec2Client,
    instance_id: &str,
    instance_type: &str,
) -> Result<InstanceInfo> {
    // Wait a moment for instance to be available
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let response = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws("Instance not found".to_string()))?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| format!("{}", s))
        .unwrap_or_else(|| "unknown".to_string());

    let cost_per_hour = crate::utils::get_instance_cost(instance_type);

    Ok(InstanceInfo {
        success: true,
        instance_id: instance_id.to_string(),
        instance_type: instance_type.to_string(),
        public_ip: instance.public_ip_address().map(|ip| ip.to_string()),
        private_ip: instance.private_ip_address().map(|ip| ip.to_string()),
        state,
        cost_per_hour,
        message: format!("Instance {} created successfully", instance_id),
    })
}

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
    },
    /// Start training job on an EC2 instance
    ///
    /// Uploads training script and dependencies, then starts training in the background.
    /// Training runs in a detached process and can be monitored with 'runctl aws monitor'.
    ///
    /// Examples:
    ///   runctl aws train i-1234567890abcdef0 training/train.py
    ///   runctl aws train i-1234567890abcdef0 training/train.py -- --epochs 50 --batch-size 32
    #[command(alias = "run", alias = "start")]
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
    /// The instance can be restarted later. Use 'terminate' to permanently delete.
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
}

/// Options for creating an EC2 instance
#[derive(Debug, Clone)]
pub struct CreateInstanceOptions {
    pub instance_type: String,
    pub use_spot: bool,
    pub spot_max_price: Option<String>,
    pub no_fallback: bool,
    pub key_name: Option<String>,
    pub security_group: Option<String>,
    pub ami_id: Option<String>,
    pub root_volume_size: Option<i32>,
    pub data_volume_size: Option<i32>,
    pub project_name: String,
    pub iam_instance_profile: Option<String>,
}

/// Options for training on an instance
#[derive(Debug, Clone)]
pub struct TrainInstanceOptions {
    pub instance_id: String,
    pub script: PathBuf,
    #[allow(dead_code)] // Reserved for future S3 data source support
    pub data_s3: Option<String>,
    #[allow(dead_code)] // Reserved for future S3 output support
    pub output_s3: Option<String>,
    pub sync_code: bool,
    pub include_patterns: Vec<String>,
    pub project_name: String,
    pub script_args: Vec<String>,
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
        } => {
            let final_project_name = get_project_name(project_name, config);
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
        } => {
            let final_project_name = get_project_name(project_name, config);
            let options = TrainInstanceOptions {
                instance_id,
                script,
                data_s3,
                output_s3: _output_s3,
                sync_code,
                include_patterns: include_pattern,
                project_name: final_project_name,
                script_args,
            };
            train_on_instance(options, config, &aws_config, output_format).await
        }
        AwsCommands::Monitor {
            instance_id,
            follow,
        } => monitor_instance(instance_id, follow, &aws_config, output_format).await,
        AwsCommands::Stop { instance_id, force } => {
            stop_instance(instance_id, force, &aws_config, output_format).await
        }
        AwsCommands::Terminate { instance_id, force } => {
            terminate_instance(instance_id, force, &aws_config, output_format).await
        }
        AwsCommands::Processes {
            instance_id,
            detailed,
            watch,
            interval,
        } => {
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

/// Get user identifier for tagging
fn get_user_id(config: &Config) -> String {
    // Try config first
    if let Some(aws_cfg) = &config.aws {
        if let Some(user_id) = &aws_cfg.user_id {
            return user_id.clone();
        }
    }

    // Auto-detect from username
    if let Ok(username) = std::env::var("USER") {
        return username;
    }
    if let Ok(username) = std::env::var("USERNAME") {
        return username;
    }

    // Fallback
    "unknown".to_string()
}

/// Get project name, deriving from current directory if not provided
fn get_project_name(provided: Option<String>, config: &Config) -> String {
    // Use provided value if given
    if let Some(name) = provided {
        return name;
    }

    // Try config
    if let Some(aws_cfg) = &config.aws {
        if let Some(project) = &aws_cfg.default_project_name {
            return project.clone();
        }
    }

    // Derive from current directory
    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(dir_name) = current_dir.file_name() {
            if let Some(name_str) = dir_name.to_str() {
                // Sanitize directory name for use as project name
                let sanitized = name_str
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>();
                if !sanitized.is_empty() {
                    return sanitized;
                }
            }
        }
    }

    // Final fallback
    "runctl-project".to_string()
}

async fn create_instance(
    options: CreateInstanceOptions,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let aws_cfg = config.aws.as_ref().ok_or_else(|| {
        TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
    })?;

    let client = Ec2Client::new(aws_config);

    // Safety check: Prevent accidental mass creation
    let running_count = count_running_instances(&client).await?;
    if running_count >= 50 {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Too many instances running ({}). Creation blocked to prevent accidental mass creation.\n\n\
                To resolve:\n\
                  1. List running instances: runctl resources list --platform aws\n\
                  2. Terminate unused instances: runctl aws terminate <instance-id>\n\
                  3. Stop instances (preserves data): runctl aws stop <instance-id>\n\
                  4. Use a different AWS account or region\n\n\
                To override this limit, modify the safety check in the code.",
                running_count
            ),
            source: None,
        });
    } else if running_count >= 10 {
        println!(
            "WARNING: {} instances already running. Proceeding with caution.",
            running_count
        );
        println!("  Use 'runctl resources list' to review running instances.");
    }

    info!(
        "Creating EC2 instance: type={}, spot={}",
        options.instance_type, options.use_spot
    );

    // Auto-detect AMI if not provided
    let final_ami = if let Some(ami) = &options.ami_id {
        ami.clone()
    } else {
        // Check if GPU instance (g4dn, p3, p4, etc.)
        let is_gpu = options.instance_type.starts_with("g")
            || options.instance_type.starts_with("p")
            || options.instance_type.contains("gpu");

        if is_gpu {
            // Try to find Deep Learning AMI
            match find_deep_learning_ami(&client, &aws_cfg.region).await {
                Ok(ami) => {
                    println!("   Using Deep Learning AMI: {}", ami);
                    ami
                }
                Err(e) => {
                    println!("WARNING: Could not find Deep Learning AMI: {}", e);
                    println!("   Using default AMI: {}", aws_cfg.default_ami);
                    aws_cfg.default_ami.clone()
                }
            }
        } else {
            aws_cfg.default_ami.clone()
        }
    };

    // Determine root volume size (larger for GPU instances or if specified)
    let root_size = options.root_volume_size.unwrap_or_else(|| {
        if options.instance_type.starts_with("g") || options.instance_type.starts_with("p") {
            50 // GPU instances need more space for CUDA/PyTorch
        } else {
            30 // Default
        }
    });

    // Generate user data script
    let user_data = generate_user_data(&options.project_name, options.data_volume_size.is_some());

    // Try spot instance first if requested
    if options.use_spot {
        let spot_options = CreateSpotInstanceOptions {
            instance_type: options.instance_type.clone(),
            ami_id: final_ami.clone(),
            user_data: user_data.clone(),
            max_price: options.spot_max_price.clone(),
            key_name: options.key_name.clone(),
            security_group: options.security_group.clone(),
            root_volume_size: root_size,
            iam_instance_profile: options.iam_instance_profile.clone(),
        };
        match create_spot_instance(&client, spot_options).await {
            Ok(instance_id) => {
                if output_format == "json" {
                    let instance_info =
                        get_instance_info_json(&client, &instance_id, &options.instance_type)
                            .await?;
                    println!("{}", serde_json::to_string_pretty(&instance_info)?);
                } else {
                    println!("Created spot instance: {}", instance_id);
                }
                if let Err(e) =
                    tag_instance(&client, &instance_id, &options.project_name, config).await
                {
                    warn!("Failed to tag instance {}: {}", instance_id, e);
                    if output_format != "json" {
                        println!("  Instance created but tagging failed. You can tag manually if needed.");
                    }
                }
                return Ok(());
            }
            Err(e) if !options.no_fallback => {
                println!("WARNING: Spot instance failed: {}", e);
                println!("Falling back to on-demand...");
            }
            Err(e) => {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Spot instance failed and no fallback: {}\n\
                        \n\
                        Suggestions:\n\
                          - Try on-demand instance: remove --spot flag\n\
                          - Check spot price limits: current max price may be too low\n\
                          - Try a different instance type or region\n\
                          - Check AWS spot instance availability in your region",
                        e
                    ),
                    source: None,
                });
            }
        }
    }

    // Create on-demand instance
    let instance_id = create_ondemand_instance(
        &client,
        &options.instance_type,
        &final_ami,
        &user_data,
        options.key_name.as_deref(),
        options.security_group.as_deref(),
        root_size,
        options.iam_instance_profile.as_deref(),
    )
    .await?;

    if output_format == "json" {
        let instance_info =
            get_instance_info_json(&client, &instance_id, &options.instance_type).await?;
        println!("{}", serde_json::to_string_pretty(&instance_info)?);
    } else {
        println!("Created on-demand instance: {}", instance_id);
    }

    if let Err(e) = tag_instance(&client, &instance_id, &options.project_name, config).await {
        warn!("Failed to tag instance {}: {}", instance_id, e);
        if output_format != "json" {
            println!("  Instance created but tagging failed. You can tag manually if needed.");
        }
    }

    // Auto-attach data volume if requested
    if let Some(data_size) = options.data_volume_size {
        if output_format != "json" {
            println!("   Creating and attaching {}GB data volume...", data_size);
        }
        if let Err(e) =
            auto_attach_data_volume(&client, &instance_id, data_size, &aws_cfg.region).await
        {
            if output_format != "json" {
                println!("WARNING: Failed to attach data volume: {}", e);
                println!(
                    "   You can attach manually: runctl aws ebs create --size {} --attach",
                    data_size
                );
            }
        }
    }

    Ok(())
}

/// Find latest Deep Learning AMI for GPU instances
async fn find_deep_learning_ami(client: &Ec2Client, _region: &str) -> Result<String> {
    use aws_sdk_ec2::types::Filter;

    // Try multiple Deep Learning AMI patterns
    let patterns = vec![
        "Deep Learning AMI GPU PyTorch * (Amazon Linux 2)*",
        "Deep Learning AMI GPU PyTorch *",
        "Deep Learning AMI (Amazon Linux 2)*",
        "Deep Learning Base AMI (Amazon Linux 2)*",
    ];

    for pattern in patterns {
        let response = client
            .describe_images()
            .owners("amazon")
            .filters(Filter::builder().name("name").values(pattern).build())
            .filters(Filter::builder().name("state").values("available").build())
            .send()
            .await
            .map_err(|e| {
                TrainctlError::Aws(format!("Failed to search for Deep Learning AMI: {}", e))
            })?;

        let images = response.images();
        if !images.is_empty() {
            // Sort by creation date (newest first)
            let mut sorted: Vec<_> = images.iter().collect();
            sorted.sort_by(|a, b| {
                let a_date = a.creation_date().unwrap_or("");
                let b_date = b.creation_date().unwrap_or("");
                b_date.cmp(a_date)
            });

            return Ok(sorted[0]
                .image_id()
                .ok_or_else(|| TrainctlError::Aws("AMI has no image ID".to_string()))?
                .to_string());
        }
    }

    Err(TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "No Deep Learning AMI found with any pattern".to_string(),
        source: None,
    })
}

/// Generate user data script for instance initialization
fn generate_user_data(project_name: &str, _has_data_volume: bool) -> String {
    format!(
        r#"#!/bin/bash
set -e

# Log all output for debugging
exec > >(tee /var/log/user-data.log)
exec 2>&1

echo "Starting instance setup..."

# Detect OS (Ubuntu vs Amazon Linux)
if [ -f /etc/os-release ]; then
    . /etc/os-release
    OS=$ID
else
    OS="unknown"
fi

echo "Detected OS: $OS"

# Update system
if [ "$OS" = "ubuntu" ]; then
    export DEBIAN_FRONTEND=noninteractive
apt-get update -y
    apt-get upgrade -y -qq
    apt-get install -y python3-pip python3-venv git curl build-essential
    USER="ubuntu"
    HOME_DIR="/home/ubuntu"
elif [ "$OS" = "amzn" ] || [ "$OS" = "rhel" ]; then
    yum update -y -q
    # Install Python 3 and pip
    yum install -y python3 python3-pip git curl gcc gcc-c++ make
    # Ensure pip3 is available
    if ! command -v pip3 &> /dev/null; then
        # Try alternative installation methods
        if command -v python3 &> /dev/null; then
            curl -sS https://bootstrap.pypa.io/get-pip.py | python3
        fi
    fi
    USER="ec2-user"
    HOME_DIR="/home/ec2-user"
else
    echo "WARNING: Unknown OS, using defaults"
    USER="ubuntu"
    HOME_DIR="/home/ubuntu"
fi

# Install uv for Python package management
echo "Installing uv..."
curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME_DIR/.local/bin:$HOME_DIR/.cargo/bin:$PATH"
echo 'export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"' >> $HOME_DIR/.bashrc

# Pre-install common ML libraries (cached for faster training startup)
echo "Pre-installing common ML libraries..."
if command -v uv &> /dev/null; then
    uv pip install --system --quiet numpy pandas || pip3 install --quiet --user numpy pandas
else
    pip3 install --quiet --user numpy pandas || python3 -m pip install --quiet --user numpy pandas
fi

# Create dependency cache directory
mkdir -p /opt/runctl-cache
chmod 777 /opt/runctl-cache
echo "Dependency cache: /opt/runctl-cache"

# Setup data volume if attached
if [ -b /dev/nvme1n1 ] || [ -b /dev/xvdf ]; then
    echo "Setting up data volume..."
    DEVICE=$(lsblk -o NAME,TYPE,SIZE | grep -E '^nvme[0-9]+n1' | grep -v nvme0n1 | awk '{{print $1}}' | head -1)
    if [ -z "$DEVICE" ]; then
        for dev in /dev/xvdf /dev/sdf /dev/nvme1n1; do
            if [ -b "$dev" ]; then
                DEVICE=$(basename $dev)
                break
            fi
        done
    fi
    
    if [ -n "$DEVICE" ]; then
        FULL_DEVICE="/dev/$DEVICE"
        MOUNT_POINT="/mnt/data"
        
        # Format if not already formatted
        if ! blkid $FULL_DEVICE > /dev/null 2>&1; then
            echo "   Formatting volume..."
            mkfs.ext4 -F $FULL_DEVICE
        fi
        
        # Mount
        mkdir -p $MOUNT_POINT
        if ! mountpoint -q $MOUNT_POINT; then
            mount $FULL_DEVICE $MOUNT_POINT
            UUID=$(blkid -s UUID -o value $FULL_DEVICE)
            echo "UUID=$UUID $MOUNT_POINT ext4 defaults,nofail 0 2" >> /etc/fstab
        fi
        
        chown -R $USER:$USER $MOUNT_POINT
        echo "Data volume mounted at $MOUNT_POINT"
    fi
fi

# Create project directory
PROJECT_DIR="$HOME_DIR/{project_name}"
mkdir -p $PROJECT_DIR
chown $USER:$USER $PROJECT_DIR

# Create data directory (use mounted volume if available, else local)
if [ -d /mnt/data ]; then
    DATA_DIR="/mnt/data"
else
    DATA_DIR="$HOME_DIR/data"
fi
mkdir -p $DATA_DIR
chown $USER:$USER $DATA_DIR

# Setup Python environment
export PYTHONPATH=$PROJECT_DIR:$PYTHONPATH
echo "export PYTHONPATH=$PROJECT_DIR:\$PYTHONPATH" >> $HOME_DIR/.bashrc

# Create helper script for training
cat > $HOME_DIR/start_training.sh << 'TRAIN_SCRIPT'
#!/bin/bash
cd $PROJECT_DIR
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
export PYTHONPATH=$PROJECT_DIR:$PYTHONPATH

if command -v uv &> /dev/null; then
    uv run python3 -m training.train_lightning "$@"
else
    python3 -m training.train_lightning "$@"
fi
TRAIN_SCRIPT
chmod +x $HOME_DIR/start_training.sh
chown $USER:$USER $HOME_DIR/start_training.sh

echo "Instance setup complete"
echo "   Project directory: $PROJECT_DIR"
echo "   Data directory: $DATA_DIR"
echo "   To start training: $HOME_DIR/start_training.sh"
"#,
        project_name = project_name
    )
}

/// Auto-attach and setup data volume
async fn auto_attach_data_volume(
    client: &Ec2Client,
    instance_id: &str,
    size_gb: i32,
    _region: &str,
) -> Result<()> {
    // Get instance AZ
    let instance_response = client
        .describe_instances()
        .instance_ids(instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws("Instance not found".to_string()))?;

    let az = instance
        .placement()
        .and_then(|p| p.availability_zone())
        .ok_or_else(|| TrainctlError::Aws("Instance has no availability zone".to_string()))?;

    // Create volume
    let volume_response = client
        .create_volume()
        .size(size_gb)
        .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
        .availability_zone(az)
        .tag_specifications(
            aws_sdk_ec2::types::TagSpecification::builder()
                .resource_type(aws_sdk_ec2::types::ResourceType::Volume)
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("Name")
                        .value(format!("{}-data", instance_id))
                        .build(),
                )
                .tags(
                    aws_sdk_ec2::types::Tag::builder()
                        .key("CreatedBy")
                        .value("runctl")
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create data volume: {}", e)))?;

    let volume_id = volume_response
        .volume_id()
        .ok_or_else(|| TrainctlError::Aws("Volume ID not in response".to_string()))?;

    // Wait for volume to be available
    println!("   Waiting for volume to be available...");
    let mut attempts = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        attempts += 1;

        let vol_response = client
            .describe_volumes()
            .volume_ids(volume_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe volume: {}", e)))?;

        let vol = vol_response
            .volumes()
            .first()
            .ok_or_else(|| TrainctlError::Aws("Volume not found".to_string()))?;
        let state = vol.state().map(|s| format!("{:?}", s)).unwrap_or_default();

        if state == "available" {
            break;
        }
        if attempts > 30 {
            return Err(TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "Volume creation timed out".to_string(),
                source: None,
            });
        }
    }

    // Attach volume (use /dev/sdf for compatibility)
    client
        .attach_volume()
        .volume_id(volume_id)
        .instance_id(instance_id)
        .device("/dev/sdf")
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to attach volume: {}", e)))?;

    println!(
        "Data volume {} attached (will be auto-mounted by user-data)",
        volume_id
    );

    Ok(())
}

/// Options for creating a spot instance
#[derive(Debug, Clone)]
struct CreateSpotInstanceOptions {
    instance_type: String,
    ami_id: String,
    user_data: String,
    max_price: Option<String>,
    key_name: Option<String>,
    security_group: Option<String>,
    root_volume_size: i32,
    iam_instance_profile: Option<String>,
}

async fn create_spot_instance(
    client: &Ec2Client,
    options: CreateSpotInstanceOptions,
) -> Result<String> {
    use aws_sdk_ec2::types::InstanceType as Ec2InstanceType;

    // Base64 encode user data
    let user_data_b64 = base64::engine::general_purpose::STANDARD.encode(&options.user_data);

    // Create spot instance request with launch specification
    let mut spec_builder = aws_sdk_ec2::types::RequestSpotLaunchSpecification::builder()
        .image_id(&options.ami_id)
        .instance_type(Ec2InstanceType::from(options.instance_type.as_str()))
        .user_data(&user_data_b64)
        .ebs_optimized(true); // Enable EBS optimization for better I/O performance

    if let Some(key) = &options.key_name {
        spec_builder = spec_builder.key_name(key);
    }
    if let Some(sg) = &options.security_group {
        spec_builder = spec_builder.security_groups(sg);
    }

    // Configure root volume size (device name depends on AMI - try both common ones)
    // For Ubuntu: /dev/sda1, for Amazon Linux: /dev/xvda
    let block_device = aws_sdk_ec2::types::BlockDeviceMapping::builder()
        .device_name("/dev/sda1") // Ubuntu default
        .ebs(
            aws_sdk_ec2::types::EbsBlockDevice::builder()
                .volume_size(options.root_volume_size)
                .delete_on_termination(true)
                .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
                .build(),
        )
        .build();
    spec_builder = spec_builder.block_device_mappings(block_device);

    // Add IAM instance profile if provided
    if let Some(profile_name) = &options.iam_instance_profile {
        spec_builder = spec_builder.iam_instance_profile(
            aws_sdk_ec2::types::IamInstanceProfileSpecification::builder()
                .name(profile_name)
                .build(),
        );
    }

    let spec = spec_builder.build();

    let mut spot_request = client
        .request_spot_instances()
        .instance_count(1)
        .launch_specification(spec);

    // Set spot price if provided
    if let Some(price) = &options.max_price {
        spot_request = spot_request.spot_price(price);
    } else {
        // Use one-time spot request by default
        spot_request = spot_request.spot_price("0.10"); // Default max price
    }

    let response = spot_request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to request spot instance: {}", e)))?;

    let spot_request_id = response
        .spot_instance_requests()
        .first()
        .and_then(|req| req.spot_instance_request_id())
        .ok_or_else(|| TrainctlError::Aws("No spot request ID in response".to_string()))?
        .to_string();

    // Wait for spot instance to be fulfilled
    info!(
        "Waiting for spot instance to be fulfilled (request ID: {})",
        spot_request_id
    );

    let mut attempts = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        attempts += 1;

        let describe_response = client
            .describe_spot_instance_requests()
            .spot_instance_request_ids(&spot_request_id)
            .send()
            .await
            .map_err(|e| TrainctlError::Aws(format!("Failed to describe spot request: {}", e)))?;

        let request = describe_response
            .spot_instance_requests()
            .first()
            .ok_or_else(|| TrainctlError::Aws("Spot request not found".to_string()))?;

        let state = request.state().and_then(|s| s.as_str().into());

        match state {
            Some("fulfilled") => {
                let instance_id = request
                    .instance_id()
                    .ok_or_else(|| {
                        TrainctlError::Aws("No instance ID in fulfilled request".to_string())
                    })?
                    .to_string();
                return Ok(instance_id);
            }
            Some("open") | Some("active") => {
                // Still waiting
                if attempts > 60 {
                    return Err(TrainctlError::CloudProvider {
                        provider: "aws".to_string(),
                        message: "Spot request timed out after 5 minutes".to_string(),
                        source: None,
                    });
                }
                continue;
            }
            Some("failed") | Some("cancelled") | Some("closed") => {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Spot request {}: {}",
                        spot_request_id,
                        state.unwrap_or("unknown")
                    ),
                    source: None,
                });
            }
            _ => {
                if attempts > 60 {
                    return Err(TrainctlError::CloudProvider {
                        provider: "aws".to_string(),
                        message: format!("Spot request in unknown state: {:?}", state),
                        source: None,
                    });
                }
                continue;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn create_ondemand_instance(
    client: &Ec2Client,
    instance_type: &str,
    ami_id: &str,
    user_data: &str,
    key_name: Option<&str>,
    security_group: Option<&str>,
    root_volume_size: i32,
    iam_instance_profile: Option<&str>,
) -> Result<String> {
    use aws_sdk_ec2::types::InstanceType as Ec2InstanceType;

    // Base64 encode user data
    let user_data_b64 = base64::engine::general_purpose::STANDARD.encode(user_data);

    let mut run_request = client
        .run_instances()
        .image_id(ami_id)
        .instance_type(Ec2InstanceType::from(instance_type))
        .min_count(1)
        .max_count(1)
        .user_data(&user_data_b64)
        .ebs_optimized(true); // Enable EBS optimization for better I/O performance

    if let Some(key) = key_name {
        run_request = run_request.key_name(key);
    }
    if let Some(sg) = security_group {
        run_request = run_request.security_group_ids(sg);
    }

    // Add IAM instance profile if provided
    if let Some(profile_name) = iam_instance_profile {
        run_request = run_request.iam_instance_profile(
            aws_sdk_ec2::types::IamInstanceProfileSpecification::builder()
                .name(profile_name)
                .build(),
        );
    }

    // Configure root volume size (device name depends on AMI)
    let block_device = aws_sdk_ec2::types::BlockDeviceMapping::builder()
        .device_name("/dev/sda1") // Ubuntu default, works for most AMIs
        .ebs(
            aws_sdk_ec2::types::EbsBlockDevice::builder()
                .volume_size(root_volume_size)
                .delete_on_termination(true)
                .volume_type(aws_sdk_ec2::types::VolumeType::Gp3)
                .build(),
        )
        .build();
    run_request = run_request.block_device_mappings(block_device);

    let response = run_request
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to create instance: {}", e)))?;

    let instance_id = response
        .instances()
        .first()
        .and_then(|inst| inst.instance_id())
        .ok_or_else(|| TrainctlError::Aws("No instance ID in response".to_string()))?
        .to_string();

    Ok(instance_id)
}

/// Tag an instance with Name and runctl metadata
async fn tag_instance(
    client: &Ec2Client,
    instance_id: &str,
    project_name: &str,
    config: &Config,
) -> Result<()> {
    use aws_sdk_ec2::types::Tag;

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let user_id = get_user_id(config);

    // config is used via get_user_id() call above
    let name_tag = format!("runctl-{}-{}-{}", user_id, project_name, &instance_id[..8]);

    client
        .create_tags()
        .resources(instance_id)
        .tags(Tag::builder().key("Name").value(&name_tag).build())
        .tags(
            Tag::builder()
                .key("runctl:created")
                .value(timestamp)
                .build(),
        )
        .tags(
            Tag::builder()
                .key("runctl:project")
                .value(project_name)
                .build(),
        )
        .tags(Tag::builder().key("runctl:user").value(&user_id).build())
        .tags(Tag::builder().key("CreatedBy").value("runctl").build())
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to tag instance: {}", e)))?;

    Ok(())
}

async fn train_on_instance(
    options: TrainInstanceOptions,
    _config: &Config,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let ec2_client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    info!("Starting training on instance: {}", options.instance_id);

    // Get instance details (IP, key name, user)
    let instance_response = ec2_client
        .describe_instances()
        .instance_ids(&options.instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| {
            i.instance_id()
                .map(|id| id == options.instance_id.as_str())
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            TrainctlError::Aws(format!(
                "Instance {} not found.\n\n\
            To resolve:\n\
              1. Verify instance ID: runctl resources list --platform aws\n\
              2. Check if instance was terminated: aws ec2 describe-instances --instance-ids {}\n\
              3. Verify you're using the correct AWS region/account",
                options.instance_id, options.instance_id
            ))
        })?;

    let public_ip = instance.public_ip_address().ok_or_else(|| {
        TrainctlError::Aws(format!(
            "Instance {} has no public IP address.\n\n\
            To resolve:\n\
              1. Check if instance is in a public subnet with internet gateway\n\
              2. Verify security groups allow SSH (port 22)\n\
              3. Check instance state: runctl aws processes {}\n\
              4. Use SSM instead if IAM role is configured: runctl aws train {} --sync-code false",
            options.instance_id, options.instance_id, options.instance_id
        ))
    })?;

    let key_name = instance.key_name();
    let key_path = key_name
        .and_then(|k| {
            // Try common key locations
            let paths = [
                format!("~/.ssh/{}.pem", k),
                format!("~/.ssh/{}", k),
                "~/.ssh/id_rsa".to_string(),
            ];
            paths.iter().find_map(|p| {
                let expanded = shellexpand::tilde(p).to_string();
                if std::path::Path::new(&expanded).exists() {
                    Some(expanded)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            let key_name_str = key_name.unwrap_or("unknown");
            TrainctlError::Aws(format!(
                "Could not find SSH key for key pair '{}'.\n\n\
                To resolve:\n\
                  1. Set SSH_KEY_PATH environment variable: export SSH_KEY_PATH=~/.ssh/{}.pem\n\
                  2. Place key in standard location: ~/.ssh/{}.pem or ~/.ssh/{}\n\
                  3. Set correct permissions: chmod 600 ~/.ssh/{}.pem\n\
                  4. Use SSM instead (if IAM role configured): runctl aws train {} --sync-code false",
                key_name_str, key_name_str, key_name_str, key_name_str, key_name_str, options.instance_id
            ))
        })?;

    // Determine user based on AMI
    let user = if instance
        .image_id()
        .map(|id| id.contains("ubuntu") || id.contains("Ubuntu"))
        .unwrap_or(false)
    {
        "ubuntu"
    } else {
        "ec2-user"
    };

    let project_dir = format!("/home/{}/{}", user, options.project_name);

    // Sync code if requested
    if options.sync_code {
        if output_format != "json" {
            println!("Syncing code to instance...");
        }
        if let Err(e) = sync_code_to_instance(
            &key_path,
            public_ip,
            user,
            &project_dir,
            &options.script,
            output_format,
            &options.include_patterns,
        )
        .await
        {
            if output_format != "json" {
                return Err(TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!(
                        "Code sync failed: {}\n\n\
                        To resolve:\n\
                          1. Check SSH key permissions: chmod 600 {}\n\
                          2. Verify instance is accessible: ssh -i {} {}@{}\n\
                          3. Check network connectivity and security groups\n\
                          4. Try syncing manually: rsync -avz -e 'ssh -i {}' ./ {}@{}:{}/",
                        e,
                        key_path,
                        key_path,
                        user,
                        public_ip,
                        key_path,
                        user,
                        public_ip,
                        project_dir
                    ),
                    source: None,
                });
            } else {
                return Err(e);
            }
        } else if output_format != "json" {
            println!("   Code synced successfully");
        }
    }

    // Pre-load data from S3 if provided (critical for performance)
    if let Some(data_s3) = &options.data_s3 {
        if output_format != "json" {
            println!("Pre-loading data from S3 (this may take a while)...");
        }

        // Check if SSM is available (same check as used later for training command)
        let use_ssm = instance.iam_instance_profile().is_some();

        // Determine data directory (prefer EBS volume if mounted, else use project data dir)
        let data_dir = format!("/mnt/data/{}", options.project_name);
        let data_dir_alt = format!("{}/data", project_dir);

        // Use s5cmd for parallel downloads (much faster than aws s3 cp)
        let download_cmd = format!(
            r#"
# Pre-load data from S3 using s5cmd (parallel, fast) or aws s3 cp (fallback)
DATA_DIR="{data_dir}"
if [ ! -d "$DATA_DIR" ]; then
    DATA_DIR="{data_dir_alt}"
    mkdir -p "$DATA_DIR"
fi

echo "Downloading data from {data_s3} to $DATA_DIR..."

# Try s5cmd first (much faster with parallel downloads)
if command -v s5cmd &> /dev/null; then
    echo "Using s5cmd for parallel download..."
    s5cmd cp --recursive --concurrency 10 "{data_s3}/*" "$DATA_DIR/" 2>&1 || {{
        echo "s5cmd failed, trying aws s3 cp..."
        aws s3 cp "{data_s3}" "$DATA_DIR/" --recursive 2>&1
    }}
else
    echo "s5cmd not found, using aws s3 cp..."
    aws s3 cp "{data_s3}" "$DATA_DIR/" --recursive 2>&1
fi

if [ $? -eq 0 ]; then
    echo "Data pre-loaded successfully to $DATA_DIR"
    echo "DATA_DIR=$DATA_DIR" > {project_dir}/.data_path
else
    echo "WARNING: Data download failed, training may be slow"
    echo "DATA_DIR={data_s3}" > {project_dir}/.data_path
fi
"#,
            data_dir = data_dir,
            data_dir_alt = data_dir_alt,
            data_s3 = data_s3,
            project_dir = project_dir
        );

        // Execute data download via SSM (preferred) or SSH (fallback)
        let download_result: Result<String> = if use_ssm {
            execute_ssm_command(&ssm_client, &options.instance_id, &download_cmd)
                .await
                .map_err(|e| TrainctlError::Ssm(format!("SSM execution failed: {}", e)))
        } else {
            // Fallback to SSH - execute_via_ssh returns Result<()>, so we convert to Result<String>
            execute_via_ssh(&key_path, public_ip, user, &download_cmd)
                .await
                .map(|_| "Data download initiated".to_string())
        };

        match download_result {
            Ok(_) => {
                if output_format != "json" {
                    println!("Data pre-loading initiated (downloading in background)");
                }
            }
            Err(e) => {
                if output_format != "json" {
                    println!("WARNING: Data pre-loading failed: {}", e);
                    println!(
                        "   Training will proceed but may be slow if data is accessed on-demand"
                    );
                }
            }
        }
    }

    // Build training command
    // Convert script path to module path relative to project root
    // e.g., /path/to/matryoshka-box/training/train_lightning.py -> training.train_lightning
    let script_str = options.script.to_string_lossy().to_string();
    let script_module = if script_str.ends_with(".py") {
        // Find project root by looking for common markers
        let mut current = options.script.parent();
        let project_root = loop {
            if let Some(dir) = current {
                let markers = ["requirements.txt", "setup.py", "pyproject.toml", ".git"];
                if markers.iter().any(|m| dir.join(m).exists()) {
                    break Some(dir);
                }
                current = dir.parent();
            } else {
                break None;
            }
        };

        // Get relative path from project root
        if let Some(root) = project_root {
            options
                .script
                .strip_prefix(root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|p| p.strip_suffix(".py").unwrap_or(p).replace(['/', '\\'], "."))
                .unwrap_or_else(|| {
                    // Fallback: use filename without extension
                    options
                        .script
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("train_lightning")
                        .to_string()
                })
        } else {
            // Fallback: use filename without extension
            options
                .script
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("train_lightning")
                .to_string()
        }
    } else {
        options
            .script
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("train_lightning")
            .to_string()
    };

    let args_str = if options.script_args.is_empty() {
        String::new()
    } else {
        format!(" {}", options.script_args.join(" "))
    };

    // Create training command
    let command = format!(
        r#"
cd {project_dir}
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
export PYTHONPATH="$HOME/.local/lib/python3.9/site-packages:{project_dir}:$PYTHONPATH"

# Install dependencies if needed
if [ -f requirements.txt ]; then
    # Check if torch is installed (indicator of ML dependencies)
    if ! python3 -c "import torch" 2>/dev/null; then
        echo "Installing dependencies from requirements.txt..."
        if command -v uv &> /dev/null; then
            uv pip install --system -r requirements.txt 2>&1 || python3 -m pip install --user -r requirements.txt 2>&1 || pip3 install --user -r requirements.txt 2>&1
        else
            # Try python3 -m pip first (works on Amazon Linux)
            python3 -m pip install --user -r requirements.txt 2>&1 || pip3 install --user -r requirements.txt 2>&1 || (curl -sS https://bootstrap.pypa.io/get-pip.py | python3 && python3 -m pip install --user -r requirements.txt 2>&1)
        fi
        echo "Dependencies installed"
    else
        echo "Dependencies already installed"
    fi
fi

# Run training
echo "Starting training..."
python3 -m {script_module}{args} > training.log 2>&1 &
TRAIN_PID=$!
echo $TRAIN_PID > training.pid
echo "Training started (PID: $TRAIN_PID)"
echo "Monitor with: tail -f {project_dir}/training.log"
"#,
        project_dir = project_dir,
        script_module = script_module,
        args = args_str
    );

    // Try SSM first (requires IAM role), fallback to SSH
    let use_ssm = instance.iam_instance_profile().is_some();

    let training_info = if use_ssm {
        match execute_ssm_command(&ssm_client, &options.instance_id, &command).await {
            Ok(_) => TrainingInfo {
                success: true,
                method: "ssm".to_string(),
                instance_id: options.instance_id.clone(),
                log_path: format!("{}/training.log", project_dir),
                monitor_command: format!("runctl aws monitor {}", options.instance_id),
            },
            Err(e) => {
                if output_format != "json" {
                    println!("WARNING: SSM failed: {}, trying SSH...", e);
                }
                // Fallback to SSH
                execute_via_ssh(&key_path, public_ip, user, &command).await?;
                TrainingInfo {
                    success: true,
                    method: "ssh".to_string(),
                    instance_id: options.instance_id.clone(),
                    log_path: format!("{}/training.log", project_dir),
                    monitor_command: format!("runctl aws monitor {}", options.instance_id),
                }
            }
        }
    } else {
        // Fallback to SSH
        execute_via_ssh(&key_path, public_ip, user, &command).await?;
        TrainingInfo {
            success: true,
            method: "ssh".to_string(),
            instance_id: options.instance_id.clone(),
            log_path: format!("{}/training.log", project_dir),
            monitor_command: format!("runctl aws monitor {}", options.instance_id),
        }
    };

    if output_format == "json" {
        println!("{}", serde_json::to_string_pretty(&training_info)?);
    } else {
        println!("Training started");
        println!(
            "   Monitor: ssh -i {} {}@{} 'tail -f {}/training.log'",
            key_path, user, public_ip, project_dir
        );
        println!("   Or: runctl aws monitor {}", options.instance_id);
    }

    Ok(())
}

/// Sync code to instance using native Rust SSH and tar
///
/// Uses incremental sync if code already exists, full sync otherwise.
async fn sync_code_to_instance(
    key_path: &str,
    ip: &str,
    user: &str,
    project_dir: &str,
    script_path: &std::path::Path,
    output_format: &str,
    include_patterns: &[String],
) -> Result<()> {
    // Get project root (parent of script's directory)
    let script_dir = script_path
        .parent()
        .ok_or_else(|| TrainctlError::Aws("Script has no parent directory: {}".to_string()))?;

    // Find project root (look for requirements.txt, setup.py, pyproject.toml, etc.)
    let mut current = script_dir;
    let project_root = loop {
        let markers = [
            "requirements.txt",
            "setup.py",
            "pyproject.toml",
            "Cargo.toml",
            ".git",
        ];
        if markers.iter().any(|m| current.join(m).exists()) {
            break current;
        }
        match current.parent() {
            Some(p) => current = p,
            None => break script_dir, // Fallback to script directory
        }
    };

    if output_format != "json" {
        println!("   Syncing from: {}", project_root.display());
    }

    // Use native Rust SSH sync
    crate::ssh_sync::sync_code_native(
        key_path,
        ip,
        user,
        project_dir,
        project_root,
        output_format,
        include_patterns,
    )
    .await
    .map_err(|e| {
        TrainctlError::DataTransfer(format!(
            "Native code sync failed: {}\n\n\
            To resolve:\n\
              1. Check SSH key permissions: chmod 600 {}\n\
              2. Verify instance is accessible: ssh -i {} {}@{}\n\
              3. Check network connectivity and security groups\n\
              4. Ensure instance has sufficient disk space\n\
              5. Fallback: Use shell-based sync by setting TRAINCTL_USE_SHELL_SYNC=1",
            e, key_path, key_path, user, ip
        ))
    })
}

// execute_via_ssm removed - use crate::aws_utils::execute_ssm_command instead

/// Execute command via SSH
async fn execute_via_ssh(key_path: &str, ip: &str, user: &str, command: &str) -> Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("ssh");
    cmd.arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("ConnectTimeout=10")
        .arg("-i")
        .arg(key_path)
        .arg(format!("{}@{}", user, ip))
        .arg(command);

    let output = cmd
        .output()
        .map_err(|e| TrainctlError::Aws(format!("Failed to execute SSH command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!("SSH command failed: {}", stderr),
            source: None,
        });
    }

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

async fn monitor_instance(
    instance_id: String,
    _follow: bool,
    _aws_config: &aws_config::SdkConfig,
    _output_format: &str,
) -> Result<()> {
    // Get command output via SSM
    // Simplified - would need to track command ID
    println!("Monitoring instance: {} (follow={})", instance_id, _follow);
    println!("Use AWS Console or SSM Session Manager to view logs");

    Ok(())
}

async fn terminate_instance(
    instance_id: String,
    force: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    // Check for attached volumes
    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    // Check for attached volumes
    let block_devices = instance.block_device_mappings();
    let has_data_volumes = block_devices.iter().any(|bd| {
        bd.device_name()
            .map(|d| d != "/dev/xvda" && d != "/dev/sda1")
            .unwrap_or(false)
    });

    if has_data_volumes {
        println!(
            "WARNING: Instance {} has attached EBS volumes.",
            instance_id
        );
        println!("Volumes will remain after instance termination.");
        println!(
            "   List volumes: runctl aws ebs list --instance-id {}",
            instance_id
        );
    }

    // Check for running training jobs and resource usage (unless force is used)
    if !force {
        if let Some(_iam_profile) = instance.iam_instance_profile() {
            // Check for high resource usage (warns but doesn't block)
            match check_high_resource_usage(&ssm_client, &instance_id).await {
                Ok(Some(warnings)) => {
                    println!(
                        "WARNING: High resource usage detected on instance {}:",
                        instance_id
                    );
                    println!("{}", warnings);
                    println!("Consider stopping active processes before termination.");
                    println!("Use --force to override and terminate anyway.");
                }
                Ok(None) => {
                    // No high usage, but still check for training processes
                }
                Err(e) => {
                    println!("WARNING: Could not check resource usage: {}", e);
                }
            }

            // Try SSM to check for training processes (blocks termination)
            let check_training_cmd = r#"
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
    else
        echo "TRAINING_STOPPED"
    fi
else
    # Check for common training process names
    if pgrep -f "python.*train\|python.*training\|python.*main.py" > /dev/null; then
        echo "TRAINING_RUNNING:$(pgrep -f 'python.*train\|python.*training\|python.*main.py' | head -1)"
    else
        echo "NO_TRAINING"
    fi
fi
"#;

            // Check for running training jobs using shared SSM utility
            match execute_ssm_command(&ssm_client, &instance_id, check_training_cmd).await {
                Ok(output) => {
                    if output.contains("TRAINING_RUNNING") {
                        println!("ERROR: Training job is running on instance {}", instance_id);
                        println!("Termination blocked to prevent data loss.");
                        println!("Please stop the training job first or use --force to override.");
                        return Err(TrainctlError::CloudProvider {
                            provider: "aws".to_string(),
                            message: "Termination blocked: training job is running".to_string(),
                            source: None,
                        });
                    } else {
                        println!("No training jobs detected, proceeding with termination");
                    }
                }
                Err(e) => {
                    println!("WARNING: Could not check for training jobs: {}", e);
                    println!("Proceeding with termination. Use --force to suppress this warning.");
                }
            }
        }
    } else {
        println!("Force termination enabled, skipping safety checks.");
    }

    client
        .terminate_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to terminate instance: {}", e)))?;

    let state = "terminating".to_string();

    if output_format == "json" {
        let result = TerminateInstanceResult {
            success: true,
            instance_id: instance_id.clone(),
            state: state.clone(),
            has_data_volumes,
            message: format!("Instance {} termination requested", instance_id),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Instance termination requested: {}", instance_id);
    }

    Ok(())
}

async fn stop_instance(
    instance_id: String,
    force: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);

    // Check instance state
    let instance_response = client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = instance_response
        .reservations()
        .iter()
        .flat_map(|r| r.instances())
        .find(|i| i.instance_id().map(|id| id == instance_id).unwrap_or(false))
        .ok_or_else(|| TrainctlError::Aws(format!("Instance not found: {}", instance_id)))?;

    let state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    if state == "stopped" || state == "stopping" {
        println!("Instance {} is already stopped or stopping", instance_id);
        return Ok(());
    }

    if state != "running" {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!(
                "Instance {} is in state '{}', cannot stop",
                instance_id, state
            ),
            source: None,
        });
    }

    // Graceful shutdown: save checkpoints and stop training cleanly
    if !force {
        if let Some(_iam_profile) = instance.iam_instance_profile() {
            let graceful_stop_cmd = r#"
# Check for training and gracefully stop
if [ -f training.pid ]; then
    PID=$(cat training.pid 2>/dev/null)
    if ps -p $PID > /dev/null 2>&1; then
        echo "TRAINING_RUNNING:$PID"
        # Send SIGTERM for graceful shutdown
        kill -TERM $PID 2>/dev/null || true
        # Wait up to 30 seconds for graceful shutdown
        for i in {1..30}; do
            if ! ps -p $PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                exit 0
            fi
            sleep 1
        done
        # Force kill if still running
        kill -9 $PID 2>/dev/null || true
        echo "TRAINING_FORCE_STOPPED"
    else
        echo "TRAINING_STOPPED"
    fi
else
    # Check for training processes
    TRAINING_PID=$(pgrep -f "python.*train\|python.*training\|python.*main.py" | head -1)
    if [ -n "$TRAINING_PID" ]; then
        echo "TRAINING_RUNNING:$TRAINING_PID"
        # Send SIGTERM for graceful shutdown
        kill -TERM $TRAINING_PID 2>/dev/null || true
        # Wait up to 30 seconds
        for i in {1..30}; do
            if ! ps -p $TRAINING_PID > /dev/null 2>&1; then
                echo "TRAINING_STOPPED_GRACEFULLY"
                exit 0
            fi
            sleep 1
        done
        # Force kill if still running
        kill -9 $TRAINING_PID 2>/dev/null || true
        echo "TRAINING_FORCE_STOPPED"
    else
        echo "NO_TRAINING"
    fi
fi
"#;

            match execute_ssm_command(&ssm_client, &instance_id, graceful_stop_cmd).await {
                Ok(output) => {
                    if output.contains("TRAINING_RUNNING") {
                        println!(
                            "Training detected on instance {}, attempting graceful shutdown...",
                            instance_id
                        );
                    } else if output.contains("TRAINING_STOPPED_GRACEFULLY") {
                        println!("Training stopped gracefully on instance {}", instance_id);
                    } else if output.contains("TRAINING_FORCE_STOPPED") {
                        println!("WARNING: Training force-stopped on instance {} (graceful shutdown timeout)", instance_id);
                    }
                }
                Err(e) => {
                    println!("WARNING: Could not gracefully stop training: {}", e);
                }
            }
        }
    }

    client
        .stop_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to stop instance: {}", e)))?;

    let state = "stopping".to_string();

    if output_format == "json" {
        let result = StopInstanceResult {
            success: true,
            instance_id: instance_id.clone(),
            state: state.clone(),
            message: format!("Instance {} stop requested", instance_id),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Instance stop requested: {}", instance_id);
        println!(
            "Instance can be restarted with: runctl aws start {}",
            instance_id
        );
    }

    Ok(())
}

async fn show_processes(
    instance_id: String,
    detailed: bool,
    watch: bool,
    interval: u64,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    use crate::diagnostics::get_instance_resource_usage;
    // Table formatting removed - using plain text output
    use std::io::{self, Write};

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
