//! Easy data transfer between local, S3, and training environments
//!
//! Provides seamless data pipeline for training workloads with
//! optimized transfer strategies.

use anyhow::{Context, Result};
use crate::config::Config;
use std::path::{Path, PathBuf};
use aws_sdk_s3::Client as S3Client;
use aws_sdk_ssm::Client as SsmClient;
use aws_config::{BehaviorVersion, SdkConfig};
use tracing::{info, warn};
use indicatif::{ProgressBar, ProgressStyle};

/// Data transfer source/destination
#[derive(Debug, Clone)]
pub enum DataLocation {
    Local(PathBuf),
    S3(String), // s3://bucket/key
    TrainingInstance(String, PathBuf), // instance_id, remote_path
}

/// Transfer options
#[derive(Debug, Clone)]
pub struct TransferOptions {
    pub parallel: Option<usize>, // Number of parallel transfers
    pub compression: bool,
    pub verify: bool, // Verify checksums
    pub resume: bool, // Resume interrupted transfers
    pub exclude: Vec<String>, // Patterns to exclude
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            parallel: Some(4),
            compression: false,
            verify: true,
            resume: true,
            exclude: vec!["*.pyc".to_string(), "__pycache__".to_string()],
        }
    }
}

/// Parse location string into DataLocation
fn parse_location(loc: &str) -> Result<DataLocation> {
    if loc.starts_with("s3://") {
        Ok(DataLocation::S3(loc.to_string()))
    } else if loc.contains(':') && !loc.starts_with("file://") {
        // Assume instance:path format
        let parts: Vec<&str> = loc.splitn(2, ':').collect();
        if parts.len() == 2 {
            Ok(DataLocation::TrainingInstance(
                parts[0].to_string(),
                PathBuf::from(parts[1]),
            ))
        } else {
            anyhow::bail!("Invalid instance location format. Use instance-id:/path/to/dest");
        }
    } else {
        // Local path
        Ok(DataLocation::Local(PathBuf::from(loc)))
    }
}

/// CLI handler for transfer command
pub async fn handle_transfer(
    source: String,
    destination: String,
    parallel: Option<usize>,
    compress: bool,
    verify: bool,
    resume: bool,
    config: &Config,
    ) -> Result<()> {
    let src = parse_location(&source)?;
    let dst = parse_location(&destination)?;
    
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    
    // Create DataTransfer with config reference
    // Note: DataTransfer needs to own Config, so we clone it
    let transfer = DataTransfer::new(config.clone(), Some(&aws_config));
    
    let options = TransferOptions {
        parallel,
        compression: compress,
        verify,
        resume,
        exclude: vec!["*.pyc".to_string(), "__pycache__".to_string()],
    };
    
    transfer.transfer(&src, &dst, options).await
        .map_err(|e| anyhow::anyhow!("Transfer failed: {}", e))?;
    
    println!("Transfer complete: {} -> {}", source, destination);
    Ok(())
}

/// Transfer data between locations
pub struct DataTransfer {
    s3_client: Option<S3Client>,
    ssm_client: Option<SsmClient>,
    config: Config,
}

impl DataTransfer {
    pub fn new(config: Config, aws_config: Option<&SdkConfig>) -> Self {
        let s3_client = aws_config.map(|cfg| S3Client::new(cfg));
        let ssm_client = aws_config.map(|cfg| SsmClient::new(cfg));
        Self {
            s3_client,
            ssm_client,
            config,
        }
    }
    
    /// Transfer data from source to destination
    pub async fn transfer(
        &self,
        source: &DataLocation,
        destination: &DataLocation,
        options: TransferOptions,
    ) -> Result<()> {
        info!("Transferring data: {:?} -> {:?}", source, destination);
        
        match (source, destination) {
            (DataLocation::Local(src), DataLocation::S3(dst)) => {
                self.local_to_s3(src, dst, options).await
            }
            (DataLocation::S3(src), DataLocation::Local(dst)) => {
                self.s3_to_local(src, dst, options).await
            }
            (DataLocation::Local(src), DataLocation::TrainingInstance(instance_id, dst)) => {
                self.local_to_instance(src, instance_id, dst, options).await
            }
            (DataLocation::S3(src), DataLocation::TrainingInstance(instance_id, dst)) => {
                self.s3_to_instance(src, instance_id, dst, options).await
            }
            _ => anyhow::bail!("Unsupported transfer combination"),
        }
    }
    
    /// Transfer from local to S3 with optimization
    async fn local_to_s3(
        &self,
        source: &Path,
        s3_path: &str,
        options: TransferOptions,
    ) -> Result<()> {
        let client = self.s3_client.as_ref()
            .context("S3 client not configured")?;
        
        let (bucket, key) = parse_s3_path(s3_path)?;
        
        // Use s5cmd for faster parallel uploads if available
        if check_s5cmd() && options.parallel.is_some() {
            return self.s5cmd_upload(source, s3_path, options).await;
        }
        
        // Fallback to AWS SDK
        if source.is_dir() {
            self.upload_directory(client, source, &bucket, &key, options).await
        } else {
            self.upload_file(client, source, &bucket, &key).await
        }
    }
    
    /// Transfer from S3 to local with optimization
    async fn s3_to_local(
        &self,
        s3_path: &str,
        destination: &Path,
        options: TransferOptions,
    ) -> Result<()> {
        let client = self.s3_client.as_ref()
            .context("S3 client not configured")?;
        
        let (bucket, key) = parse_s3_path(s3_path)?;
        
        // Use s5cmd for faster parallel downloads
        if check_s5cmd() && options.parallel.is_some() {
            return self.s5cmd_download(s3_path, destination, options).await;
        }
        
        // Fallback to AWS SDK
        self.download_from_s3(client, &bucket, &key, destination).await
    }
    
    /// Transfer from local to training instance
    async fn local_to_instance(
        &self,
        source: &Path,
        instance_id: &str,
        remote_path: &PathBuf,
        _options: TransferOptions,
    ) -> Result<()> {
        // Strategy: Upload to S3 first, then download on instance
        // This is faster and more reliable than direct transfer
        
        let s3_bucket = self.config.aws.as_ref()
            .and_then(|c| c.s3_bucket.as_ref())
            .context("S3 bucket not configured")?;
        
        let temp_s3_path = format!("s3://{}/trainctl-temp/{}/{}", 
            s3_bucket, instance_id, 
            source.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("data"));
        
        // Upload to S3
        info!("Uploading to S3 staging area: {}", temp_s3_path);
        self.local_to_s3(
            source,
            &temp_s3_path,
            TransferOptions::default(),
        ).await?;
        
        // Download on instance via SSM
        info!("Downloading on instance {} to {}", instance_id, remote_path.display());
        
        let ssm_client = self.ssm_client.as_ref()
            .context("SSM client not available. Ensure AWS credentials are configured.")?;
        
        // Ensure remote directory exists
        let mkdir_cmd = format!("mkdir -p {}", remote_path.parent().map(|p| p.to_string_lossy()).unwrap_or_else(|| ".".into()));
        execute_ssm_command(ssm_client, instance_id, &mkdir_cmd).await?;
        
        // Download from S3 using AWS CLI (available on most AMIs)
        let download_cmd = format!(
            "aws s3 cp {} {} --recursive",
            temp_s3_path,
            remote_path.display()
        );
        
        execute_ssm_command(ssm_client, instance_id, &download_cmd).await?;
        
        info!("Data transferred to instance {}:{}", instance_id, remote_path.display());
        Ok(())
    }
    
    /// Transfer from S3 to training instance (fastest path)
    async fn s3_to_instance(
        &self,
        s3_path: &str,
        instance_id: &str,
        remote_path: &PathBuf,
        _options: TransferOptions,
    ) -> Result<()> {
        // Use s5cmd on instance for fastest transfer (fallback to aws s3 if s5cmd not available)
        info!("Transferring {} to instance {}:{}", s3_path, instance_id, remote_path.display());
        
        let ssm_client = self.ssm_client.as_ref()
            .context("SSM client not available. Ensure AWS credentials are configured.")?;
        
        // Ensure remote directory exists
        let mkdir_cmd = format!("mkdir -p {}", remote_path.parent().map(|p| p.to_string_lossy()).unwrap_or_else(|| ".".into()));
        execute_ssm_command(ssm_client, instance_id, &mkdir_cmd).await?;
        
        // Try s5cmd first (faster), fallback to aws s3
        let s5cmd_cmd = format!(
            "if command -v s5cmd &> /dev/null; then s5cmd cp --recursive {} {}; else aws s3 cp {} {} --recursive; fi",
            s3_path, remote_path.display(), s3_path, remote_path.display()
        );
        
        execute_ssm_command(ssm_client, instance_id, &s5cmd_cmd).await?;
        
        info!("Data transferred to instance {}:{}", instance_id, remote_path.display());
        Ok(())
    }
    
    /// Upload directory with parallel transfers
    async fn upload_directory(
        &self,
        client: &S3Client,
        source: &Path,
        bucket: &str,
        prefix: &str,
        options: TransferOptions,
    ) -> Result<()> {
        use walkdir::WalkDir;
        
        let files: Vec<_> = WalkDir::new(source)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();
        
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap());
        
        let parallel = options.parallel.unwrap_or(4);
        let mut handles = Vec::new();
        
        for file in files {
            let client = client.clone();
            let bucket = bucket.to_string();
            let source_path = file.path().to_path_buf();
            let relative = source_path.strip_prefix(source)
                .unwrap_or(&source_path);
            let key = format!("{}/{}", prefix, relative.display());
            let pb = pb.clone();
            
            let handle = tokio::spawn(async move {
                let result = upload_single_file(&client, &bucket, &key, &source_path).await;
                pb.inc(1);
                result
            });
            
            handles.push(handle);
            
            // Limit concurrency
            if handles.len() >= parallel {
                // Wait for one to complete
                let (result, _idx, remaining) = futures::future::select_all(handles).await;
                match result {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => warn!("Upload failed: {}", e),
                    Err(e) => warn!("Task join error: {}", e),
                }
                handles = remaining;
            }
        }
        
        // Wait for remaining
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {},
                Ok(Err(e)) => warn!("Upload failed: {}", e),
                Err(e) => warn!("Task join error: {}", e),
            }
        }
        
        pb.finish_with_message("Upload complete");
        Ok(())
    }
    
    async fn upload_file(
        &self,
        client: &S3Client,
        source: &Path,
        bucket: &str,
        key: &str,
    ) -> Result<()> {
        upload_single_file(client, bucket, key, source).await
    }
    
    async fn download_from_s3(
        &self,
        client: &S3Client,
        bucket: &str,
        key: &str,
        destination: &Path,
    ) -> Result<()> {
        let response = client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .context("Download failed")?;
        
        let data = response.body.collect().await
            .context("Failed to read response")?;
        
        std::fs::write(destination, data.into_bytes())?;
        Ok(())
    }
    
    async fn s5cmd_upload(
        &self,
        source: &Path,
        s3_path: &str,
        options: TransferOptions,
    ) -> Result<()> {
        use std::process::Command;
        
        let mut cmd = Command::new("s5cmd");
        cmd.arg("cp");
        
        if let Some(parallel) = options.parallel {
            cmd.arg("--concurrency").arg(parallel.to_string());
        }
        
        if source.is_dir() {
            cmd.arg("--recursive");
        }
        
        cmd.arg(source.to_string_lossy().as_ref());
        cmd.arg(s3_path);
        
        let output = cmd.output()
            .context("Failed to execute s5cmd")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd upload failed: {}", stderr);
        }
        
        Ok(())
    }
    
    async fn s5cmd_download(
        &self,
        s3_path: &str,
        destination: &Path,
        options: TransferOptions,
    ) -> Result<()> {
        use std::process::Command;
        
        let mut cmd = Command::new("s5cmd");
        cmd.arg("cp");
        
        if let Some(parallel) = options.parallel {
            cmd.arg("--concurrency").arg(parallel.to_string());
        }
        
        cmd.arg("--recursive");
        cmd.arg(s3_path);
        cmd.arg(destination.to_string_lossy().as_ref());
        
        let output = cmd.output()
            .context("Failed to execute s5cmd")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("s5cmd download failed: {}", stderr);
        }
        
        Ok(())
    }
}

async fn upload_single_file(
    client: &S3Client,
    bucket: &str,
    key: &str,
    file_path: &Path,
) -> Result<()> {
    let body = aws_sdk_s3::primitives::ByteStream::from_path(file_path).await
        .context("Failed to read file")?;
    
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .context("Upload failed")?;
    
    Ok(())
}

// Use shared AWS utilities
use crate::aws_utils::execute_ssm_command;

pub fn parse_s3_path(s3_path: &str) -> Result<(String, String)> {
    if !s3_path.starts_with("s3://") {
        anyhow::bail!("S3 path must start with s3://");
    }
    
    let path = &s3_path[5..];
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    
    if parts.len() != 2 {
        anyhow::bail!("Invalid S3 path format. Expected s3://bucket/key");
    }
    
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn check_s5cmd() -> bool {
    which::which("s5cmd").is_ok()
}
