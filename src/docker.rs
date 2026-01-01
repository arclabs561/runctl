//! Docker container support for runctl
//!
//! Provides functionality for building Docker images and running training in containers.
//! Supports AWS ECR for image storage and retrieval.

use crate::error::{Result, TrainctlError};
use aws_config::SdkConfig;
use aws_sdk_ecr::Client as EcrClient;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Detect if project has a Dockerfile
///
/// Checks for Dockerfile in common locations (in priority order):
/// 1. `Dockerfile` in project root (most common)
/// 2. `Dockerfile.train` in project root
/// 3. `docker/Dockerfile` in project root
/// 4. `deployment/Dockerfile` in project root
/// 5. `training/Dockerfile` in project root
/// 6. `scripts/Dockerfile` in project root
/// 7. `src/Dockerfile` in project root
///
/// Note: This checks common patterns but is not exhaustive. For projects with
/// Dockerfiles in other locations, consider using a symlink or specifying
/// the path explicitly (future enhancement).
pub fn detect_dockerfile(project_root: &Path) -> Option<PathBuf> {
    let candidates = [
        project_root.join("Dockerfile"),
        project_root.join("Dockerfile.train"),
        project_root.join("docker").join("Dockerfile"),
        project_root.join("deployment").join("Dockerfile"),
        project_root.join("training").join("Dockerfile"),
        project_root.join("scripts").join("Dockerfile"),
        project_root.join("src").join("Dockerfile"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    None
}

/// Build Docker image from Dockerfile
///
/// # Arguments
///
/// * `dockerfile_path`: Path to Dockerfile
/// * `image_name`: Name for the built image (e.g., "my-training:latest")
/// * `project_root`: Project root directory (context for build)
pub fn build_image(dockerfile_path: &Path, image_name: &str, project_root: &Path) -> Result<()> {
    info!(
        "Building Docker image: {} from {:?}",
        image_name, dockerfile_path
    );

    let output = Command::new("docker")
        .arg("build")
        .arg("-f")
        .arg(dockerfile_path)
        .arg("-t")
        .arg(image_name)
        .arg(project_root)
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to execute docker build: {}",
                e
            )))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined_error = if stderr.is_empty() {
            stdout.to_string()
        } else {
            format!("{}\n{}", stderr, stdout)
        };

        return Err(TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: format!(
                "Docker build failed: {}\n\n\
                To resolve:\n\
                  1. Check Dockerfile syntax and base image availability\n\
                  2. Verify Docker is running: docker ps\n\
                  3. Check disk space: df -h\n\
                  4. Review build logs above for specific errors\n\
                  5. Test build locally: docker build -f {:?} -t test-image {}",
                combined_error.trim(),
                dockerfile_path,
                project_root.display()
            ),
            source: None,
        });
    }

    info!("Docker image built successfully: {}", image_name);
    Ok(())
}

/// Get ECR login token and authenticate Docker
async fn ecr_login(ecr_client: &EcrClient, _region: &str) -> Result<()> {
    info!("Authenticating Docker with ECR...");

    let auth_data = ecr_client
        .get_authorization_token()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to get ECR auth token: {}", e)))?;

    let token = auth_data
        .authorization_data()
        .first()
        .and_then(|d| d.authorization_token())
        .ok_or_else(|| TrainctlError::Aws("No ECR authorization token returned".to_string()))?;

    // Decode base64 token (format: AWS:password)
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .map_err(|e| TrainctlError::Aws(format!("Failed to decode ECR token: {}", e)))?;

    let credentials = String::from_utf8(decoded)
        .map_err(|e| TrainctlError::Aws(format!("Failed to parse ECR credentials: {}", e)))?;

    let parts: Vec<&str> = credentials.split(':').collect();
    if parts.len() != 2 {
        return Err(TrainctlError::Aws(
            "Invalid ECR credentials format".to_string(),
        ));
    }

    let password = parts[1];

    // Get registry URL
    let registry_url = auth_data
        .authorization_data()
        .first()
        .and_then(|d| d.proxy_endpoint())
        .ok_or_else(|| TrainctlError::Aws("No ECR registry URL returned".to_string()))?;

    // Login to ECR
    let mut output = Command::new("docker")
        .arg("login")
        .arg("--username")
        .arg("AWS")
        .arg("--password-stdin")
        .arg(registry_url)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to spawn docker login: {}",
                e
            )))
        })?;

    // Write password to stdin
    use std::io::Write;
    if let Some(ref mut stdin) = output.stdin {
        stdin.write_all(password.as_bytes()).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to write password to docker login: {}",
                e
            )))
        })?;
    }

    let result = output.wait_with_output().map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to wait for docker login: {}",
            e
        )))
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: format!(
                "Docker ECR login failed: {}\n\n\
                To resolve:\n\
                  1. Verify AWS credentials: aws sts get-caller-identity\n\
                  2. Check ECR permissions: aws ecr get-authorization-token\n\
                  3. Verify Docker is running: docker ps\n\
                  4. Check network connectivity to ECR registry\n\
                  5. Try manual login: aws ecr get-login-password | docker login --username AWS --password-stdin {}",
                stderr.trim(), registry_url
            ),
            source: None,
        });
    }

    info!("Docker authenticated with ECR");
    Ok(())
}

/// Push Docker image to ECR
///
/// # Arguments
///
/// * `image_name`: Local image name
/// * `ecr_repository`: ECR repository name (e.g., "my-training")
/// * `tag`: Image tag (e.g., "latest")
/// * `region`: AWS region
/// * `aws_config`: AWS SDK configuration
pub async fn push_to_ecr(
    image_name: &str,
    ecr_repository: &str,
    tag: &str,
    region: &str,
    aws_config: &SdkConfig,
) -> Result<String> {
    info!(
        "Pushing Docker image to ECR: {}/{}:{}",
        ecr_repository, tag, region
    );

    let ecr_client = EcrClient::new(aws_config);

    // Get account ID for ECR registry URL
    let account_id = get_account_id(aws_config).await?;
    let registry_url = format!("{}.dkr.ecr.{}.amazonaws.com", account_id, region);
    let ecr_image = format!("{}/{}:{}", registry_url, ecr_repository, tag);

    // Authenticate with ECR
    ecr_login(&ecr_client, region).await?;

    // Tag image for ECR
    let tag_output = Command::new("docker")
        .arg("tag")
        .arg(image_name)
        .arg(&ecr_image)
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to tag Docker image: {}",
                e
            )))
        })?;

    if !tag_output.status.success() {
        let stderr = String::from_utf8_lossy(&tag_output.stderr);
        return Err(TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: format!("Failed to tag image: {}", stderr),
            source: None,
        });
    }

    // Push to ECR
    let push_output = Command::new("docker")
        .arg("push")
        .arg(&ecr_image)
        .output()
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to push Docker image: {}",
                e
            )))
        })?;

    if !push_output.status.success() {
        let stderr = String::from_utf8_lossy(&push_output.stderr);
        return Err(TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: format!(
                "Docker push to ECR failed: {}\n\n\
                To resolve:\n\
                  1. Verify ECR repository exists: aws ecr describe-repositories --repository-names {}\n\
                  2. Check ECR authentication: aws ecr get-login-password\n\
                  3. Verify IAM permissions for ECR push\n\
                  4. Check network connectivity to ECR registry\n\
                  5. Review push logs above for specific errors",
                stderr.trim(), ecr_repository
            ),
            source: None,
        });
    }

    info!("Docker image pushed to ECR: {}", ecr_image);
    Ok(ecr_image)
}

/// Get AWS account ID
async fn get_account_id(aws_config: &SdkConfig) -> Result<String> {
    use aws_sdk_sts::Client as StsClient;

    let sts_client = StsClient::new(aws_config);
    let identity = sts_client
        .get_caller_identity()
        .send()
        .await
        .map_err(|e| TrainctlError::Aws(format!("Failed to get caller identity: {}", e)))?;

    identity
        .account()
        .ok_or_else(|| TrainctlError::Aws("No account ID in STS response".to_string()))
        .map(|s| s.to_string())
}

/// Create or get ECR repository
async fn ensure_ecr_repository(ecr_client: &EcrClient, repository_name: &str) -> Result<()> {
    // Try to describe repository (will fail if doesn't exist)
    match ecr_client
        .describe_repositories()
        .repository_names(repository_name)
        .send()
        .await
    {
        Ok(_) => {
            info!("ECR repository already exists: {}", repository_name);
            Ok(())
        }
        Err(_) => {
            // Repository doesn't exist, create it
            info!("Creating ECR repository: {}", repository_name);
            ecr_client
                .create_repository()
                .repository_name(repository_name)
                .send()
                .await
                .map_err(|e| {
                    TrainctlError::Aws(format!("Failed to create ECR repository: {}", e))
                })?;
            info!("ECR repository created: {}", repository_name);
            Ok(())
        }
    }
}

/// Build and push Docker image to ECR
///
/// This is a convenience function that:
/// 1. Detects Dockerfile
/// 2. Builds image
/// 3. Creates ECR repository (if needed)
/// 4. Pushes to ECR
pub async fn build_and_push_to_ecr(
    project_root: &Path,
    repository_name: &str,
    tag: &str,
    region: &str,
    aws_config: &SdkConfig,
) -> Result<String> {
    // Detect Dockerfile
    let dockerfile =
        detect_dockerfile(project_root).ok_or_else(|| TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: "No Dockerfile found in project root".to_string(),
            source: None,
        })?;

    // Build image
    let local_image = format!("{}:{}", repository_name, tag);
    build_image(&dockerfile, &local_image, project_root)?;

    // Ensure ECR repository exists
    let ecr_client = EcrClient::new(aws_config);
    ensure_ecr_repository(&ecr_client, repository_name).await?;

    // Push to ECR
    let ecr_image = push_to_ecr(&local_image, repository_name, tag, region, aws_config).await?;

    Ok(ecr_image)
}

/// Detect EBS volumes mounted on an instance
///
/// Returns a list of (host_path, container_path) tuples for volumes to mount.
pub async fn detect_ebs_mounts(
    instance_id: &str,
    ec2_client: &aws_sdk_ec2::Client,
    ssm_client: &aws_sdk_ssm::Client,
) -> Result<Vec<(String, String)>> {
    use crate::aws_utils::execute_ssm_command;

    // First, get attached volumes from EC2 API
    let response = ec2_client
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
        .ok_or_else(|| TrainctlError::Aws(format!("Instance {} not found", instance_id)))?;

    // Check for attached EBS volumes
    let block_devices = instance.block_device_mappings();
    let mut ebs_volumes = Vec::new();

    for device in block_devices {
        if let Some(ebs) = device.ebs() {
            if let Some(_volume_id) = ebs.volume_id() {
                // Check if volume is mounted on the instance
                let check_mount_cmd = r#"
# Check common mount points for EBS volumes
for mount in /mnt/data /mnt/checkpoints /data /checkpoints; do
    if mountpoint -q "$mount" 2>/dev/null; then
        echo "$mount"
    fi
done
"#
                .to_string();

                let mount_output =
                    execute_ssm_command(ssm_client, instance_id, &check_mount_cmd).await?;
                let mounts: Vec<&str> = mount_output.lines().filter(|l| !l.is_empty()).collect();

                for mount in mounts {
                    // Use same path in container for simplicity
                    ebs_volumes.push((mount.to_string(), mount.to_string()));
                }
            }
        }
    }

    Ok(ebs_volumes)
}

/// Execute training command in Docker container on instance
///
/// Uses SSM to run Docker commands on the EC2 instance.
/// Automatically detects and mounts EBS volumes if present.
pub async fn run_training_in_container(
    instance_id: &str,
    ecr_image: &str,
    script_path: &Path,
    script_args: &[String],
    project_dir: &str,
    ssm_client: &aws_sdk_ssm::Client,
    ec2_client: Option<&aws_sdk_ec2::Client>,
) -> Result<()> {
    info!(
        "Running training in Docker container: {} on instance {}",
        ecr_image, instance_id
    );

    // Get relative path from project root to script
    let script_relative = script_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| TrainctlError::Aws("Invalid script path".to_string()))?;

    let script_args_str = if script_args.is_empty() {
        String::new()
    } else {
        format!(" {}", script_args.join(" "))
    };

    // Detect EBS mounts if EC2 client provided
    let mut volume_mounts = String::new();
    if let Some(ec2_client) = ec2_client {
        match detect_ebs_mounts(instance_id, ec2_client, ssm_client).await {
            Ok(mounts) => {
                if !mounts.is_empty() {
                    info!(
                        "Detected {} EBS volume(s), mounting in container",
                        mounts.len()
                    );
                    warn!(
                        "⚠️  EBS Volume Safety: {} EBS volume(s) detected and will be mounted in container. \
                         Ensure these volumes are NOT attached to other instances. \
                         Concurrent write access from multiple instances will cause filesystem corruption.",
                        mounts.len()
                    );
                    for (host_path, container_path) in &mounts {
                        info!(
                            "Mounting EBS volume: {} -> {} (container)",
                            host_path, container_path
                        );
                        volume_mounts
                            .push_str(&format!("    -v {}:{} \\\n", host_path, container_path));
                    }
                    println!("   ⚠️  WARNING: EBS volumes mounted in container. Ensure volumes are not shared with other instances.");
                } else {
                    info!("No EBS volumes detected on instance {}", instance_id);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to detect EBS mounts: {}, continuing without EBS volumes",
                    e
                );
            }
        }
    }

    // Build Docker run command
    let docker_cmd = format!(
        r#"
cd {} && \
docker pull {} && \
docker run --rm \
    -v $(pwd):/workspace \
{}    -w /workspace \
    --gpus all \
    {} \
    python3 {}{}
"#,
        project_dir, ecr_image, volume_mounts, ecr_image, script_relative, script_args_str
    );

    // Execute via SSM
    crate::aws_utils::execute_ssm_command(ssm_client, instance_id, &docker_cmd)
        .await
        .map_err(|e| TrainctlError::CloudProvider {
            provider: "docker".to_string(),
            message: format!("Failed to run Docker container: {}", e),
            source: None,
        })?;

    info!("Training completed in Docker container");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_dockerfile_in_root() {
        let temp_dir = TempDir::new().unwrap();
        let dockerfile = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_train() {
        let temp_dir = TempDir::new().unwrap();
        let dockerfile = temp_dir.path().join("Dockerfile.train");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_in_docker_dir() {
        let temp_dir = TempDir::new().unwrap();
        let docker_dir = temp_dir.path().join("docker");
        std::fs::create_dir_all(&docker_dir).unwrap();
        let dockerfile = docker_dir.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_in_training_dir() {
        let temp_dir = TempDir::new().unwrap();
        let training_dir = temp_dir.path().join("training");
        std::fs::create_dir_all(&training_dir).unwrap();
        let dockerfile = training_dir.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_priority_order() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple Dockerfiles - should return first match in priority order
        let root_dockerfile = temp_dir.path().join("Dockerfile");
        std::fs::write(&root_dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let training_dir = temp_dir.path().join("training");
        std::fs::create_dir_all(&training_dir).unwrap();
        let training_dockerfile = training_dir.join("Dockerfile");
        std::fs::write(&training_dockerfile, "FROM ubuntu:22.04\n").unwrap();

        // Should prefer root Dockerfile over training/Dockerfile
        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(root_dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_in_deployment_dir() {
        let temp_dir = TempDir::new().unwrap();
        let deployment_dir = temp_dir.path().join("deployment");
        std::fs::create_dir_all(&deployment_dir).unwrap();
        let dockerfile = deployment_dir.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_in_scripts_dir() {
        let temp_dir = TempDir::new().unwrap();
        let scripts_dir = temp_dir.path().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let dockerfile = scripts_dir.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_in_src_dir() {
        let temp_dir = TempDir::new().unwrap();
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let dockerfile = src_dir.join("Dockerfile");
        std::fs::write(&dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_priority_across_all_locations() {
        let temp_dir = TempDir::new().unwrap();

        // Create Dockerfiles in multiple locations
        let root_dockerfile = temp_dir.path().join("Dockerfile");
        std::fs::write(&root_dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let training_dir = temp_dir.path().join("training");
        std::fs::create_dir_all(&training_dir).unwrap();
        let training_dockerfile = training_dir.join("Dockerfile");
        std::fs::write(&training_dockerfile, "FROM ubuntu:22.04\n").unwrap();

        let deployment_dir = temp_dir.path().join("deployment");
        std::fs::create_dir_all(&deployment_dir).unwrap();
        let deployment_dockerfile = deployment_dir.join("Dockerfile");
        std::fs::write(&deployment_dockerfile, "FROM ubuntu:22.04\n").unwrap();

        // Should prefer root Dockerfile (highest priority)
        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, Some(root_dockerfile));
    }

    #[test]
    fn test_detect_dockerfile_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let result = detect_dockerfile(temp_dir.path());
        assert_eq!(result, None);
    }
}
