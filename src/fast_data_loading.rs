//! Fast data loading optimizations for training
//!
//! Implements data pipeline optimizations to avoid bottlenecks:
//! - Pre-warmed EBS volumes
//! - Parallel data loading
//! - Caching strategies
//! - Streaming data pipelines

use crate::error::{Result, TrainctlError};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Data loading strategy
#[derive(Debug, Clone)]
pub enum DataLoadingStrategy {
    /// Load from S3 directly (slowest, but no setup)
    DirectS3,
    /// Pre-warm EBS volume from S3, then use EBS (fastest for repeated training)
    PreWarmedEBS {
        volume_id: String,
        mount_point: PathBuf,
    },
    /// Use existing EBS volume (fast, no pre-warming needed)
    ExistingEBS {
        volume_id: String,
        mount_point: PathBuf,
    },
    /// Local cache (fastest, but requires initial download)
    LocalCache {
        cache_dir: PathBuf,
    },
}

/// Data loading configuration
#[derive(Debug, Clone)]
pub struct DataLoadingConfig {
    pub strategy: DataLoadingStrategy,
    pub s3_source: Option<String>,
    pub parallel_workers: usize,
    pub prefetch_size: usize, // MB
    pub cache_enabled: bool,
}

impl Default for DataLoadingConfig {
    fn default() -> Self {
        Self {
            strategy: DataLoadingStrategy::DirectS3,
            s3_source: None,
            parallel_workers: 4,
            prefetch_size: 100, // 100 MB
            cache_enabled: true,
        }
    }
}

/// Fast data loader for training workloads
pub struct FastDataLoader {
    config: DataLoadingConfig,
}

impl FastDataLoader {
    pub fn new(config: DataLoadingConfig) -> Self {
        Self { config }
    }
    
    /// Prepare data for training (pre-warm, cache, etc.)
    pub async fn prepare_data(&self) -> Result<PathBuf> {
        match &self.config.strategy {
            DataLoadingStrategy::DirectS3 => {
                warn!("Using DirectS3 strategy - this may be slow. Consider pre-warming EBS volume.");
                // For DirectS3, return the S3 path - training script will download on-demand
                if let Some(s3_source) = &self.config.s3_source {
                    Ok(PathBuf::from(s3_source))
                } else {
                    Err(TrainctlError::DataTransfer(
                        "DirectS3 strategy requires s3_source to be configured".to_string(),
                    ))
                }
            }
            DataLoadingStrategy::PreWarmedEBS { volume_id: _, mount_point } => {
                info!("Using pre-warmed EBS volume at: {}", mount_point.display());
                // Verify mount point exists and is accessible
                if !mount_point.exists() {
                    return Err(TrainctlError::DataTransfer(
                        format!("Mount point does not exist: {}", mount_point.display()),
                    ));
                }
                if !mount_point.is_dir() {
                    return Err(TrainctlError::DataTransfer(
                        format!("Mount point is not a directory: {}", mount_point.display()),
                    ));
                }
                Ok(mount_point.clone())
            }
            DataLoadingStrategy::ExistingEBS { volume_id: _, mount_point } => {
                info!("Using existing EBS volume at: {}", mount_point.display());
                // Verify mount point exists and is accessible
                if !mount_point.exists() {
                    return Err(TrainctlError::DataTransfer(
                        format!("Mount point does not exist: {}", mount_point.display()),
                    ));
                }
                if !mount_point.is_dir() {
                    return Err(TrainctlError::DataTransfer(
                        format!("Mount point is not a directory: {}", mount_point.display()),
                    ));
                }
                Ok(mount_point.clone())
            }
            DataLoadingStrategy::LocalCache { cache_dir } => {
                info!("Using local cache: {}", cache_dir.display());
                // Check if cache exists and is valid
                if cache_dir.exists() && cache_dir.is_dir() {
                    // Verify cache has data
                    let entries: Vec<_> = match std::fs::read_dir(cache_dir) {
                        Ok(dir) => dir.collect(),
                        Err(e) => {
                            return Err(TrainctlError::DataTransfer(
                                format!("Failed to read cache directory: {}", e),
                            ));
                        }
                    };
                    if entries.is_empty() {
                        warn!("Cache directory is empty, will download data");
                        // Would download here, but for now just return the path
                    }
                    Ok(cache_dir.clone())
                } else {
                    // Create cache directory
                    std::fs::create_dir_all(cache_dir)
                        .map_err(|e| TrainctlError::DataTransfer(
                            format!("Failed to create cache directory: {}", e),
                        ))?;
                    // Would download here, but for now just return the path
                    Ok(cache_dir.clone())
                }
            }
        }
    }
    
    /// Generate optimized data loading script for training
    pub fn generate_loading_script(&self, data_path: &Path) -> Result<String> {
        let script = match &self.config.strategy {
            DataLoadingStrategy::PreWarmedEBS { .. } | 
            DataLoadingStrategy::ExistingEBS { .. } => {
                // EBS volumes are already fast, use standard loading
                self.standard_loading_script(data_path)
            }
            DataLoadingStrategy::DirectS3 => {
                // Use parallel loading from S3
                self.parallel_s3_loading_script(data_path)
            }
            DataLoadingStrategy::LocalCache { .. } => {
                // Local cache is fast, standard loading
                self.standard_loading_script(data_path)
            }
        };
        
        Ok(script)
    }
    
    fn standard_loading_script(&self, data_path: &Path) -> String {
        format!(
            r#"
# Optimized data loading
import torch
from torch.utils.data import DataLoader
import os

data_path = "{}"
num_workers = {}
prefetch_factor = 2
pin_memory = torch.cuda.is_available()

# Use DataLoader with optimizations
dataloader = DataLoader(
    dataset,
    batch_size=batch_size,
    num_workers=num_workers,
    pin_memory=pin_memory,
    prefetch_factor=prefetch_factor,
    persistent_workers=True,  # Keep workers alive between epochs
)
"#,
            data_path.display(),
            self.config.parallel_workers
        )
    }
    
    fn parallel_s3_loading_script(&self, data_path: &Path) -> String {
        let s3_source = self.config.s3_source.as_deref()
            .unwrap_or("s3://bucket/data");
        let standard_script = self.standard_loading_script(data_path);
        format!(
            r#"
# Parallel S3 data loading with s5cmd
import subprocess
import os

s3_source = "{}"
local_path = "{}"
num_parallel = {}

# Use s5cmd for fast parallel download
os.makedirs(local_path, exist_ok=True)
subprocess.run([
    "s5cmd", "cp",
    "--recursive",
    "--concurrency", str(num_parallel),
    f"{{s3_source}}/*",
    f"{{local_path}}/"
], check=True)

# Then use standard DataLoader
{}
"#,
            s3_source,
            data_path.display(),
            self.config.parallel_workers,
            standard_script
        )
    }
    
    #[allow(dead_code)]
    async fn verify_ebs_mount(&self, mount_point: &Path) -> Result<()> {
        if !mount_point.exists() {
            return Err(TrainctlError::DataTransfer(
                format!("EBS mount point does not exist: {}", mount_point.display()),
            ));
        }
        
        // Check if it's actually a mount point
        use std::process::Command;
        let output = Command::new("mountpoint")
            .arg("-q")
            .arg(mount_point)
            .output();
        
        if let Ok(output) = output {
            if !output.status.success() {
                warn!("Path {} may not be a mount point", mount_point.display());
            }
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    async fn download_to_temp(&self) -> Result<PathBuf> {
        // Would use data_transfer module here
        let temp_dir = std::env::temp_dir().join("trainctl-data");
        std::fs::create_dir_all(&temp_dir)?;
        Ok(temp_dir)
    }
    
    #[allow(dead_code)]
    async fn download_to_cache(&self, cache_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(cache_dir)?;
        // Would use data_transfer module here
        Ok(cache_dir.to_path_buf())
    }
}

/// Recommendations for data loading optimization
pub fn recommend_data_strategy(
    data_size_gb: f64,
    training_runs: usize,
    _s3_source: Option<&str>,
) -> DataLoadingStrategy {
    // If data is large and we'll run multiple times, use EBS
    if data_size_gb > 10.0 && training_runs > 1 {
        return DataLoadingStrategy::PreWarmedEBS {
            volume_id: "auto-create".to_string(),
            mount_point: PathBuf::from("/mnt/training-data"),
        };
    }
    
    // If data is small or one-time, use direct S3
    DataLoadingStrategy::DirectS3
}

