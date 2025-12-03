use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub runpod: Option<RunpodConfig>,
    pub aws: Option<AwsConfig>,
    pub local: Option<LocalConfig>,
    pub checkpoint: CheckpointConfig,
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunpodConfig {
    pub api_key: Option<String>,
    pub default_gpu: String,
    pub default_disk_gb: u32,
    pub default_image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsConfig {
    pub region: String,
    pub default_instance_type: String,
    pub default_ami: String,
    pub use_spot: bool,
    pub spot_max_price: Option<String>,
    pub iam_instance_profile: Option<String>,
    pub s3_bucket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalConfig {
    pub default_device: String,
    pub checkpoint_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    pub dir: PathBuf,
    pub save_interval: u32,
    pub keep_last_n: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub log_dir: PathBuf,
    pub update_interval_secs: u64,
    pub enable_warnings: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            runpod: Some(RunpodConfig {
                api_key: None,
                default_gpu: "NVIDIA GeForce RTX 4080 SUPER".to_string(),
                default_disk_gb: 30,
                default_image: "runpod/pytorch:2.1.0-py3.10-cuda11.8.0-devel-ubuntu22.04".to_string(),
            }),
            aws: Some(AwsConfig {
                region: "us-east-1".to_string(),
                default_instance_type: "t3.medium".to_string(),
                default_ami: "ami-08fa3ed5577079e64".to_string(), // Amazon Linux 2023
                use_spot: true,
                spot_max_price: None,
                iam_instance_profile: None,
                s3_bucket: None,
            }),
            local: Some(LocalConfig {
                default_device: "auto".to_string(),
                checkpoint_dir: PathBuf::from("checkpoints"),
            }),
            checkpoint: CheckpointConfig {
                dir: PathBuf::from("checkpoints"),
                save_interval: 5,
                keep_last_n: 10,
            },
            monitoring: MonitoringConfig {
                log_dir: PathBuf::from("logs"),
                update_interval_secs: 10,
                enable_warnings: true,
            },
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            p.to_path_buf()
        } else {
            // Try .trainctl.toml in current dir, then ~/.config/trainctl/config.toml
            let local = PathBuf::from(".trainctl.toml");
            if local.exists() {
                local
            } else {
                dirs::config_dir()
                    .map(|d| d.join("trainctl").join("config.toml"))
                    .unwrap_or_else(|| PathBuf::from(".trainctl.toml"))
            }
        };

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config: {}", config_path.display()))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| {
                    let mut err = format!("Failed to parse config: {}", config_path.display());
                    err.push_str("\n  Common issues:");
                    err.push_str("\n    - Invalid TOML syntax");
                    err.push_str("\n    - Missing required fields");
                    err.push_str("\n    - Incorrect value types");
                    err.push_str(&format!("\n  Tip: Run 'trainctl init' to create a new config file"));
                    err
                })?;
            Ok(config)
        } else {
            // Use defaults but warn if user explicitly provided a path
            if path.is_some() {
                eprintln!("⚠️  Config file not found: {}", config_path.display());
                eprintln!("   Using default configuration. Run 'trainctl init' to create a config file.");
            }
            Ok(Config::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;
        Ok(())
    }
}

pub fn init_config(output: &Path) -> Result<()> {
    let config = Config::default();
    config.save(output)?;
    println!("✅ Created config file: {}", output.display());
    Ok(())
}

