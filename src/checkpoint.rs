use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

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
    List { dir: PathBuf },
    Info { path: PathBuf },
    Resume { path: PathBuf, script: PathBuf },
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

pub async fn handle_command(cmd: CheckpointCommands) -> Result<()> {
    match cmd {
        CheckpointCommands::List { dir } => list_checkpoints(&dir).await,
        CheckpointCommands::Info { path } => show_info(&path).await,
        CheckpointCommands::Resume { path, script } => resume_from(&path, &script).await,
        CheckpointCommands::Cleanup { dir, keep_last_n, dry_run } => {
            cleanup_checkpoints(&dir, keep_last_n, dry_run).await
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

async fn list_checkpoints(dir: &Path) -> Result<()> {
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

    println!("Checkpoints in {}:", dir.display());
    println!("{:-<80}", "");
    println!("{:<50} {:<20} {:<10}", "Path", "Modified", "Size");
    println!("{:-<80}", "");

    for (path, modified) in checkpoints {
        let size = fs::metadata(&path)?.len();
        let size_str = format_size(size);
        let modified_str = format!("{:?}", modified);
        println!("{:<50} {:<20} {:<10}", 
            path.file_name().unwrap().to_string_lossy(),
            modified_str,
            size_str
        );
    }

    Ok(())
}

async fn show_info(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Checkpoint not found: {}", path.display());
    }

    let metadata = fs::metadata(path)?;
    println!("Checkpoint: {}", path.display());
    println!("Size: {}", format_size(metadata.len()));
    println!("Modified: {:?}", metadata.modified()?);

    // Try to extract checkpoint info using training module
    if let Ok(info) = crate::training::extract_checkpoint_info(path) {
        if let Some(epoch) = info.epoch {
            println!("Epoch: {}", epoch);
        }
        if let Some(loss) = info.loss {
            println!("Loss: {:.4}", loss);
        }
    }

    // Try to load PyTorch checkpoint metadata if possible
    // (This would require torch-sys or similar - simplified for now)
    println!("\nNote: Use PyTorch to inspect checkpoint contents:");
    println!("  python -c \"import torch; ckpt = torch.load('{}', map_location='cpu'); print(ckpt.keys())\"", path.display());

    Ok(())
}

async fn resume_from(checkpoint: &Path, script: &Path) -> Result<()> {
    if !checkpoint.exists() {
        anyhow::bail!("Checkpoint not found: {}", checkpoint.display());
    }

    if !script.exists() {
        anyhow::bail!("Script not found: {}", script.display());
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

async fn cleanup_checkpoints(dir: &Path, keep_last_n: usize, dry_run: bool) -> Result<()> {
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
        println!("✅ Only {} checkpoint(s), nothing to clean up", checkpoints.len());
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
            .with_context(|| format!("Failed to delete {}", path.display()))?;
        println!("  ✓ Deleted {}", path.display());
    }

    println!("✅ Cleanup complete");
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

