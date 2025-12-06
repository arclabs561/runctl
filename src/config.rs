use crate::error::{Result, TrainctlError, ConfigError};
use clap::Subcommand;
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
                    .unwrap_or_else(|| {
                        // Fallback to current directory if config dir not available
                        PathBuf::from(".trainctl.toml")
                    })
            }
        };

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| TrainctlError::Config(ConfigError::ParseError(
                    format!("Failed to read config {}: {}", config_path.display(), e)
                )))?;
            let config: Config = toml::from_str(&content)
                .map_err(|_e| TrainctlError::Config(ConfigError::ParseError(
                    format!("Failed to parse config: {}\n  Common issues:\n    - Invalid TOML syntax\n    - Missing required fields\n    - Incorrect value types\n  Tip: Run 'trainctl init' to create a new config file", config_path.display())
                )))?;
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
            .map_err(|e| TrainctlError::Config(ConfigError::ParseError(
                format!("Failed to serialize config: {}", e)
            )))?;
        std::fs::write(path, content)
            .map_err(|e| TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write config {}: {}", path.display(), e),
            )))?;
        Ok(())
    }
}

#[derive(Subcommand, Clone)]
pub enum ConfigCommands {
    /// Show current configuration
    ///
    /// Displays the current configuration, including defaults and loaded values.
    /// Shows the config file path if one is loaded.
    ///
    /// Examples:
    ///   trainctl config show
    ///   trainctl config show --output json
    Show,
    /// Set a configuration value
    ///
    /// Sets a configuration value using dot notation (e.g., aws.region).
    /// The value is written to the config file. Use 'show' to verify changes.
    ///
    /// Examples:
    ///   trainctl config set aws.region us-west-2
    ///   trainctl config set aws.default_instance_type g4dn.xlarge
    ///   trainctl config set checkpoint.save_interval 10
    Set {
        /// Configuration key (dot notation, e.g., aws.region)
        #[arg(value_name = "KEY")]
        key: String,
        /// Configuration value
        #[arg(value_name = "VALUE")]
        value: String,
        /// Config file path (default: .trainctl.toml or ~/.config/trainctl/config.toml)
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Validate configuration file
    ///
    /// Checks if the configuration file is valid TOML and contains all required fields.
    /// Reports any errors or warnings.
    ///
    /// Examples:
    ///   trainctl config validate
    ///   trainctl config validate --config ~/.config/trainctl/config.toml
    Validate {
        /// Config file path (default: .trainctl.toml or ~/.config/trainctl/config.toml)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

pub fn init_config(output: &Path) -> Result<()> {
    let config = Config::default();
    config.save(output)?;
    println!("Created config file: {}", output.display());
    Ok(())
}

pub async fn handle_command(cmd: ConfigCommands, config_path: Option<&Path>, output_format: &str) -> Result<()> {
    match cmd {
        ConfigCommands::Show { .. } => {
            let config = Config::load(config_path)?;
            if output_format == "json" {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!("Configuration:");
                if let Some(aws) = &config.aws {
                    println!("  AWS:");
                    println!("    Region: {}", aws.region);
                    println!("    Default Instance Type: {}", aws.default_instance_type);
                    println!("    Default AMI: {}", aws.default_ami);
                    println!("    Use Spot: {}", aws.use_spot);
                    if let Some(price) = &aws.spot_max_price {
                        println!("    Spot Max Price: {}", price);
                    }
                    if let Some(profile) = &aws.iam_instance_profile {
                        println!("    IAM Instance Profile: {}", profile);
                    }
                    if let Some(bucket) = &aws.s3_bucket {
                        println!("    S3 Bucket: {}", bucket);
                    }
                    if let Some(project) = &aws.default_project_name {
                        println!("    Default Project Name: {}", project);
                    }
                    if let Some(user) = &aws.user_id {
                        println!("    User ID: {}", user);
                    }
                }
                if let Some(runpod) = &config.runpod {
                    println!("  RunPod:");
                    println!("    Default GPU: {}", runpod.default_gpu);
                    println!("    Default Disk: {} GB", runpod.default_disk_gb);
                    println!("    Default Image: {}", runpod.default_image);
                }
                if let Some(local) = &config.local {
                    println!("  Local:");
                    println!("    Default Device: {}", local.default_device);
                    println!("    Checkpoint Dir: {}", local.checkpoint_dir.display());
                }
                println!("  Checkpoint:");
                println!("    Directory: {}", config.checkpoint.dir.display());
                println!("    Save Interval: {} epochs", config.checkpoint.save_interval);
                println!("    Keep Last N: {}", config.checkpoint.keep_last_n);
                println!("  Monitoring:");
                println!("    Log Directory: {}", config.monitoring.log_dir.display());
                println!("    Update Interval: {} seconds", config.monitoring.update_interval_secs);
                println!("    Enable Warnings: {}", config.monitoring.enable_warnings);
            }
            Ok(())
        }
        ConfigCommands::Set { key, value, config: config_file } => {
            let config_path = config_file.as_deref().or(config_path);
            let mut config = Config::load(config_path)?;
            
            // Clone value for display before it's moved
            let value_display = value.clone();
            
            // Simple key-value setting (basic implementation)
            // For full dot notation support, would need a more sophisticated parser
            if key == "aws.region" {
                if let Some(aws) = &mut config.aws {
                    aws.region = value;
                } else {
                    return Err(TrainctlError::Config(ConfigError::MissingField("aws".to_string())));
                }
            } else if key == "aws.default_instance_type" {
                if let Some(aws) = &mut config.aws {
                    aws.default_instance_type = value;
                } else {
                    return Err(TrainctlError::Config(ConfigError::MissingField("aws".to_string())));
                }
            } else if key == "aws.default_ami" {
                if let Some(aws) = &mut config.aws {
                    aws.default_ami = value;
                } else {
                    return Err(TrainctlError::Config(ConfigError::MissingField("aws".to_string())));
                }
            } else if key == "aws.use_spot" {
                if let Some(aws) = &mut config.aws {
                    let bool_value = value.parse::<bool>()
                        .map_err(|_| TrainctlError::Config(ConfigError::InvalidValue {
                            field: key.clone(),
                            reason: format!("Invalid boolean value: {}", value_display),
                        }))?;
                    aws.use_spot = bool_value;
                } else {
                    return Err(TrainctlError::Config(ConfigError::MissingField("aws".to_string())));
                }
            } else if key == "checkpoint.save_interval" {
                config.checkpoint.save_interval = value.parse::<u32>()
                    .map_err(|_| TrainctlError::Config(ConfigError::InvalidValue {
                        field: key.clone(),
                        reason: format!("Invalid number: {}", value_display),
                    }))?;
            } else if key == "checkpoint.keep_last_n" {
                config.checkpoint.keep_last_n = value.parse::<u32>()
                    .map_err(|_| TrainctlError::Config(ConfigError::InvalidValue {
                        field: key.clone(),
                        reason: format!("Invalid number: {}", value_display),
                    }))?;
            } else {
                return Err(TrainctlError::Config(ConfigError::InvalidValue {
                    field: key,
                    reason: "Unknown configuration key. Supported keys: aws.region, aws.default_instance_type, aws.default_ami, aws.use_spot, checkpoint.save_interval, checkpoint.keep_last_n".to_string(),
                }));
            }
            
            let save_path = config_path.unwrap_or_else(|| Path::new(".trainctl.toml"));
            config.save(save_path)?;
            
            if output_format == "json" {
                let result = serde_json::json!({
                    "success": true,
                    "key": key,
                    "value": value_display,
                    "config_path": save_path.display().to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Set {} = {}", key, value_display);
                println!("Configuration saved to: {}", save_path.display());
            }
            Ok(())
        }
        ConfigCommands::Validate { config: config_file } => {
            let config_path = config_file.as_deref().or(config_path);
            match Config::load(config_path) {
                Ok(_config) => {
                    println!("✓ Configuration is valid");
                    if config_path.is_none() {
                        println!("  Using default configuration (no config file found)");
                    } else {
                        println!("  Loaded from: {}", config_path.unwrap().display());
                    }
                    Ok(())
                }
                Err(e) => {
                    eprintln!("✗ Configuration validation failed:");
                    eprintln!("  {}", e);
                    Err(e)
                }
            }
        }
    }
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

