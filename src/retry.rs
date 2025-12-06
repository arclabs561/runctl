//! Retry logic with exponential backoff
//!
//! Provides retry policies for handling transient failures
//! in cloud API calls and other operations.

use crate::error::{IsRetryable, Result, TrainctlError};
use std::future::Future;
use std::time::Duration;
use tracing::{info, warn};

/// Default retry configuration constants
const DEFAULT_INITIAL_RETRY_DELAY_MS: u64 = 100;
const DEFAULT_MAX_RETRY_DELAY_SECS: u64 = 30;
const DEFAULT_JITTER_FACTOR: f64 = 0.1;
const DEFAULT_MAX_ATTEMPTS: u32 = 3;
const CLOUD_API_MAX_ATTEMPTS: u32 = 5;

/// Retry policy trait
///
/// Note: Using async fn in traits generates a clippy warning about auto trait bounds,
/// but this is acceptable for our use case. The alternative (explicit Future return types)
/// adds significant complexity without clear benefits for this API.
#[allow(async_fn_in_trait)]
pub trait RetryPolicy: Send + Sync {
    /// Execute a function with retry logic
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T>> + Send;
}

/// Exponential backoff retry policy
pub struct ExponentialBackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
}

impl ExponentialBackoffPolicy {
    /// Create a new exponential backoff policy
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            initial_delay: Duration::from_millis(DEFAULT_INITIAL_RETRY_DELAY_MS),
            max_delay: Duration::from_secs(DEFAULT_MAX_RETRY_DELAY_SECS),
            jitter_factor: DEFAULT_JITTER_FACTOR,
        }
    }

    /// Create default policy (3 attempts)
    ///
    /// Note: This is not the `Default` trait implementation to avoid
    /// confusion with the standard library's `Default::default()`.
    pub fn default_policy() -> Self {
        Self::new(DEFAULT_MAX_ATTEMPTS)
    }

    /// Create policy for cloud API calls (5 attempts)
    pub fn for_cloud_api() -> Self {
        Self::new(CLOUD_API_MAX_ATTEMPTS)
    }

    /// Calculate backoff delay for given attempt number
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let exponential = self.initial_delay.as_millis() as f64 * 2f64.powi(attempt as i32);
        let delay_ms = exponential.min(self.max_delay.as_millis() as f64);

        // Add jitter to prevent thundering herd
        let jitter = delay_ms * self.jitter_factor * fastrand::f64();
        Duration::from_millis((delay_ms + jitter) as u64)
    }
}

impl RetryPolicy for ExponentialBackoffPolicy {
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T>> + Send,
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
                    // Check if error is retryable
                    if !e.is_retryable() {
                        warn!("Non-retryable error, aborting: {}", e);
                        return Err(e);
                    }

                    // Check if we've exhausted retries
                    if attempt == self.max_attempts - 1 {
                        warn!("Max retries ({}) reached", self.max_attempts);
                        return Err(TrainctlError::Retryable {
                            attempt: attempt + 1,
                            max_attempts: self.max_attempts,
                            reason: format!("{}", e),
                            source: Some(Box::new(e)),
                        });
                    }

                    // Store error for potential return
                    last_error = Some(e);
                    // Safe: we just set last_error above, so unwrap is safe
                    let err = last_error.as_ref().expect("last_error should be Some here");

                    // Calculate and wait for backoff
                    let backoff = self.calculate_backoff(attempt);
                    warn!(
                        "Retryable error (attempt {}/{}), retrying in {:?}: {}",
                        attempt + 1,
                        self.max_attempts,
                        backoff,
                        err
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }

        // Should never reach here, but handle it anyway
        Err(last_error.unwrap_or_else(|| TrainctlError::Retryable {
            attempt: self.max_attempts,
            max_attempts: self.max_attempts,
            reason: "Unknown error".to_string(),
            source: None,
        }))
    }
}

/// No retry policy (for operations that shouldn't be retried)
pub struct NoRetryPolicy;

impl RetryPolicy for NoRetryPolicy {
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T>> + Send,
    {
        f().await
    }
}
