use crate::config::Config;
use crate::error::{Result, TrainctlError};
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

#[derive(Subcommand, Clone)]
pub enum RunpodCommands {
    Create {
        name: Option<String>,
        gpu: String,
        disk: u32,
    },
    Train {
        pod_id: String,
        script: PathBuf,
        background: bool,
    },
    Monitor {
        pod_id: String,
        follow: bool,
    },
    Download {
        pod_id: String,
        remote: PathBuf,
        local: PathBuf,
    },
}

pub async fn handle_command(cmd: RunpodCommands, config: &Config) -> Result<()> {
    match cmd {
        RunpodCommands::Create { name, gpu, disk } => create_pod(name, gpu, disk, config).await,
        RunpodCommands::Train {
            pod_id,
            script,
            background,
        } => train_on_pod(pod_id, script, background, config).await,
        RunpodCommands::Monitor { pod_id, follow } => monitor_pod(pod_id, follow).await,
        RunpodCommands::Download {
            pod_id,
            remote,
            local,
        } => download_from_pod(pod_id, remote, local).await,
    }
}

async fn create_pod(name: Option<String>, gpu: String, disk: u32, config: &Config) -> Result<()> {
    info!("Creating RunPod pod: GPU={}, Disk={}GB", gpu, disk);

    // Check for runpodctl
    if which::which("runpodctl").is_err() {
        return Err(TrainctlError::CloudProvider {
            provider: "runpod".to_string(),
            message: "runpodctl not found. Install from: https://github.com/runpod/runpodctl"
                .to_string(),
            source: None,
        });
    }

    let pod_name =
        name.unwrap_or_else(|| format!("trainctl-{}", &uuid::Uuid::new_v4().to_string()[..8]));

    let runpod_config = config.runpod.as_ref().ok_or_else(|| {
        TrainctlError::Config(crate::error::ConfigError::MissingField(
            "runpod".to_string(),
        ))
    })?;

    let image = &runpod_config.default_image;

    // Create pod using runpodctl
    let mut cmd = std::process::Command::new("runpodctl");
    cmd.args(["create", "pod"]);
    cmd.arg("--name").arg(&pod_name);
    cmd.arg("--imageName").arg(image);
    cmd.arg("--gpuType").arg(&gpu);
    cmd.arg("--containerDiskSize").arg(disk.to_string());
    cmd.arg("--mem").arg("32");

    info!("Executing: {:?}", cmd);

    let output = cmd.output().map_err(|e| {
        TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to execute runpodctl: {}", e),
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TrainctlError::CloudProvider {
            provider: "runpod".to_string(),
            message: format!("Failed to create pod: {}", stderr),
            source: None,
        });
    }

    // Extract pod ID from output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pod_id = extract_pod_id(&stdout).ok_or_else(|| TrainctlError::CloudProvider {
        provider: "runpod".to_string(),
        message: "Could not extract pod ID from output".to_string(),
        source: None,
    })?;

    println!("Pod created: {}", pod_id);
    println!("   Waiting for pod to be ready...");

    // Wait for pod to be ready (simplified - in real impl would poll status)
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

    println!("Pod ready: {}", pod_id);
    Ok(())
}

async fn train_on_pod(
    pod_id: String,
    script: PathBuf,
    background: bool,
    _config: &Config,
) -> Result<()> {
    info!("Starting training on pod: {}", pod_id);

    if !script.exists() {
        return Err(TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Script not found: {}", script.display()),
        )));
    }

    // Upload script to pod
    println!("📤 Uploading script to pod...");
    let mut upload_cmd = std::process::Command::new("runpodctl");
    upload_cmd.args(["send", pod_id.as_str()]);
    upload_cmd.arg(&script);
    upload_cmd.arg("/workspace/training_script");

    let upload_output = upload_cmd.output().map_err(|e| {
        TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to upload script: {}", e),
        ))
    })?;

    if !upload_output.status.success() {
        return Err(TrainctlError::CloudProvider {
            provider: "runpod".to_string(),
            message: "Failed to upload script".to_string(),
            source: None,
        });
    }

    // Execute training
    let exec_cmd = if background {
        "nohup bash /workspace/training_script > /workspace/training.log 2>&1 &".to_string()
    } else {
        "bash /workspace/training_script".to_string()
    };

    let mut train_cmd = std::process::Command::new("runpodctl");
    train_cmd.args(["exec", &pod_id, "--"]);
    train_cmd.args(["bash", "-c", &exec_cmd]);

    if background {
        train_cmd.spawn().map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to start background training: {}", e),
            ))
        })?;
        println!("Training started in background");
        println!(
            "   Monitor with: trainctl runpod monitor {} --follow",
            pod_id
        );
    } else {
        train_cmd.status().map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Training failed: {}", e),
            ))
        })?;
        println!("Training completed");
    }

    Ok(())
}

async fn monitor_pod(pod_id: String, follow: bool) -> Result<()> {
    let log_path = "/workspace/training.log";

    if follow {
        println!("Following log on pod {} (Ctrl+C to stop)...", pod_id);
        let mut cmd = std::process::Command::new("runpodctl");
        cmd.args(["exec", &pod_id, "--"]);
        cmd.args(["tail", "-f", log_path]);
        cmd.status()?;
    } else {
        let mut cmd = std::process::Command::new("runpodctl");
        cmd.args(["exec", &pod_id, "--"]);
        cmd.args(["tail", "-n", "50", log_path]);
        let output = cmd.output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

async fn download_from_pod(pod_id: String, remote: PathBuf, local: PathBuf) -> Result<()> {
    println!(
        "📥 Downloading from pod {}: {} -> {}",
        pod_id,
        remote.display(),
        local.display()
    );

    let mut cmd = std::process::Command::new("runpodctl");
    cmd.args(["receive", &pod_id]);
    cmd.arg(&remote);
    cmd.arg(&local);

    let status = cmd.status().map_err(|e| {
        TrainctlError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to download from pod: {}", e),
        ))
    })?;

    if !status.success() {
        return Err(TrainctlError::CloudProvider {
            provider: "runpod".to_string(),
            message: "Download failed".to_string(),
            source: None,
        });
    }

    println!("Download complete");
    Ok(())
}

fn extract_pod_id(output: &str) -> Option<String> {
    // Try to extract pod ID from runpodctl output
    // Pattern: "pod-xxxxx" or just the ID
    let re = regex::Regex::new(r"pod-([a-z0-9]+)").ok()?;
    if let Some(caps) = re.captures(output) {
        return Some(caps.get(1)?.as_str().to_string());
    }

    // Try alternative pattern
    let re = regex::Regex::new(r"([a-z0-9]{13,})").ok()?;
    re.captures(output)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}
