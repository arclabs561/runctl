//! Unit tests for cost calculation utilities
//!
//! Tests the cost calculation helper function and instance cost lookup.

use chrono::{Duration, Utc};
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::{ResourceTracker, ResourceUsage};
use runctl::utils::get_instance_cost_with_tracker;

#[tokio::test]
async fn test_get_instance_cost_with_tracker_uses_tracker() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "cost-test-1".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(2)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // Update usage (note: this doesn't update accumulated_cost automatically)
    let usage = ResourceUsage {
        cpu_percent: 50.0,
        memory_mb: 512.0,
        gpu_utilization: None,
        network_in_mb: 0.0,
        network_out_mb: 0.0,
        timestamp: Utc::now(),
    };
    tracker.update_usage(&status.id, usage).await.unwrap();

    // Get cost should use tracker data for hourly cost
    // Note: accumulated_cost in tracker starts at 0.0, so helper will calculate it
    let (hourly, accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &status.id,
        "t3.micro",
        status.launch_time,
        true,
    )
    .await;

    assert_eq!(hourly, 0.01);
    // accumulated_cost is now automatically calculated when get_by_id is called
    // Should be approximately 0.01 * 2 hours = 0.02
    assert!(accumulated > 0.0);
    assert!(accumulated >= 0.015); // At least 1.5 hours worth
    assert!(accumulated <= 0.025); // At most 2.5 hours worth (allowing some margin)
}

#[tokio::test]
async fn test_get_instance_cost_with_tracker_fallback() {
    let tracker = ResourceTracker::new();

    // Don't register the resource - should fallback to calculation
    let instance_id = "not-tracked".to_string();
    let instance_type = "t3.micro";
    let launch_time = Some(Utc::now() - Duration::hours(1));

    let (hourly, accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &instance_id,
        instance_type,
        launch_time,
        true,
    )
    .await;

    // Should use calculated cost
    assert!(hourly > 0.0);
    assert!(accumulated > 0.0);
    // Accumulated should be approximately hourly * hours
    assert!(accumulated >= hourly * 0.9); // Allow some margin
    assert!(accumulated <= hourly * 1.1);
}

#[tokio::test]
async fn test_get_instance_cost_with_tracker_no_tracker() {
    // No tracker provided - should calculate
    let instance_id = "no-tracker".to_string();
    let instance_type = "g4dn.xlarge";
    let launch_time = Some(Utc::now() - Duration::hours(3));

    let (hourly, accumulated) =
        get_instance_cost_with_tracker(None, &instance_id, instance_type, launch_time, true).await;

    // Should use calculated cost
    assert!(hourly > 0.0);
    assert!(accumulated > 0.0);
    // GPU instances should have higher cost
    assert!(hourly > 0.5);
}

#[tokio::test]
async fn test_get_instance_cost_with_tracker_not_running() {
    let tracker = ResourceTracker::new();

    let status = ResourceStatus {
        id: "stopped-cost-test".to_string(),
        name: None,
        state: ResourceState::Stopped,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now() - Duration::hours(1)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(status.clone()).await.unwrap();

    // For stopped instances, accumulated cost should be 0
    let (hourly, accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &status.id,
        "t3.micro",
        status.launch_time,
        false, // not running
    )
    .await;

    assert_eq!(hourly, 0.01);
    // When not running, accumulated should be 0 (from calculation)
    assert_eq!(accumulated, 0.0);
}

#[tokio::test]
async fn test_get_instance_cost_known_types() {
    use runctl::utils::get_instance_cost;

    // Test known instance types
    assert_eq!(get_instance_cost("t3.micro"), 0.0104);
    assert_eq!(get_instance_cost("t3.small"), 0.0208);
    assert_eq!(get_instance_cost("g4dn.xlarge"), 0.526);
    assert_eq!(get_instance_cost("p5.48xlarge"), 98.32);

    // Test unknown types (should use fallback)
    let unknown_cost = get_instance_cost("unknown.type");
    assert!(unknown_cost > 0.0);
}

#[tokio::test]
async fn test_cost_accumulation_over_time() {
    let tracker = ResourceTracker::new();

    // Create resource that's been running for different durations
    let now = Utc::now();

    let short_running = ResourceStatus {
        id: "short-running".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(now - Duration::minutes(30)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    let long_running = ResourceStatus {
        id: "long-running".to_string(),
        name: None,
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(now - Duration::hours(10)),
        cost_per_hour: 0.01,
        public_ip: None,
        tags: vec![],
    };

    tracker.register(short_running.clone()).await.unwrap();
    tracker.register(long_running.clone()).await.unwrap();

    // Get costs - accumulated_cost is now automatically calculated
    let (short_hourly, short_accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &short_running.id,
        "t3.micro",
        short_running.launch_time,
        true,
    )
    .await;

    let (long_hourly, long_accumulated) = get_instance_cost_with_tracker(
        Some(&tracker),
        &long_running.id,
        "t3.micro",
        long_running.launch_time,
        true,
    )
    .await;

    // Verify hourly costs
    assert_eq!(short_hourly, 0.01);
    assert_eq!(long_hourly, 0.01);

    // Verify accumulated costs are calculated correctly
    // Short running: ~30 minutes = ~0.005
    // Long running: ~10 hours = ~0.10
    assert!(short_accumulated > 0.0);
    assert!(long_accumulated > 0.0);
    // Long running should have much higher accumulated cost
    assert!(long_accumulated > short_accumulated * 15.0); // At least 15x more
    assert!(long_accumulated < short_accumulated * 25.0); // But not more than 25x
}
