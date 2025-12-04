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
    /// Default project name (auto-detected from current directory if not set)
    pub default_project_name: Option<String>,
    /// User identifier for multi-user environments (auto-detected from username if not set)
    pub user_id: Option<String>,
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
                default_project_name: None, // Auto-detect from current directory
                user_id: None, // Auto-detect from username
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
                    err.push_str("\n  Tip: Run 'trainctl init' to create a new config file");
                    err
                })?;
            Ok(config)
        } else {
            // Use defaults but warn if user explicitly provided a path
            if path.is_some() {
                eprintln!("WARNING: Config file not found: {}", config_path.display());
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
    println!("Created config file: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.runpod.is_some());
        assert!(config.aws.is_some());
        assert!(config.local.is_some());
        assert_eq!(config.checkpoint.save_interval, 5);
        assert_eq!(config.checkpoint.keep_last_n, 10);
    }

    #[test]
    fn test_config_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        let config = Config::default();
        assert!(config.save(&config_path).is_ok());
        assert!(config_path.exists());
        
        let loaded = Config::load(Some(&config_path)).unwrap();
        assert_eq!(loaded.checkpoint.save_interval, config.checkpoint.save_interval);
        assert_eq!(loaded.checkpoint.keep_last_n, config.checkpoint.keep_last_n);
    }

    #[test]
    fn test_config_load_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let fake_path = temp_dir.path().join("nonexistent.toml");
        
        // Should return default config
        let config = Config::load(Some(&fake_path)).unwrap();
        assert_eq!(config.checkpoint.save_interval, 5);
    }

    #[test]
    fn test_config_load_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.toml");
        std::fs::write(&config_path, "invalid toml content {").unwrap();
        
        let result = Config::load(Some(&config_path));
        assert!(result.is_err());
    }

    #[test]
    fn test_init_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("init_test.toml");
        
        assert!(init_config(&config_path).is_ok());
        assert!(config_path.exists());
        
        // Verify it's valid TOML
        let config = Config::load(Some(&config_path)).unwrap();
        assert!(config.runpod.is_some());
    }
}

