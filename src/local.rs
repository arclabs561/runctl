use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::training::TrainingSession;
use crate::utils::ensure_dir;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

pub async fn train(script: PathBuf, args: Vec<String>, config: &Config) -> Result<()> {
    crate::validation::validate_path(&script.display().to_string())?;

    if !script.exists() {
        let mut err = format!("Script not found: {}", script.display());

        // Suggest common fixes
        if let Some(parent) = script.parent() {
            if !parent.exists() {
                err.push_str(&format!(
                    "\n  Directory does not exist: {}",
                    parent.display()
                ));
            }
        }

        // Check for common alternatives
        if let Some(file_name) = script.file_name() {
            let current_dir = std::env::current_dir()?;
            if let Ok(entries) = std::fs::read_dir(&current_dir) {
                let mut suggestions = Vec::new();
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if let Some(file_str) = file_name.to_str() {
                            if name.contains(file_str) {
                                suggestions.push(name.to_string());
                            }
                        }
                    }
                }
                if !suggestions.is_empty() {
                    err.push_str("\n  Did you mean one of these?");
                    for sug in suggestions.iter().take(5) {
                        err.push_str(&format!("\n    - {}", sug));
                    }
                }
            }
        }

        err.push_str("\n  Tip: Use absolute paths or check your current directory with 'pwd'");
        return Err(TrainctlError::ResourceNotFound {
            resource_type: "script".to_string(),
            resource_id: script.display().to_string(),
        });
    }

    info!("Starting local training: {}", script.display());

    // Create training session
    let checkpoint_dir = config
        .local
        .as_ref()
        .map(|c| c.checkpoint_dir.clone())
        .unwrap_or_else(|| PathBuf::from("checkpoints"));
    ensure_dir(&checkpoint_dir)?;

    let session = TrainingSession::new("local".to_string(), script.clone(), checkpoint_dir.clone());

    // Save session metadata
    let sessions_dir = PathBuf::from(".trainctl");
    session.save(&sessions_dir).map_err(|e| {
        TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to save training session: {}", e),
        ))
    })?;

    // Check if script is Python and use uv if available
    let is_python = script
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s == "py")
        .unwrap_or(false);

    let mut cmd = if is_python {
        // Try uv first, fallback to python
        if which::which("uv").is_ok() {
            let mut c = Command::new("uv");
            c.arg("run");
            c.arg(&script);
            c
        } else {
            let mut c = Command::new("python3");
            c.arg(&script);
            c
        }
    } else {
        // Assume it's executable
        Command::new(&script)
    };

    cmd.args(&args);

    // Set environment variables from config
    if let Some(local_config) = &config.local {
        cmd.env("TRAINCTL_CHECKPOINT_DIR", &local_config.checkpoint_dir);
        cmd.env("TRAINCTL_DEVICE", &local_config.default_device);
    }

    info!("Executing: {:?}", cmd);

    let status = cmd.status().map_err(|e| {
        TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to execute script {}: {}", script.display(), e),
        ))
    })?;

    if !status.success() {
        let mut err = format!("Training failed with exit code: {:?}", status.code());

        // Provide helpful suggestions
        err.push_str("\n\n  Troubleshooting:");
        err.push_str("\n    - Check script syntax: python3 -m py_compile <script>");
        err.push_str("\n    - Verify dependencies are installed");
        err.push_str("\n    - Check script logs for detailed error messages");
        err.push_str("\n    - Run with --verbose for more details");

        if let Some(code) = status.code() {
            match code {
                127 => {
                    err.push_str("\n    - Command not found - check PATH or install missing tools")
                }
                126 => err.push_str("\n    - Permission denied - check file permissions"),
                1 => err.push_str("\n    - Script error - check script output above"),
                _ => {}
            }
        }

        return Err(TrainctlError::Resource {
            resource_type: "training".to_string(),
            operation: "execute".to_string(),
            resource_id: Some(script.display().to_string()),
            message: err,
            source: None,
        });
    }

    info!("Training completed successfully");
    Ok(())
}
