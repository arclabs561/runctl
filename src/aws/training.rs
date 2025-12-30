//! Training operations on EC2 instances
//!
//! Handles starting training jobs, syncing code, and monitoring training progress.

use crate::aws::ssm_sync::sync_code_via_ssm;
use crate::aws::types::{TrainInstanceOptions, TrainingInfo};
use crate::aws_utils::execute_ssm_command;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use tracing::info;

/// Start training on an instance
pub async fn train_on_instance(
    options: TrainInstanceOptions,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let ec2_client = Ec2Client::new(aws_config);
    let ssm_client = SsmClient::new(aws_config);
    let s3_client = S3Client::new(aws_config);

    info!("Starting training on instance: {}", options.instance_id);

    // Get instance details
    let instance_response = ec2_client
        .describe_instances()
        .instance_ids(&options.instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance =
        crate::aws::helpers::find_instance_in_response(&instance_response, &options.instance_id)
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

    // Determine if we should use SSM (check before requiring SSH key)
    let use_ssm_for_sync = instance.iam_instance_profile().is_some()
        && config
            .aws
            .as_ref()
            .and_then(|c| c.s3_bucket.as_ref())
            .is_some();

    // Only require public IP and SSH key if not using SSM
    let (public_ip, key_path) = if !use_ssm_for_sync {
        let ip = instance.public_ip_address().ok_or_else(|| {
            TrainctlError::Aws(format!(
                "Instance {} has no public IP address.\n\n\
                To resolve:\n\
                  1. Check if instance is in a public subnet with internet gateway\n\
                  2. Verify security groups allow SSH (port 22)\n\
                  3. Check instance state: runctl aws processes {}\n\
                  4. Use SSM instead: Create instance with --iam-instance-profile and configure s3_bucket in config",
                options.instance_id, options.instance_id
            ))
        })?;

        let key_name = instance.key_name();
        let key = key_name
            .and_then(|k| {
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
                      4. Use SSM instead: Create instance with --iam-instance-profile and configure s3_bucket in config",
                    key_name_str, key_name_str, key_name_str, key_name_str, key_name_str
                ))
            })?;
        (Some(ip), Some(key))
    } else {
        (instance.public_ip_address(), None)
    };

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

    // use_ssm_for_sync already determined above

    // Sync code if requested
    if options.sync_code {
        if output_format != "json" {
            println!("Syncing code to instance...");
        }

        // Get project root for syncing
        let script_dir = options
            .script
            .parent()
            .ok_or_else(|| TrainctlError::Aws("Script has no parent directory".to_string()))?;

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
                None => break script_dir,
            }
        };

        if use_ssm_for_sync {
            // Use SSM-based sync (via S3)
            if let Err(e) = sync_code_via_ssm(crate::aws::ssm_sync::SsmSyncOptions {
                project_root,
                instance_id: &options.instance_id,
                project_dir: &project_dir,
                script_path: &options.script,
                include_patterns: &options.include_patterns,
                s3_client: &s3_client,
                ssm_client: &ssm_client,
                config,
                output_format,
            })
            .await
            {
                if output_format != "json" {
                    return Err(TrainctlError::CloudProvider {
                        provider: "aws".to_string(),
                        message: format!(
                            "SSM code sync failed: {}\n\n\
                            To resolve:\n\
                              1. Verify S3 bucket is configured in config: aws.s3_bucket\n\
                              2. Check instance has IAM role with S3 access\n\
                              3. Verify SSM connectivity: aws ssm describe-instance-information --instance-ids {}\n\
                              4. Fallback: Use SSH by providing --key-name when creating instance",
                            e, options.instance_id
                        ),
                        source: None,
                    });
                } else {
                    return Err(e);
                }
            }
        } else {
            // Use SSH-based sync (fallback)
            let kp = key_path.as_ref().ok_or_else(|| {
                TrainctlError::Aws("SSH key required for SSH-based code sync".to_string())
            })?;
            let ip = public_ip.as_ref().ok_or_else(|| {
                TrainctlError::Aws("Public IP required for SSH-based code sync".to_string())
            })?;

            if let Err(e) = sync_code_to_instance(
                kp,
                ip,
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
                              4. Ensure instance has sufficient disk space\n\
                              5. Use SSM instead: Create instance with --iam-instance-profile and configure s3_bucket in config",
                            e, kp, kp, user, ip
                        ),
                        source: None,
                    });
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Build training command
    let script_name = options
        .script
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("train.py");
    let script_path = format!("{}/{}", project_dir, script_name);

    let mut command = format!(
        "cd {} && nohup python3 {} > training.log 2>&1 & echo $! > training.pid",
        project_dir, script_path
    );

    // Add script arguments if provided
    if !options.script_args.is_empty() {
        let args_str = options.script_args.join(" ");
        command = format!("{} {}", command, args_str);
    }

    // Try SSM first (more secure, no SSH keys needed)
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
                // Fallback to SSH (if available)
                if let (Some(kp), Some(ip)) = (&key_path, &public_ip) {
                    execute_via_ssh(kp, ip, user, &command).await?;
                    TrainingInfo {
                        success: true,
                        method: "ssh".to_string(),
                        instance_id: options.instance_id.clone(),
                        log_path: format!("{}/training.log", project_dir),
                        monitor_command: format!("runctl aws monitor {}", options.instance_id),
                    }
                } else {
                    return Err(TrainctlError::Aws(format!(
                        "SSM command failed and SSH fallback not available (no key/IP).\n\
                        SSM error: {}\n\n\
                        To resolve:\n\
                          1. Check SSM connectivity: aws ssm describe-instance-information --instance-ids {}\n\
                          2. Verify IAM role has SSM permissions\n\
                          3. Or provide SSH key when creating instance",
                        e, options.instance_id
                    )));
                }
            }
        }
    } else {
        // Use SSH (required when SSM not available)
        let kp = key_path.as_ref().ok_or_else(|| {
            TrainctlError::Aws("SSH key required when SSM is not available".to_string())
        })?;
        let ip = public_ip
            .as_ref()
            .ok_or_else(|| TrainctlError::Aws("Public IP required for SSH".to_string()))?;

        execute_via_ssh(kp, ip, user, &command).await?;
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
        if let (Some(kp), Some(ip)) = (key_path, public_ip) {
            println!(
                "   Monitor: ssh -i {} {}@{} 'tail -f {}/training.log'",
                kp, user, ip, project_dir
            );
        }
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

/// Monitor training progress on an instance
pub async fn monitor_instance(
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
