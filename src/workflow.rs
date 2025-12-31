//! Workflow commands for complete training workflows
//!
//! High-level commands that orchestrate multiple operations to provide
//! a streamlined developer experience.

use crate::aws::{
    create_instance_and_get_id, get_project_name, train_on_instance, CreateInstanceOptions,
    TrainInstanceOptions,
};
use crate::config::Config;
use crate::error::Result;
use aws_config::BehaviorVersion;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand, Clone)]
pub enum WorkflowCommands {
    /// Complete training workflow: create instance → train → wait → verify
    ///
    /// This command orchestrates the complete training workflow:
    /// 1. Creates an EC2 instance (with --wait to ensure it's ready)
    /// 2. Syncs code and starts training
    /// 3. Waits for training to complete
    /// 4. Verifies training succeeded
    ///
    /// Examples:
    ///   runctl workflow train training/train.py --instance-type g4dn.xlarge
    ///   runctl workflow train training/train.py --instance-type t3.micro --spot
    Train {
        /// Training script path
        #[arg(value_name = "SCRIPT")]
        script: PathBuf,

        /// EC2 instance type (e.g., t3.medium, g4dn.xlarge)
        #[arg(long, value_name = "INSTANCE_TYPE")]
        instance_type: String,

        /// Use spot instance (cheaper, can be interrupted)
        #[arg(long)]
        spot: bool,

        /// Additional arguments to pass to training script
        #[arg(last = true, value_name = "ARGS")]
        script_args: Vec<String>,
    },
}

pub async fn handle_command(
    cmd: WorkflowCommands,
    config: &Config,
    output_format: &str,
) -> Result<()> {
    match cmd {
        WorkflowCommands::Train {
            script,
            instance_type,
            spot,
            script_args,
        } => {
            let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

            // Step 1: Create instance with --wait
            if output_format != "json" {
                println!("Step 1: Creating instance...");
            }

            let create_options = CreateInstanceOptions {
                instance_type: instance_type.clone(),
                use_spot: spot,
                spot_max_price: None,
                no_fallback: false,
                key_name: None,
                security_group: None,
                ami_id: None,
                root_volume_size: None,
                data_volume_size: None,
                project_name: get_project_name(None, config),
                iam_instance_profile: None, // TODO: Get from config
                wait: true,                 // Always wait for instance to be ready
            };

            // Create instance and get instance ID
            let instance_id =
                create_instance_and_get_id(create_options, config, &aws_config).await?;

            if output_format != "json" {
                println!("Created instance: {}", instance_id);
            }

            if output_format != "json" {
                println!("Step 2: Starting training...");
            }

            // Step 2: Train with --wait
            let train_options = TrainInstanceOptions {
                instance_id: instance_id.clone(),
                script,
                data_s3: None,
                output_s3: None,
                sync_code: true,
                include_patterns: vec![],
                project_name: get_project_name(None, config),
                script_args,
                wait: true, // Always wait for training to complete
            };

            train_on_instance(train_options, config, &aws_config, output_format).await?;

            if output_format != "json" {
                println!("Step 3: Training completed successfully!");
                println!("Instance: {}", instance_id);
                println!("  To terminate: runctl aws terminate {}", instance_id);
            }

            Ok(())
        }
    }
}
