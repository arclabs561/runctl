//! Local training execution
//!
//! Provides functionality for running training scripts locally on your machine.
//! Automatically detects and uses `uv` for Python scripts when available, falling
//! back to `python3` if `uv` is not installed.
//!
//! ## Features
//!
//! - **Automatic Python detection**: Detects `.py` files and uses appropriate interpreter
//! - **Environment variables**: Sets `TRAINCTL_CHECKPOINT_DIR` and `TRAINCTL_DEVICE` from config
//! - **Session tracking**: Creates and saves training session metadata
//! - **Helpful error messages**: Provides suggestions when scripts fail or are not found
//!
//! ## Usage
//!
//! ```rust,no_run
//! use runctl::{local, Config};
//!
//! # async fn example() -> runctl::error::Result<()> {
//! let config = Config::load(None)?;
//! local::train("train.py".into(), vec!["--epochs".to_string(), "10".to_string()], &config).await?;
//! # Ok(())
//! # }
//! ```

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::training::TrainingSession;
use crate::utils::ensure_dir;
use std::path::PathBuf;
use std::process::Command;
use tracing::info;

/// Execute a training script locally
///
/// Runs the specified script with the given arguments. For Python scripts,
/// automatically uses `uv` if available, otherwise falls back to `python3`.
/// For other scripts, assumes they are executable.
///
/// # Arguments
///
/// * `script` - Path to the training script (Python `.py` file or executable)
/// * `args` - Additional arguments to pass to the script
/// * `config` - Configuration containing checkpoint directory and device settings
///
/// # Errors
///
/// Returns `TrainctlError::ResourceNotFound` if the script doesn't exist,
/// or `TrainctlError::Resource` if the script execution fails.
///
/// # Examples
///
/// ```rust,no_run
/// use runctl::{local, Config};
///
/// # async fn example() -> runctl::error::Result<()> {
/// let config = Config::load(None)?;
/// // Run a Python training script
/// local::train("train.py".into(), vec![], &config).await?;
///
/// // Run with arguments
/// local::train(
///     "train.py".into(),
///     vec!["--epochs".to_string(), "50".to_string()],
///     &config
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn train(script: PathBuf, args: Vec<String>, config: &Config) -> Result<()> {
    crate::validation::validate_path_path(&script)?;

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
    let sessions_dir = PathBuf::from(".runctl");
    session.save(&sessions_dir).map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to save training session: {}",
            e
        )))
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
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to execute script {}: {}",
            script.display(),
            e
        )))
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
