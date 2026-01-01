//! Training operations on EC2 instances
//!
//! Handles starting training jobs, syncing code, and monitoring training progress.

// Use fully qualified path for spot_monitor to minimize circular dependency risk
use crate::aws::ssm_sync::sync_code_via_ssm;
use crate::aws::types::{TrainInstanceOptions, TrainingInfo};
use crate::aws_utils::execute_ssm_command;
use crate::config::Config;
use crate::docker::{detect_dockerfile, run_training_in_container};
use crate::error::{Result, TrainctlError};
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use std::time::Duration;
use tracing::{info, warn};

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

    // Validate instance state before proceeding
    let instance_state = instance
        .state()
        .and_then(|s| s.name())
        .map(|s| s.as_str())
        .unwrap_or("unknown");

    match instance_state {
        "stopped" | "stopping" => {
            return Err(TrainctlError::Aws(format!(
                "Instance {} is in '{}' state. Cannot start training on stopped instance.\n\n\
                To resolve:\n\
                  1. Start the instance: runctl aws start {}\n\
                  2. Wait for instance to be running: runctl aws wait {}\n\
                  3. Then retry training",
                options.instance_id, instance_state, options.instance_id, options.instance_id
            )));
        }
        "terminated" | "shutting-down" => {
            return Err(TrainctlError::Aws(format!(
                "Instance {} is {} and cannot be used for training.\n\n\
                To resolve:\n\
                  1. Create a new instance: runctl aws create\n\
                  2. Use a different instance ID",
                options.instance_id, instance_state
            )));
        }
        "pending" => {
            if output_format != "json" {
                println!("Instance is still starting. Waiting for it to be ready...");
            }
            // Wait a bit for instance to become running
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
        "running" => {
            // Good state, proceed
        }
        _ => {
            warn!(
                "Instance {} is in unexpected state: {}",
                options.instance_id, instance_state
            );
            if output_format != "json" {
                println!(
                    "Warning: Instance is in '{}' state. Proceeding anyway...",
                    instance_state
                );
            }
        }
    }

    // Determine if we should use SSM (check before requiring SSH key)
    let has_iam_profile = instance.iam_instance_profile().is_some();
    let has_s3_bucket = config
        .aws
        .as_ref()
        .and_then(|c| c.s3_bucket.as_ref())
        .is_some();
    let use_ssm_for_sync = has_iam_profile && has_s3_bucket;

    // If instance has IAM profile but no S3 bucket configured, provide helpful error
    if has_iam_profile && !has_s3_bucket {
        return Err(TrainctlError::Aws(
            "Instance has IAM profile (SSM available) but S3 bucket not configured.\n\n\
            SSM-based code sync requires an S3 bucket for temporary storage.\n\n\
            To resolve:\n\
              1. Add S3 bucket to .runctl.toml:\n\
                 [aws]\n\
                 s3_bucket = \"your-bucket-name\"\n\n\
              2. Or use SSH fallback:\n\
                 Create instance with --key-name instead of --iam-instance-profile\n\n\
            Note: You can use any existing S3 bucket. The bucket is only used for temporary\n\
            code transfer during sync and files are automatically cleaned up."
                .to_string(),
        ));
    }

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
                      4. Use SSM instead (recommended):\n\
                         a. Setup SSM (one-time): ./scripts/setup-ssm-role.sh\n\
                         b. Create instance with: --iam-instance-profile runctl-ssm-profile\n\
                         c. Configure S3 bucket in .runctl.toml: [aws] s3_bucket = \"your-bucket\"",
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

    // Validate script path exists before starting training
    let script_path = options.script.as_path().to_string_lossy();
    let validate_script_cmd = format!(
        "if [ -f {}/{} ]; then echo 'SCRIPT_EXISTS'; else echo 'SCRIPT_NOT_FOUND'; fi",
        project_dir, script_path
    );

    if use_ssm_for_sync {
        match crate::aws_utils::execute_ssm_command(
            &ssm_client,
            &options.instance_id,
            &validate_script_cmd,
        )
        .await
        {
            Ok(output) => {
                if output.trim() == "SCRIPT_NOT_FOUND" {
                    return Err(TrainctlError::Aws(format!(
                        "Training script not found: {}/{}\n\n\
                            The script may not have been synced to the instance yet.\n\
                            Try:\n\
                              1. Ensure --sync-code is enabled (default)\n\
                              2. Check script path: {}\n\
                              3. Verify project directory: {}",
                        project_dir,
                        script_path,
                        options.script.display(),
                        project_dir
                    )));
                }
            }
            Err(e) => {
                warn!(
                    "Could not validate script path (SSM error): {}. Proceeding anyway.",
                    e
                );
            }
        }
    }

    // Check if training is already running on this instance (prevent concurrent training)
    if use_ssm_for_sync {
        let check_training_cmd = format!(
            "if [ -f {}/training.pid ]; then \
             PID=$(cat {}/training.pid 2>/dev/null); \
             if ps -p $PID > /dev/null 2>&1; then \
                 echo 'TRAINING_RUNNING:$PID'; \
             else \
                 echo 'NO_TRAINING'; \
             fi; \
             else \
             echo 'NO_TRAINING'; \
             fi",
            project_dir, project_dir
        );

        match crate::aws_utils::execute_ssm_command(
            &ssm_client,
            &options.instance_id,
            &check_training_cmd,
        )
        .await
        {
            Ok(output) => {
                if output.contains("TRAINING_RUNNING") {
                    let pid = output
                        .lines()
                        .find(|l| l.starts_with("TRAINING_RUNNING:"))
                        .and_then(|l| l.strip_prefix("TRAINING_RUNNING:"))
                        .unwrap_or("unknown");

                    return Err(TrainctlError::Aws(format!(
                        "Training already running on instance {} (PID: {}).\n\n\
                        To start new training, either:\n\
                          1. Wait for current training to complete: runctl aws monitor {}\n\
                          2. Stop current training gracefully: runctl aws stop {}\n\
                          3. Check training status: runctl aws monitor {} --follow\n\
                          4. Force kill existing training (not recommended): runctl aws stop {} --force",
                        options.instance_id, pid, options.instance_id, options.instance_id, options.instance_id, options.instance_id
                    )));
                }
            }
            Err(_) => {
                // If check fails, proceed (might be first training or SSM issue)
                // Don't block training if we can't check
            }
        }
    }

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

        // Resolve symlinks to get canonical path (handles symlinked directories)
        let mut current = script_dir;
        let canonical_current = current
            .canonicalize()
            .unwrap_or_else(|_| current.to_path_buf());
        current = canonical_current.as_path();

        let project_root = loop {
            let markers = [
                "requirements.txt",
                "setup.py",
                "pyproject.toml",
                "Cargo.toml",
                ".git",
            ];
            // Prioritize .git as most authoritative marker
            if current.join(".git").exists() {
                break current.to_path_buf();
            }
            // Check other markers but continue searching for .git
            if markers.iter().any(|m| current.join(m).exists()) {
                // Found a marker, but continue searching upward for .git
                match current.parent() {
                    Some(p) => {
                        current = p;
                        continue;
                    }
                    None => {
                        // Reached root, return the marker we found
                        break current.to_path_buf();
                    }
                }
            } else {
                match current.parent() {
                    Some(p) => current = p,
                    None => break script_dir.to_path_buf(),
                }
            }
        };

        if use_ssm_for_sync {
            // Use SSM-based sync (via S3)
            if let Err(e) = sync_code_via_ssm(
                &project_root,
                &options.instance_id,
                &project_dir,
                &options.script,
                &options.include_patterns,
                &s3_client,
                &ssm_client,
                config,
                output_format,
            )
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
    // Calculate relative path from project root to script (preserve subdirectory structure)
    // We already found project_root during sync, but need to recalculate here for consistency
    // Use the project_root we found during sync, or detect it from the script path
    let script_path_abs = if options.script.is_absolute() {
        options.script.clone()
    } else {
        // Resolve relative path from current working directory
        std::env::current_dir()
            .map_err(|e| TrainctlError::Aws(format!("Failed to get current directory: {}", e)))?
            .join(&options.script)
    };

    let script_dir = script_path_abs
        .parent()
        .ok_or_else(|| TrainctlError::Aws("Script has no parent directory".to_string()))?;

    // Resolve symlinks to get canonical path (handles symlinked directories)
    let mut current = script_dir;
    // Try to canonicalize, but fall back to original if it fails (e.g., path doesn't exist yet)
    let canonical_current = current
        .canonicalize()
        .unwrap_or_else(|_| current.to_path_buf());
    current = canonical_current.as_path();

    let project_root_for_script = loop {
        let markers = [
            "requirements.txt",
            "setup.py",
            "pyproject.toml",
            "Cargo.toml",
            ".git",
        ];
        // Prioritize .git as most authoritative marker
        if current.join(".git").exists() {
            break current.to_path_buf();
        }
        // Check other markers but continue searching for .git
        if markers.iter().any(|m| current.join(m).exists()) {
            // Found a marker, but continue searching upward for .git
            match current.parent() {
                Some(p) => {
                    current = p;
                    continue;
                }
                None => {
                    // Reached root, return the marker we found
                    break current.to_path_buf();
                }
            }
        } else {
            match current.parent() {
                Some(p) => current = p,
                None => break script_dir.to_path_buf(),
            }
        }
    };

    // Get relative path from project root to script
    // Use the canonical script path for comparison
    let script_relative = script_path_abs
        .strip_prefix(&project_root_for_script)
        .map_err(|_| {
            TrainctlError::Aws(format!(
                "Script {:?} is not under detected project root {:?}.\n\n\
                Detected project root: {}\n\
                Script path (absolute): {}\n\
                Script path (original): {}\n\n\
                To resolve:\n\
                  1. Ensure script is within the project directory\n\
                  2. Or use an absolute path to the script\n\
                  3. Check if project root detection found the wrong directory\n\
                  4. Verify project markers (.git, requirements.txt, etc.) are in the correct location",
                script_path_abs, project_root_for_script,
                project_root_for_script.display(),
                script_path_abs.display(),
                options.script.display()
            ))
        })?;

    let script_path = format!("{}/{}", project_dir, script_relative.display());

    // Build training command with proper error handling
    // Use nohup to run in background and capture output
    // Properly quote/escape script arguments to handle spaces and special characters
    // Save PID to training.pid for process tracking and cleanup
    let script_args_str = if options.script_args.is_empty() {
        String::new()
    } else {
        // Quote each argument to handle spaces and special characters
        // Use single quotes and escape single quotes within arguments
        let quoted_args: Vec<String> = options
            .script_args
            .iter()
            .map(|arg| {
                // Escape single quotes by replacing ' with '\''
                format!("'{}'", arg.replace('\'', "'\"'\"'"))
            })
            .collect();
        format!(" {}", quoted_args.join(" "))
    };

    // Check if requirements.txt exists and install dependencies
    // Determine if we should use SSM for command execution
    let use_ssm = instance.iam_instance_profile().is_some();

    let setup_cmd = format!(
        "cd {} && \
        export PATH=\"$HOME/.local/bin:$PATH\" && \
        if [ -f requirements.txt ]; then \
            echo 'Installing dependencies from requirements.txt...' && \
            if command -v uv >/dev/null 2>&1; then \
                uv pip install -r requirements.txt 2>&1 || (echo 'uv failed, trying python3 -m pip...' && python3 -m pip install --user -r requirements.txt 2>&1); \
            else \
                echo 'uv not found, using python3 -m pip...' && python3 -m pip install --user -r requirements.txt 2>&1; \
            fi && \
            echo 'Dependency installation completed' || echo 'WARNING: Dependency installation may have failed'; \
        fi",
        project_dir
    );

    // Run setup first (best effort - don't fail if it doesn't work)
    if use_ssm {
        if output_format != "json" {
            println!("   Installing dependencies (this may take a few minutes)...");
        }
        if let Err(e) = execute_ssm_command(&ssm_client, &options.instance_id, &setup_cmd).await {
            warn!("Setup command failed (non-critical): {}", e);
        }
    } else if let (Some(kp), Some(ip)) = (key_path.as_ref(), public_ip.as_ref()) {
        if let Err(e) = execute_via_ssh(kp, ip, user, &setup_cmd).await {
            warn!("Setup command failed (non-critical): {}", e);
        }
    }

    // Check if Docker training is requested
    if options.docker {
        if !use_ssm {
            return Err(TrainctlError::Aws(
                "Docker training requires SSM. Use --iam-instance-profile when creating instance."
                    .to_string(),
            ));
        }

        // Determine ECR image
        let ecr_image = if let Some(img) = &options.docker_image {
            img.clone()
        } else {
            // Auto-detect: build and push from Dockerfile
            let project_root = std::env::current_dir().map_err(|e| {
                TrainctlError::Io(std::io::Error::other(format!(
                    "Failed to get current directory: {}",
                    e
                )))
            })?;

            let _dockerfile = detect_dockerfile(&project_root).ok_or_else(|| {
                TrainctlError::CloudProvider {
                    provider: "docker".to_string(),
                    message: "No Dockerfile found. Use --docker-image to specify ECR image, or create a Dockerfile.".to_string(),
                    source: None,
                }
            })?;

            let aws_cfg = config.aws.as_ref().ok_or_else(|| {
                TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
            })?;

            let region = aws_cfg.region.as_str();
            let repository_name = options.project_name.clone();
            let tag = "latest";

            if output_format != "json" {
                println!("Building and pushing Docker image to ECR...");
            }

            crate::docker::build_and_push_to_ecr(
                &project_root,
                &repository_name,
                tag,
                region,
                aws_config,
            )
            .await?
        };

        // Run training in Docker container
        run_training_in_container(
            &options.instance_id,
            &ecr_image,
            &options.script,
            &options.script_args,
            &project_dir,
            &ssm_client,
            Some(&ec2_client),
        )
        .await?;

        if output_format == "json" {
            println!(
                "{{\"success\": true, \"method\": \"docker\", \"ecr_image\": \"{}\"}}",
                ecr_image
            );
        } else {
            println!("Training completed in Docker container: {}", ecr_image);
        }

        // Wait for completion if requested (Docker runs synchronously, so this is a no-op)
        if options.wait && output_format != "json" {
            println!("Training completed in Docker container");
        }

        return Ok(());
    }

    let command = format!(
        "cd {} && \
        export PATH=\"$HOME/.local/bin:$PATH\" && \
        (nohup python3 {}{} > training.log 2>&1; echo $? > training_exit_code.txt) & \
        echo $! > training.pid && \
        sleep 2 && \
        if ps -p $(cat training.pid 2>/dev/null) > /dev/null 2>&1; then \
            echo 'Training started successfully (PID: $(cat training.pid))'; \
        else \
            echo 'WARNING: Training process may have failed - check training.log'; \
        fi",
        project_dir, script_path, script_args_str
    );

    // use_ssm already determined above for dependency installation

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

    // Automatically start spot monitoring if instance is a spot instance
    let is_spot = instance.spot_instance_request_id().is_some();
    if is_spot && use_ssm {
        let checkpoint_dir = format!("{}/checkpoints", project_dir);
        let s3_bucket = config
            .aws
            .as_ref()
            .and_then(|c| c.s3_bucket.as_ref())
            .cloned();
        let s3_prefix = Some("checkpoints/spot-interruptions".to_string());
        let poll_interval = Duration::from_secs(30);
        let graceful_shutdown_timeout = Duration::from_secs(90);
        // Auto-resume is enabled via environment variable
        // It uses process spawning to break circular dependency
        let auto_resume = std::env::var("TRAINCTL_AUTO_RESUME").is_ok();
        let script_path = Some(options.script.clone());

        let instance_id = options.instance_id.clone();
        let ec2_client_clone = ec2_client.clone();
        let ssm_client_clone = ssm_client.clone();
        let s3_client_opt = s3_bucket.as_ref().map(|_| s3_client.clone());
        // Clone configs for the spawned task (Config and SdkConfig implement Clone)
        let config_clone = config.clone();
        let aws_config_clone = aws_config.clone();

        if output_format != "json" {
            println!("   Spot instance detected - starting automatic interruption monitoring...");
        }

        // Spawn background task for spot monitoring
        // Auto-resume uses process spawning to break circular dependency
        tokio::spawn(async move {
            if let Err(e) = crate::aws::spot_monitor::monitor_spot_interruption(
                &instance_id,
                &checkpoint_dir,
                s3_bucket.as_deref(),
                s3_prefix.as_deref(),
                poll_interval,
                graceful_shutdown_timeout,
                &ssm_client_clone,
                &ec2_client_clone,
                s3_client_opt.as_ref(),
                auto_resume,
                script_path,
                Some(&config_clone),
                Some(&aws_config_clone),
            )
            .await
            {
                warn!("Spot monitoring failed for instance {}: {}", instance_id, e);
            }
        });
    }

    // Wait for training to complete if requested
    if options.wait {
        if use_ssm {
            wait_for_training_completion(
                &ssm_client,
                &options.instance_id,
                &project_dir,
                output_format,
                options.timeout_minutes,
            )
            .await?;
        } else {
            return Err(TrainctlError::Aws(
                "Cannot wait for training completion without SSM. Use --iam-instance-profile when creating instance.".to_string()
            ));
        }
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
    // Resolve symlinks to get canonical path (handles symlinked directories)
    let mut current = script_dir;
    let canonical_current = current
        .canonicalize()
        .unwrap_or_else(|_| current.to_path_buf());
    current = canonical_current.as_path();

    let project_root = loop {
        let markers = [
            "requirements.txt",
            "setup.py",
            "pyproject.toml",
            "Cargo.toml",
            ".git",
        ];
        // Prioritize .git as most authoritative marker
        if current.join(".git").exists() {
            break current.to_path_buf();
        }
        // Check other markers but continue searching for .git
        if markers.iter().any(|m| current.join(m).exists()) {
            // Found a marker, but continue searching upward for .git
            match current.parent() {
                Some(p) => {
                    current = p;
                    continue;
                }
                None => {
                    // Reached root, return the marker we found
                    break current.to_path_buf();
                }
            }
        } else {
            match current.parent() {
                Some(p) => current = p,
                None => break script_dir.to_path_buf(), // Fallback to script directory
            }
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
        &project_root,
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
    follow: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let ssm_client = SsmClient::new(aws_config);

    // Get instance details to determine user and project directory
    let ec2_client = Ec2Client::new(aws_config);
    let instance_response = ec2_client
        .describe_instances()
        .instance_ids(&instance_id)
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))?;

    let instance = crate::aws::helpers::find_instance_in_response(&instance_response, &instance_id)
        .ok_or_else(|| TrainctlError::Aws(format!("Instance {} not found", instance_id)))?;

    let user = if instance
        .image_id()
        .map(|id| id.contains("ubuntu") || id.contains("Ubuntu"))
        .unwrap_or(false)
    {
        "ubuntu"
    } else {
        "ec2-user"
    };

    // Try to detect project name from instance tags
    let project_name = instance
        .tags()
        .iter()
        .find(|t| t.key().map(|k| k == "Project").unwrap_or(false))
        .and_then(|t| t.value())
        .unwrap_or("runctl");

    let project_dir = format!("/home/{}/{}", user, project_name);
    let log_path = format!("{}/training.log", project_dir);

    if follow {
        // Poll log file periodically
        if output_format != "json" {
            println!("Monitoring training log: {} (following)", log_path);
            println!("Press Ctrl+C to stop");
        }

        let mut last_size = 0u64;
        loop {
            let cmd = format!(
                "tail -c +{} {} 2>/dev/null || echo ''",
                last_size + 1,
                log_path
            );

            match execute_ssm_command(&ssm_client, &instance_id, &cmd).await {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        if output_format == "json" {
                            let json = serde_json::json!({
                                "instance_id": instance_id,
                                "log_path": log_path,
                                "output": output
                            });
                            println!("{}", serde_json::to_string(&json)?);
                        } else {
                            print!("{}", output);
                            use std::io::Write;
                            std::io::stdout().flush().ok();
                        }
                        last_size += output.len() as u64;
                    }
                }
                Err(e) => {
                    if output_format != "json" {
                        eprintln!("Error reading log: {}", e);
                    } else {
                        return Err(e);
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    } else {
        // Show recent log output
        if output_format != "json" {
            println!("Recent training log from: {}", log_path);
        }
        let cmd = format!(
            "tail -50 {} 2>/dev/null || echo 'Log file not found or empty'",
            log_path
        );

        match execute_ssm_command(&ssm_client, &instance_id, &cmd).await {
            Ok(output) => {
                if output_format == "json" {
                    let json = serde_json::json!({
                        "instance_id": instance_id,
                        "log_path": log_path,
                        "output": output
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                } else {
                    println!("{}", output);
                }
            }
            Err(e) => {
                if output_format != "json" {
                    println!("Could not read log: {}", e);
                    println!("Use AWS Console or SSM Session Manager to view logs");
                } else {
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

/// Check if training has completed
///
/// Uses multiple heuristics:
/// 1. Check for training_complete.txt marker
/// 2. Check if training process (PID) is still running
/// 3. Check training.log for completion indicators
/// 4. Check exit code if available
/// 5. Verify checkpoints were created (optional validation)
async fn check_training_completion(
    ssm_client: &SsmClient,
    instance_id: &str,
    project_dir: &str,
) -> Result<bool> {
    // Method 1: Check for training_complete.txt marker
    // Use atomic check: verify file exists AND is readable (not being written)
    // Also check file size > 0 to avoid false positives from empty files
    let check_marker_cmd = format!(
        "if [ -f {}/training_complete.txt ] && [ -r {}/training_complete.txt ] && [ -s {}/training_complete.txt ]; then \
         echo 'COMPLETE'; \
         else \
         echo 'RUNNING'; \
         fi",
        project_dir, project_dir, project_dir
    );
    match crate::aws_utils::execute_ssm_command(ssm_client, instance_id, &check_marker_cmd).await {
        Ok(output) => {
            if output.trim() == "COMPLETE" {
                info!("Training completion detected via marker file");

                // Verify marker file is stable (not being written) by checking modification time
                // If file was modified < 2 seconds ago, might still be writing
                let verify_stable_cmd = format!(
                    "if [ -f {}/training_complete.txt ]; then \
                     MOD_TIME=$(stat -c %Y {}/training_complete.txt 2>/dev/null || stat -f %m {}/training_complete.txt 2>/dev/null || echo '0'); \
                     NOW=$(date +%s); \
                     AGE=$((NOW - MOD_TIME)); \
                     if [ $AGE -ge 2 ]; then \
                         echo 'STABLE'; \
                     else \
                         echo 'UNSTABLE'; \
                     fi; \
                     else \
                     echo 'MISSING'; \
                     fi",
                    project_dir, project_dir, project_dir
                );

                // Check if marker is stable (not being written)
                if let Ok(stable_output) = crate::aws_utils::execute_ssm_command(
                    ssm_client,
                    instance_id,
                    &verify_stable_cmd,
                )
                .await
                {
                    if stable_output.trim() == "UNSTABLE" {
                        warn!("Marker file exists but was recently modified, waiting for stability...");
                        // Return false to continue checking - file might still be written
                        return Ok(false);
                    }
                }

                // Also check exit code if available
                let exit_code_cmd = format!(
                    "if [ -f {}/training_exit_code.txt ]; then cat {}/training_exit_code.txt; else echo '0'; fi",
                    project_dir, project_dir
                );
                if let Ok(exit_code_str) =
                    crate::aws_utils::execute_ssm_command(ssm_client, instance_id, &exit_code_cmd)
                        .await
                {
                    if let Ok(exit_code) = exit_code_str.trim().parse::<i32>() {
                        if exit_code != 0 {
                            warn!(
                                "Training completed but exit code is {} (non-zero)",
                                exit_code
                            );
                            // Still return true - training completed, but may have failed
                        }
                    }
                }
                return Ok(true);
            }
        }
        Err(_) => {
            // Marker check failed, try other methods
        }
    }

    // Method 2: Check if training process is still running
    let check_process_cmd = format!(
        "if [ -f {}/training.pid ]; then \
         PID=$(cat {}/training.pid 2>/dev/null); \
         if ps -p $PID > /dev/null 2>&1; then \
             echo 'RUNNING'; \
         else \
             echo 'COMPLETE'; \
         fi; \
         else \
         echo 'NO_PID'; \
         fi",
        project_dir, project_dir
    );

    match crate::aws_utils::execute_ssm_command(ssm_client, instance_id, &check_process_cmd).await {
        Ok(output) => {
            if output.trim() == "COMPLETE" {
                info!("Training process completed (PID file indicates process finished)");
                return Ok(true);
            } else if output.trim() == "NO_PID" {
                // No PID file - check training.log for completion indicators
                let check_log_cmd = format!(
                    "if [ -f {}/training.log ]; then \
                     if grep -q -E '(Training complete|Training finished|COMPLETE|DONE)' {}/training.log 2>/dev/null; then \
                         echo 'COMPLETE'; \
                     else \
                         echo 'RUNNING'; \
                     fi; \
                     else \
                     echo 'NO_LOG'; \
                     fi",
                    project_dir, project_dir
                );

                match crate::aws_utils::execute_ssm_command(ssm_client, instance_id, &check_log_cmd)
                    .await
                {
                    Ok(log_output) => {
                        if log_output.trim() == "COMPLETE" {
                            info!("Training completion detected in log file");
                            return Ok(true);
                        }
                    }
                    Err(_) => {
                        // Log check failed - assume still running
                    }
                }
            }
        }
        Err(_) => {
            // Process check failed - assume still running
        }
    }

    Ok(false)
}

/// Wait for training to complete
async fn wait_for_training_completion(
    ssm_client: &SsmClient,
    instance_id: &str,
    project_dir: &str,
    output_format: &str,
    timeout_minutes: u64,
) -> Result<()> {
    use serde_json::json;
    use std::time::Duration;
    use tokio::time::sleep;

    if output_format != "json" {
        println!(
            "Waiting for training to complete (timeout: {} minutes)...",
            timeout_minutes
        );
        println!(
            "Note: Timeout only stops waiting - training continues running in the background."
        );
    }

    let mut check_count = 0;
    let check_interval = Duration::from_secs(2);
    // Calculate max checks based on timeout
    let max_checks = if timeout_minutes > 0 {
        (timeout_minutes * 60) / check_interval.as_secs()
    } else {
        u64::MAX / check_interval.as_secs() // Effectively no timeout, but prevent overflow
    };
    let max_timeout_minutes = timeout_minutes;

    loop {
        sleep(check_interval).await;
        check_count += 1;

        match check_training_completion(ssm_client, instance_id, project_dir).await {
            Ok(true) => {
                if output_format == "json" {
                    let result = json!({
                        "success": true,
                        "instance_id": instance_id,
                        "message": "Training completed successfully"
                    });
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Training completed successfully");
                }
                return Ok(());
            }
            Ok(false) => {
                // Still running
                if check_count % 30 == 0 && output_format != "json" {
                    // Print status every minute (30 checks * 2 seconds = 60 seconds)
                    let elapsed_minutes = (check_count * check_interval.as_secs()) / 60;
                    println!(
                        "Training still in progress... (checked {} times, ~{} minutes elapsed)",
                        check_count, elapsed_minutes
                    );
                }
            }
            Err(e) => {
                warn!("Error checking training completion: {}", e);
                // Continue monitoring despite errors
            }
        }

        if check_count >= max_checks {
            return Err(TrainctlError::Resource {
                resource_type: "training".to_string(),
                operation: "wait_for_completion".to_string(),
                resource_id: Some(instance_id.to_string()),
                message: format!(
                    "Training did not complete within {} minutes ({} hours). Check manually: runctl aws monitor {}",
                    max_timeout_minutes,
                    max_timeout_minutes / 60,
                    instance_id
                ),
                source: None,
            });
        }
    }
}
