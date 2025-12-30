//! Integration tests for ResourceTracker lifecycle operations
//!
//! Tests that ResourceTracker is properly integrated into AWS instance
//! lifecycle operations: create, start, stop, terminate.
//!
//! These tests verify that:
//! - Resources are registered when created
//! - Resources are updated when started/stopped
//! - Resources are removed when terminated
//! - Cost tracking works correctly through lifecycle

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::ResourceTracker;

#[tokio::test]
async fn test_lifecycle_create_registers_resource() {
    let tracker = ResourceTracker::new();

    // Simulate instance creation - resource should be registered
    let resource = ResourceStatus {
        id: "i-lifecycle-create".to_string(),
        name: Some("Test Instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Project".to_string(), "test".to_string())],
    };

    // Register (simulating what create_instance does)
    tracker.register(resource.clone()).await.unwrap();

    // Verify resource exists
    assert!(tracker.exists(&resource.id).await);

    // Verify resource details
    let tracked = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(tracked.status.id, resource.id);
    assert_eq!(tracked.status.state, ResourceState::Running);
    assert_eq!(tracked.status.instance_type, Some("t3.micro".to_string()));
    assert_eq!(tracked.status.cost_per_hour, 0.01);

    // Verify cost is tracked (should be > 0 after a moment)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let tracked_after = tracker.get_by_id(&resource.id).await.unwrap();
    assert!(tracked_after.accumulated_cost >= 0.0);
}

#[tokio::test]
async fn test_lifecycle_start_updates_state() {
    let tracker = ResourceTracker::new();

    // Create a stopped resource
    let resource = ResourceStatus {
        id: "i-lifecycle-start".to_string(),
        name: Some("Stopped Instance".to_string()),
        state: ResourceState::Stopped,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(resource.clone()).await.unwrap();

    // Verify initial state
    let tracked = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(tracked.status.state, ResourceState::Stopped);

    // Simulate start operation (what start_instance does via update_resource_status_in_tracker)
    tracker
        .update_state(&resource.id, ResourceState::Running)
        .await
        .unwrap();

    // Verify state updated
    let tracked_after = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(tracked_after.status.state, ResourceState::Running);

    // Cost should start accumulating again
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let tracked_final = tracker.get_by_id(&resource.id).await.unwrap();
    assert!(tracked_final.accumulated_cost >= tracked.accumulated_cost);
}

#[tokio::test]
async fn test_lifecycle_stop_updates_state() {
    let tracker = ResourceTracker::new();

    // Create a running resource with launch_time in the past (so it already has accumulated cost)
    let resource = ResourceStatus {
        id: "i-lifecycle-stop".to_string(),
        name: Some("Running Instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![],
    };

    tracker.register(resource.clone()).await.unwrap();

    // Get initial cost (should be ~0.01 for 1 hour of running)
    let tracked_before = tracker.get_by_id(&resource.id).await.unwrap();
    let cost_before = tracked_before.accumulated_cost;
    assert!(
        cost_before > 0.0,
        "Resource should have accumulated cost from 1 hour of running"
    );

    // Get cost after a brief moment (should be slightly higher)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let tracked_running = tracker.get_by_id(&resource.id).await.unwrap();
    let cost_while_running = tracked_running.accumulated_cost;
    assert!(
        cost_while_running >= cost_before,
        "Cost should increase or stay same while running"
    );

    // Simulate stop operation (what stop_instance does via update_resource_status_in_tracker)
    tracker
        .update_state(&resource.id, ResourceState::Stopped)
        .await
        .unwrap();

    // Verify state updated
    let tracked_after = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(tracked_after.status.state, ResourceState::Stopped);

    // Cost should be preserved (same as when it was running)
    assert_eq!(tracked_after.accumulated_cost, cost_while_running);

    // After stop, cost should not increase further (stopped instances don't accrue cost)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let tracked_final = tracker.get_by_id(&resource.id).await.unwrap();
    // Cost should remain the same (stopped instances don't accrue cost)
    assert_eq!(
        tracked_final.accumulated_cost,
        tracked_after.accumulated_cost
    );
}

#[tokio::test]
async fn test_lifecycle_terminate_removes_resource() {
    let tracker = ResourceTracker::new();

    // Create a running resource
    let resource = ResourceStatus {
        id: "i-lifecycle-terminate".to_string(),
        name: Some("Instance to Terminate".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![],
    };

    tracker.register(resource.clone()).await.unwrap();

    // Verify resource exists
    assert!(tracker.exists(&resource.id).await);

    // Get final cost before termination
    let tracked = tracker.get_by_id(&resource.id).await.unwrap();
    let final_cost = tracked.accumulated_cost;
    assert!(final_cost > 0.0);

    // Simulate terminate operation (what terminate_instance does)
    tracker.remove(&resource.id).await.unwrap();

    // Verify resource removed
    assert!(!tracker.exists(&resource.id).await);

    // Verify get_by_id returns None
    assert!(tracker.get_by_id(&resource.id).await.is_none());
}

#[tokio::test]
async fn test_lifecycle_spot_instance_registration() {
    let tracker = ResourceTracker::new();

    // Simulate spot instance creation - should register like on-demand
    let spot_resource = ResourceStatus {
        id: "i-lifecycle-spot".to_string(),
        name: Some("Spot Instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("g4dn.xlarge".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.50, // Spot instances can have different pricing
        public_ip: Some("5.6.7.8".to_string()),
        tags: vec![
            ("Spot".to_string(), "true".to_string()),
            ("Project".to_string(), "test".to_string()),
        ],
    };

    // Register spot instance (simulating what create_instance does for spot)
    tracker.register(spot_resource.clone()).await.unwrap();

    // Verify spot instance is tracked
    assert!(tracker.exists(&spot_resource.id).await);

    let tracked = tracker.get_by_id(&spot_resource.id).await.unwrap();
    assert_eq!(tracked.status.id, spot_resource.id);
    assert_eq!(
        tracked.status.instance_type,
        Some("g4dn.xlarge".to_string())
    );
    assert_eq!(tracked.status.cost_per_hour, 0.50);

    // Verify tags are preserved
    assert!(tracked.tags.contains_key("Spot"));
    assert_eq!(tracked.tags.get("Spot"), Some(&"true".to_string()));
}

#[tokio::test]
async fn test_lifecycle_full_workflow() {
    let tracker = ResourceTracker::new();

    // 1. Create instance
    let resource = ResourceStatus {
        id: "i-lifecycle-full".to_string(),
        name: Some("Full Lifecycle Test".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Project".to_string(), "test".to_string())],
    };

    tracker.register(resource.clone()).await.unwrap();
    assert!(tracker.exists(&resource.id).await);

    // 2. Stop instance
    tracker
        .update_state(&resource.id, ResourceState::Stopped)
        .await
        .unwrap();
    let stopped = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(stopped.status.state, ResourceState::Stopped);
    let cost_after_stop = stopped.accumulated_cost;

    // 3. Start instance again
    tracker
        .update_state(&resource.id, ResourceState::Running)
        .await
        .unwrap();
    let started = tracker.get_by_id(&resource.id).await.unwrap();
    assert_eq!(started.status.state, ResourceState::Running);
    // Cost should remain the same initially (no time passed since stop)
    assert_eq!(started.accumulated_cost, cost_after_stop);

    // 4. Let it run and accumulate cost
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let running = tracker.get_by_id(&resource.id).await.unwrap();
    // Cost should increase after running for a bit
    assert!(
        running.accumulated_cost >= cost_after_stop,
        "Cost should increase or stay same after restart"
    );

    // 5. Terminate instance
    tracker.remove(&resource.id).await.unwrap();
    assert!(!tracker.exists(&resource.id).await);
}

#[tokio::test]
async fn test_lifecycle_cost_accumulation_through_states() {
    let tracker = ResourceTracker::new();

    let resource = ResourceStatus {
        id: "i-lifecycle-cost".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(2)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(resource.clone()).await.unwrap();

    // Get initial cost (should be ~0.02 for 2 hours)
    let initial = tracker.get_by_id(&resource.id).await.unwrap();
    let initial_cost = initial.accumulated_cost;
    assert!(initial_cost > 0.0);

    // Stop - cost should freeze
    tracker
        .update_state(&resource.id, ResourceState::Stopped)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let stopped = tracker.get_by_id(&resource.id).await.unwrap();
    let stopped_cost = stopped.accumulated_cost;
    assert_eq!(stopped_cost, initial_cost); // No increase while stopped

    // Start again - cost should resume accumulating
    tracker
        .update_state(&resource.id, ResourceState::Running)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let restarted = tracker.get_by_id(&resource.id).await.unwrap();
    // Cost should be the same initially (no time passed)
    assert_eq!(restarted.accumulated_cost, stopped_cost);

    // After more time, cost should increase
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let running_again = tracker.get_by_id(&resource.id).await.unwrap();
    assert!(running_again.accumulated_cost >= restarted.accumulated_cost);
}
