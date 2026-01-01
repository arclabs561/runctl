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
use tracing::{info, warn};

/// Collect files to sync (similar to ssh_sync logic)
fn collect_files_to_sync(project_root: &Path, include_patterns: &[String]) -> Result<Vec<PathBuf>> {
    // Build gitignore matcher
    let mut builder = GitignoreBuilder::new(project_root);

    // Add .gitignore if it exists
    let gitignore_path = project_root.join(".gitignore");
    if gitignore_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
            for line in content.lines() {
                builder.add_line(None, line).map_err(|e| {
                    TrainctlError::Io(std::io::Error::other(format!(
                        "Failed to add gitignore line: {}",
                        e
                    )))
                })?;
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
        builder.add_line(None, &normalized_pattern).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to add include pattern '{}': {}",
                pattern, e
            )))
        })?;
    }

    let gitignore = builder.build().map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to build gitignore: {}",
            e
        )))
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
            let matches_include = include_patterns.iter().any(|pattern| {
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

/// Sync code to instance via SSM using S3 as intermediate storage
///
/// Strategy:
/// 1. Create tar.gz archive of project code
/// 2. Upload to S3 temporary location
/// 3. Use SSM to download and extract on instance
/// 4. Clean up S3 temporary file
#[allow(clippy::too_many_arguments)] // TODO: Refactor to use a struct for parameters
pub async fn sync_code_via_ssm(
    project_root: &Path,
    instance_id: &str,
    project_dir: &str,
    script_path: &Path,
    include_patterns: &[String],
    s3_client: &S3Client,
    ssm_client: &SsmClient,
    config: &Config,
    output_format: &str,
) -> Result<()> {
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
                .expect("Progress bar template should be valid"),
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

    if files_to_sync.is_empty() {
        return Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "No files to sync. Check that project root is correct and files are not all gitignored.".to_string(),
            source: None,
        });
    }

    info!("Syncing {} files via SSM", files_to_sync.len());

    if let Some(ref p) = pb {
        p.set_message(format!("Archiving {} files...", files_to_sync.len()));
    }

    let temp_archive =
        std::env::temp_dir().join(format!("runctl-code-{}.tar.gz", uuid::Uuid::new_v4()));

    {
        let file = File::create(&temp_archive).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to create archive: {}",
                e
            )))
        })?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(encoder);

        let mut files_added = 0;
        for file_path in &files_to_sync {
            // Skip if file doesn't exist (might have been deleted)
            if !file_path.exists() {
                warn!("Skipping non-existent file: {}", file_path.display());
                continue;
            }

            let relative_path = file_path.strip_prefix(project_root).map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Failed to get relative path: {}", e),
                ))
            })?;

            tar.append_path_with_name(file_path, relative_path)
                .map_err(|e| {
                    TrainctlError::Io(std::io::Error::other(format!(
                        "Failed to add file to archive: {}",
                        e
                    )))
                })?;
            files_added += 1;
        }

        if files_added == 0 {
            return Err(TrainctlError::CloudProvider {
                provider: "aws".to_string(),
                message: "No files were added to archive. All files may have been deleted or inaccessible.".to_string(),
                source: None,
            });
        }

        info!("Added {} files to archive", files_added);

        tar.finish().map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to finalize archive: {}",
                e
            )))
        })?;
    }

    let archive_size = std::fs::metadata(&temp_archive)
        .map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to get archive size: {}",
                e
            )))
        })?
        .len();

    if let Some(ref p) = pb {
        p.set_message(format!(
            "Archive created: {:.1} MB",
            archive_size as f64 / 1_000_000.0
        ));
    }

    // Step 2: Upload to S3
    if let Some(ref p) = pb {
        p.set_message("Uploading to S3...");
    }

    let s3_key = format!(
        "runctl-temp/{}/{}.tar.gz",
        instance_id,
        uuid::Uuid::new_v4()
    );
    let s3_path = format!("s3://{}/{}", s3_bucket, s3_key);

    // Upload to S3 (S3 uploads are generally reliable)
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
    if let Err(e) = std::fs::remove_file(&temp_archive) {
        warn!(
            "Failed to cleanup temporary archive {}: {}",
            temp_archive.display(),
            e
        );
    }

    // Step 3: Download and extract on instance via SSM
    if let Some(ref p) = pb {
        p.set_message("Downloading and extracting on instance...");
    }

    // Create project directory
    let mkdir_cmd = format!("mkdir -p {}", project_dir);
    execute_ssm_command(ssm_client, instance_id, &mkdir_cmd).await?;

    // Download from S3 and extract
    // Use a more robust command that handles errors and provides feedback
    let download_cmd = format!(
        "cd {} && \
        echo 'Downloading code archive from S3...' && \
        aws s3 cp {} code.tar.gz && \
        echo 'Extracting archive...' && \
        tar -xzf code.tar.gz && \
        echo 'Cleaning up...' && \
        rm -f code.tar.gz && \
        echo 'Code sync complete' && \
        ls -la | head -10",
        project_dir, s3_path
    );

    let output = execute_ssm_command(ssm_client, instance_id, &download_cmd).await?;

    info!("Code sync completed: {}", output.trim());

    // Verify code was extracted (check for script and common directories)
    let script_name = script_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("train.py");

    let verify_cmd = format!(
        "cd {} && \
        (test -f {} && echo 'VERIFIED: script exists') || echo 'WARNING: script not found' && \
        (test -d training && echo 'VERIFIED: training directory exists') || echo 'WARNING: training directory not found' && \
        echo 'Files synced:' && \
        find . -type f -name '*.py' | head -5",
        project_dir, script_name
    );

    if let Ok(verify_output) = execute_ssm_command(ssm_client, instance_id, &verify_cmd).await {
        let verified = verify_output.contains("VERIFIED");
        if verified {
            info!("Code sync verification passed");
            if output_format != "json" {
                println!("   Code sync verified: script and directories found");
            }
        } else {
            warn!("Code sync verification warning: {}", verify_output.trim());
            if output_format != "json" {
                println!("   WARNING: Some expected files/directories not found after sync");
            }
        }
    }

    // Step 4: Clean up S3 temporary file
    if let Some(ref p) = pb {
        p.set_message("Cleaning up temporary files...");
    }

    // Clean up S3 file (best effort - don't fail if cleanup fails)
    match s3_client
        .delete_object()
        .bucket(s3_bucket)
        .key(&s3_key)
        .send()
        .await
    {
        Ok(_) => {
            info!("Cleaned up S3 temporary file: {}", s3_key);
        }
        Err(e) => {
            warn!("Failed to clean up S3 temporary file {}: {}. You may want to clean it up manually.", s3_key, e);
        }
    }

    if let Some(ref p) = pb {
        p.finish_with_message("Code sync complete");
    }

    Ok(())
}
