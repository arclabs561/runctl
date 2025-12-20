//! End-to-end tests for safe cleanup operations
//!
//! Tests the safe cleanup system with protection mechanisms.
//! Run with: TRAINCTL_E2E=1 cargo test --test safe_cleanup_test --features e2e

use chrono::Utc;
use std::env;
use trainctl::provider::{ResourceState, ResourceStatus};
use trainctl::resource_tracking::ResourceTracker;
use trainctl::safe_cleanup::{safe_cleanup, CleanupSafety};

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

#[tokio::test]
#[ignore]
async fn test_protected_resources() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let tracker = ResourceTracker::new();
    let mut safety = CleanupSafety::new();

    // Create a protected resource
    let protected_resource = ResourceStatus {
        id: "protected-resource".to_string(),
        name: Some("Important Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("trainctl:protected".to_string(), "true".to_string())],
    };

    tracker.register(protected_resource.clone()).await.unwrap();
    safety.protect(protected_resource.id.clone());

    // Try to delete protected resource
    // Get tracked resource to access created_at
    let tracked = tracker.get_by_id(&protected_resource.id).await.unwrap();
    let can_delete = safety
        .can_delete(
            &protected_resource.id,
            &tracker,
            Some(tracked.created_at),
            false,
        )
        .await
        .unwrap();
    assert!(!can_delete, "Protected resource should not be deletable");

    // Cleanup
    tracker.remove(&protected_resource.id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_dry_run_cleanup() {
    if !should_run_e2e() {
        return;
    }

    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::new();

    // Create test resources
    let resource1 = ResourceStatus {
        id: "cleanup-test-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(resource1.clone()).await.unwrap();

    // Dry-run cleanup
    let result = safe_cleanup(
        vec![resource1.id.clone()],
        &tracker,
        &safety,
        true,  // dry_run
        false, // force
    )
    .await
    .unwrap();

    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.skipped.len(), 0);
    assert_eq!(result.errors.len(), 0);

    // Resource should still exist (dry-run)
    assert!(tracker.exists(&resource1.id).await);

    // Cleanup
    tracker.remove(&resource1.id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_cleanup_with_protection() {
    if !should_run_e2e() {
        return;
    }

    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::new();

    // Create protected and unprotected resources
    let protected = ResourceStatus {
        id: "protected-cleanup".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("trainctl:protected".to_string(), "true".to_string())],
    };

    let unprotected = ResourceStatus {
        id: "unprotected-cleanup".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(protected.clone()).await.unwrap();
    tracker.register(unprotected.clone()).await.unwrap();

    // Try to cleanup both
    let result = safe_cleanup(
        vec![protected.id.clone(), unprotected.id.clone()],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Protected should be skipped, unprotected should be deleted
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.deleted.len(), 1);

    // Cleanup
    tracker.remove(&protected.id).await.unwrap();
    if tracker.exists(&unprotected.id).await {
        tracker.remove(&unprotected.id).await.unwrap();
    }
}
