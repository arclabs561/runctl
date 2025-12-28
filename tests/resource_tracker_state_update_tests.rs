//! Tests for resource state updates
//!
//! Tests the update_state functionality for tracking resource state changes.

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::ResourceTracker;

#[tokio::test]
async fn test_update_state_running_to_stopped() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "state-update-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Initially running, should have cost
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.status.state, ResourceState::Running);
    assert!(tracked.accumulated_cost > 0.0);

    // Update to stopped
    tracker
        .update_state(&status.id, ResourceState::Stopped)
        .await
        .unwrap();

    // After stopping, cost should be 0 (stopped resources don't accrue costs)
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.status.state, ResourceState::Stopped);
    assert_eq!(tracked.accumulated_cost, 0.0);
}

#[tokio::test]
async fn test_update_state_stopped_to_running() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "state-update-2".to_string(),
        name: None,
        state: ResourceState::Stopped,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Initially stopped, should have 0 cost
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.status.state, ResourceState::Stopped);
    assert_eq!(tracked.accumulated_cost, 0.0);

    // Update to running
    tracker
        .update_state(&status.id, ResourceState::Running)
        .await
        .unwrap();

    // After starting, cost should be calculated based on launch time
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.status.state, ResourceState::Running);
    assert!(tracked.accumulated_cost > 0.0);
}

#[tokio::test]
async fn test_update_state_nonexistent_resource() {
    let tracker = ResourceTracker::new();

    // Try to update state of non-existent resource
    let result = tracker
        .update_state(&"nonexistent".to_string(), ResourceState::Running)
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        runctl::error::TrainctlError::ResourceNotFound { resource_id, .. } => {
            assert_eq!(resource_id, "nonexistent");
        }
        _ => panic!("Expected ResourceNotFound error"),
    }
}

#[tokio::test]
async fn test_update_state_preserves_other_fields() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "state-preserve-test".to_string(),
        name: Some("Test Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("g4dn.xlarge".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.50,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Project".to_string(), "test".to_string())],
    };

    tracker.register(status.clone()).await.unwrap();

    // Update state
    tracker
        .update_state(&status.id, ResourceState::Stopped)
        .await
        .unwrap();

    // Verify other fields are preserved
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.status.name, status.name);
    assert_eq!(tracked.status.instance_type, status.instance_type);
    assert_eq!(tracked.status.cost_per_hour, status.cost_per_hour);
    assert_eq!(tracked.status.public_ip, status.public_ip);
    assert_eq!(tracked.status.tags, status.tags);
    assert_eq!(tracked.status.launch_time, status.launch_time);

    // Only state should have changed
    assert_eq!(tracked.status.state, ResourceState::Stopped);
}
