//! Integration tests for resource tracking with cost calculation
//!
//! Tests the integration between ResourceTracker, cost calculation helpers,
//! and safe cleanup in realistic scenarios.

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::{ResourceTracker, ResourceUsage};
use runctl::safe_cleanup::{safe_cleanup, CleanupSafety};
use runctl::utils::get_instance_cost_with_tracker;

#[tokio::test]
async fn test_integration_cost_tracking_workflow() {
    let tracker = ResourceTracker::new();

    // Create multiple resources with different characteristics
    let resources = vec![
        ResourceStatus {
            id: "integration-1".to_string(),
            name: Some("Small Instance".to_string()),
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(2)),
            cost_per_hour: 0.01,
            public_ip: Some("1.2.3.4".to_string()),
            tags: vec![("Project".to_string(), "test".to_string())],
        },
        ResourceStatus {
            id: "integration-2".to_string(),
            name: Some("GPU Instance".to_string()),
            state: ResourceState::Running,
            instance_type: Some("g4dn.xlarge".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(1)),
            cost_per_hour: 0.50,
            public_ip: Some("5.6.7.8".to_string()),
            tags: vec![("Project".to_string(), "test".to_string())],
        },
        ResourceStatus {
            id: "integration-3".to_string(),
            name: Some("Stopped Instance".to_string()),
            state: ResourceState::Stopped,
            instance_type: Some("t3.small".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(3)),
            cost_per_hour: 0.02,
            public_ip: None,
            tags: vec![],
        },
    ];

    // Register all resources
    for resource in &resources {
        tracker.register(resource.clone()).await.unwrap();
    }

    // Update usage for running resources
    for i in 0..3 {
        let usage = ResourceUsage {
            cpu_percent: (i * 25) as f64,
            memory_mb: (i * 256) as f64,
            gpu_utilization: if i == 1 { Some(80.0) } else { None },
            network_in_mb: (i * 100) as f64,
            network_out_mb: (i * 50) as f64,
            timestamp: Utc::now(),
        };
        tracker.update_usage(&resources[i].id, usage).await.unwrap();
    }

    // Get costs using helper function
    let (hourly1, acc1) = get_instance_cost_with_tracker(
        Some(&tracker),
        &resources[0].id,
        "t3.micro",
        resources[0].launch_time,
        true,
    )
    .await;

    let (hourly2, acc2) = get_instance_cost_with_tracker(
        Some(&tracker),
        &resources[1].id,
        "g4dn.xlarge",
        resources[1].launch_time,
        true,
    )
    .await;

    // Verify costs
    assert_eq!(hourly1, 0.01);
    assert_eq!(hourly2, 0.50);
    assert!(acc1 > 0.0);
    assert!(acc2 > 0.0);
    // GPU instance should have higher accumulated cost despite shorter runtime
    // (because it has much higher hourly rate)
    assert!(acc2 > acc1);

    // Get total cost
    let total = tracker.get_total_cost().await;
    assert!(total > 0.0);
    assert!(total >= acc1 + acc2);

    // Filter by tag
    let test_resources = tracker.get_by_tag("Project", "test").await;
    assert_eq!(test_resources.len(), 2);

    // Cleanup old resources
    let safety = CleanupSafety::with_min_age(0);
    let cleanup_result = safe_cleanup(
        vec![resources[0].id.clone(), resources[1].id.clone()],
        &tracker,
        &safety,
        false, // dry_run
        false, // force
    )
    .await
    .unwrap();

    assert_eq!(cleanup_result.deleted.len(), 2);

    // Manually remove from tracker (safe_cleanup validates but doesn't remove from tracker)
    tracker.remove(&resources[0].id).await.unwrap();
    tracker.remove(&resources[1].id).await.unwrap();

    assert!(!tracker.exists(&resources[0].id).await);
    assert!(!tracker.exists(&resources[1].id).await);
}

#[tokio::test]
async fn test_integration_protected_resource_cleanup() {
    let tracker = ResourceTracker::new();
    let mut safety = CleanupSafety::with_min_age(0);

    // Create protected and unprotected resources
    let protected = ResourceStatus {
        id: "protected-integration".to_string(),
        name: Some("Important Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("g4dn.xlarge".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(5)),
        cost_per_hour: 0.50,
        public_ip: Some("10.0.0.1".to_string()),
        tags: vec![
            ("runctl:protected".to_string(), "true".to_string()),
            ("Project".to_string(), "production".to_string()),
        ],
    };

    let unprotected = ResourceStatus {
        id: "unprotected-integration".to_string(),
        name: Some("Test Resource".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![("Project".to_string(), "test".to_string())],
    };

    tracker.register(protected.clone()).await.unwrap();
    tracker.register(unprotected.clone()).await.unwrap();

    safety.protect(protected.id.clone());

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

    // Protected should be skipped, unprotected deleted
    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.deleted[0], unprotected.id);
    assert_eq!(result.skipped.len(), 1);

    // Manually remove unprotected (safe_cleanup validates but doesn't remove from tracker)
    tracker.remove(&unprotected.id).await.unwrap();

    // Verify state
    assert!(tracker.exists(&protected.id).await);
    assert!(!tracker.exists(&unprotected.id).await);

    // Protected resource should still have usage history
    let protected_tracked = tracker.get_by_id(&protected.id).await.unwrap();
    assert_eq!(protected_tracked.status.cost_per_hour, 0.50);
}

#[tokio::test]
async fn test_integration_cost_accumulation_with_usage() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "usage-cost-integration".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Simulate usage updates over time
    for i in 0..10 {
        let usage = ResourceUsage {
            cpu_percent: (i * 10) as f64,
            memory_mb: (i * 100) as f64,
            gpu_utilization: None,
            network_in_mb: (i * 50) as f64,
            network_out_mb: (i * 25) as f64,
            timestamp: Utc::now() + Duration::minutes(i as i64),
        };
        tracker.update_usage(&status.id, usage).await.unwrap();
    }

    // Get cost - should use tracker data
    let (hourly, accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &status.id,
        "t3.micro",
        status.launch_time,
        true,
    )
    .await;

    assert_eq!(hourly, 0.01);
    assert!(accumulated > 0.0);

    // Verify usage history
    let tracked = tracker.get_by_id(&status.id).await.unwrap();
    assert_eq!(tracked.usage_history.len(), 10);
    assert_eq!(tracked.usage_history[0].cpu_percent, 0.0);
    assert_eq!(tracked.usage_history[9].cpu_percent, 90.0);
}
