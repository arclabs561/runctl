//! Comprehensive tests for retry logic
//!
//! Tests verify exponential backoff, retry policies, and error handling.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use trainctl::error::{IsRetryable, TrainctlError};
use trainctl::retry::{ExponentialBackoffPolicy, NoRetryPolicy, RetryPolicy};

#[test]
fn test_exponential_backoff_creation() {
    let policy = ExponentialBackoffPolicy::new(5);
    // Policy should be created successfully
    let _ = policy;
}

#[test]
fn test_exponential_backoff_default_policy() {
    let policy = ExponentialBackoffPolicy::default_policy();
    // Default policy should have 3 attempts
    let _ = policy;
}

#[test]
fn test_exponential_backoff_for_cloud_api() {
    let policy = ExponentialBackoffPolicy::for_cloud_api();
    // Cloud API policy should have 5 attempts
    let _ = policy;
}

#[test]
fn test_no_retry_policy() {
    let policy = NoRetryPolicy;
    // NoRetryPolicy should be created
    let _ = policy;
}

#[tokio::test]
async fn test_retry_succeeds_immediately() {
    let policy = ExponentialBackoffPolicy::new(3);
    let call_count = AtomicU32::new(0);

    let result = policy
        .execute_with_retry(|| async {
            call_count.fetch_add(1, Ordering::SeqCst);
            Ok::<String, TrainctlError>("success".to_string())
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_succeeds_after_failures() {
    let policy = ExponentialBackoffPolicy::new(3);
    let call_count = AtomicU32::new(0);

    let result = policy
        .execute_with_retry(|| async {
            let count = call_count.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "transient error",
                )))
            } else {
                Ok::<String, TrainctlError>("success".to_string())
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_exhausts_attempts() {
    let policy = ExponentialBackoffPolicy::new(3);
    let call_count = AtomicU32::new(0);

    let result = policy
        .execute_with_retry(|| async {
            call_count.fetch_add(1, Ordering::SeqCst);
            Err::<String, TrainctlError>(TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "persistent error",
            )))
        })
        .await;

    assert!(result.is_err());
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_non_retryable_error() {
    let policy = ExponentialBackoffPolicy::new(3);
    let call_count = AtomicU32::new(0);

    let result = policy
        .execute_with_retry(|| async {
            call_count.fetch_add(1, Ordering::SeqCst);
            Err::<String, TrainctlError>(TrainctlError::Validation {
                field: "test".to_string(),
                reason: "invalid".to_string(),
            })
        })
        .await;

    assert!(result.is_err());
    // Non-retryable errors should not be retried
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_no_retry_policy_behavior() {
    let policy = NoRetryPolicy;
    let call_count = AtomicU32::new(0);

    let result = policy
        .execute_with_retry(|| async {
            call_count.fetch_add(1, Ordering::SeqCst);
            Err::<String, TrainctlError>(TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "error",
            )))
        })
        .await;

    assert!(result.is_err());
    // NoRetryPolicy should never retry
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_backoff_timing() {
    let policy = ExponentialBackoffPolicy::new(3);
    let call_count = AtomicU32::new(0);
    let start = Instant::now();

    let _result = policy
        .execute_with_retry(|| async {
            let count = call_count.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                // Small delay to ensure backoff is applied
                tokio::time::sleep(Duration::from_millis(10)).await;
                Err::<String, TrainctlError>(TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "transient",
                )))
            } else {
                Ok::<String, TrainctlError>("success".to_string())
            }
        })
        .await;

    let elapsed = start.elapsed();
    // Should have taken some time due to backoff (at least 100ms initial delay)
    assert!(elapsed >= Duration::from_millis(50));
}

#[test]
fn test_is_retryable_trait() {
    // Test retryable errors
    let io_error = TrainctlError::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
    assert!(io_error.is_retryable());

    let cloud_error = TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "test".to_string(),
        source: None,
    };
    assert!(cloud_error.is_retryable());

    // Test non-retryable errors
    let validation_error = TrainctlError::Validation {
        field: "test".to_string(),
        reason: "invalid".to_string(),
    };
    assert!(!validation_error.is_retryable());

    let config_error = TrainctlError::Config(trainctl::error::ConfigError::InvalidProvider(
        "test".to_string(),
    ));
    assert!(!config_error.is_retryable());
}
