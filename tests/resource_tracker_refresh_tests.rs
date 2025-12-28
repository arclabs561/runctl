//! Tests for cost refresh functionality
//!
//! Tests the automatic and manual cost refresh mechanisms.

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::ResourceTracker;

#[tokio::test]
async fn test_refresh_costs_updates_all() {
    let tracker = ResourceTracker::new();

    // Create resources with different launch times
    let now = Utc::now();
    let resources = vec![
        ResourceStatus {
            id: "refresh-1".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(now - Duration::hours(1)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "refresh-2".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("g4dn.xlarge".to_string()),
            launch_time: Some(now - Duration::hours(2)),
            cost_per_hour: 0.50,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "refresh-3".to_string(),
            name: None,
            state: ResourceState::Stopped,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(now - Duration::hours(3)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
    ];

    for resource in resources {
        tracker.register(resource).await.unwrap();
    }

    // Refresh all costs
    tracker.refresh_costs().await;

    // Verify costs are updated
    let total = tracker.get_total_cost().await;
    assert!(total > 0.0);

    // Verify individual costs
    let resource1 = tracker.get_by_id(&"refresh-1".to_string()).await.unwrap();
    let resource2 = tracker.get_by_id(&"refresh-2".to_string()).await.unwrap();
    let resource3 = tracker.get_by_id(&"refresh-3".to_string()).await.unwrap();

    // Running resources should have costs
    assert!(resource1.accumulated_cost > 0.0);
    assert!(resource2.accumulated_cost > resource1.accumulated_cost); // Higher hourly rate

    // Stopped resource should have 0 cost
    assert_eq!(resource3.accumulated_cost, 0.0);
}

#[tokio::test]
async fn test_automatic_cost_update_on_access() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "auto-update-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Initially, accumulated_cost is 0.0 (set at registration)
    // But when we access it, it should be automatically calculated
    let tracked = tracker.get_by_id(&status.id).await.unwrap();

    // Cost should be automatically calculated
    assert!(tracked.accumulated_cost > 0.0);
    // Should be approximately 0.01 * 1 hour = 0.01
    assert!(tracked.accumulated_cost >= 0.009);
    assert!(tracked.accumulated_cost <= 0.011);
}

#[tokio::test]
async fn test_cost_updates_over_time() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "time-update-test".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::minutes(30)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Get cost immediately
    let cost1 = tracker
        .get_by_id(&status.id)
        .await
        .unwrap()
        .accumulated_cost;
    assert!(cost1 > 0.0);

    // Wait a bit (simulating time passing)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Get cost again - should be slightly higher
    let cost2 = tracker
        .get_by_id(&status.id)
        .await
        .unwrap()
        .accumulated_cost;
    assert!(cost2 >= cost1); // Should be same or slightly higher
}

#[tokio::test]
async fn test_get_running_updates_costs() {
    let tracker = ResourceTracker::new();

    let resources = vec![
        ResourceStatus {
            id: "running-1".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(1)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "running-2".to_string(),
            name: None,
            state: ResourceState::Stopped,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(1)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
    ];

    for resource in resources {
        tracker.register(resource).await.unwrap();
    }

    // Get running resources - costs should be automatically updated
    let running = tracker.get_running().await;
    assert_eq!(running.len(), 1);

    // Running resource should have updated cost
    assert!(running[0].accumulated_cost > 0.0);

    // Verify stopped resource is not in running list
    assert_eq!(running[0].status.id, "running-1");
}

#[tokio::test]
async fn test_get_total_cost_updates_all() {
    let tracker = ResourceTracker::new();

    let resources = vec![
        ResourceStatus {
            id: "total-1".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(1)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
        ResourceStatus {
            id: "total-2".to_string(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(2)),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        },
    ];

    for resource in resources {
        tracker.register(resource).await.unwrap();
    }

    // Get total cost - should update all costs and sum them
    let total = tracker.get_total_cost().await;
    assert!(total > 0.0);

    // Total should be sum of individual costs
    let running = tracker.get_running().await;
    let sum: f64 = running.iter().map(|r| r.accumulated_cost).sum();
    assert!((total - sum).abs() < 0.0001); // Should be approximately equal
}
