# Missing Paradigms and Architectural Patterns

Based on research of Rust CLI best practices and ML orchestration patterns (2024), this document identifies critical missing paradigms in `trainctl`.

## Critical Missing Patterns

### 1. Custom Error Types with Context Preservation

**Current State:**
- Uses `anyhow::Result` throughout (good for prototyping)
- No error categorization (retryable vs. permanent)
- Limited error context for debugging

**Missing:**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TrainctlError {
    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigError),
    
    #[error("Cloud provider error: {provider} - {message}")]
    CloudProviderError {
        provider: String,
        message: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    
    #[error("Retryable error (attempt {attempt}/{max_attempts}): {reason}")]
    RetryableError {
        attempt: u32,
        max_attempts: u32,
        reason: String,
    },
    
    #[error("Resource lifecycle error: {resource_type} - {operation}")]
    ResourceLifecycleError {
        resource_type: String,
        operation: String,
        state: ResourceState,
    },
}
```

**Impact:** Better error messages, retry decisions, and debugging.

### 2. Retry Logic with Exponential Backoff

**Current State:**
- No retry logic for cloud API calls
- AWS SDK has built-in retries, but not configurable
- No distinction between transient vs. permanent failures

**Missing:**
```rust
pub struct ExponentialBackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
}

#[async_trait]
pub trait RetryPolicy {
    async fn execute_with_retry<F, T, E>(&self, f: F) -> Result<T, E>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: IsRetryable + std::error::Error;
}

pub trait IsRetryable {
    fn is_retryable(&self) -> bool;
}
```

**Impact:** Resilience against transient cloud API failures, network issues.

### 3. Graceful Shutdown and Signal Handling

**Current State:**
- No signal handling (SIGTERM/SIGINT)
- No graceful shutdown
- Training jobs can be interrupted without checkpoint save
- Documented in `IMPLEMENTATION_GAPS.md` but not implemented

**Missing:**
```rust
use tokio::signal;
use tokio::sync::broadcast;

pub struct GracefulShutdownManager {
    shutdown_tx: broadcast::Sender<()>,
    active_jobs: Arc<tokio::sync::Mutex<HashMap<String, JobHandle>>>,
    shutdown_timeout: Duration,
}

impl GracefulShutdownManager {
    pub async fn wait_for_signal(&self) -> Result<()> {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
        
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, initiating graceful shutdown");
                self.initiate_shutdown().await;
            }
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT, initiating graceful shutdown");
                self.initiate_shutdown().await;
            }
        }
        Ok(())
    }
    
    async fn initiate_shutdown(&self) {
        // Save checkpoints
        // Clean up resources
        // Wait for active jobs with timeout
    }
}
```

**Impact:** Prevents data loss, enables clean resource cleanup.

### 4. Observability and Telemetry

**Current State:**
- Basic `tracing` logging (good foundation)
- No structured metrics
- No distributed tracing for multi-cloud operations
- No cost tracking integration

**Missing:**
```rust
pub struct OrchestrationMetrics {
    pub training_jobs_started: prometheus::Counter,
    pub training_jobs_completed: prometheus::Counter,
    pub training_jobs_failed: prometheus::Counter,
    pub cloud_api_calls: prometheus::Histogram,
    pub cost_accumulated: prometheus::Gauge,
    pub resource_allocation_time: prometheus::Histogram,
}

#[instrument(skip(runtime), fields(
    job_id = %job.id,
    provider = %job.provider,
    training_duration = field::Empty,
))]
pub async fn orchestrate_training(
    job: TrainingJob,
    runtime: Arc<OrchestrationRuntime>,
) -> Result<JobResult> {
    // Structured logging with correlation IDs
    // Metrics collection
    // Distributed tracing
}
```

**Impact:** Better debugging, cost tracking, performance monitoring.

### 5. Configuration Validation

**Current State:**
- Basic TOML deserialization
- Some validation in `Config::load()` but not comprehensive
- No validation of cloud provider credentials accessibility

**Missing:**
```rust
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct OrchestratorConfig {
    #[validate(length(min = 1))]
    pub cloud_providers: Vec<CloudProviderConfig>,
    
    #[validate(range(min = 1, max = 100))]
    pub max_parallel_jobs: u32,
    
    #[validate(url)]
    pub cost_tracking_endpoint: String,
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &OrchestratorConfig) -> Result<(), Vec<String>> {
        // Schema validation
        // Business logic validation
        // Credential accessibility checks
        // Provider-specific validation
    }
}
```

**Impact:** Fail fast on invalid config, better error messages.

### 6. Resource Lifecycle Management

**Current State:**
- Basic resource creation/termination
- No cleanup on failure
- No state persistence for resumability
- No resource ownership tracking

**Missing:**
```rust
#[async_trait]
pub trait ResourceManager: Send + Sync {
    async fn allocate(&self, spec: &ResourceSpec) -> Result<ResourceHandle>;
    async fn monitor(&self, handle: &ResourceHandle) -> Result<ResourceStatus>;
    async fn deallocate(&self, handle: &ResourceHandle) -> Result<()>;
}

pub struct CloudResourceLifecycle {
    managers: Arc<HashMap<String, Arc<dyn ResourceManager>>>,
    state_store: Arc<dyn StateStore>,
}

impl CloudResourceLifecycle {
    pub async fn track_resource_lifecycle(
        &self,
        job_id: &str,
        provider: &str,
        spec: ResourceSpec,
    ) -> Result<()> {
        // Allocate with state persistence
        // Monitor with automatic cleanup on failure
        // Deallocate with cleanup verification
    }
}
```

**Impact:** Prevents resource leaks, enables resumability, better cost control.

### 7. Cost Tracking Integration

**Current State:**
- Hardcoded cost estimates in `resources.rs`
- No real-time cost tracking
- No cost breakdown by provider/resource type
- No cost attribution to jobs

**Missing:**
```rust
pub trait CostTracker: Send + Sync {
    async fn record_resource_usage(
        &self,
        job_id: &str,
        provider: &str,
        usage: ResourceUsage,
    ) -> Result<()>;
    
    async fn get_job_cost(&self, job_id: &str) -> Result<CostBreakdown>;
}

#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub compute_hours: f64,
    pub memory_gb_hours: f64,
    pub storage_gb_hours: f64,
    pub data_transfer_gb: f64,
    pub timestamp: SystemTime,
}

#[derive(Debug)]
pub struct CostBreakdown {
    pub total: f64,
    pub by_provider: HashMap<String, f64>,
    pub by_resource_type: HashMap<String, f64>,
}
```

**Impact:** Cost visibility, budget management, chargeback.

### 8. Testing Strategies

**Current State:**
- Basic unit tests
- Integration tests exist
- No mocking for cloud providers
- No property-based testing
- No chaos engineering tests

**Missing:**
```rust
#[cfg(test)]
mod tests {
    use mockall::predicate::*;
    
    #[tokio::test]
    async fn test_retry_logic_with_transient_failure() {
        // Test retry behavior
    }
    
    #[tokio::test]
    async fn test_resource_lifecycle_cleanup_on_error() {
        // Test cleanup on failure
    }
    
    #[test]
    fn test_config_validation() {
        // Property-based config validation
    }
}
```

**Impact:** Higher confidence, catch bugs early, prevent regressions.

## Implementation Priority

### High Priority (Critical for Production)
1. **Graceful Shutdown** - Prevents data loss
2. **Custom Error Types** - Better debugging and user experience
3. **Retry Logic** - Resilience against transient failures
4. **Resource Lifecycle Management** - Prevents resource leaks

### Medium Priority (Important for Scale)
5. **Configuration Validation** - Fail fast, better errors
6. **Cost Tracking** - Cost visibility and control
7. **Observability** - Better debugging and monitoring

### Low Priority (Nice to Have)
8. **Advanced Testing** - Higher quality, but existing tests are adequate

## References

- Rust CLI Best Practices (2024): Error handling, retry logic, graceful shutdown
- ML Orchestration Patterns: Multi-cloud abstraction, resource lifecycle, cost tracking
- Clap Documentation: Custom error handling, validation
- Production Rust Patterns: Observability, testing strategies

## Related Documentation

- `docs/IMPLEMENTATION_GAPS.md` - Already documents graceful shutdown need
- `docs/PROVIDER_ARCHITECTURE.md` - Provider abstraction patterns
- `docs/REFERENCE_PATTERNS.md` - Patterns from reference repos

