use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use clap::Subcommand;
use std::path::PathBuf;
use crate::config::Config;
use tracing::info;

#[derive(Subcommand, Clone)]
pub enum AwsCommands {
    Create {
        instance_type: String,
        spot: bool,
        spot_max_price: Option<String>,
        no_fallback: bool,
    },
    Train {
        instance_id: String,
        script: PathBuf,
        data_s3: Option<String>,
        _output_s3: Option<String>,
    },
    Monitor {
        instance_id: String,
        follow: bool,
    },
    Terminate {
        instance_id: String,
    },
}

pub async fn handle_command(cmd: AwsCommands, config: &Config) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    
    match cmd {
        AwsCommands::Create { instance_type, spot, spot_max_price, no_fallback } => {
            create_instance(instance_type, spot, spot_max_price, no_fallback, config, &aws_config).await
        }
        AwsCommands::Train { instance_id, script, data_s3, _output_s3 } => {
            train_on_instance(instance_id, script, data_s3, _output_s3, &aws_config).await
        }
        AwsCommands::Monitor { instance_id, follow } => {
            monitor_instance(instance_id, follow, &aws_config).await
        }
        AwsCommands::Terminate { instance_id } => {
            terminate_instance(instance_id, &aws_config).await
        }
    }
}

async fn create_instance(
    instance_type: String,
    use_spot: bool,
    spot_max_price: Option<String>,
    no_fallback: bool,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let aws_cfg = config.aws.as_ref()
        .context("AWS config not found")?;

    let client = Ec2Client::new(aws_config);

    info!("Creating EC2 instance: type={}, spot={}", instance_type, use_spot);

    // User data script for EC2 initialization
    let user_data = r#"#!/bin/bash
yum update -y
yum install -y python3.11 python3.11-pip git
pip3.11 install uv
"#;

    // Try spot instance first if requested
    if use_spot {
        match create_spot_instance(&client, &instance_type, &aws_cfg.default_ami, user_data, spot_max_price.as_deref()).await {
            Ok(instance_id) => {
                println!("✅ Created spot instance: {}", instance_id);
                return Ok(());
            }
            Err(e) if !no_fallback => {
                println!("⚠️  Spot instance failed: {}", e);
                println!("🔄 Falling back to on-demand...");
            }
            Err(e) => {
                anyhow::bail!("Spot instance failed and no fallback: {}", e);
            }
        }
    }

    // Create on-demand instance
    let instance_id = create_ondemand_instance(&client, &instance_type, &aws_cfg.default_ami, user_data).await?;
    println!("✅ Created on-demand instance: {}", instance_id);

    Ok(())
}

async fn create_spot_instance(
    _client: &Ec2Client,
    _instance_type: &str,
    _ami_id: &str,
    _user_data: &str,
    _max_price: Option<&str>,
) -> Result<String> {
    // Implementation would use EC2 RunInstances with SpotOptions
    // Simplified for now
    anyhow::bail!("Spot instance creation not yet implemented")
}

async fn create_ondemand_instance(
    _client: &Ec2Client,
    _instance_type: &str,
    _ami_id: &str,
    _user_data: &str,
) -> Result<String> {
    // Implementation would use EC2 RunInstances
    // Simplified for now
    anyhow::bail!("Instance creation not yet implemented")
}

async fn train_on_instance(
    instance_id: String,
    script: PathBuf,
    _data_s3: Option<String>,
    _output_s3: Option<String>,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let client = SsmClient::new(aws_config);

    info!("Starting training on instance: {}", instance_id);

    // Upload script to instance via S3 or inline
    let script_content = std::fs::read_to_string(&script)
        .context("Failed to read script")?;

    // Create SSM command
    let command = format!(
        r#"
cd /tmp
cat > training_script.sh << 'EOF'
{}
EOF
chmod +x training_script.sh
./training_script.sh
"#,
        script_content
    );

    // AWS SDK v1 SSM send_command API
    // The parameters method expects Vec<String>
    let response = client
        .send_command()
        .instance_ids(&instance_id)
        .document_name("AWS-RunShellScript")
        .parameters("commands", vec![command])
        .send()
        .await
        .context("Failed to send SSM command")?;

    let command_id = response.command()
        .and_then(|c| c.command_id())
        .context("No command ID in response")?
        .to_string();
    
    println!("✅ Training started (command ID: {})", command_id);
    println!("   Monitoring progress...");

    // Poll for completion
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

        let status = client
            .get_command_invocation()
            .command_id(&command_id)
            .instance_id(&instance_id)
            .send()
            .await?;

        let status_str = status.status().unwrap().as_str();
        match status_str {
            "Success" => {
                println!("✅ Training completed successfully");
                break;
            }
            "Failed" => {
                anyhow::bail!("Training failed: {}", 
                    status.standard_error_content().unwrap_or("Unknown error"));
            }
            _ => {
                // Still running
                continue;
            }
        }
    }

    Ok(())
}

async fn monitor_instance(
    instance_id: String,
    _follow: bool,
    _aws_config: &aws_config::SdkConfig,
) -> Result<()> {

    // Get command output via SSM
    // Simplified - would need to track command ID
    println!("Monitoring instance: {} (follow={})", instance_id, _follow);
    println!("Use AWS Console or SSM Session Manager to view logs");

    Ok(())
}

async fn terminate_instance(
    instance_id: String,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let client = Ec2Client::new(aws_config);

    client
        .terminate_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .context("Failed to terminate instance")?;

    println!("✅ Instance termination requested: {}", instance_id);
    Ok(())
}

