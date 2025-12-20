# Quick Start: Implementation Guide

This is a practical, step-by-step guide to implement the missing paradigms. Start here for immediate action.

## Prerequisites

```bash
# Add required dependencies
cargo add validator --features derive
cargo add fastrand
cargo add mockall --dev
```

## Phase 1: Custom Error Types (Start Here - 2-3 hours)

### Step 1: Create Error Module

**Create:** `src/error.rs`

```rust
use thiserror::Error;
use crate::provider::ResourceId;

#[derive(Error, Debug)]
pub enum TrainctlError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("Cloud provider error: {provider} - {message}")]
    CloudProvider {
        provider: String,
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("AWS SDK error: {0}")]
    Aws(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Invalid value for {field}: {reason}")]
    InvalidValue { field: String, reason: String },
}

pub type Result<T> = std::result::Result<T, TrainctlError>;

pub trait IsRetryable {
    fn is_retryable(&self) -> bool;
}

impl IsRetryable for TrainctlError {
    fn is_retryable(&self) -> bool {
        matches!(self, TrainctlError::CloudProvider { .. })
            || matches!(self, TrainctlError::Io(_))
    }
}
```

### Step 2: Update lib.rs

```rust
// src/lib.rs
pub mod error;
pub use error::{Result, TrainctlError, ConfigError, IsRetryable};
```

### Step 3: Migrate One Module First (EBS - Smallest Impact)

**Update:** `src/ebs.rs`

```rust
// Change from:
use anyhow::{Context, Result};

// To:
use crate::error::{Result, TrainctlError, ConfigError};

// Example migration:
// Before:
.await
.context("Failed to create volume")?;

// After:
.await
.map_err(|e| TrainctlError::CloudProvider {
    provider: "aws".to_string(),
    message: format!("Failed to create volume: {}", e),
    source: Some(Box::new(e)),
})?;
```

**Test:** `cargo test --lib ebs`

## Phase 2: Retry Logic (3-4 hours)

### Step 1: Create Retry Module

**Create:** `src/retry.rs`

```rust
use crate::error::{Result, TrainctlError, IsRetryable};
use std::time::Duration;
use async_trait::async_trait;

pub struct ExponentialBackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl ExponentialBackoffPolicy {
    pub fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
        }
    }
    
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let exponential = self.initial_delay.as_millis() as f64 * 2f64.powi(attempt as i32);
        let delay_ms = exponential.min(self.max_delay.as_millis() as f64);
        Duration::from_millis(delay_ms as u64)
    }
    
    pub async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send,
    {
        for attempt in 0..self.max_attempts {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.is_retryable() || attempt == self.max_attempts - 1 {
                        return Err(e);
                    }
                    let backoff = self.calculate_backoff(attempt);
                    tracing::warn!("Retry {}/{} after {:?}", attempt + 1, self.max_attempts, backoff);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
        unreachable!()
    }
}
```

### Step 2: Use in EBS Operations

**Update:** `src/ebs.rs`

```rust
use crate::retry::ExponentialBackoffPolicy;

async fn create_volume(...) -> Result<()> {
    let retry = ExponentialBackoffPolicy::default();
    let volume_id = retry.execute_with_retry(|| {
        let client = client.clone();
        let size = size;
        async move {
            client
                .create_volume()
                .size(size)
                .send()
                .await
                .map_err(|e| TrainctlError::CloudProvider {
                    provider: "aws".to_string(),
                    message: format!("EC2 create_volume failed: {}", e),
                    source: Some(Box::new(e)),
                })
        }
    }).await?;
    
    // ... rest of function
}
```

## Phase 3: Configuration Validation (2 hours)

### Step 1: Add Validation to Config

**Update:** `src/config.rs`

```rust
use validator::Validate;
use crate::error::{Result, TrainctlError, ConfigError};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Config {
    #[validate]
    pub aws: Option<AwsConfig>,
    // ... other fields
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AwsConfig {
    #[validate(length(min = 1))]
    pub region: String,
    
    #[validate(length(min = 1))]
    pub default_instance_type: String,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        // Schema validation
        self.validate()
            .map_err(|e| TrainctlError::Config(
                ConfigError::InvalidValue {
                    field: "config".to_string(),
                    reason: format!("{:?}", e),
                }
            ))?;
        
        // Business logic
        if self.aws.is_none() && self.runpod.is_none() {
            return Err(TrainctlError::Config(
                ConfigError::InvalidValue {
                    field: "providers".to_string(),
                    reason: "At least one provider required".to_string(),
                }
            ));
        }
        
        Ok(())
    }
}

// Update load() method
pub fn load(path: Option<&str>) -> Result<Config> {
    let config = /* existing load logic */;
    config.validate()?;
    Ok(config)
}
```

## Phase 4: Graceful Shutdown (3-4 hours)

### Step 1: Create Shutdown Manager

**Create:** `src/shutdown.rs`

```rust
use tokio::signal;
use tokio::sync::broadcast;
use std::time::Duration;
use tracing::info;

pub struct GracefulShutdown {
    shutdown_tx: broadcast::Sender<()>,
}

impl GracefulShutdown {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);
        Self { shutdown_tx }
    }
    
    pub fn signal(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
    
    pub async fn wait_for_signal(&self) {
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt()).unwrap();
            
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("SIGTERM received");
                }
                _ = sigint.recv() => {
                    info!("SIGINT received");
                }
            }
        }
        
        #[cfg(not(unix))]
        {
            signal::ctrl_c().await.unwrap();
            info!("Ctrl+C received");
        }
        
        let _ = self.shutdown_tx.send(());
    }
}
```

### Step 2: Integrate with Main

**Update:** `src/main.rs`

```rust
use crate::shutdown::GracefulShutdown;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Setup logging...
    
    // Setup graceful shutdown
    let shutdown = Arc::new(GracefulShutdown::new());
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        shutdown_clone.wait_for_signal().await;
    });
    
    // Execute command with shutdown awareness
    match cli.command {
        Commands::Local { script, args } => {
            local::train(script, args, &config, shutdown.signal()).await?;
        }
        // ... other commands
    }
    
    Ok(())
}
```

### Step 3: Update Local Training

**Update:** `src/local.rs`

```rust
use tokio::sync::broadcast;

pub async fn train(
    script: PathBuf,
    args: Vec<String>,
    config: &Config,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<()> {
    // ... existing setup
    
    // Check for shutdown signal during execution
    tokio::select! {
        result = execute_training(...) => {
            result
        }
        _ = shutdown.recv() => {
            info!("Shutdown signal received, saving checkpoint...");
            // Save checkpoint logic
            Err(TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Training interrupted by user"
            )))
        }
    }
}
```

## Testing Strategy

### Unit Tests for Each Phase

**Create:** `tests/error_test.rs`

```rust
use runctl::error::*;

#[test]
fn test_retryable_error() {
    let err = TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "test".to_string(),
        source: None,
    };
    assert!(err.is_retryable());
}

#[test]
fn test_config_error() {
    let err = ConfigError::MissingField("aws".to_string());
    let runctl_err: TrainctlError = err.into();
    assert!(!runctl_err.is_retryable());
}
```

**Create:** `tests/retry_test.rs`

```rust
use runctl::retry::ExponentialBackoffPolicy;
use runctl::error::*;

#[tokio::test]
async fn test_retry_succeeds_on_second_attempt() {
    let policy = ExponentialBackoffPolicy::default();
    let mut attempts = 0;
    
    let result = policy.execute_with_retry(|| {
        attempts += 1;
        async move {
            if attempts < 2 {
                Err(TrainctlError::CloudProvider {
                    provider: "test".to_string(),
                    message: "transient".to_string(),
                    source: None,
                })
            } else {
                Ok(())
            }
        }
    }).await;
    
    assert!(result.is_ok());
    assert_eq!(attempts, 2);
}
```

## Migration Checklist

- [ ] Phase 1: Error types created and tested
- [ ] Phase 1: EBS module migrated to new error types
- [ ] Phase 2: Retry logic implemented and tested
- [ ] Phase 2: EBS operations use retry logic
- [ ] Phase 3: Config validation added
- [ ] Phase 3: Config validates on load
- [ ] Phase 4: Shutdown manager created
- [ ] Phase 4: Main integrates shutdown
- [ ] Phase 4: Local training respects shutdown

## Next Steps After Quick Start

Once these 4 phases are complete, proceed with:
- Resource lifecycle management
- Cost tracking
- Advanced observability
- Comprehensive testing

See `IMPLEMENTATION_PLAN.md` for full details.

