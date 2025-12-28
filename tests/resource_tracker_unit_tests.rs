//! Comprehensive unit tests for ResourceTracker
//!
//! Tests resource tracking, cost accumulation, usage updates, and filtering
//! without requiring AWS credentials.

use chrono::{Duration, Utc};
use runctl::error::TrainctlError;
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::{ResourceTracker, ResourceUsage};

#[tokio::test]
async fn test_resource_tracker_creation() {
    let tracker = ResourceTracker::new();
    assert!(tracker.get_running().await.is_empty());
    assert_eq!(tracker.get_total_cost().await, 0.0);
}

#[tokio::test]
async fn test_register_resource() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "test-1".to_string(),
        name: Some("Test Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Environment".to_string(), "test".to_string())],
    };

    // Register should succeed
    assert!(tracker.register(status.clone()).await.is_ok());

    // Resource should exist
    assert!(tracker.exists(&status.id).await);

    // Should be in running list
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].status.id, status.id);
}

#[tokio::test]
async fn test_register_duplicate_resource() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "duplicate-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    // First registration should succeed
    assert!(tracker.register(status.clone()).await.is_ok());

    // Second registration should fail
    let result = tracker.register(status).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TrainctlError::ResourceExists { resource_id, .. } => {
            assert_eq!(resource_id, "duplicate-test");
        }
        _ => panic!("Expected ResourceExists error"),
    }
}

#[tokio::test]
async fn test_update_usage() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "usage-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Update usage multiple times
    for i in 0..5 {
        let usage = ResourceUsage {
            cpu_percent: (i * 10) as f64,
            memory_mb: (i * 100) as f64,
            gpu_utilization: if i % 2 == 0 { Some(i as f64) } else { None },
            network_in_mb: (i * 50) as f64,
            network_out_mb: (i * 25) as f64,
            timestamp: Utc::now(),
        };

        assert!(tracker.update_usage(&status.id, usage).await.is_ok());
    }

    // Verify usage history
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.usage_history.len(), 5);
    assert_eq!(tracked.usage_history[0].cpu_percent, 0.0);
    assert_eq!(tracked.usage_history[4].cpu_percent, 40.0);
}

#[tokio::test]
async fn test_update_usage_nonexistent_resource() {
    let tracker = ResourceTracker::new();

    let usage = ResourceUsage {
        cpu_percent: 50.0,
        memory_mb: 512.0,
        gpu_utilization: None,
        network_in_mb: 0.0,
        network_out_mb: 0.0,
        timestamp: Utc::now(),
    };

    let result = tracker
        .update_usage(&"nonexistent".to_string(), usage)
        .await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TrainctlError::ResourceNotFound { resource_id, .. } => {
            assert_eq!(resource_id, "nonexistent");
        }
        _ => panic!("Expected ResourceNotFound error"),
    }
}

#[tokio::test]
async fn test_get_running_resources() {
    let tracker = ResourceTracker::new();

    let running_resource = ResourceStatus {
        id: "running-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    let starting_resource = ResourceStatus {
        id: "starting-1".to_string(),
        name: None,
        state: ResourceState::Starting,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    let stopped_resource = ResourceStatus {
        id: "stopped-1".to_string(),
        name: None,
        state: ResourceState::Stopped,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(running_resource).await.unwrap();
    tracker.register(starting_resource).await.unwrap();
    tracker.register(stopped_resource).await.unwrap();

    let running = tracker.get_running().await;
    assert_eq!(running.len(), 2); // Running + Starting
    assert!(running.iter().any(|r| r.status.id == "running-1"));
    assert!(running.iter().any(|r| r.status.id == "starting-1"));
    assert!(!running.iter().any(|r| r.status.id == "stopped-1"));
}

#[tokio::test]
async fn test_get_total_cost() {
    let tracker = ResourceTracker::new();

    // Create resources with different costs
    let resources = vec![
        ResourceStatus {
            id: "cost-1".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(1)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "cost-2".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("g4dn.xlarge".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(2)),
            cost_per_hour: 0.50,
            public_ip: None,
            tags: vec![],
        },
    ];

    for resource in resources {
        tracker.register(resource).await.unwrap();
    }

    // accumulated_cost is now automatically calculated when resources are accessed
    let total = tracker.get_total_cost().await;
    // Should be > 0 since resources have been running (1 hour and 2 hours)
    assert!(total > 0.0);

    // Verify individual resources exist with updated costs
    let running = tracker.get_running().await;
    let cost1 = running.iter().find(|r| r.status.id == "cost-1").unwrap();
    let cost2 = running.iter().find(|r| r.status.id == "cost-2").unwrap();

    // Verify cost_per_hour is set correctly
    assert_eq!(cost1.status.cost_per_hour, 0.01);
    assert_eq!(cost2.status.cost_per_hour, 0.50);

    // Verify accumulated_cost is calculated (approximately cost_per_hour * hours_running)
    assert!(cost1.accumulated_cost > 0.0);
    assert!(cost2.accumulated_cost > cost1.accumulated_cost); // cost-2 has higher hourly rate
                                                              // cost-2 should be approximately 2x cost-1 (2 hours vs 1 hour, but 50x hourly rate)
    assert!(cost2.accumulated_cost > cost1.accumulated_cost * 40.0);
}

#[tokio::test]
async fn test_get_by_tag() {
    let tracker = ResourceTracker::new();

    let resource1 = ResourceStatus {
        id: "tagged-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![
            ("Project".to_string(), "test".to_string()),
            ("Environment".to_string(), "dev".to_string()),
        ],
    };

    let resource2 = ResourceStatus {
        id: "tagged-2".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![
            ("Project".to_string(), "production".to_string()),
            ("Environment".to_string(), "prod".to_string()),
        ],
    };

    let resource3 = ResourceStatus {
        id: "tagged-3".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![
            ("Project".to_string(), "test".to_string()),
            ("Environment".to_string(), "staging".to_string()),
        ],
    };

    tracker.register(resource1).await.unwrap();
    tracker.register(resource2).await.unwrap();
    tracker.register(resource3).await.unwrap();

    // Filter by Project=test
    let test_resources = tracker.get_by_tag("Project", "test").await;
    assert_eq!(test_resources.len(), 2);
    assert!(test_resources.iter().any(|r| r.status.id == "tagged-1"));
    assert!(test_resources.iter().any(|r| r.status.id == "tagged-3"));
    assert!(!test_resources.iter().any(|r| r.status.id == "tagged-2"));

    // Filter by Environment=prod
    let prod_resources = tracker.get_by_tag("Environment", "prod").await;
    assert_eq!(prod_resources.len(), 1);
    assert_eq!(prod_resources[0].status.id, "tagged-2");
}

#[tokio::test]
async fn test_get_by_id() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "get-by-id-test".to_string(),
        name: Some("Test Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Tag".to_string(), "Value".to_string())],
    };

    tracker.register(status.clone()).await.unwrap();

    // Get by ID should return the resource
    let tracked = tracker.get_by_id(&status.id).await;
    assert!(tracked.is_some());
    let tracked = tracked.unwrap();
    assert_eq!(tracked.status.id, status.id);
    assert_eq!(tracked.status.name, status.name);
    assert_eq!(tracked.status.cost_per_hour, status.cost_per_hour);

    // Get non-existent ID should return None
    assert!(tracker
        .get_by_id(&"nonexistent".to_string())
        .await
        .is_none());
}

#[tokio::test]
async fn test_remove_resource() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "remove-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();
    assert!(tracker.exists(&status.id).await);

    // Remove should succeed
    assert!(tracker.remove(&status.id).await.is_ok());
    assert!(!tracker.exists(&status.id).await);

    // Removing again should fail
    let result = tracker.remove(&status.id).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        TrainctlError::ResourceNotFound { resource_id, .. } => {
            assert_eq!(resource_id, "remove-test");
        }
        _ => panic!("Expected ResourceNotFound error"),
    }
}

#[tokio::test]
async fn test_usage_history_limit() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "usage-limit-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Add more than 1000 usage records
    for i in 0..1005 {
        let usage = ResourceUsage {
            cpu_percent: i as f64,
            memory_mb: (i * 10) as f64,
            gpu_utilization: None,
            network_in_mb: 0.0,
            network_out_mb: 0.0,
            timestamp: Utc::now(),
        };
        tracker.update_usage(&status.id, usage).await.unwrap();
    }

    // History should be capped at 1000
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.usage_history.len(), 1000);

    // Oldest record should be removed (first 5 should be gone)
    assert_eq!(tracked.usage_history[0].cpu_percent, 5.0);
    assert_eq!(tracked.usage_history[999].cpu_percent, 1004.0);
}

#[tokio::test]
async fn test_concurrent_operations() {
    use std::sync::Arc;
    let tracker = Arc::new(ResourceTracker::new());

    // Create multiple resources concurrently
    let mut handles = vec![];
    for i in 0..10 {
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
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    // All resources should be registered
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 10);
}
