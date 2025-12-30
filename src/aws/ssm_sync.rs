//! SSM-based code syncing using S3 as intermediate storage
//!
//! Provides code syncing functionality for instances with SSM (no SSH keys required).
//! Uses S3 as intermediate storage: upload code to S3, then download on instance via SSM.

use crate::aws_utils::execute_ssm_command;
use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use flate2::write::GzEncoder;
use flate2::Compression;
use ignore::gitignore::GitignoreBuilder;
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::Builder;
use tracing::info;

/// Collect files to sync (similar to ssh_sync logic)
fn collect_files_to_sync(project_root: &Path, include_patterns: &[String]) -> Result<Vec<PathBuf>> {
    // Build gitignore matcher
    let mut builder = GitignoreBuilder::new(project_root);
    
    // Add .gitignore if it exists
    let gitignore_path = project_root.join(".gitignore");
    if gitignore_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
            for line in content.lines() {
                let _ = builder.add_line(None, line);
            }
        }
    }
    
    // Add negations for include patterns (override gitignore)
    for pattern in include_patterns {
        let normalized_pattern = if pattern.ends_with('/') {
            format!("!{}**", pattern)
        } else {
            format!("!{}", pattern)
        };
        let _ = builder.add_line(None, &normalized_pattern);
    }
    
    let gitignore = builder.build().map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!("Failed to build gitignore: {}", e)))
    })?;

    // Walk all files
    let files: Vec<PathBuf> = WalkBuilder::new(project_root)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            if !path.is_file() {
                return None;
            }

            let rel_path = match path.strip_prefix(project_root) {
                Ok(p) => p,
                Err(_) => return None,
            };

            // Check if matches include pattern
            let matches_include = include_patterns
                .iter()
                .any(|pattern| {
                    let pattern_path = Path::new(pattern);
                    rel_path.starts_with(pattern_path)
                        || rel_path
                            .parent()
                            .map(|p| p == pattern_path || p.starts_with(pattern_path))
                            .unwrap_or(false)
                });

            // Check gitignore
            let matched = gitignore.matched(rel_path, false);

            // Include if matches include pattern or not gitignored
            if matches_include || !matched.is_ignore() {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .collect();

    Ok(files)
}

/// Options for SSM-based code synchronization
#[derive(Debug)]
pub struct SsmSyncOptions<'a> {
    pub project_root: &'a Path,
    pub instance_id: &'a str,
    pub project_dir: &'a str,
    pub script_path: &'a Path,
    pub include_patterns: &'a [String],
    pub s3_client: &'a S3Client,
    pub ssm_client: &'a SsmClient,
    pub config: &'a Config,
    pub output_format: &'a str,
}

/// Sync code to instance via SSM using S3 as intermediate storage
///
/// Strategy:
/// 1. Create tar.gz archive of project code
/// 2. Upload to S3 temporary location
/// 3. Use SSM to download and extract on instance
/// 4. Clean up S3 temporary file
pub async fn sync_code_via_ssm(options: SsmSyncOptions<'_>) -> Result<()> {
    let SsmSyncOptions {
        project_root,
        instance_id,
        project_dir,
        script_path: _script_path,
        include_patterns,
        s3_client,
        ssm_client,
        config,
        output_format,
    } = options;
    // Get S3 bucket from config
    let s3_bucket = config
        .aws
        .as_ref()
        .and_then(|c| c.s3_bucket.as_ref())
        .ok_or_else(|| {
            TrainctlError::Config(crate::error::ConfigError::MissingField(
                "s3_bucket".to_string(),
            ))
        })?;

    let pb = if output_format != "json" {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .map_err(|e| TrainctlError::Io(std::io::Error::other(format!("Invalid progress bar template: {}", e))))?,
        );
        pb.set_message("Creating code archive...");
        Some(pb)
    } else {
        None
    };

    // Step 1: Create tar.gz archive
    if let Some(ref p) = pb {
        p.set_message("Creating code archive...");
    }

    let files_to_sync = collect_files_to_sync(project_root, include_patterns)?;
    info!("Syncing {} files via SSM", files_to_sync.len());

    let temp_archive = std::env::temp_dir().join(format!("runctl-code-{}.tar.gz", uuid::Uuid::new_v4()));
    
    {
        let file = File::create(&temp_archive)
            .map_err(|e| TrainctlError::Io(std::io::Error::other(format!("Failed to create archive: {}", e))))?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(encoder);

        for file_path in &files_to_sync {
            let relative_path = file_path.strip_prefix(project_root).map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Failed to get relative path: {}", e),
                ))
            })?;

            tar.append_path_with_name(file_path, relative_path).map_err(|e| {
                TrainctlError::Io(std::io::Error::other(format!("Failed to add file to archive: {}", e)))
            })?;
        }

        tar.finish().map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!("Failed to finalize archive: {}", e)))
        })?;
    }

    let archive_size = std::fs::metadata(&temp_archive)
        .map_err(|e| TrainctlError::Io(std::io::Error::other(format!("Failed to get archive size: {}", e))))?
        .len();

    if let Some(ref p) = pb {
        p.set_message(format!("Archive created: {:.1} MB", archive_size as f64 / 1_000_000.0));
    }

    // Step 2: Upload to S3
    if let Some(ref p) = pb {
        p.set_message("Uploading to S3...");
    }

    let s3_key = format!("runctl-temp/{}/{}.tar.gz", instance_id, uuid::Uuid::new_v4());
    let s3_path = format!("s3://{}/{}", s3_bucket, s3_key);

    let body = aws_sdk_s3::primitives::ByteStream::from_path(&temp_archive)
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to read archive: {}", e)))?;

    s3_client
        .put_object()
        .bucket(s3_bucket)
        .key(&s3_key)
        .body(body)
        .send()
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to upload to S3: {}", e)))?;

    info!("Uploaded code archive to {}", s3_path);

    // Clean up local archive
    let _ = std::fs::remove_file(&temp_archive);

    // Step 3: Download and extract on instance via SSM
    if let Some(ref p) = pb {
        p.set_message("Downloading and extracting on instance...");
    }

    // Create project directory
    let mkdir_cmd = format!("mkdir -p {}", project_dir);
    execute_ssm_command(ssm_client, instance_id, &mkdir_cmd).await?;

    // Download from S3 and extract
    let download_cmd = format!(
        "cd {} && aws s3 cp {} code.tar.gz && tar -xzf code.tar.gz && rm code.tar.gz && echo 'Code sync complete'",
        project_dir, s3_path
    );
    
    let output = execute_ssm_command(ssm_client, instance_id, &download_cmd).await?;
    
    info!("Code sync completed: {}", output.trim());

    // Step 4: Clean up S3 temporary file
    if let Some(ref p) = pb {
        p.set_message("Cleaning up...");
    }

    let _ = s3_client
        .delete_object()
        .bucket(s3_bucket)
        .key(&s3_key)
        .send()
        .await;

    if let Some(ref p) = pb {
        p.finish_with_message("Code sync complete");
    }

    Ok(())
}
