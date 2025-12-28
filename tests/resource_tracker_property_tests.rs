//! Property-based tests for ResourceTracker
//!
//! Uses proptest to verify invariants and properties hold across
//! a wide range of inputs and scenarios.

use chrono::{Duration, Utc};
use proptest::prelude::*;
use runctl::provider::{ResourceState, ResourceStatus};
use runctl::resource_tracking::{ResourceTracker, ResourceUsage};
use std::collections::HashSet;

proptest! {
    #[test]
    fn test_resource_tracker_cost_always_non_negative(
        cost_per_hour in 0.0f64..1000.0f64,
        hours_ago in 0i64..720i64  // 0 to 30 days
    ) {
        let tracker = ResourceTracker::new();
        let status = ResourceStatus {
            id: format!("test-{}", fastrand::u64(..)),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(hours_ago)),
            cost_per_hour,
            public_ip: None,
            tags: vec![],
        };

        // Register and get cost
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            tracker.register(status.clone()).await.unwrap();
            tracker.refresh_costs().await;
            let tracked = tracker.get_by_id(&status.id).await.unwrap();

            // Cost should always be non-negative
            prop_assert!(tracked.accumulated_cost >= 0.0);
            prop_assert!(tracked.status.cost_per_hour >= 0.0);
        });
    }

    #[test]
    fn test_resource_tracker_cost_monotonic(
        cost_per_hour in 0.0f64..100.0f64,
        hours1 in 0i64..100i64,
        hours2 in 0i64..100i64
    ) {
        // If hours1 < hours2, cost1 should be <= cost2 (for same hourly rate)
        if hours1 < hours2 {
            let tracker = ResourceTracker::new();
            let status1 = ResourceStatus {
                id: "test-1".to_string(),
                name: None,
                state: ResourceState::Running,
                instance_type: Some("t3.micro".to_string()),
                launch_time: Some(Utc::now() - Duration::hours(hours1)),
                cost_per_hour,
                public_ip: None,
                tags: vec![],
            };

            let status2 = ResourceStatus {
                id: "test-2".to_string(),
                name: None,
                state: ResourceState::Running,
                instance_type: Some("t3.micro".to_string()),
                launch_time: Some(Utc::now() - Duration::hours(hours2)),
                cost_per_hour,
                public_ip: None,
                tags: vec![],
            };

            tokio::runtime::Runtime::new().unwrap().block_on(async {
                tracker.register(status1.clone()).await.unwrap();
                tracker.register(status2.clone()).await.unwrap();
                tracker.refresh_costs().await;

                let tracked1 = tracker.get_by_id(&status1.id).await.unwrap();
                let tracked2 = tracker.get_by_id(&status2.id).await.unwrap();

                // Longer running should have higher or equal cost
                prop_assert!(tracked2.accumulated_cost >= tracked1.accumulated_cost);
            });
        }
    }

    #[test]
    fn test_resource_tracker_stopped_resources_zero_cost(
        cost_per_hour in 0.0f64..100.0f64,
        hours_ago in 1i64..720i64
    ) {
        let tracker = ResourceTracker::new();
        let status = ResourceStatus {
            id: format!("test-{}", fastrand::u64(..)),
            name: None,
            state: ResourceState::Stopped,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now() - Duration::hours(hours_ago)),
            cost_per_hour,
            public_ip: None,
            tags: vec![],
        };

        run_async(|| async {
            tracker.register(status.clone()).await.unwrap();
            let tracked = tracker.get_by_id(&status.id).await.unwrap();

            // Stopped resources should have 0 accumulated cost
            prop_assert_eq!(tracked.accumulated_cost, 0.0);
            Ok(())
        })?;
    }

    #[test]
    fn test_resource_tracker_no_duplicate_ids(
        ids in prop::collection::vec("[a-z0-9-]{1,50}", 1..100)
    ) {
        let tracker = ResourceTracker::new();
        let unique_ids: HashSet<String> = ids.iter().cloned().collect();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut registered = HashSet::new();

            for id in ids {
                let status = ResourceStatus {
                    id: id.clone(),
                    name: None,
                    state: ResourceState::Running,
                    instance_type: Some("t3.micro".to_string()),
                    launch_time: Some(Utc::now()),
                    cost_per_hour: 0.01,
                    public_ip: None,
                    tags: vec![],
                };

                if registered.contains(&id) {
                    // Should fail to register duplicate
                    prop_assert!(tracker.register(status).await.is_err());
                } else {
                    // Should succeed for new ID
                    prop_assert!(tracker.register(status.clone()).await.is_ok());
                    registered.insert(id);
                }
            }

            // Final count should match unique IDs
            let running = tracker.get_running().await;
            prop_assert_eq!(running.len(), unique_ids.len());
            Ok(())
        })?;
    }

    #[test]
    fn test_resource_tracker_usage_history_limit(
        num_updates in 1000..2000usize
    ) {
        let tracker = ResourceTracker::new();
        let resource_id = "usage-limit-test".to_string();

        let status = ResourceStatus {
            id: resource_id.clone(),
            name: None,
            state: ResourceState::Running,
            instance_type: Some("t3.micro".to_string()),
            launch_time: Some(Utc::now()),
            cost_per_hour: 0.01,
            public_ip: None,
            tags: vec![],
        };

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            tracker.register(status).await.unwrap();

            // Add many usage updates
            for i in 0..num_updates {
                let usage = ResourceUsage {
                    cpu_percent: (i % 100) as f64,
                    memory_mb: (i * 10) as f64,
                    gpu_utilization: if i % 2 == 0 { Some(i as f64) } else { None },
                    network_in_mb: (i * 5) as f64,
                    network_out_mb: (i * 2) as f64,
                    timestamp: Utc::now(),
                };
                tracker.update_usage(&resource_id, usage).await.unwrap();
            }

            // History should be capped at 1000
            let tracked = tracker.get_by_id(&resource_id).await.unwrap();
            prop_assert!(tracked.usage_history.len() <= 1000);
            prop_assert_eq!(tracked.usage_history.len(), 1000);
        });
    }

    #[test]
    fn test_resource_tracker_total_cost_sum_property(
        num_resources in 1..50usize,
        cost_per_hour in 0.01f64..10.0f64
    ) {
        let tracker = ResourceTracker::new();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut expected_sum = 0.0;

            for i in 0..num_resources {
                let status = ResourceStatus {
                    id: format!("resource-{}", i),
                    name: None,
                    state: ResourceState::Running,
                    instance_type: Some("t3.micro".to_string()),
                    launch_time: Some(Utc::now() - Duration::hours(i as i64)),
                    cost_per_hour,
                    public_ip: None,
                    tags: vec![],
                };

                tracker.register(status.clone()).await.unwrap();
            }

            tracker.refresh_costs().await;

            for i in 0..num_resources {
                let tracked = tracker.get_by_id(&format!("resource-{}", i)).await.unwrap();
                expected_sum += tracked.accumulated_cost;
            }

            // Total cost should equal sum of individual costs
            let total = tracker.get_total_cost().await;
            // Allow small floating point differences
            prop_assert!((total - expected_sum).abs() < 0.0001);
        });
    }
}
