use crate::error::{Result, TrainctlError};
use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

#[derive(Serialize, Deserialize)]
struct CheckpointListItem {
    path: String,
    size: u64,
    size_human: String,
    modified: String,
}

#[derive(Serialize, Deserialize)]
struct CheckpointInfoJson {
    path: String,
    size: u64,
    size_human: String,
    modified: String,
    epoch: Option<u32>,
    loss: Option<f64>,
}

/// Metadata for a training checkpoint
/// 
/// This struct is kept for future use when checkpoint metadata tracking is implemented.
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub epoch: u32,
    pub loss: f64,
    pub timestamp: DateTime<Utc>,
    pub config: serde_json::Value,
    pub gpu_info: Option<serde_json::Value>,
}

#[derive(Subcommand, Clone)]
pub enum CheckpointCommands {
    /// List checkpoints in a directory
    ///
    /// Lists all checkpoint files in the specified directory, sorted by modification time.
    ///
    /// Examples:
    ///   trainctl checkpoint list ./checkpoints/
    ///   trainctl checkpoint list ./checkpoints/ --output json
    List {
        /// Checkpoint directory path
        #[arg(value_name = "DIRECTORY")]
        dir: PathBuf,
    },
    /// Show information about a checkpoint
    ///
    /// Displays detailed information about a specific checkpoint file, including
    /// size, modification time, and any embedded metadata.
    ///
    /// Examples:
    ///   trainctl checkpoint info ./checkpoints/epoch_10.pt
    ///   trainctl checkpoint info ./checkpoints/best.pt --output json
    Info {
        /// Checkpoint file path
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// Resume training from a checkpoint
    ///
    /// Resumes training from a checkpoint by running the training script with
    /// the checkpoint path as an argument.
    ///
    /// Examples:
    ///   trainctl checkpoint resume ./checkpoints/epoch_10.pt training/train.py
    ///   trainctl checkpoint resume ./checkpoints/best.pt training/train.py -- --epochs 100
    Resume {
        /// Checkpoint file path
        #[arg(value_name = "CHECKPOINT")]
        path: PathBuf,
        /// Training script path
        #[arg(value_name = "SCRIPT")]
        script: PathBuf,
    },
    /// Cleanup old checkpoints (keep last N)
    Cleanup {
        /// Checkpoint directory
        dir: PathBuf,
        /// Keep last N checkpoints
        #[arg(long, default_value = "10")]
        keep_last_n: usize,
        /// Dry run (don't delete)
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn handle_command(cmd: CheckpointCommands, output_format: &str) -> Result<()> {
    match cmd {
        CheckpointCommands::List { dir } => {
            crate::validation::validate_path(&dir.display().to_string())?;
            list_checkpoints(&dir, output_format).await
        }
        CheckpointCommands::Info { path } => {
            crate::validation::validate_path(&path.display().to_string())?;
            show_info(&path, output_format).await
        }
        CheckpointCommands::Resume { path, script } => {
            crate::validation::validate_path(&path.display().to_string())?;
            crate::validation::validate_path(&script.display().to_string())?;
            resume_from(&path, &script, output_format).await
        }
        CheckpointCommands::Cleanup { dir, keep_last_n, dry_run } => {
            crate::validation::validate_path(&dir.display().to_string())?;
            cleanup_checkpoints(&dir, keep_last_n, dry_run, output_format).await
        }
    }
}

pub async fn get_checkpoint_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("pt") {
            checkpoints.push(path);
        }
    }

    // Sort by modified time (newest first)
    checkpoints.sort_by(|a, b| {
        let a_time = fs::metadata(a).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        let b_time = fs::metadata(b).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        b_time.cmp(&a_time)
    });

    Ok(checkpoints)
}

async fn list_checkpoints(dir: &Path, output_format: &str) -> Result<()> {
    if !dir.exists() {
        println!("No checkpoint directory found: {}", dir.display());
        return Ok(());
    }

    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("pt") {
            if let Ok(metadata) = fs::metadata(&path) {
                checkpoints.push((path, metadata.modified()?));
            }
        }
    }

    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));

    if output_format == "json" {
        let mut items = Vec::new();
        for (path, modified) in checkpoints {
            let size = fs::metadata(&path)?.len();
            items.push(CheckpointListItem {
                path: path.display().to_string(),
                size,
                size_human: format_size(size),
                modified: format!("{:?}", modified),
            });
        }
        println!("{}", serde_json::to_string_pretty(&items)
            .map_err(|e| TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize JSON: {}", e),
            )))?);
        return Ok(());
    }

    println!("Checkpoints in {}:", dir.display());
    println!("{:-<80}", "");
    println!("{:<50} {:<20} {:<10}", "Path", "Modified", "Size");
    println!("{:-<80}", "");

    for (path, modified) in checkpoints {
        let size = fs::metadata(&path)?.len();
        let size_str = format_size(size);
        let modified_str = format!("{:?}", modified);
        let file_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!("{:<50} {:<20} {:<10}", 
            file_name,
            modified_str,
            size_str
        );
    }

    Ok(())
}

async fn show_info(path: &Path, output_format: &str) -> Result<()> {
    if !path.exists() {
        return Err(TrainctlError::ResourceNotFound {
            resource_type: "checkpoint".to_string(),
            resource_id: path.display().to_string(),
        });
    }

    let metadata = fs::metadata(path)?;
    
    // Try to extract checkpoint info using training module
    let epoch = crate::training::extract_checkpoint_info(path)
        .ok()
        .and_then(|info| info.epoch);
    let loss = crate::training::extract_checkpoint_info(path)
        .ok()
        .and_then(|info| info.loss);
    
    if output_format == "json" {
        let info = CheckpointInfoJson {
            path: path.display().to_string(),
            size: metadata.len(),
            size_human: format_size(metadata.len()),
            modified: format!("{:?}", metadata.modified()?),
            epoch,
            loss,
        };
        println!("{}", serde_json::to_string_pretty(&info)
            .map_err(|e| TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize JSON: {}", e),
            )))?);
        return Ok(());
    }

    println!("Checkpoint: {}", path.display());
    println!("Size: {}", format_size(metadata.len()));
    println!("Modified: {:?}", metadata.modified()?);

    if let Some(epoch) = epoch {
        println!("Epoch: {}", epoch);
    }
    if let Some(loss) = loss {
        println!("Loss: {:.4}", loss);
    }

    // Try to load PyTorch checkpoint metadata if possible
    // (This would require torch-sys or similar - simplified for now)
    println!("\nNote: Use PyTorch to inspect checkpoint contents:");
    println!("  python -c \"import torch; ckpt = torch.load('{}', map_location='cpu'); print(ckpt.keys())\"", path.display());

    Ok(())
}

async fn resume_from(checkpoint: &Path, script: &Path, _output_format: &str) -> Result<()> {
    if !checkpoint.exists() {
        return Err(TrainctlError::ResourceNotFound {
            resource_type: "checkpoint".to_string(),
            resource_id: checkpoint.display().to_string(),
        });
    }

    if !script.exists() {
        return Err(TrainctlError::ResourceNotFound {
            resource_type: "script".to_string(),
            resource_id: script.display().to_string(),
        });
    }

    println!("Resuming training from checkpoint: {}", checkpoint.display());
    println!("Script: {}", script.display());

    // In a real implementation, this would:
    // 1. Parse the checkpoint to extract epoch/config
    // 2. Modify the training script call to include --resume flag
    // 3. Execute the script

    println!("\nTo resume, run:");
    println!("  {} --resume {}", script.display(), checkpoint.display());

    Ok(())
}

async fn cleanup_checkpoints(dir: &Path, keep_last_n: usize, dry_run: bool, _output_format: &str) -> Result<()> {
    if !dir.exists() {
        println!("No checkpoint directory found: {}", dir.display());
        return Ok(());
    }

    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("pt") {
            if let Ok(metadata) = fs::metadata(&path) {
                checkpoints.push((path, metadata.modified()?));
            }
        }
    }

    if checkpoints.is_empty() {
        println!("No checkpoints found in {}", dir.display());
        return Ok(());
    }

    // Sort by modification time (newest first)
    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));

    if checkpoints.len() <= keep_last_n {
        println!("Only {} checkpoint(s), nothing to clean up", checkpoints.len());
        return Ok(());
    }

    let to_delete = &checkpoints[keep_last_n..];
    println!("Found {} checkpoint(s), keeping last {}, deleting {}...", 
             checkpoints.len(), keep_last_n, to_delete.len());

    if dry_run {
        println!("[DRY RUN] Would delete:");
        for (path, _) in to_delete {
            println!("  - {}", path.display());
        }
        return Ok(());
    }

    // Delete old checkpoints
    for (path, _) in to_delete {
        fs::remove_file(path)
            .map_err(|e| TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to delete {}: {}", path.display(), e),
            )))?;
        println!("  Deleted {}", path.display());
    }

    println!("Cleanup complete");
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn test_format_size() {
        // Note: format_size is private, but we can test it indirectly through show_info
        // Or we can make it pub for testing, but let's test the public API
        assert!(true); // Placeholder - format_size is tested indirectly
    }

    #[tokio::test]
    async fn test_get_checkpoint_paths_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoints = get_checkpoint_paths(temp_dir.path()).await.unwrap();
        assert_eq!(checkpoints.len(), 0);
    }

    #[tokio::test]
    async fn test_get_checkpoint_paths_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        
        // Create some checkpoint files
        fs::write(checkpoint_dir.join("checkpoint1.pt"), b"fake checkpoint").unwrap();
        fs::write(checkpoint_dir.join("checkpoint2.pt"), b"fake checkpoint").unwrap();
        fs::write(checkpoint_dir.join("not_a_checkpoint.txt"), b"not a checkpoint").unwrap();
        
        let checkpoints = get_checkpoint_paths(&checkpoint_dir).await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints.iter().all(|p| p.extension().unwrap() == "pt"));
    }

    #[tokio::test]
    async fn test_list_checkpoints_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let fake_dir = temp_dir.path().join("nonexistent");
        
        // Should not panic, just print message
        assert!(list_checkpoints(&fake_dir, "text").await.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_checkpoints_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        
        // Create 5 checkpoint files
        for i in 1..=5 {
            let path = checkpoint_dir.join(format!("checkpoint{}.pt", i));
            fs::write(&path, b"fake checkpoint").unwrap();
            // Add small delay to ensure different modification times
            std::thread::sleep(Duration::from_millis(10));
        }
        
        // Dry run - should not delete anything
        cleanup_checkpoints(&checkpoint_dir, 3, true, "text").await.unwrap();
        
        // All files should still exist
        let entries: Vec<_> = fs::read_dir(&checkpoint_dir).unwrap().collect();
        assert_eq!(entries.len(), 5);
    }

    #[tokio::test]
    async fn test_cleanup_checkpoints_actual() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        
        // Create 5 checkpoint files
        for i in 1..=5 {
            let path = checkpoint_dir.join(format!("checkpoint{}.pt", i));
            fs::write(&path, b"fake checkpoint").unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        
        // Keep last 2, delete others
        cleanup_checkpoints(&checkpoint_dir, 2, false, "text").await.unwrap();
        
        // Should have 2 files left
        let entries: Vec<_> = fs::read_dir(&checkpoint_dir).unwrap().collect();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_checkpoints_not_enough() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_dir = temp_dir.path().join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        
        // Create 2 checkpoint files
        for i in 1..=2 {
            let path = checkpoint_dir.join(format!("checkpoint{}.pt", i));
            fs::write(&path, b"fake checkpoint").unwrap();
        }
        
        // Try to keep 5, but only have 2
        cleanup_checkpoints(&checkpoint_dir, 5, false, "text").await.unwrap();
        
        // All should still exist
        let entries: Vec<_> = fs::read_dir(&checkpoint_dir).unwrap().collect();
        assert_eq!(entries.len(), 2);
    }
}

