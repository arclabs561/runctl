use crate::error::{Result, TrainctlError};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tokio::time::{sleep, Duration};

pub async fn monitor(
    log: Option<PathBuf>,
    checkpoint: Option<PathBuf>,
    follow: bool,
) -> Result<()> {
    let has_log = log.is_some();
    let has_checkpoint = checkpoint.is_some();

    if let Some(log_path) = &log {
        crate::validation::validate_path(&log_path.display().to_string())?;
        monitor_log(log_path, follow).await?;
    }

    if let Some(checkpoint_dir) = &checkpoint {
        crate::validation::validate_path(&checkpoint_dir.display().to_string())?;
        monitor_checkpoint(checkpoint_dir).await?;
    }

    if !has_log && !has_checkpoint {
        println!("No log or checkpoint specified. Use --log or --checkpoint");
    }

    Ok(())
}

async fn monitor_log(log_path: &Path, follow: bool) -> Result<()> {
    if !log_path.exists() {
        println!("Log file not found: {}", log_path.display());
        println!("Waiting for log file to be created...");

        // Wait for file to be created
        let mut attempts = 0;
        while !log_path.exists() && attempts < 60 {
            sleep(Duration::from_secs(1)).await;
            attempts += 1;
        }

        if !log_path.exists() {
            return Err(TrainctlError::ResourceNotFound {
                resource_type: "log file".to_string(),
                resource_id: log_path.display().to_string(),
            });
        }
    }

    println!("Monitoring log: {}", log_path.display());
    println!("{:-<80}", "");

    if follow {
        // Follow mode - watch for changes
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        let mut watcher =
            notify::recommended_watcher(move |event: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = event {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        let _ = tx.try_send(event);
                    }
                }
            })
            .map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to create file watcher: {}", e),
                ))
            })?;

        watcher
            .watch(log_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to watch log file: {}", e),
                ))
            })?;

        let mut last_pos = 0u64;
        loop {
            tokio::select! {
                _ = rx.recv() => {
                    // File changed, read new lines
                    if let Ok(mut file) = fs::File::open(log_path) {
                        use std::io::Seek;
                        file.seek(std::io::SeekFrom::Start(last_pos))?;
                        let mut reader = BufReader::new(file);

                        let mut line = String::new();
                        while reader.read_line(&mut line)? > 0 {
                            print!("{}", line);
                            line.clear();
                        }

                        last_pos = reader.stream_position()?;
                    }
                }
                _ = sleep(Duration::from_secs(1)) => {
                    // Periodic check
                }
            }
        }
    } else {
        // One-time read of last N lines
        if let Ok(file) = fs::File::open(log_path) {
            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().collect::<std::result::Result<_, _>>()?;

            let last_n = 20;
            let start = if lines.len() > last_n {
                lines.len() - last_n
            } else {
                0
            };

            println!("Last {} lines:", last_n);
            for line in &lines[start..] {
                print!("{}", line);
            }
        }
    }

    Ok(())
}

async fn monitor_checkpoint(checkpoint_dir: &Path) -> Result<()> {
    if !checkpoint_dir.exists() {
        println!(
            "Checkpoint directory not found: {}",
            checkpoint_dir.display()
        );
        return Ok(());
    }

    println!("Monitoring checkpoints in: {}", checkpoint_dir.display());

    let mut last_checkpoints = Vec::new();

    loop {
        let mut checkpoints = Vec::new();
        if let Ok(entries) = fs::read_dir(checkpoint_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("pt") {
                    if let Ok(metadata) = fs::metadata(&path) {
                        checkpoints.push((path.clone(), metadata.modified()?, metadata.len()));
                    }
                }
            }
        }

        checkpoints.sort_by(|a, b| b.1.cmp(&a.1));

        // Check for new checkpoints
        if checkpoints != last_checkpoints {
            println!("\n{} Checkpoints found:", checkpoints.len());
            for (path, modified, size) in &checkpoints[..checkpoints.len().min(5)] {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!(
                    "  {} - {} ({})",
                    file_name,
                    format!("{:?}", modified),
                    format_size(*size)
                );
            }
            last_checkpoints = checkpoints;
        }

        sleep(Duration::from_secs(10)).await;
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2}{}", size, UNITS[unit_idx])
}
