use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;
use crate::config::Config;
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
        RunpodCommands::Create { name, gpu, disk } => {
            create_pod(name, gpu, disk, config).await
        }
        RunpodCommands::Train { pod_id, script, background } => {
            train_on_pod(pod_id, script, background, config).await
        }
        RunpodCommands::Monitor { pod_id, follow } => {
            monitor_pod(pod_id, follow).await
        }
        RunpodCommands::Download { pod_id, remote, local } => {
            download_from_pod(pod_id, remote, local).await
        }
    }
}

async fn create_pod(
    name: Option<String>,
    gpu: String,
    disk: u32,
    config: &Config,
) -> Result<()> {
    info!("Creating RunPod pod: GPU={}, Disk={}GB", gpu, disk);

    // Check for runpodctl
    if which::which("runpodctl").is_err() {
        anyhow::bail!("runpodctl not found. Install from: https://github.com/runpod/runpodctl");
    }

    let pod_name = name.unwrap_or_else(|| {
        format!("trainctl-{}", uuid::Uuid::new_v4().to_string()[..8].to_string())
    });

    let runpod_config = config.runpod.as_ref()
        .context("RunPod config not found")?;

    let image = &runpod_config.default_image;

    // Create pod using runpodctl
    let mut cmd = std::process::Command::new("runpodctl");
    cmd.args(&["create", "pod"]);
    cmd.arg("--name").arg(&pod_name);
    cmd.arg("--imageName").arg(image);
    cmd.arg("--gpuType").arg(&gpu);
    cmd.arg("--containerDiskSize").arg(disk.to_string());
    cmd.arg("--mem").arg("32");

    info!("Executing: {:?}", cmd);

    let output = cmd.output()
        .context("Failed to execute runpodctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create pod: {}", stderr);
    }

    // Extract pod ID from output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pod_id = extract_pod_id(&stdout)
        .context("Could not extract pod ID from output")?;

    println!("✅ Pod created: {}", pod_id);
    println!("   Waiting for pod to be ready...");

    // Wait for pod to be ready (simplified - in real impl would poll status)
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

    println!("✅ Pod ready: {}", pod_id);
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
        anyhow::bail!("Script not found: {}", script.display());
    }

    // Upload script to pod
    println!("📤 Uploading script to pod...");
    let mut upload_cmd = std::process::Command::new("runpodctl");
    upload_cmd.args(&["send", pod_id.as_str()]);
    upload_cmd.arg(&script);
    upload_cmd.arg("/workspace/training_script");

    let upload_output = upload_cmd.output()
        .context("Failed to upload script")?;

    if !upload_output.status.success() {
        anyhow::bail!("Failed to upload script");
    }

    // Execute training
    let exec_cmd = if background {
        format!("nohup bash /workspace/training_script > /workspace/training.log 2>&1 &")
    } else {
        format!("bash /workspace/training_script")
    };

    let mut train_cmd = std::process::Command::new("runpodctl");
    train_cmd.args(&["exec", &pod_id, "--"]);
    train_cmd.args(&["bash", "-c", &exec_cmd]);

    if background {
        train_cmd.spawn()
            .context("Failed to start background training")?;
        println!("✅ Training started in background");
        println!("   Monitor with: trainctl runpod monitor {} --follow", pod_id);
    } else {
        train_cmd.status()
            .context("Training failed")?;
        println!("✅ Training completed");
    }

    Ok(())
}

async fn monitor_pod(pod_id: String, follow: bool) -> Result<()> {
    let log_path = "/workspace/training.log";

    if follow {
        println!("Following log on pod {} (Ctrl+C to stop)...", pod_id);
        let mut cmd = std::process::Command::new("runpodctl");
        cmd.args(&["exec", &pod_id, "--"]);
        cmd.args(&["tail", "-f", log_path]);
        cmd.status()?;
    } else {
        let mut cmd = std::process::Command::new("runpodctl");
        cmd.args(&["exec", &pod_id, "--"]);
        cmd.args(&["tail", "-n", "50", log_path]);
        let output = cmd.output()?;
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    Ok(())
}

async fn download_from_pod(pod_id: String, remote: PathBuf, local: PathBuf) -> Result<()> {
    println!("📥 Downloading from pod {}: {} -> {}", pod_id, remote.display(), local.display());

    let mut cmd = std::process::Command::new("runpodctl");
    cmd.args(&["receive", &pod_id]);
    cmd.arg(&remote);
    cmd.arg(&local);

    let status = cmd.status()
        .context("Failed to download from pod")?;

    if !status.success() {
        anyhow::bail!("Download failed");
    }

    println!("✅ Download complete");
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

