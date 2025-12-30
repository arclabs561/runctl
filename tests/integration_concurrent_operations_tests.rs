//! Integration tests for concurrent resource operations
//!
//! Tests verify that ResourceTracker handles concurrent access correctly
//! and that operations are thread-safe.

use chrono::Utc;
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::ResourceTracker;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_concurrent_registration() {
    // Test that multiple resources can be registered concurrently
    let tracker = Arc::new(ResourceTracker::new());

    let mut handles = vec![];
    for i in 0..20 {
        let tracker_clone = Arc::clone(&tracker);
        let handle = tokio::spawn(async move {
            let status = ResourceStatus {
                id: format!("concurrent-{}", i),
                name: None,
                state: ResourceState::Running,
                instance_type: Some("t3.micro".to_string()),
                launch_time: Some(Utc::now()),
                cost_per_hour: 0.01,
                public_ip: None,
                tags: vec![],
            };
            tracker_clone.register(status).await
        });
        handles.push(handle);
    }

    // Wait for all registrations
    let mut success_count = 0;
    for handle in handles {
        if handle.await.expect("Task should complete").is_ok() {
            success_count += 1;
        }
    }

    // All should succeed
    assert_eq!(success_count, 20);

    // Verify all are registered
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 20);
}

#[tokio::test]
async fn test_concurrent_read_write() {
    // Test concurrent reads and writes
    let tracker = Arc::new(ResourceTracker::new());

    // Register initial resource
    let status = ResourceStatus {
        id: "test-concurrent".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.10,
        public_ip: None,
        tags: vec![],
    };
    tracker.register(status).await.expect("Should register");

    // Spawn concurrent readers and writers
    let mut handles = vec![];

    // 10 concurrent readers
    let resource_id = "test-concurrent".to_string();
    for _ in 0..10 {
        let tracker_clone = Arc::clone(&tracker);
        let resource_id_clone = resource_id.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = tracker_clone.get_by_id(&resource_id_clone).await;
                sleep(Duration::from_millis(1)).await;
            }
        });
        handles.push(handle);
    }

    // 2 concurrent state updaters
    for i in 0..2 {
        let tracker_clone = Arc::clone(&tracker);
        let resource_id_clone = resource_id.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..10 {
                let new_state = if i % 2 == 0 {
                    ResourceState::Running
                } else {
                    ResourceState::Stopped
                };
                let _ = tracker_clone
                    .update_state(&resource_id_clone, new_state)
                    .await;
                sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        handle.await.expect("Task should complete");
    }

    // Resource should still exist and be accessible
    let resource_id = "test-concurrent".to_string();
    let resource = tracker.get_by_id(&resource_id).await;
    assert!(resource.is_some());
}

#[tokio::test]
async fn test_concurrent_cost_calculation() {
    // Test that cost calculation works correctly under concurrent access
    let tracker = Arc::new(ResourceTracker::new());

    // Register multiple resources
    for i in 0..10 {
        let status = ResourceStatus {
            id: format!("cost-test-{}", i),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - chrono::Duration::hours(i)),
            cost_per_hour: 0.10,
            public_ip: None,
            tags: vec![],
        };
        tracker.register(status).await.expect("Should register");
    }

    // Concurrent cost calculations
    let mut handles = vec![];
    for _ in 0..5 {
        let tracker_clone = Arc::clone(&tracker);
        let handle = tokio::spawn(async move {
            for _ in 0..20 {
                let _ = tracker_clone.get_total_cost().await;
                sleep(Duration::from_millis(1)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        handle.await.expect("Task should complete");
    }

    // Final cost should be consistent
    let final_cost = tracker.get_total_cost().await;
    assert!(final_cost >= 0.0);
}
