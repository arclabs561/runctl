use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client as S3Client;
use clap::Subcommand;
use std::path::PathBuf;
use crate::config::Config;
use tracing::info;
use which::which;

#[derive(Subcommand, Clone)]
pub enum S3Commands {
    /// Upload checkpoints to S3
    Upload {
        /// Local path to upload
        source: PathBuf,
        /// S3 destination (s3://bucket/path)
        destination: String,
        /// Use s5cmd if available (faster)
        #[arg(long, default_value_t = true)]
        use_s5cmd: bool,
        /// Recursive upload
        #[arg(short, long)]
        recursive: bool,
    },
    /// Download from S3
    Download {
        /// S3 source (s3://bucket/path)
        source: String,
        /// Local destination path
        destination: PathBuf,
        /// Use s5cmd if available (faster)
        #[arg(long, default_value_t = true)]
        use_s5cmd: bool,
        /// Recursive download
        #[arg(short, long)]
        recursive: bool,
    },
    /// Sync local directory with S3
    Sync {
        /// Local path
        local: PathBuf,
        /// S3 path (s3://bucket/path)
        s3_path: String,
        /// Direction: up (local->s3), down (s3->local), or both
        #[arg(long, default_value = "up")]
        direction: String,
        /// Use s5cmd if available
        #[arg(long, default_value_t = true)]
        use_s5cmd: bool,
    },
    /// List S3 objects
    List {
        /// S3 path (s3://bucket/path)
        path: String,
        /// Recursive listing
        #[arg(short, long)]
        recursive: bool,
        /// Show sizes
        #[arg(short, long)]
        human_readable: bool,
    },
    /// Cleanup old checkpoints in S3
    Cleanup {
        /// S3 path to checkpoints (s3://bucket/checkpoints/)
        path: String,
        /// Keep last N checkpoints
        #[arg(long, default_value = "10")]
        keep_last_n: usize,
        /// Dry run (don't delete)
        #[arg(long)]
        dry_run: bool,
    },
    /// Watch S3 bucket for new files
    Watch {
        /// S3 path to watch (s3://bucket/path)
        path: String,
        /// Poll interval in seconds
        #[arg(long, default_value = "30")]
        interval: u64,
    },
    /// Review/audit S3 training artifacts
    Review {
        /// S3 path to review (s3://bucket/path)
        path: String,
        /// Show detailed info
        #[arg(short, long)]
        detailed: bool,
    },
}

pub async fn handle_command(cmd: S3Commands, _config: &Config) -> Result<()> {
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    
    match cmd {
        S3Commands::Upload { source, destination, use_s5cmd, recursive } => {
            upload_to_s3(source, destination, use_s5cmd, recursive, &aws_config).await
        }
        S3Commands::Download { source, destination, use_s5cmd, recursive } => {
            download_from_s3(source, destination, use_s5cmd, recursive, &aws_config).await
        }
        S3Commands::Sync { local, s3_path, direction, use_s5cmd } => {
            sync_s3(local, s3_path, direction, use_s5cmd, &aws_config).await
        }
        S3Commands::List { path, recursive, human_readable } => {
            list_s3(path, recursive, human_readable, &aws_config).await
        }
        S3Commands::Cleanup { path, keep_last_n, dry_run } => {
            cleanup_s3(path, keep_last_n, dry_run, &aws_config).await
        }
        S3Commands::Watch { path, interval } => {
            watch_s3(path, interval, &aws_config).await
        }
        S3Commands::Review { path, detailed } => {
            review_s3(path, detailed, &aws_config).await
        }
    }
}

/// Check if s5cmd is available
fn check_s5cmd() -> bool {
    which("s5cmd").is_ok()
}

/// Upload to S3 using s5cmd (faster) or AWS SDK
async fn upload_to_s3(
    source: PathBuf,
    destination: String,
    use_s5cmd: bool,
    recursive: bool,
    _aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd for faster upload");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("cp");
        if recursive {
            cmd.arg("--recursive");
        }
        cmd.arg(source.to_string_lossy().as_ref());
        cmd.arg(&destination);
        
        let output = cmd.output()
            .with_context(|| "Failed to execute s5cmd")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd upload failed: {}", stderr);
        }
        
        println!("✅ Uploaded to {}", destination);
        return Ok(());
    }
    
    // Fallback to AWS SDK
    info!("Using AWS SDK for upload");
    let client = S3Client::new(_aws_config);
    
    // Parse S3 path
    let (bucket, key) = parse_s3_path(&destination)?;
    
    if source.is_file() {
        let body = aws_sdk_s3::primitives::ByteStream::from_path(&source).await
            .with_context(|| "Failed to read file")?;
        
        client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(body)
            .send()
            .await
            .with_context(|| "Failed to upload file")?;
        
        println!("✅ Uploaded {} to {}", source.display(), destination);
    } else if recursive && source.is_dir() {
        // Recursive directory upload
        anyhow::bail!("Recursive directory upload not yet implemented with AWS SDK. Use s5cmd or implement directory walk");
    } else {
        anyhow::bail!("Source must be a file or use --recursive for directories");
    }
    
    Ok(())
}

/// Download from S3 using s5cmd or AWS SDK
async fn download_from_s3(
    source: String,
    destination: PathBuf,
    use_s5cmd: bool,
    recursive: bool,
    _aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd for faster download");
        let mut cmd = std::process::Command::new("s5cmd");
        cmd.arg("cp");
        if recursive {
            cmd.arg("--recursive");
        }
        cmd.arg(&source);
        cmd.arg(destination.to_string_lossy().as_ref());
        
        let output = cmd.output()
            .with_context(|| "Failed to execute s5cmd")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd download failed: {}", stderr);
        }
        
        println!("✅ Downloaded from {} to {}", source, destination.display());
        return Ok(());
    }
    
    // Fallback to AWS SDK
    info!("Using AWS SDK for download");
    let client = S3Client::new(_aws_config);
    let (bucket, key) = parse_s3_path(&source)?;
    
    let response = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .with_context(|| "Failed to download object")?;
    
    let data = response.body.collect().await
        .with_context(|| "Failed to read response body")?;
    
    std::fs::write(&destination, data.into_bytes())
        .with_context(|| "Failed to write file")?;
    
    println!("✅ Downloaded {} to {}", source, destination.display());
    Ok(())
}

/// Sync local directory with S3
async fn sync_s3(
    local: PathBuf,
    s3_path: String,
    direction: String,
    use_s5cmd: bool,
    _aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    if use_s5cmd && check_s5cmd() {
        info!("Using s5cmd for sync");
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
                anyhow::bail!("Bidirectional sync not supported. Use 'up' or 'down'");
            }
            _ => {
                anyhow::bail!("Direction must be 'up' or 'down'");
            }
        }
        
        let output = cmd.output()
            .with_context(|| "Failed to execute s5cmd sync")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd sync failed: {}", stderr);
        }
        
        println!("✅ Synced {} with {}", local.display(), s3_path);
        return Ok(());
    }
    
    anyhow::bail!("Sync requires s5cmd. Install from: https://github.com/peak/s5cmd");
}

/// List S3 objects
async fn list_s3(
    path: String,
    recursive: bool,
    human_readable: bool,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    if check_s5cmd() {
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
        
        let output = cmd.output()
            .with_context(|| "Failed to execute s5cmd")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd list failed: {}", stderr);
        }
        
        print!("{}", String::from_utf8_lossy(&output.stdout));
        return Ok(());
    }
    
    // Fallback to AWS SDK
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;
    
    let mut list_objects = client
        .list_objects_v2()
        .bucket(&bucket);
    
    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }
    
    let response = list_objects
        .send()
        .await
        .with_context(|| "Failed to list objects")?;
    
    let contents = response.contents();
    if !contents.is_empty() {
        for obj in contents {
            let key = obj.key().unwrap_or("");
            let size = obj.size().unwrap_or(0);
            let modified_str = obj.last_modified()
                .map(|dt| dt.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            
            if human_readable {
                println!("{:>12}  {}  {}", format_size(size as u64), modified_str, key);
            } else {
                println!("{}  {}  {}", size, modified_str, key);
            }
        }
    }
    
    Ok(())
}

/// Cleanup old checkpoints in S3
async fn cleanup_s3(
    path: String,
    keep_last_n: usize,
    dry_run: bool,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;
    
    // List all checkpoints
    let mut list_objects = client
        .list_objects_v2()
        .bucket(&bucket);
    
    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }
    
    let response = list_objects
        .send()
        .await
        .with_context(|| "Failed to list checkpoints")?;
    
    let contents = response.contents();
    let mut checkpoints: Vec<_> = contents
        .iter()
        .map(|obj| {
            let key = obj.key().unwrap_or("").to_string();
            // Use timestamp as u64 for sorting
            let modified = obj.last_modified()
                .map(|dt| dt.to_millis().unwrap_or(0))
                .unwrap_or(0);
            (key, modified)
        })
        .collect();
    
    // Sort by modification time (newest first)
    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));
    
    if checkpoints.len() <= keep_last_n {
        println!("✅ Only {} checkpoint(s), nothing to clean up", checkpoints.len());
        return Ok(());
    }
    
    let to_delete = &checkpoints[keep_last_n..];
    println!("Found {} checkpoint(s), keeping last {}, deleting {}...", 
             checkpoints.len(), keep_last_n, to_delete.len());
    
    if dry_run {
        println!("[DRY RUN] Would delete:");
        for (key, _) in to_delete {
            println!("  - {}", key);
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
            .with_context(|| format!("Failed to delete {}", key))?;
        println!("  ✓ Deleted {}", key);
    }
    
    println!("✅ Cleanup complete");
    Ok(())
}

/// Watch S3 bucket for new files
async fn watch_s3(
    path: String,
    interval: u64,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;
    
    let mut last_seen = std::collections::HashSet::new();
    
    println!("Watching {} (checking every {}s)...", path, interval);
    println!("Press Ctrl+C to stop");
    
    loop {
        let mut list_objects = client
            .list_objects_v2()
            .bucket(&bucket);
        
        if !prefix.is_empty() {
            list_objects = list_objects.prefix(&prefix);
        }
        
        let response = list_objects
            .send()
            .await
            .with_context(|| "Failed to list objects")?;
        
        let contents = response.contents();
        if !contents.is_empty() {
            for obj in contents {
                let key = obj.key().unwrap_or("");
                if !last_seen.contains(key) {
                    let size = obj.size().unwrap_or(0);
                    let modified_str = obj.last_modified()
                .map(|dt| dt.to_string())
                .unwrap_or_else(|| "unknown".to_string());
                    println!("🆕 New file: {} ({} bytes, {})", key, size, modified_str);
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
) -> Result<()> {
    let client = S3Client::new(aws_config);
    let (bucket, prefix) = parse_s3_path(&path)?;
    
    let mut list_objects = client
        .list_objects_v2()
        .bucket(&bucket);
    
    if !prefix.is_empty() {
        list_objects = list_objects.prefix(&prefix);
    }
    
    let response = list_objects
        .send()
        .await
        .with_context(|| "Failed to list objects")?;
    
    let mut total_size = 0u64;
    let mut checkpoint_count = 0;
    let mut model_count = 0;
    let mut log_count = 0;
    
    let contents = response.contents();
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
            
            if detailed {
                let modified_str = obj.last_modified()
                .map(|dt| dt.to_string())
                .unwrap_or_else(|| "unknown".to_string());
                println!("{}  {:>12}  {}", modified_str, format_size(size), key);
            }
        }
    }
    
    println!("{}", "=".repeat(70));
    println!("S3 Review: {}", path);
    println!("{}", "=".repeat(70));
    println!("Total objects: {}", response.key_count().unwrap_or(0));
    println!("Total size: {}", format_size(total_size));
    println!("Checkpoints: {}", checkpoint_count);
    println!("Models: {}", model_count);
    println!("Logs: {}", log_count);
    
    Ok(())
}

/// Parse S3 path (s3://bucket/key) into bucket and key
fn parse_s3_path(s3_path: &str) -> Result<(String, String)> {
    if !s3_path.starts_with("s3://") {
        anyhow::bail!("S3 path must start with s3://");
    }
    
    let path = &s3_path[5..]; // Remove "s3://"
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    
    if parts.is_empty() {
        anyhow::bail!("Invalid S3 path: {}", s3_path);
    }
    
    let bucket = parts[0].to_string();
    let key = if parts.len() > 1 { parts[1].to_string() } else { String::new() };
    
    Ok((bucket, key))
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

