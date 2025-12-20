# Implementation Plan: Missing Paradigms

This document provides a concrete, step-by-step plan for implementing the missing architectural patterns identified in `MISSING_PARADIGMS.md`.

## Implementation Order (Dependencies)

1. **Custom Error Types** (Foundation - needed by everything else)
2. **Configuration Validation** (Early validation - prevents runtime errors)
3. **Retry Logic** (Uses custom error types)
4. **Resource Lifecycle Management** (Uses retry logic and error types)
5. **Graceful Shutdown** (Uses resource lifecycle)
6. **Cost Tracking** (Uses resource lifecycle)
7. **Observability** (Can be added incrementally)
8. **Advanced Testing** (Tests everything above)

## Phase 1: Custom Error Types (Week 1)

### Step 1.1: Create Error Module

**File:** `src/error.rs` (new)

```rust
use thiserror::Error;
use crate::provider::{ResourceState, ResourceId};

/// Main error type for runctl
#[derive(Error, Debug)]
pub enum TrainctlError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("Cloud provider error: {provider} - {message}")]
    CloudProvider {
        provider: String,
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("Resource error: {resource_type} - {operation} failed")]
    Resource {
        resource_type: String,
        operation: String,
        resource_id: Option<ResourceId>,
        state: Option<ResourceState>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("Retryable error (attempt {attempt}/{max_attempts}): {reason}")]
    Retryable {
        attempt: u32,
        max_attempts: u32,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("AWS SDK error: {0}")]
    Aws(#[from] aws_sdk_ec2::Error),
    
    #[error("S3 error: {0}")]
    S3(#[from] aws_sdk_s3::Error),
    
    #[error("SSM error: {0}")]
    Ssm(#[from] aws_sdk_ssm::Error),
    
    #[error("Validation error: {field} - {reason}")]
    Validation { field: String, reason: String },
    
    #[error("Cost tracking error: {0}")]
    CostTracking(String),
}

/// Configuration-specific errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid cloud provider: {0}")]
    InvalidProvider(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Invalid value for {field}: {reason}")]
    InvalidValue { field: String, reason: String },
    
    #[error("Config file not found: {0}")]
    NotFound(String),
    
    #[error("Failed to parse config: {0}")]
    ParseError(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, TrainctlError>;

/// Trait for determining if an error is retryable
pub trait IsRetryable {
    fn is_retryable(&self) -> bool;
}

impl IsRetryable for TrainctlError {
    fn is_retryable(&self) -> bool {
        matches!(self, TrainctlError::Retryable { .. })
            || matches!(self, TrainctlError::CloudProvider { .. })
            || matches!(self, TrainctlError::Io(_))
    }
}

/// Helper to convert AWS errors to TrainctlError
impl From<aws_sdk_ec2::types::SdkError<aws_sdk_ec2::error::CreateVolumeError>> for TrainctlError {
    fn from(err: aws_sdk_ec2::types::SdkError<aws_sdk_ec2::error::CreateVolumeError>) -> Self {
        TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: format!("EC2 operation failed: {}", err),
            source: Some(Box::new(err)),
        }
    }
}
```

### Step 1.2: Update Existing Code to Use New Error Types

**Migration Strategy:**
1. Start with new code (retry logic, resource lifecycle)
2. Gradually migrate existing modules:
   - `src/aws.rs` - Convert AWS operations
   - `src/ebs.rs` - Convert EBS operations
   - `src/s3.rs` - Convert S3 operations
   - `src/provider.rs` - Update trait methods

**Example Migration:**
```rust
// Before (src/aws.rs)
use anyhow::{Context, Result};

async fn create_instance(...) -> Result<()> {
    let aws_cfg = config.aws.as_ref()
        .context("AWS config not found")?;
    // ...
}

// After
use crate::error::{Result, TrainctlError, ConfigError};

async fn create_instance(...) -> Result<()> {
    let aws_cfg = config.aws.as_ref()
        .ok_or_else(|| TrainctlError::Config(
            ConfigError::MissingField("aws".to_string())
        ))?;
    // ...
}
```

### Step 1.3: Update lib.rs

```rust
// src/lib.rs
pub mod error;
pub use error::{Result, TrainctlError, ConfigError, IsRetryable};
```

## Phase 2: Configuration Validation (Week 1-2)

### Step 2.1: Add Validation Dependencies

**File:** `Cargo.toml`
```toml
[dependencies]
validator = { version = "0.18", features = ["derive"] }
```

### Step 2.2: Add Validation to Config

**File:** `src/config.rs`

```rust
use validator::Validate;
use crate::error::{Result, TrainctlError, ConfigError};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Config {
    #[validate]
    pub runpod: Option<RunpodConfig>,
    
    #[validate]
    pub aws: Option<AwsConfig>,
    
    #[validate]
    pub local: Option<LocalConfig>,
    
    #[validate]
    pub checkpoint: CheckpointConfig,
    
    #[validate]
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AwsConfig {
    #[validate(length(min = 1))]
    pub region: String,
    
    #[validate(length(min = 1))]
    pub default_instance_type: String,
    
    #[validate(length(min = 1))]
    pub default_ami: String,
    
    pub use_spot: bool,
    pub spot_max_price: Option<String>,
    pub iam_instance_profile: Option<String>,
    
    #[validate(url)]
    pub s3_bucket: Option<String>,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        // Schema validation
        self.validate()
            .map_err(|e| TrainctlError::Config(
                ConfigError::InvalidValue {
                    field: "config".to_string(),
                    reason: format!("Validation failed: {:?}", e),
                }
            ))?;
        
        // Business logic validation
        if self.runpod.is_none() && self.aws.is_none() {
            return Err(TrainctlError::Config(
                ConfigError::InvalidValue {
                    field: "providers".to_string(),
                    reason: "At least one cloud provider must be configured".to_string(),
                }
            ));
        }
        
        // Validate AWS credentials if AWS is configured
        if let Some(aws_cfg) = &self.aws {
            // Check if AWS credentials are accessible
            // This could check ~/.aws/credentials or environment variables
        }
        
        Ok(())
    }
}
```

### Step 2.3: Validate on Load

**File:** `src/config.rs` - Update `load()` method

```rust
pub fn load(path: Option<&str>) -> Result<Config> {
    let config = /* existing load logic */;
    config.validate()?;
    Ok(config)
}
```

## Phase 3: Retry Logic (Week 2)

### Step 3.1: Create Retry Module

**File:** `src/retry.rs` (new)

```rust
use crate::error::{Result, TrainctlError, IsRetryable};
use std::time::Duration;
use async_trait::async_trait;
use tracing::{warn, info};

#[async_trait]
pub trait RetryPolicy: Send + Sync {
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send;
}

pub struct ExponentialBackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
}

impl ExponentialBackoffPolicy {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.1,
        }
    }
    
    pub fn default() -> Self {
        Self::new(3)
    }
    
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let exponential = self.initial_delay.as_millis() as f64 
            * 2f64.powi(attempt as i32);
        let delay_ms = exponential.min(self.max_delay.as_millis() as f64);
        
        // Add jitter to prevent thundering herd
        let jitter = delay_ms * self.jitter_factor * fastrand::f64();
        Duration::from_millis((delay_ms + jitter) as u64)
    }
}

#[async_trait]
impl RetryPolicy for ExponentialBackoffPolicy {
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send,
    {
        let mut last_error = None;
        
        for attempt in 0..self.max_attempts {
            match f().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("Operation succeeded after {} retries", attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e);
                    let err = last_error.as_ref().unwrap();
                    
                    if !err.is_retryable() {
                        warn!("Non-retryable error, aborting: {}", err);
                        return Err(last_error.unwrap());
                    }
                    
                    if attempt == self.max_attempts - 1 {
                        warn!("Max retries ({}) reached", self.max_attempts);
                        return Err(TrainctlError::Retryable {
                            attempt: attempt + 1,
                            max_attempts: self.max_attempts,
                            reason: format!("{}", err),
                            source: Some(Box::new(err)),
                        });
                    }
                    
                    let backoff = self.calculate_backoff(attempt);
                    warn!("Retryable error (attempt {}/{}), retrying in {:?}: {}", 
                        attempt + 1, self.max_attempts, backoff, err);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
        
        Err(last_error.unwrap())
    }
}
```

### Step 3.2: Add Dependencies

**File:** `Cargo.toml`
```toml
[dependencies]
fastrand = "2.0"  # For jitter
```

### Step 3.3: Use Retry in AWS Operations

**File:** `src/aws.rs`

```rust
use crate::retry::{RetryPolicy, ExponentialBackoffPolicy};

async fn create_instance(...) -> Result<()> {
    let retry_policy = ExponentialBackoffPolicy::default();
    let client = retry_policy.execute_with_retry(|| {
        let config = aws_config.clone();
        async move {
            Ec2Client::new(&config)
        }
    }).await?;
    
    // Use client...
}
```

## Phase 4: Resource Lifecycle Management (Week 2-3)

### Step 4.1: Create Resource Lifecycle Module

**File:** `src/resource_lifecycle.rs` (new)

```rust
use crate::error::Result;
use crate::provider::{ResourceId, ResourceStatus, ResourceState};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHandle {
    pub id: ResourceId,
    pub provider: String,
    pub resource_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub job_id: Option<String>,
}

pub trait StateStore: Send + Sync {
    async fn save_resource(&self, job_id: &str, handle: &ResourceHandle) -> Result<()>;
    async fn load_resource(&self, job_id: &str) -> Result<Option<ResourceHandle>>;
    async fn list_resources(&self) -> Result<Vec<ResourceHandle>>;
    async fn delete_resource(&self, job_id: &str) -> Result<()>;
}

pub struct FileStateStore {
    state_dir: std::path::PathBuf,
}

impl FileStateStore {
    pub fn new(state_dir: impl AsRef<std::path::Path>) -> Self {
        Self {
            state_dir: state_dir.as_ref().to_path_buf(),
        }
    }
}

#[async_trait::async_trait]
impl StateStore for FileStateStore {
    async fn save_resource(&self, job_id: &str, handle: &ResourceHandle) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", job_id));
        let content = serde_json::to_string_pretty(handle)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }
    
    async fn load_resource(&self, job_id: &str) -> Result<Option<ResourceHandle>> {
        let path = self.state_dir.join(format!("{}.json", job_id));
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        let handle: ResourceHandle = serde_json::from_str(&content)?;
        Ok(Some(handle))
    }
    
    async fn list_resources(&self) -> Result<Vec<ResourceHandle>> {
        // Read all .json files in state_dir
        // ...
    }
    
    async fn delete_resource(&self, job_id: &str) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", job_id));
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

pub struct ResourceLifecycleManager {
    active_resources: Arc<Mutex<HashMap<String, ResourceHandle>>>,
    state_store: Arc<dyn StateStore>,
}

impl ResourceLifecycleManager {
    pub fn new(state_store: Arc<dyn StateStore>) -> Self {
        Self {
            active_resources: Arc::new(Mutex::new(HashMap::new())),
            state_store,
        }
    }
    
    pub async fn track_resource(
        &self,
        job_id: &str,
        handle: ResourceHandle,
    ) -> Result<()> {
        // Save to state store
        self.state_store.save_resource(job_id, &handle).await?;
        
        // Track in memory
        self.active_resources.lock().await.insert(
            job_id.to_string(),
            handle,
        );
        
        Ok(())
    }
    
    pub async fn cleanup_on_failure(
        &self,
        job_id: &str,
        provider: &dyn crate::provider::TrainingProvider,
    ) -> Result<()> {
        if let Some(handle) = self.active_resources.lock().await.remove(job_id) {
            tracing::warn!("Cleaning up resource {} due to failure", handle.id);
            
            // Attempt to terminate resource
            if let Err(e) = provider.terminate(&handle.id).await {
                tracing::error!("Failed to cleanup resource {}: {}", handle.id, e);
                // Continue cleanup even if termination fails
            }
            
            // Remove from state store
            self.state_store.delete_resource(job_id).await?;
        }
        
        Ok(())
    }
}
```

### Step 4.2: Integrate with Provider Operations

**File:** `src/aws.rs` - Update `create_instance`

```rust
use crate::resource_lifecycle::{ResourceLifecycleManager, ResourceHandle};

async fn create_instance(
    instance_type: String,
    use_spot: bool,
    spot_max_price: Option<String>,
    no_fallback: bool,
    config: &Config,
    aws_config: &aws_config::SdkConfig,
    lifecycle: &ResourceLifecycleManager,
) -> Result<String> {
    let instance_id = /* create instance logic */;
    
    let handle = ResourceHandle {
        id: instance_id.clone(),
        provider: "aws".to_string(),
        resource_type: "ec2-instance".to_string(),
        created_at: chrono::Utc::now(),
        job_id: None,
    };
    
    lifecycle.track_resource(&instance_id, handle).await?;
    
    Ok(instance_id)
}
```

## Phase 5: Graceful Shutdown (Week 3)

### Step 5.1: Create Shutdown Manager

**File:** `src/shutdown.rs` (new)

```rust
use crate::error::Result;
use crate::resource_lifecycle::ResourceLifecycleManager;
use tokio::signal;
use tokio::sync::broadcast;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub struct GracefulShutdownManager {
    shutdown_tx: broadcast::Sender<()>,
    lifecycle: Arc<ResourceLifecycleManager>,
    shutdown_timeout: Duration,
}

impl GracefulShutdownManager {
    pub fn new(
        lifecycle: Arc<ResourceLifecycleManager>,
        shutdown_timeout: Duration,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);
        Self {
            shutdown_tx,
            lifecycle,
            shutdown_timeout,
        }
    }
    
    pub fn shutdown_signal(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
    
    pub async fn wait_for_signal(&self) -> Result<()> {
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
            
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                    self.initiate_shutdown().await;
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, initiating graceful shutdown");
                    self.initiate_shutdown().await;
                }
            }
        }
        
        #[cfg(not(unix))]
        {
            signal::ctrl_c().await?;
            info!("Received Ctrl+C, initiating graceful shutdown");
            self.initiate_shutdown().await;
        }
        
        Ok(())
    }
    
    async fn initiate_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        
        let deadline = tokio::time::Instant::now() + self.shutdown_timeout;
        
        // Wait for active resources to complete
        loop {
            let active = self.lifecycle.active_resources.lock().await.len();
            
            if active == 0 {
                info!("All resources cleaned up, shutdown complete");
                return;
            }
            
            if tokio::time::Instant::now() > deadline {
                warn!("Shutdown timeout reached, {} resources still active", active);
                // Force cleanup could go here
                return;
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
```

### Step 5.2: Integrate with Main

**File:** `src/main.rs`

```rust
use crate::shutdown::GracefulShutdownManager;
use crate::resource_lifecycle::{ResourceLifecycleManager, FileStateStore};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Setup logging...
    
    // Setup resource lifecycle
    let state_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".runctl")
        .join("state");
    std::fs::create_dir_all(&state_dir)?;
    
    let state_store = Arc::new(FileStateStore::new(&state_dir));
    let lifecycle = Arc::new(ResourceLifecycleManager::new(state_store));
    let shutdown = Arc::new(GracefulShutdownManager::new(
        lifecycle.clone(),
        Duration::from_secs(30),
    ));
    
    // Spawn shutdown handler
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        if let Err(e) = shutdown_clone.wait_for_signal().await {
            eprintln!("Shutdown error: {}", e);
        }
    });
    
    // Execute command with shutdown awareness
    match cli.command {
        Commands::Local { script, args } => {
            local::train(script, args, &config, shutdown.shutdown_signal()).await?;
        }
        // ... other commands
    }
    
    Ok(())
}
```

## Phase 6: Cost Tracking (Week 3-4)

### Step 6.1: Create Cost Tracking Module

**File:** `src/cost.rs` (new)

```rust
use crate::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub compute_hours: f64,
    pub memory_gb_hours: f64,
    pub storage_gb_hours: f64,
    pub data_transfer_gb: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub total: f64,
    pub by_provider: HashMap<String, f64>,
    pub by_resource_type: HashMap<String, f64>,
    pub by_job: HashMap<String, f64>,
}

pub trait CostTracker: Send + Sync {
    async fn record_usage(
        &self,
        job_id: &str,
        provider: &str,
        usage: ResourceUsage,
    ) -> Result<()>;
    
    async fn get_job_cost(&self, job_id: &str) -> Result<CostBreakdown>;
    
    async fn get_total_cost(&self) -> Result<f64>;
}

pub struct InMemoryCostTracker {
    usage_records: Arc<Mutex<Vec<(String, String, ResourceUsage)>>>,
    cost_cache: Arc<Mutex<HashMap<String, CostBreakdown>>>,
}

impl InMemoryCostTracker {
    pub fn new() -> Self {
        Self {
            usage_records: Arc::new(Mutex::new(Vec::new())),
            cost_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl CostTracker for InMemoryCostTracker {
    async fn record_usage(
        &self,
        job_id: &str,
        provider: &str,
        usage: ResourceUsage,
    ) -> Result<()> {
        self.usage_records.lock().await.push((
            job_id.to_string(),
            provider.to_string(),
            usage,
        ));
        
        // Invalidate cache
        self.cost_cache.lock().await.remove(job_id);
        
        Ok(())
    }
    
    async fn get_job_cost(&self, job_id: &str) -> Result<CostBreakdown> {
        // Check cache first
        if let Some(cached) = self.cost_cache.lock().await.get(job_id) {
            return Ok(cached.clone());
        }
        
        // Calculate cost from usage records
        let records = self.usage_records.lock().await;
        let job_records: Vec<_> = records
            .iter()
            .filter(|(jid, _, _)| jid == job_id)
            .collect();
        
        let mut breakdown = CostBreakdown {
            total: 0.0,
            by_provider: HashMap::new(),
            by_resource_type: HashMap::new(),
            by_job: HashMap::new(),
        };
        
        // Calculate costs (simplified - would use actual pricing)
        for (_, provider, usage) in job_records {
            let cost = usage.compute_hours * 0.10; // Example: $0.10/hour
            breakdown.total += cost;
            *breakdown.by_provider.entry(provider.clone()).or_insert(0.0) += cost;
        }
        
        // Cache result
        self.cost_cache.lock().await.insert(job_id.to_string(), breakdown.clone());
        
        Ok(breakdown)
    }
    
    async fn get_total_cost(&self) -> Result<f64> {
        let records = self.usage_records.lock().await;
        let total: f64 = records
            .iter()
            .map(|(_, _, usage)| usage.compute_hours * 0.10)
            .sum();
        Ok(total)
    }
}
```

### Step 6.2: Integrate with Resource Lifecycle

**File:** `src/resource_lifecycle.rs` - Add cost tracking

```rust
use crate::cost::{CostTracker, ResourceUsage};

impl ResourceLifecycleManager {
    pub async fn record_resource_usage(
        &self,
        job_id: &str,
        provider: &str,
        cost_tracker: &dyn CostTracker,
    ) -> Result<()> {
        // Calculate usage from resource status
        // Record in cost tracker
        // ...
    }
}
```

## Phase 7: Observability (Week 4)

### Step 7.1: Add Metrics

**File:** `Cargo.toml`
```toml
[dependencies]
prometheus = "0.13"
```

**File:** `src/metrics.rs` (new)

```rust
use prometheus::{Counter, Histogram, Gauge, Registry};

pub struct OrchestrationMetrics {
    pub training_jobs_started: Counter,
    pub training_jobs_completed: Counter,
    pub training_jobs_failed: Counter,
    pub cloud_api_calls: Histogram,
    pub cost_accumulated: Gauge,
    pub resource_allocation_time: Histogram,
}

impl OrchestrationMetrics {
    pub fn new(registry: &Registry) -> Self {
        let training_jobs_started = Counter::new(
            "runctl_training_jobs_started_total",
            "Total number of training jobs started"
        ).unwrap();
        
        // ... other metrics
        
        registry.register(Box::new(training_jobs_started.clone())).unwrap();
        
        Self {
            training_jobs_started,
            // ...
        }
    }
}
```

## Phase 8: Testing (Ongoing)

### Step 8.1: Add Mock Dependencies

**File:** `Cargo.toml`
```toml
[dev-dependencies]
mockall = "0.12"
```

### Step 8.2: Create Test Utilities

**File:** `tests/test_utils.rs`

```rust
use mockall::mock;
use crate::provider::TrainingProvider;

mock! {
    pub Provider {}
    
    #[async_trait::async_trait]
    impl TrainingProvider for Provider {
        fn name(&self) -> &'static str;
        async fn create_resource(&self, instance_type: &str, options: CreateResourceOptions) -> Result<ResourceId>;
        // ... other methods
    }
}
```

## Implementation Timeline

- **Week 1:** Error types + Configuration validation
- **Week 2:** Retry logic + Resource lifecycle (basic)
- **Week 3:** Graceful shutdown + Cost tracking (basic)
- **Week 4:** Observability + Testing

## Migration Strategy

1. **Incremental Migration:** Don't change everything at once
2. **Feature Flags:** Use feature flags for new patterns
3. **Backward Compatibility:** Keep `anyhow::Result` as alias initially
4. **Testing:** Add tests for each phase before moving to next

## Success Criteria

- [ ] All error types use `TrainctlError`
- [ ] All cloud operations have retry logic
- [ ] Graceful shutdown saves checkpoints
- [ ] Resource cleanup happens on failure
- [ ] Cost tracking records all usage
- [ ] Configuration validates on load
- [ ] Metrics exported for observability

