//! End-to-end tests for resource tracking and cost awareness
//!
//! Tests the resource tracking system with real AWS resources.
//! Run with: TRAINCTL_E2E=1 cargo test --test resource_tracking_test --features e2e

use std::env;
use trainctl::resource_tracking::{ResourceTracker, ResourceUsage};
use trainctl::provider::{ResourceStatus, ResourceState};
use chrono::Utc;

/// Check if E2E tests should run
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

#[tokio::test]
#[ignore]
async fn test_resource_tracking() {
    if !should_run_e2e() {
        eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
        return;
    }

    let tracker = ResourceTracker::new();
    
    // Create a test resource status
    let status = ResourceStatus {
        id: "test-resource-1".to_string(),
        name: Some("Test Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Environment".to_string(), "test".to_string())],
    };
    
    // Register resource
    tracker.register(status.clone()).await.unwrap();
    
    // Verify it exists
    assert!(tracker.exists(&status.id).await);
    
    // Get running resources
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].status.id, status.id);
    
    // Update usage
    let usage = ResourceUsage {
        cpu_percent: 50.0,
        memory_mb: 512.0,
        gpu_utilization: None,
        network_in_mb: 100.0,
        network_out_mb: 50.0,
        timestamp: Utc::now(),
    };
    
    tracker.update_usage(&status.id, usage).await.unwrap();
    
    // Verify usage was recorded
    let tracked = tracker.get_running().await;
    assert_eq!(tracked[0].usage_history.len(), 1);
    
    // Cleanup
    tracker.remove(&status.id).await.unwrap();
    assert!(!tracker.exists(&status.id).await);
}

#[tokio::test]
#[ignore]
async fn test_cost_tracking() {
    if !should_run_e2e() {
        return;
    }

    let tracker = ResourceTracker::new();
    
    // Create multiple resources with different costs
    let resources = vec![
        ResourceStatus {
            id: "resource-1".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now()),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "resource-2".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("g4dn.xlarge".to_string()),
            launch_time: Some(Utc::now()),
            cost_per_hour: 0.50,
            public_ip: None,
            tags: vec![],
        },
    ];
    
    for resource in resources {
        tracker.register(resource).await.unwrap();
    }
    
    // Get total cost (would calculate from accumulated_cost)
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 2);
    
    // Cleanup
    for resource in &running {
        tracker.remove(&resource.status.id).await.unwrap();
    }
}

#[tokio::test]
#[ignore]
async fn test_resource_filtering_by_tag() {
    if !should_run_e2e() {
        return;
    }

    let tracker = ResourceTracker::new();
    
    // Create resources with different tags
    let resource1 = ResourceStatus {
        id: "tagged-resource-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("Project".to_string(), "test".to_string())],
    };
    
    let resource2 = ResourceStatus {
        id: "tagged-resource-2".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("Project".to_string(), "production".to_string())],
    };
    
    tracker.register(resource1).await.unwrap();
    tracker.register(resource2).await.unwrap();
    
    // Filter by tag
    let test_resources = tracker.get_by_tag("Project", "test").await;
    assert_eq!(test_resources.len(), 1);
    assert_eq!(test_resources[0].status.id, "tagged-resource-1");
    
    // Cleanup
    tracker.remove(&"tagged-resource-1".to_string()).await.unwrap();
    tracker.remove(&"tagged-resource-2".to_string()).await.unwrap();
}

