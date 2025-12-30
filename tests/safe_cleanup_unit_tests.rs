//! Unit tests for safe cleanup functionality
//!
//! Tests protection mechanisms, dry-run mode, and cleanup validation
//! without requiring AWS credentials.

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::ResourceTracker;
use runctl::safe_cleanup::{safe_cleanup, CleanupSafety};

#[tokio::test]
async fn test_cleanup_safety_creation() {
    let safety = CleanupSafety::new();
    // Should be created successfully
    let _ = safety;
}

#[tokio::test]
async fn test_cleanup_safety_with_min_age() {
    let safety = CleanupSafety::with_min_age(60); // 60 minutes
                                                  // Should be created successfully
    let _ = safety;
}

#[tokio::test]
async fn test_can_delete_new_resource() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::new(); // Default min age is 5 minutes

    let status = ResourceStatus {
        id: "new-resource".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()), // Just created
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();
    let tracked = tracker.get_by_id(&status.id).await.unwrap();

    // New resource should not be deletable (too young)
    let can_delete = safety
        .can_delete(&status.id, &tracker, Some(tracked.created_at), false)
        .await
        .unwrap();

    assert!(!can_delete, "New resource should be protected");
}

#[tokio::test]
async fn test_can_delete_old_resource() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(0); // No minimum age for testing

    let status = ResourceStatus {
        id: "old-resource".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();
    let tracked = tracker.get_by_id(&status.id).await.unwrap();

    // Old resource should be deletable
    let can_delete = safety
        .can_delete(&status.id, &tracker, Some(tracked.created_at), false)
        .await
        .unwrap();

    assert!(can_delete, "Old resource should be deletable");
}

#[tokio::test]
async fn test_can_delete_protected_resource() {
    let tracker = ResourceTracker::new();
    let mut safety = CleanupSafety::with_min_age(0);

    let status = ResourceStatus {
        id: "protected-resource".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("runctl:protected".to_string(), "true".to_string())],
    };

    tracker.register(status.clone()).await.unwrap();
    let tracked = tracker.get_by_id(&status.id).await.unwrap();

    // Protect the resource
    safety.protect(status.id.clone());

    // Protected resource should not be deletable
    let can_delete = safety
        .can_delete(&status.id, &tracker, Some(tracked.created_at), false)
        .await
        .unwrap();

    assert!(!can_delete, "Protected resource should not be deletable");
}

#[tokio::test]
async fn test_can_delete_with_force() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(10); // 10 minutes minimum age

    let status = ResourceStatus {
        id: "force-delete-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()), // Just created, too new
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();
    let tracked = tracker.get_by_id(&status.id).await.unwrap();

    // Without force, new resource should not be deletable
    let can_delete_no_force = safety
        .can_delete(&status.id, &tracker, Some(tracked.created_at), false)
        .await
        .unwrap();

    assert!(
        !can_delete_no_force,
        "New resource should not be deletable without force"
    );

    // With force, time-based protection should be bypassed
    let can_delete_force = safety
        .can_delete(&status.id, &tracker, Some(tracked.created_at), true)
        .await
        .unwrap();

    assert!(
        can_delete_force,
        "Force should bypass time-based protection"
    );
}

#[tokio::test]
async fn test_safe_cleanup_dry_run() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(0);

    let status = ResourceStatus {
        id: "dry-run-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Dry-run cleanup
    let result = safe_cleanup(
        vec![status.id.clone()],
        &tracker,
        &safety,
        true,  // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Should report what would be deleted
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.skipped.len(), 0);
    assert_eq!(result.errors.len(), 0);

    // Resource should still exist (dry-run doesn't actually delete)
    assert!(tracker.exists(&status.id).await);
}

#[tokio::test]
async fn test_safe_cleanup_actual_deletion() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(0);

    let status = ResourceStatus {
        id: "actual-delete-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();
    assert!(tracker.exists(&status.id).await);

    // Actual cleanup (not dry-run)
    // Note: safe_cleanup only validates and reports, doesn't actually delete from tracker
    // The caller is responsible for removing from tracker after safe_cleanup succeeds
    let result = safe_cleanup(
        vec![status.id.clone()],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Should report deletion
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.skipped.len(), 0);
    assert_eq!(result.errors.len(), 0);

    // Manually remove from tracker (simulating actual deletion)
    tracker.remove(&status.id).await.unwrap();
    assert!(!tracker.exists(&status.id).await);
}

#[tokio::test]
async fn test_safe_cleanup_mixed_resources() {
    let tracker = ResourceTracker::new();
    // Use 0 min_age so we can test protection logic without waiting
    let mut safety = CleanupSafety::with_min_age(0);

    let protected = ResourceStatus {
        id: "protected-mixed".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("runctl:protected".to_string(), "true".to_string())],
    };

    let unprotected = ResourceStatus {
        id: "unprotected-mixed".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    let new_resource = ResourceStatus {
        id: "new-mixed".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    // Register all resources
    tracker.register(protected.clone()).await.unwrap();
    tracker.register(unprotected.clone()).await.unwrap();
    tracker.register(new_resource.clone()).await.unwrap();

    // Explicitly protect the protected resource
    safety.protect(protected.id.clone());

    // Try to cleanup all three
    let result = safe_cleanup(
        vec![
            protected.id.clone(),
            unprotected.id.clone(),
            new_resource.id.clone(),
        ],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Verify results - protected should be skipped, others should be deleted
    // (with min_age=0, both unprotected and new_resource can be deleted)
    // Protected is skipped due to explicit protection + tag
    let deleted_ids: Vec<_> = result.deleted.iter().cloned().collect();
    let skipped_ids: Vec<_> = result.skipped.iter().map(|(id, _)| id).cloned().collect();

    // Protected should be in skipped (explicit protection)
    assert!(
        skipped_ids.contains(&protected.id),
        "Protected should be skipped"
    );

    // Unprotected and new_resource should be in deleted (not protected, min_age=0)
    assert!(
        deleted_ids.contains(&unprotected.id),
        "Unprotected should be deleted"
    );
    assert!(
        deleted_ids.contains(&new_resource.id),
        "New resource should be deleted"
    );

    // Manually remove deleted resources (simulating actual deletion)
    tracker.remove(&unprotected.id).await.unwrap();
    tracker.remove(&new_resource.id).await.unwrap();

    // Verify state
    assert!(
        tracker.exists(&protected.id).await,
        "Protected should still exist"
    );
    assert!(
        !tracker.exists(&unprotected.id).await,
        "Unprotected should be removed"
    );
    assert!(
        !tracker.exists(&new_resource.id).await,
        "New resource should be removed"
    );
}

#[tokio::test]
async fn test_safe_cleanup_nonexistent_resource() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(0);

    // Try to cleanup non-existent resource
    let result = safe_cleanup(
        vec!["nonexistent".to_string()],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Non-existent resource: get_by_id returns None, so created_at is None
    // can_delete with None created_at will return Ok(true) if not explicitly protected
    // So it will be marked for deletion (even though resource doesn't exist)
    // This is expected behavior - safe_cleanup validates safety, not existence
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.skipped.len(), 0);
    assert_eq!(result.errors.len(), 0);
}

#[tokio::test]
async fn test_cleanup_result_structure() {
    let tracker = ResourceTracker::new();
    let safety = CleanupSafety::with_min_age(0);

    let status = ResourceStatus {
        id: "result-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    let result = safe_cleanup(
        vec![status.id.clone()],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    // Verify result structure
    assert!(!result.deleted.is_empty());
    assert_eq!(result.deleted[0], status.id);
    assert!(result.skipped.is_empty());
    assert!(result.errors.is_empty());
}
