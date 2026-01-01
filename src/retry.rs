//! Retry logic with exponential backoff
//!
//! Provides retry policies for handling transient failures in cloud API calls.
//!
//! ## Design Rationale
//!
//! Cloud APIs can fail transiently due to rate limiting, network issues, or
//! temporary service unavailability. Exponential backoff with jitter prevents
//! thundering herd problems when multiple clients retry simultaneously.
//!
//! The default policy uses:
//! - 5 attempts for cloud APIs (higher than default 3 due to cloud API volatility)
//! - Exponential backoff: 100ms → 200ms → 400ms → 800ms → 1600ms (capped at 30s)
//! - 10% jitter to randomize retry timing across clients
//!
//! ## When to Retry
//!
//! Only errors implementing `IsRetryable` are retried. Non-retryable errors
//! (e.g., validation errors, authentication failures) fail immediately.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use runctl::retry::{ExponentialBackoffPolicy, RetryPolicy};
//! use runctl::error::TrainctlError;
//!
//! # async fn example() -> runctl::error::Result<()> {
//! let policy = ExponentialBackoffPolicy::for_cloud_api();
//!
//! let result = policy.execute_with_retry(|| async {
//!     // Your operation that might fail
//!     // Example: cloud_client.describe_instances().send().await
//!     Ok::<(), TrainctlError>(())
//! }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Policy Selection
//!
//! - `for_cloud_api()`: Use for AWS EC2, S3, SSM calls (5 attempts)
//! - `new(n)`: Custom attempts for specific use cases
//! - `NoRetryPolicy`: For operations that must not be retried (e.g., resource deletion)

use crate::error::{IsRetryable, Result, TrainctlError};
use std::future::Future;
use std::time::Duration;
use tracing::{info, warn};

/// Default retry configuration constants
const DEFAULT_INITIAL_RETRY_DELAY_MS: u64 = 100;
const DEFAULT_MAX_RETRY_DELAY_SECS: u64 = 30;
const DEFAULT_JITTER_FACTOR: f64 = 0.1;
#[allow(dead_code)] // Reserved for future default policy
const DEFAULT_MAX_ATTEMPTS: u32 = 3;
const CLOUD_API_MAX_ATTEMPTS: u32 = 5;

/// Retry policy trait for handling transient failures
///
/// Defines the interface for retry policies that can execute operations with
/// automatic retry on failure. Implementations determine retry behavior (number
/// of attempts, backoff strategy, etc.).
///
/// ## When to Retry
///
/// Only errors that implement `IsRetryable` are retried. Non-retryable errors
/// (e.g., validation errors, authentication failures) fail immediately.
///
/// ## Note on Async Traits
///
/// Using `async fn` in traits generates a clippy warning about auto trait bounds,
/// but this is acceptable for our use case. The alternative (explicit Future return types)
/// adds significant complexity without clear benefits for this API.
///
/// ## Examples
///
/// ```rust,no_run
/// use runctl::retry::{ExponentialBackoffPolicy, RetryPolicy};
///
/// # async fn example() -> runctl::error::Result<()> {
/// let policy = ExponentialBackoffPolicy::for_cloud_api();
/// let result = policy.execute_with_retry(|| async {
///     // Operation that might fail transiently
///     Ok::<(), runctl::error::TrainctlError>(())
/// }).await?;
/// # Ok(())
/// # }
/// ```
#[allow(async_fn_in_trait)]
pub trait RetryPolicy: Send + Sync {
    /// Execute a function with retry logic
    ///
    /// Attempts to execute the provided function, retrying on retryable errors
    /// according to the policy's strategy. Non-retryable errors fail immediately.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that returns a Future producing a `Result<T>`
    ///
    /// # Returns
    ///
    /// Returns `Ok(T)` if the operation succeeds on any attempt, or the last
    /// error if all retries are exhausted.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use runctl::retry::{ExponentialBackoffPolicy, RetryPolicy};
    ///
    /// # async fn example() -> runctl::error::Result<()> {
    /// let policy = ExponentialBackoffPolicy::for_cloud_api();
    /// let result = policy.execute_with_retry(|| async {
    ///     // Your operation
    ///     Ok::<(), runctl::error::TrainctlError>(())
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn execute_with_retry<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T>> + Send;
}

/// Exponential backoff retry policy
///
/// Retries operations with exponentially increasing delays between attempts.
/// Includes jitter to prevent thundering herd problems when multiple clients
/// retry simultaneously.
///
/// ## Backoff Strategy
///
/// - Initial delay: 100ms
/// - Exponential growth: Each retry doubles the delay (100ms → 200ms → 400ms → ...)
/// - Maximum delay: 30 seconds (capped to prevent excessive waits)
/// - Jitter: 10% randomization to spread out retry attempts
///
/// ## Default Policies
///
/// - `for_cloud_api()`: 5 attempts (recommended for AWS EC2, S3, SSM calls)
/// - `new(n)`: Custom number of attempts
///
/// ## Examples
///
/// ```rust,no_run
/// use runctl::retry::ExponentialBackoffPolicy;
///
/// // Use default cloud API policy (5 attempts)
/// let policy = ExponentialBackoffPolicy::for_cloud_api();
///
/// // Custom policy with 3 attempts
/// let policy = ExponentialBackoffPolicy::new(3);
/// ```
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
    #[allow(dead_code)] // Reserved for future use
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
                    let err = last_error.as_ref().unwrap();

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
#[allow(dead_code)] // Reserved for future use
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
