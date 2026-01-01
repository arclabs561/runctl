//! Docker CLI commands for runctl
//!
//! Provides CLI interface for Docker operations including build, push, and training in containers.
//!
//! This is a binary-only module that uses the runctl library for core functionality.

use runctl::config::Config;
use runctl::docker::{build_and_push_to_ecr, build_image, detect_dockerfile, push_to_ecr};
use runctl::error::{Result, TrainctlError};
use aws_config::BehaviorVersion;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

#[derive(Subcommand, Clone)]
pub enum DockerCommands {
    /// Build Docker image from Dockerfile
    ///
    /// Builds a Docker image from a Dockerfile in the project root.
    /// Automatically detects Dockerfile in common locations.
    ///
    /// Examples:
    ///   runctl docker build
    ///   runctl docker build --tag my-training:v1
    Build {
        /// Image tag (default: project-name:latest)
        #[arg(long, value_name = "TAG")]
        tag: Option<String>,

        /// Dockerfile path (default: auto-detect)
        #[arg(long, value_name = "DOCKERFILE")]
        dockerfile: Option<PathBuf>,

        /// Push to ECR after building
        #[arg(long)]
        push: bool,

        /// ECR repository name (required if --push)
        #[arg(long, value_name = "REPOSITORY")]
        repository: Option<String>,
    },
    /// Push Docker image to ECR
    ///
    /// Pushes a local Docker image to AWS ECR.
    ///
    /// Examples:
    ///   runctl docker push my-training:latest --repository runctl-training
    Push {
        /// Local image name (e.g., my-training:latest)
        #[arg(value_name = "IMAGE")]
        image: String,

        /// ECR repository name
        #[arg(long, value_name = "REPOSITORY")]
        repository: String,

        /// Image tag (default: latest)
        #[arg(long, value_name = "TAG", default_value = "latest")]
        tag: String,
    },
    /// Build and push Docker image to ECR
    ///
    /// Convenience command that builds and pushes in one step.
    ///
    /// Examples:
    ///   runctl docker build-push --repository runctl-training
    BuildPush {
        /// ECR repository name
        #[arg(long, value_name = "REPOSITORY")]
        repository: String,

        /// Image tag (default: latest)
        #[arg(long, value_name = "TAG", default_value = "latest")]
        tag: String,

        /// Dockerfile path (default: auto-detect)
        #[arg(long, value_name = "DOCKERFILE")]
        dockerfile: Option<PathBuf>,
    },
}

pub async fn handle_command(
    cmd: DockerCommands,
    config: &Config,
    output_format: &str,
) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let project_root = std::env::current_dir().map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to get current directory: {}",
            e
        )))
    })?;

    let aws_cfg = config.aws.as_ref().ok_or_else(|| {
        TrainctlError::Config(runctl::error::ConfigError::MissingField("aws".to_string()))
    })?;

    let region = aws_cfg.region.as_str();

    match cmd {
        DockerCommands::Build {
            tag,
            dockerfile,
            push,
            repository,
        } => {
            let dockerfile_path = if let Some(df) = dockerfile {
                df
            } else {
                detect_dockerfile(&project_root).ok_or_else(|| TrainctlError::CloudProvider {
                    provider: "docker".to_string(),
                    message: "No Dockerfile found. Use --dockerfile to specify path.".to_string(),
                    source: None,
                })?
            };

            let image_tag = tag.unwrap_or_else(|| {
                project_root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| format!("{}:latest", s))
                    .unwrap_or_else(|| "runctl-training:latest".to_string())
            });

            info!(
                "Building Docker image: {} from {:?}",
                image_tag, dockerfile_path
            );
            build_image(&dockerfile_path, &image_tag, &project_root)?;

            if output_format != "json" {
                println!("Docker image built: {}", image_tag);
            }

            if push {
                let repo = repository.ok_or_else(|| TrainctlError::CloudProvider {
                    provider: "docker".to_string(),
                    message: "--repository required when using --push".to_string(),
                    source: None,
                })?;

                let ecr_image =
                    push_to_ecr(&image_tag, &repo, "latest", region, &aws_config).await?;

                if output_format != "json" {
                    println!("Pushed to ECR: {}", ecr_image);
                } else {
                    println!("{{\"ecr_image\": \"{}\"}}", ecr_image);
                }
            } else if output_format == "json" {
                println!("{{\"image\": \"{}\"}}", image_tag);
            }
        }
        DockerCommands::Push {
            image,
            repository,
            tag,
        } => {
            let ecr_image = push_to_ecr(&image, &repository, &tag, region, &aws_config).await?;

            if output_format == "json" {
                println!("{{\"ecr_image\": \"{}\"}}", ecr_image);
            } else {
                println!("Pushed to ECR: {}", ecr_image);
            }
        }
        DockerCommands::BuildPush {
            repository,
            tag,
            dockerfile,
        } => {
            let _dockerfile_path = if let Some(df) = dockerfile {
                df
            } else {
                detect_dockerfile(&project_root).ok_or_else(|| TrainctlError::CloudProvider {
                    provider: "docker".to_string(),
                    message: "No Dockerfile found. Use --dockerfile to specify path.".to_string(),
                    source: None,
                })?
            };

            let ecr_image =
                build_and_push_to_ecr(&project_root, &repository, &tag, region, &aws_config)
                    .await?;

            if output_format == "json" {
                println!("{{\"ecr_image\": \"{}\"}}", ecr_image);
            } else {
                println!("Built and pushed to ECR: {}", ecr_image);
            }
        }
    }

    Ok(())
}
