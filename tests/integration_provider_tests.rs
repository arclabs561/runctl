//! Integration tests for provider trait system
//!
//! These tests verify that the provider trait system works correctly,
//! even though it's not yet used by the CLI (see docs/PROVIDER_TRAIT_DECISION.md).

use chrono::Utc;
use runctl::error::Result;
use runctl::provider::{ResourceId, ResourceState, ResourceStatus, TrainingProvider};
use runctl::providers::AwsProvider;

#[tokio::test]
#[ignore] // Requires AWS credentials
async fn test_aws_provider_get_resource_status() {
    // Test that AwsProvider can retrieve resource status
    // This verifies the provider trait interface works

    let config = runctl::config::Config::default();
    let provider = AwsProvider::new(config)
        .await
        .expect("Failed to create AWS provider");

    // Try to get status of a non-existent instance (should return error, not panic)
    let result = provider
        .get_resource_status(&"i-nonexistent".to_string())
        .await;

    // Should return an error, not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_provider_trait_error_handling() {
    // Test that provider implementations return proper errors
    // This verifies error handling at the trait boundary

    let config = runctl::config::Config::default();
    let provider = AwsProvider::new(config)
        .await
        .expect("Failed to create AWS provider");

    // Test that unimplemented methods return proper errors
    let result = provider
        .create_resource("t3.micro", Default::default())
        .await;
    assert!(result.is_err());

    // Error should be a CloudProvider error
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        runctl::error::TrainctlError::CloudProvider { .. }
    ));
}

#[tokio::test]
async fn test_provider_name() {
    // Test that provider name is correctly returned
    let config = runctl::config::Config::default();
    let provider = AwsProvider::new(config)
        .await
        .expect("Failed to create AWS provider");

    assert_eq!(provider.name(), "aws");
}

#[tokio::test]
async fn test_resource_status_conversion() {
    // Test that ResourceStatus can be created and converted correctly
    let status = ResourceStatus {
        id: "test-id".to_string(),
        name: Some("test-instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.10,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("runctl:project".to_string(), "test".to_string())],
    };

    assert_eq!(status.id, "test-id");
    assert_eq!(status.state, ResourceState::Running);
    assert!(status.cost_per_hour > 0.0);
}

#[tokio::test]
async fn test_error_conversion_at_boundary() {
    // Test that crate::error::Result converts correctly to anyhow::Result
    // This verifies error handling at CLI boundaries

    use runctl::error::{Result, TrainctlError};

    fn library_function() -> Result<()> {
        Err(TrainctlError::Validation {
            field: "test".to_string(),
            reason: "test error".to_string(),
        })
    }

    // Convert to anyhow at boundary
    let result: anyhow::Result<()> = library_function().map_err(|e| anyhow::Error::from(e));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Validation error"));
    assert!(err.to_string().contains("test"));
}
