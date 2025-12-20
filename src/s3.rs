use crate::config::Config;
use crate::error::{Result, TrainctlError};
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;
use which::which;

#[derive(Subcommand, Clone)]
pub enum S3Commands {
    /// Upload files or directories to S3
    ///
    /// Uploads local files or directories to S3. Uses native Rust parallel transfers
    /// by default (10 concurrent transfers). Use --use-s5cmd to use external s5cmd tool.
    ///
    /// Examples:
    ///   runctl s3 upload checkpoints/ s3://bucket/checkpoints/
    ///   runctl s3 upload model.pt s3://bucket/models/model.pt --recursive
    ///   runctl s3 upload data/ s3://bucket/data/ --use-s5cmd
    Upload {
        /// Local path to upload (file or directory)
        #[arg(value_name = "SOURCE")]
        source: PathBuf,
        /// S3 destination path (s3://bucket/path)
        #[arg(value_name = "DESTINATION")]
        destination: String,
        /// Use s5cmd if available (optional, native Rust is default)
        #[arg(long, default_value_t = false)]
        use_s5cmd: bool,
        /// Recursive upload
        #[arg(short, long)]
        recursive: bool,
    },
    /// Download files or directories from S3
    ///
    /// Downloads files or directories from S3 to local storage. Uses native Rust
    /// parallel transfers by default. Use --use-s5cmd to use external s5cmd tool.
    ///
    /// Examples:
    ///   runctl s3 download s3://bucket/checkpoints/ ./checkpoints/
    ///   runctl s3 download s3://bucket/data/ ./data/ --recursive
    Download {
        /// S3 source path (s3://bucket/path)
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Local destination path (file or directory)
        #[arg(value_name = "DESTINATION")]
        destination: PathBuf,
        /// Use s5cmd if available (optional, native Rust is default)
        #[arg(long, default_value_t = false)]
        use_s5cmd: bool,
        /// Recursive download
        #[arg(short, long)]
        recursive: bool,
    },
    /// Sync local directory with S3
    ///
    /// Synchronizes a local directory with an S3 path. Direction can be 'up' (local->S3),
    /// 'down' (S3->local), or 'both' (bidirectional). Uses native Rust by default.
    ///
    /// Examples:
    ///   runctl s3 sync ./checkpoints/ s3://bucket/checkpoints/ --direction up
    ///   runctl s3 sync ./data/ s3://bucket/data/ --direction down
    Sync {
        /// Local directory path
        #[arg(value_name = "LOCAL_PATH")]
        local: PathBuf,
        /// S3 path (s3://bucket/path)
        #[arg(value_name = "S3_PATH")]
        s3_path: String,
        /// Direction: up (local->s3), down (s3->local), or both
        #[arg(long, default_value = "up")]
        direction: String,
        /// Use s5cmd if available (optional, native Rust is default)
        #[arg(long, default_value_t = false)]
        use_s5cmd: bool,
    },
    /// List S3 objects
    ///
    /// Lists objects in an S3 bucket or prefix. Use --recursive to list all objects
    /// in subdirectories. Use --human-readable to show sizes in human-readable format.
    ///
    /// Examples:
    ///   runctl s3 list s3://bucket/checkpoints/
    ///   runctl s3 list s3://bucket/data/ --recursive --human-readable
    List {
        /// S3 path to list (s3://bucket/path)
        #[arg(value_name = "S3_PATH")]
        path: String,
        /// Recursive listing
        #[arg(short, long)]
        recursive: bool,
        /// Show sizes
        #[arg(short, long)]
        human_readable: bool,
    },
    /// Cleanup old checkpoints in S3
    ///
    /// Removes old checkpoints from S3, keeping only the most recent N checkpoints.
    /// Use --dry-run to preview what would be deleted without actually deleting.
    ///
    /// Examples:
    ///   runctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10
    ///   runctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 5 --dry-run
    Cleanup {
        /// S3 path to checkpoints (s3://bucket/checkpoints/)
        #[arg(value_name = "S3_PATH")]
        path: String,
        /// Keep last N checkpoints
        #[arg(long, default_value = "10")]
        keep_last_n: usize,
        /// Dry run (don't delete)
        #[arg(long)]
        dry_run: bool,
    },
    /// Watch S3 bucket for new files
    ///
    /// Monitors an S3 path for new files and prints notifications when they appear.
    /// Useful for monitoring training outputs or checkpoints being uploaded.
    ///
    /// Examples:
    ///   runctl s3 watch s3://bucket/checkpoints/
    ///   runctl s3 watch s3://bucket/outputs/ --interval 10
    Watch {
        /// S3 path to watch (s3://bucket/path)
        #[arg(value_name = "S3_PATH")]
        path: String,
        /// Poll interval in seconds
        #[arg(long, default_value = "30")]
        interval: u64,
    },
    /// Review/audit S3 training artifacts
    ///
    /// Analyzes S3 path and provides summary of training artifacts, including
    /// total size, object count, and organization. Use --detailed for more information.
    ///
    /// Examples:
    ///   runctl s3 review s3://bucket/checkpoints/
    ///   runctl s3 review s3://bucket/training/ --detailed
    Review {
        /// S3 path to review (s3://bucket/path)
        #[arg(value_name = "S3_PATH")]
        path: String,
        /// Show detailed info
        #[arg(short, long)]
        detailed: bool,
    },
}

pub async fn handle_command(cmd: S3Commands, _config: &Config, output_format: &str) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    match cmd {
        S3Commands::Upload {
            source,
            destination,
            use_s5cmd,
            recursive,
        } => {
            crate::validation::validate_path(&source.display().to_string())?;
            crate::validation::validate_s3_path(&destination)?;
            upload_to_s3(
                source,
                destination,
                use_s5cmd,
                recursive,
                &aws_config,
                output_format,
            )
            .await
        }
        S3Commands::Download {
            source,
            destination,
            use_s5cmd,
            recursive,
        } => {
            crate::validation::validate_s3_path(&source)?;
            crate::validation::validate_path(&destination.display().to_string())?;
            download_from_s3(
                source,
                destination,
                use_s5cmd,
                recursive,
                &aws_config,
                output_format,
            )
            .await
        }
        S3Commands::Sync {
            local,
            s3_path,
            direction,
            use_s5cmd,
        } => {
            crate::validation::validate_path(&local.display().to_string())?;
            crate::validation::validate_s3_path(&s3_path)?;
            sync_s3(
                local,
                s3_path,
                direction,
                use_s5cmd,
                &aws_config,
                output_format,
            )
            .await
        }
        S3Commands::List {
            path,
            recursive,
            human_readable,
        } => {
            crate::validation::validate_s3_path(&path)?;
            list_s3(path, recursive, human_readable, &aws_config, output_format).await
        }
        S3Commands::Cleanup {
            path,
            keep_last_n,
            dry_run,
        } => {
            crate::validation::validate_s3_path(&path)?;
            cleanup_s3(path, keep_last_n, dry_run, &aws_config, output_format).await
        }
        S3Commands::Watch { path, interval } => {
            crate::validation::validate_s3_path(&path)?;
            watch_s3(path, interval, &aws_config).await
        }
        S3Commands::Review { path, detailed } => {
            crate::validation::validate_s3_path(&path)?;
            review_s3(path, detailed, &aws_config, output_format).await
        }
    }
}

/// JSON output structs for S3 commands
#[derive(Serialize, Deserialize)]
pub struct S3UploadResult {
    pub success: bool,
    pub source: String,
    pub destination: String,
    pub method: String, // "s5cmd" or "aws-sdk"
}

#[derive(Serialize, Deserialize)]
pub struct S3DownloadResult {
    pub success: bool,
    pub source: String,
    pub destination: String,
    pub method: String,
}

#[derive(Serialize, Deserialize)]
pub struct S3SyncResult {
    pub success: bool,
    pub local: String,
    pub s3_path: String,
    pub direction: String,
    pub method: String,
}

#[derive(Serialize, Deserialize)]
pub struct S3Object {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
}

#[derive(Serialize, Deserialize)]
pub struct S3ListResult {
    pub path: String,
    pub objects: Vec<S3Object>,
    pub total_count: usize,
    pub total_size: u64,
}

#[derive(Serialize, Deserialize)]
pub struct S3CleanupResult {
    pub path: String,
    pub total_checkpoints: usize,
    pub kept: usize,
    pub deleted: usize,
    pub dry_run: bool,
}

#[derive(Serialize, Deserialize)]
pub struct S3ReviewResult {
    pub path: String,
    pub total_objects: usize,
    pub total_size: u64,
    pub checkpoints: usize,
    pub models: usize,
    pub logs: usize,
}

/// Check if s5cmd is available
fn check_s5cmd() -> bool {
    which("s5cmd").is_ok()
}

/// Upload to S3 using native Rust AWS SDK with parallel transfers
async fn upload_to_s3(
    source: PathBuf,
    destination: String,
    use_s5cmd: bool,
    recursive: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    // Use native Rust by default (faster, no external dependencies)
    // s5cmd is only used if explicitly requested and available
    let method = if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd (external tool) for upload");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("cp");
        if recursive {
            cmd.arg("--recursive");
        }
        cmd.arg(source.to_string_lossy().as_ref());
        cmd.arg(&destination);

        let output = cmd
            .output()
            .map_err(|e| TrainctlError::S3(format!("Failed to execute s5cmd: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TrainctlError::S3(format!(
                "s5cmd upload failed: {}",
                stderr
            )));
        }

        if output_format == "json" {
            let result = S3UploadResult {
                success: true,
                source: source.to_string_lossy().to_string(),
                destination: destination.clone(),
                method: "s5cmd".to_string(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Uploaded to {}", destination);
        }
        return Ok(());
    } else {
        "native-rust".to_string()
    };

    // Native Rust implementation with parallel transfers
    info!("Using native Rust AWS SDK for upload (parallel transfers)");
    let client = S3Client::new(aws_config);

    // Parse S3 path
    let (bucket, key) = parse_s3_path(&destination)?;

    if source.is_file() {
        let body = aws_sdk_s3::primitives::ByteStream::from_path(&source)
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to read file: {}", e)))?;

        client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(body)
            .send()
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to upload file: {}", e)))?;

        if output_format == "json" {
            let result = S3UploadResult {
                success: true,
                source: source.to_string_lossy().to_string(),
                destination: destination.clone(),
                method: method.clone(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Uploaded {} to {}", source.display(), destination);
        }
    } else if recursive && source.is_dir() {
        // Recursive directory upload with parallel transfers
        info!(
            "Uploading directory recursively with parallel transfers: {}",
            source.display()
        );
        upload_directory_recursive_parallel(&client, &bucket, &key, &source).await?;
        if output_format == "json" {
            let result = S3UploadResult {
                success: true,
                source: source.to_string_lossy().to_string(),
                destination: destination.clone(),
                method: method.clone(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Uploaded directory {} to {}", source.display(), destination);
        }
        return Ok(());
    } else {
        return Err(TrainctlError::S3(
            "Source must be a file or use --recursive for directories".to_string(),
        ));
    }

    Ok(())
}

/// Download from S3 using native Rust AWS SDK with parallel transfers
async fn download_from_s3(
    source: String,
    destination: PathBuf,
    use_s5cmd: bool,
    recursive: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    // Use native Rust by default (faster, no external dependencies)
    // s5cmd is only used if explicitly requested and available
    let method = if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd (external tool) for download");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("cp");
        if recursive {
            cmd.arg("--recursive");
        }
        cmd.arg(&source);
        cmd.arg(destination.to_string_lossy().as_ref());

        let output = cmd
            .output()
            .map_err(|e| TrainctlError::S3(format!("Failed to execute s5cmd: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TrainctlError::S3(format!(
                "s5cmd download failed: {}",
                stderr
            )));
        }

        if output_format == "json" {
            let result = S3DownloadResult {
                success: true,
                source: source.clone(),
                destination: destination.to_string_lossy().to_string(),
                method: "s5cmd".to_string(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Downloaded from {} to {}", source, destination.display());
        }
        return Ok(());
    } else {
        "native-rust".to_string()
    };

    // Native Rust implementation
    info!("Using native Rust AWS SDK for download (parallel transfers)");
    let client = S3Client::new(aws_config);
    let (bucket, key_prefix) = parse_s3_path(&source)?;

    if recursive {
        // Recursive download with parallel transfers
        download_directory_recursive_parallel(&client, &bucket, &key_prefix, &destination).await?;
    } else {
        // Single file download
        let response = client
            .get_object()
            .bucket(&bucket)
            .key(&key_prefix)
            .send()
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to download object: {}", e)))?;

        let data = response
            .body
            .collect()
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to read response body: {}", e)))?;

        // Ensure parent directory exists
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| TrainctlError::S3(format!("Failed to create directory: {}", e)))?;
        }

        std::fs::write(&destination, data.into_bytes())
            .map_err(|e| TrainctlError::S3(format!("Failed to write file: {}", e)))?;
    }

    if output_format == "json" {
        let result = S3DownloadResult {
            success: true,
            source: source.clone(),
            destination: destination.to_string_lossy().to_string(),
            method: method.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Downloaded {} to {}", source, destination.display());
    }
    Ok(())
}

/// Sync local directory with S3 using native Rust (parallel transfers)
async fn sync_s3(
    local: PathBuf,
    s3_path: String,
    direction: String,
    use_s5cmd: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    // Use native Rust by default
    if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd (external tool) for sync");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("sync");

        match direction.as_str() {
            "up" => {
                cmd.arg(local.to_string_lossy().as_ref());
                cmd.arg(&s3_path);
            }
            "down" => {
                cmd.arg(&s3_path);
                cmd.arg(local.to_string_lossy().as_ref());
            }
            "both" => {
                return Err(TrainctlError::S3(
                    "Bidirectional sync not supported. Use 'up' or 'down'".to_string(),
                ));
            }
            _ => {
                return Err(TrainctlError::S3(
                    "Direction must be 'up' or 'down'".to_string(),
                ));
            }
        }

        let output = cmd
            .output()
            .map_err(|e| TrainctlError::S3(format!("Failed to execute s5cmd sync: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TrainctlError::S3(format!("s5cmd sync failed: {}", stderr)));
        }

        if output_format == "json" {
            let result = S3SyncResult {
                success: true,
                local: local.to_string_lossy().to_string(),
                s3_path: s3_path.clone(),
                direction: direction.clone(),
                method: "s5cmd".to_string(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Synced {} with {}", local.display(), s3_path);
        }
        return Ok(());
    }

    // Native Rust sync implementation
    info!("Using native Rust AWS SDK for sync (parallel transfers)");
    let client = S3Client::new(aws_config);
    let (bucket, key_prefix) = parse_s3_path(&s3_path)?;

    match direction.as_str() {
        "up" => {
            // Upload local to S3
            if !local.is_dir() {
                return Err(TrainctlError::S3(
                    "Local path must be a directory for sync".to_string(),
                ));
            }
            upload_directory_recursive_parallel(&client, &bucket, &key_prefix, &local).await?;
        }
        "down" => {
            // Download S3 to local
            std::fs::create_dir_all(&local).map_err(|e| {
                TrainctlError::S3(format!("Failed to create destination directory: {}", e))
            })?;
            download_directory_recursive_parallel(&client, &bucket, &key_prefix, &local).await?;
        }
        "both" => {
            return Err(TrainctlError::S3(
                "Bidirectional sync not supported. Use 'up' or 'down'".to_string(),
            ));
        }
        _ => {
            return Err(TrainctlError::S3(
                "Direction must be 'up' or 'down'".to_string(),
            ));
        }
    }

    if output_format == "json" {
        let result = S3SyncResult {
            success: true,
            local: local.to_string_lossy().to_string(),
            s3_path: s3_path.clone(),
            direction: direction.clone(),
            method: "native-rust".to_string(),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Synced {} with {}", local.display(), s3_path);
    }

    Ok(())
}

/// List S3 objects
async fn list_s3(
    path: String,
    recursive: bool,
    human_readable: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    if check_s5cmd() && output_format != "json" {
        info!("Using s5cmd for listing");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("ls");
        if recursive {
            cmd.arg("--recursive");
        }
        if human_readable {
            cmd.arg("--human-readable");
        }
        cmd.arg(&path);

        let output = cmd
            .output()
            .map_err(|e| TrainctlError::S3(format!("Failed to execute s5cmd: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TrainctlError::S3(format!("s5cmd list failed: {}", stderr)));
        }

        print!("{}", String::from_utf8_lossy(&output.stdout));
        return Ok(());
    }

    // Use AWS SDK (required for JSON output or if s5cmd unavailable)
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;

    let mut list_objects = client.list_objects_v2().bucket(&bucket);

    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }

    let response = list_objects
        .send()
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to list objects: {}", e)))?;

    let contents = response.contents();
    let mut objects = Vec::new();
    let mut total_size = 0u64;

    if !contents.is_empty() {
        for obj in contents {
            let key = obj.key().unwrap_or("").to_string();
            let size = obj.size().unwrap_or(0) as u64;
            total_size += size;
            let modified_str = obj
                .last_modified()
                .map(|dt| dt.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            objects.push(S3Object {
                key: key.clone(),
                size,
                last_modified: modified_str.clone(),
            });

            if output_format != "json" {
                if human_readable {
                    println!("{:>12}  {}  {}", format_size(size), modified_str, key);
                } else {
                    println!("{}  {}  {}", size, modified_str, key);
                }
            }
        }
    }

    if output_format == "json" {
        let result = S3ListResult {
            path: path.clone(),
            objects,
            total_count: contents.len(),
            total_size,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    }

    Ok(())
}

/// Cleanup old checkpoints in S3
async fn cleanup_s3(
    path: String,
    keep_last_n: usize,
    dry_run: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;

    // List all checkpoints
    let mut list_objects = client.list_objects_v2().bucket(&bucket);

    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }

    let response = list_objects
        .send()
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to list checkpoints: {}", e)))?;

    let contents = response.contents();
    let mut checkpoints: Vec<_> = contents
        .iter()
        .map(|obj| {
            let key = obj.key().unwrap_or("").to_string();
            // Use timestamp as u64 for sorting
            let modified = obj
                .last_modified()
                .map(|dt| dt.to_millis().unwrap_or(0))
                .unwrap_or(0);
            (key, modified)
        })
        .collect();

    // Sort by modification time (newest first)
    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));

    let total = checkpoints.len();
    if total <= keep_last_n {
        if output_format == "json" {
            let result = S3CleanupResult {
                path: path.clone(),
                total_checkpoints: total,
                kept: total,
                deleted: 0,
                dry_run,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("Only {} checkpoint(s), nothing to clean up", total);
        }
        return Ok(());
    }

    let to_delete = &checkpoints[keep_last_n..];
    let deleted_count = to_delete.len();

    if output_format != "json" {
        println!(
            "Found {} checkpoint(s), keeping last {}, deleting {}...",
            total, keep_last_n, deleted_count
        );
    }

    if dry_run {
        if output_format != "json" {
            println!("[DRY RUN] Would delete:");
            for (key, _) in to_delete {
                println!("  - {}", key);
            }
        }
        if output_format == "json" {
            let result = S3CleanupResult {
                path: path.clone(),
                total_checkpoints: total,
                kept: keep_last_n,
                deleted: deleted_count,
                dry_run: true,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        return Ok(());
    }

    // Delete old checkpoints
    for (key, _) in to_delete {
        client
            .delete_object()
            .bucket(&bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to delete {}: {}", key, e)))?;
        if output_format != "json" {
            println!("  Deleted {}", key);
        }
    }

    if output_format == "json" {
        let result = S3CleanupResult {
            path: path.clone(),
            total_checkpoints: total,
            kept: keep_last_n,
            deleted: deleted_count,
            dry_run: false,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Cleanup complete");
    }
    Ok(())
}

/// Watch S3 bucket for new files
async fn watch_s3(path: String, interval: u64, aws_config: &aws_config::SdkConfig) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;

    let mut last_seen = std::collections::HashSet::new();

    println!("Watching {} (checking every {}s)...", path, interval);
    println!("Press Ctrl+C to stop");

    loop {
        let mut list_objects = client.list_objects_v2().bucket(&bucket);

        if !prefix.is_empty() {
            list_objects = list_objects.prefix(&prefix);
        }

        let response = list_objects
            .send()
            .await
            .map_err(|e| TrainctlError::S3(format!("Failed to list objects: {}", e)))?;

        let contents = response.contents();
        if !contents.is_empty() {
            for obj in contents {
                let key = obj.key().unwrap_or("");
                if !last_seen.contains(key) {
                    let size = obj.size().unwrap_or(0);
                    let modified_str = obj
                        .last_modified()
                        .map(|dt| dt.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    println!("New file: {} ({} bytes, {})", key, size, modified_str);
                    last_seen.insert(key.to_string());
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

/// Review/audit S3 training artifacts
async fn review_s3(
    path: String,
    detailed: bool,
    aws_config: &aws_config::SdkConfig,
    output_format: &str,
) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;

    let mut list_objects = client.list_objects_v2().bucket(&bucket);

    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }

    let response = list_objects
        .send()
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to list objects: {}", e)))?;

    let mut total_size = 0u64;
    let mut checkpoint_count = 0;
    let mut model_count = 0;
    let mut log_count = 0;

    let contents = response.contents();
    let total_objects = response.key_count().unwrap_or(0) as usize;

    if !contents.is_empty() {
        for obj in contents {
            let key = obj.key().unwrap_or("");
            let size = obj.size().unwrap_or(0) as u64;
            total_size += size;

            if key.contains("checkpoint") || key.ends_with(".pt") {
                checkpoint_count += 1;
            } else if key.contains("model") || key.ends_with(".pth") {
                model_count += 1;
            } else if key.ends_with(".log") {
                log_count += 1;
            }

            if detailed && output_format != "json" {
                let modified_str = obj
                    .last_modified()
                    .map(|dt| dt.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!("{}  {:>12}  {}", modified_str, format_size(size), key);
            }
        }
    }

    if output_format == "json" {
        let result = S3ReviewResult {
            path: path.clone(),
            total_objects,
            total_size,
            checkpoints: checkpoint_count,
            models: model_count,
            logs: log_count,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", "=".repeat(70));
        println!("S3 Review: {}", path);
        println!("{}", "=".repeat(70));
        println!("Total objects: {}", total_objects);
        println!("Total size: {}", format_size(total_size));
        println!("Checkpoints: {}", checkpoint_count);
        println!("Models: {}", model_count);
        println!("Logs: {}", log_count);
    }

    Ok(())
}

/// Parse S3 path (s3://bucket/key) into bucket and key
fn parse_s3_path(s3_path: &str) -> Result<(String, String)> {
    if !s3_path.starts_with("s3://") {
        return Err(TrainctlError::S3(
            "S3 path must start with s3://".to_string(),
        ));
    }

    let path = &s3_path[5..]; // Remove "s3://"
    let parts: Vec<&str> = path.splitn(2, '/').collect();

    if parts.is_empty() {
        return Err(TrainctlError::S3(format!("Invalid S3 path: {}", s3_path)));
    }

    let bucket = parts[0].to_string();
    let key = if parts.len() > 1 {
        parts[1].to_string()
    } else {
        String::new()
    };

    Ok((bucket, key))
}

/// Recursively upload a directory to S3 with parallel transfers (native Rust)
async fn upload_directory_recursive_parallel(
    client: &S3Client,
    bucket: &str,
    prefix: &str,
    source_dir: &Path,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use walkdir::WalkDir;

    let source_path = source_dir
        .canonicalize()
        .map_err(|e| TrainctlError::S3(format!("Failed to canonicalize source path: {}", e)))?;

    if !source_path.is_dir() {
        return Err(TrainctlError::S3(format!(
            "Source path is not a directory: {}",
            source_path.display()
        )));
    }

    // Collect all files first
    let files: Vec<_> = WalkDir::new(&source_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    let total_files = files.len();
    if total_files == 0 {
        return Err(TrainctlError::S3("No files found in directory".to_string()));
    }

    info!("Uploading {} files with parallel transfers...", total_files);

    // Create progress bar
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .expect("Progress bar template"),
    );

    // Parallel upload with concurrency limit (similar to s5cmd's default)
    const PARALLEL_CONCURRENCY: usize = 10;
    let mut handles = Vec::new();
    let mut uploaded = 0u64;
    let mut failed = 0u64;

    for entry in files {
        let client = client.clone();
        let bucket = bucket.to_string();
        let source_path_clone = source_path.clone();
        let path = entry.path().to_path_buf();
        let pb = pb.clone();

        // Calculate relative path and S3 key
        let relative_path = path
            .strip_prefix(&source_path_clone)
            .map_err(|e| TrainctlError::S3(format!("Failed to calculate relative path: {}", e)))?;

        let key = if prefix.is_empty() {
            relative_path.to_string_lossy().replace('\\', "/")
        } else {
            format!(
                "{}/{}",
                prefix.trim_end_matches('/'),
                relative_path.to_string_lossy().replace('\\', "/")
            )
        };

        let handle = tokio::spawn(async move {
            let result = upload_file_to_s3(&client, &bucket, &key, &path).await;
            pb.inc(1);
            result
        });

        handles.push(handle);

        // Limit concurrency
        if handles.len() >= PARALLEL_CONCURRENCY {
            let (result, _idx, remaining) = futures::future::select_all(handles).await;
            match result {
                Ok(Ok(())) => uploaded += 1,
                Ok(Err(_)) => failed += 1,
                Err(_) => failed += 1,
            }
            handles = remaining;
        }
    }

    // Wait for remaining uploads
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => uploaded += 1,
            Ok(Err(_)) => failed += 1,
            Err(_) => failed += 1,
        }
    }

    pb.finish_with_message("Upload complete");

    if failed > 0 {
        return Err(TrainctlError::S3(format!(
            "Uploaded {} files, but {} failed",
            uploaded, failed
        )));
    }

    info!(
        "Successfully uploaded {} files to s3://{}/{}",
        uploaded, bucket, prefix
    );
    Ok(())
}

/// Recursively upload a directory to S3 (sequential, kept for compatibility)
#[allow(dead_code)] // Reserved for future recursive upload functionality
async fn upload_directory_recursive(
    client: &S3Client,
    bucket: &str,
    prefix: &str,
    source_dir: &Path,
) -> Result<()> {
    upload_directory_recursive_parallel(client, bucket, prefix, source_dir).await
}

/// Recursively download a directory from S3 with parallel transfers (native Rust)
async fn download_directory_recursive_parallel(
    client: &S3Client,
    bucket: &str,
    key_prefix: &str,
    destination: &Path,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

    // List all objects with the prefix
    let mut list_objects = client.list_objects_v2().bucket(bucket);

    if !key_prefix.is_empty() {
        list_objects = list_objects.prefix(key_prefix);
    }

    let response = list_objects
        .send()
        .await
        .map_err(|e| TrainctlError::S3(format!("Failed to list objects: {}", e)))?;

    let contents = response.contents();
    if contents.is_empty() {
        return Err(TrainctlError::S3(
            "No objects found to download".to_string(),
        ));
    }

    let total_files = contents.len();
    info!(
        "Downloading {} files with parallel transfers...",
        total_files
    );

    // Create progress bar
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .expect("Progress bar template"),
    );

    // Ensure destination directory exists
    std::fs::create_dir_all(destination)
        .map_err(|e| TrainctlError::S3(format!("Failed to create destination directory: {}", e)))?;

    // Parallel download with concurrency limit
    const PARALLEL_CONCURRENCY: usize = 10;
    let mut handles = Vec::new();
    let mut downloaded = 0u64;
    let mut failed = 0u64;

    for obj in contents {
        let client = client.clone();
        let bucket = bucket.to_string();
        let destination = destination.to_path_buf();
        let pb = pb.clone();

        let key = obj.key().unwrap_or("").to_string();
        let _size = obj.size().unwrap_or(0);

        // Skip if key is empty or is a directory marker
        if key.is_empty() || key.ends_with('/') {
            continue;
        }

        // Calculate local file path
        let relative_key = if key_prefix.is_empty() {
            key.clone()
        } else if let Some(stripped) = key.strip_prefix(key_prefix) {
            stripped.trim_start_matches('/').to_string()
        } else {
            continue; // Skip if doesn't match prefix
        };

        let local_path = destination.join(&relative_key);

        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            let parent = parent.to_path_buf();
            let handle = tokio::spawn(async move {
                // Create parent directory
                if let Err(e) = std::fs::create_dir_all(&parent) {
                    return Err(TrainctlError::S3(format!(
                        "Failed to create directory: {}",
                        e
                    )));
                }

                // Download file
                let response = client
                    .get_object()
                    .bucket(&bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|e| TrainctlError::S3(format!("Failed to download {}: {}", key, e)))?;

                let data = response.body.collect().await.map_err(|e| {
                    TrainctlError::S3(format!("Failed to read response body: {}", e))
                })?;

                std::fs::write(&local_path, data.into_bytes())
                    .map_err(|e| TrainctlError::S3(format!("Failed to write file: {}", e)))?;

                pb.inc(1);
                Ok(())
            });

            handles.push(handle);

            // Limit concurrency
            if handles.len() >= PARALLEL_CONCURRENCY {
                let (result, _idx, remaining) = futures::future::select_all(handles).await;
                match result {
                    Ok(Ok(())) => downloaded += 1,
                    Ok(Err(_)) => failed += 1,
                    Err(_) => failed += 1,
                }
                handles = remaining;
            }
        }
    }

    // Wait for remaining downloads
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => downloaded += 1,
            Ok(Err(_)) => failed += 1,
            Err(_) => failed += 1,
        }
    }

    pb.finish_with_message("Download complete");

    if failed > 0 {
        return Err(TrainctlError::S3(format!(
            "Downloaded {} files, but {} failed",
            downloaded, failed
        )));
    }

    info!(
        "Successfully downloaded {} files to {}",
        downloaded,
        destination.display()
    );
    Ok(())
}

/// Upload a single file to S3
async fn upload_file_to_s3(
    client: &S3Client,
    bucket: &str,
    key: &str,
    file_path: &std::path::Path,
) -> Result<()> {
    let body = aws_sdk_s3::primitives::ByteStream::from_path(file_path)
        .await
        .map_err(|e| {
            TrainctlError::S3(format!(
                "Failed to read file {}: {}",
                file_path.display(),
                e
            ))
        })?;

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| {
            TrainctlError::S3(format!(
                "Failed to upload file {}: {}",
                file_path.display(),
                e
            ))
        })?;

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}
