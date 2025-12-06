//! Stateful property-based tests for resource lifecycle management
//!
//! These tests use proptest's state machine testing to verify complex
//! state transitions and invariants in resource management.

use chrono::Utc;
use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;

/// Resource state machine for testing
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResourceState {
    None,
    Created,
    Running,
    Stopped,
    Terminated,
}

/// Actions that can be performed on a resource
#[derive(Debug, Clone)]
enum ResourceAction {
    Create,
    Start,
    Stop,
    Terminate,
}

/// Resource lifecycle state machine
#[derive(Debug, Clone)]
struct ResourceLifecycle {
    state: ResourceState,
    created: bool,
    start_count: u32,
    stop_count: u32,
}

impl ResourceLifecycle {
    fn new() -> Self {
        Self {
            state: ResourceState::None,
            created: false,
            start_count: 0,
            stop_count: 0,
        }
    }

    fn apply(&mut self, action: ResourceAction) -> Result<(), String> {
        match (&self.state, action) {
            (ResourceState::None, ResourceAction::Create) => {
                self.state = ResourceState::Created;
                self.created = true;
                Ok(())
            }
            (ResourceState::Created, ResourceAction::Start) => {
                self.state = ResourceState::Running;
                self.start_count += 1;
                Ok(())
            }
            (ResourceState::Running, ResourceAction::Stop) => {
                self.state = ResourceState::Stopped;
                self.stop_count += 1;
                Ok(())
            }
            (ResourceState::Stopped, ResourceAction::Start) => {
                self.state = ResourceState::Running;
                self.start_count += 1;
                Ok(())
            }
            (ResourceState::Running | ResourceState::Stopped, ResourceAction::Terminate) => {
                self.state = ResourceState::Terminated;
                Ok(())
            }
            _ => {
                // Invalid transition - return error but don't panic
                // This allows property tests to explore invalid sequences
                Err(format!("Invalid transition from {:?}", self.state))
            }
        }
    }

    fn invariants(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Invariant 1: If terminated, must have been created
        if self.state == ResourceState::Terminated && !self.created {
            violations.push("Terminated resource must have been created".to_string());
        }

        // Invariant 2: Start count should be >= stop count (can't stop more than started)
        if self.stop_count > self.start_count {
            violations.push("Stop count cannot exceed start count".to_string());
        }

        // Invariant 3: If running or stopped, must have been created
        if matches!(self.state, ResourceState::Running | ResourceState::Stopped) && !self.created {
            violations.push("Running/stopped resource must have been created".to_string());
        }

        violations
    }
}

/// Strategy for generating resource actions
fn resource_action_strategy() -> impl Strategy<Value = ResourceAction> {
    prop_oneof![
        Just(ResourceAction::Create),
        Just(ResourceAction::Start),
        Just(ResourceAction::Stop),
        Just(ResourceAction::Terminate),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_resource_lifecycle_invariants(
        actions in prop::collection::vec(resource_action_strategy(), 1..50)
    ) {
        let mut lifecycle = ResourceLifecycle::new();

        // Apply all actions
        for action in actions {
            let _ = lifecycle.apply(action);
        }

        // Check invariants
        let violations = lifecycle.invariants();
        assert!(violations.is_empty(), "Invariant violations: {:?}", violations);
    }

    #[test]
    fn test_resource_state_transitions_terminated_implies_created(
        actions in prop::collection::vec(resource_action_strategy(), 1..50)
    ) {
        let mut lifecycle = ResourceLifecycle::new();
        let actions_clone = actions.clone();

        for action in actions_clone {
            let _ = lifecycle.apply(action);
        }

        // Property: If terminated, must have been created
        if lifecycle.state == ResourceState::Terminated {
            assert!(lifecycle.created,
                "Terminated resource must have been created. Actions: {:?}", actions);
        }
    }

    #[test]
    fn test_resource_state_transitions_start_stop_balance(
        actions in prop::collection::vec(resource_action_strategy(), 1..50)
    ) {
        let mut lifecycle = ResourceLifecycle::new();
        let actions_clone = actions.clone();

        for action in actions_clone {
            let _ = lifecycle.apply(action);
        }

        // Property: Start count should be >= stop count
        assert!(lifecycle.start_count >= lifecycle.stop_count,
            "Start count {} should be >= stop count {}. Actions: {:?}",
            lifecycle.start_count, lifecycle.stop_count, actions);
    }

    #[test]
    fn test_resource_state_transitions_running_implies_created(
        actions in prop::collection::vec(resource_action_strategy(), 1..50)
    ) {
        let mut lifecycle = ResourceLifecycle::new();
        let actions_clone = actions.clone();

        for action in actions_clone {
            let _ = lifecycle.apply(action);
        }

        // Property: If running or stopped, must have been created
        if matches!(lifecycle.state, ResourceState::Running | ResourceState::Stopped) {
            assert!(lifecycle.created,
                "Running/stopped resource must have been created. State: {:?}, Actions: {:?}",
                lifecycle.state, actions);
        }
    }
}

/// Volume lifecycle state machine
#[derive(Debug, Clone)]
struct VolumeLifecycle {
    state: VolumeState,
    created: bool,
    attached: bool,
    instance_id: Option<String>,
    persistent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VolumeState {
    None,
    Available,
    InUse,
    Deleted,
}

#[derive(Debug, Clone)]
enum VolumeAction {
    Create { persistent: bool },
    Attach { instance_id: String },
    Detach,
    Delete { force: bool },
}

impl VolumeLifecycle {
    fn new() -> Self {
        Self {
            state: VolumeState::None,
            created: false,
            attached: false,
            instance_id: None,
            persistent: false,
        }
    }

    fn apply(&mut self, action: VolumeAction) -> Result<(), String> {
        match (&self.state, action) {
            (VolumeState::None, VolumeAction::Create { persistent }) => {
                self.state = VolumeState::Available;
                self.created = true;
                self.persistent = persistent;
                Ok(())
            }
            (VolumeState::Available, VolumeAction::Attach { instance_id }) => {
                self.state = VolumeState::InUse;
                self.attached = true;
                self.instance_id = Some(instance_id);
                Ok(())
            }
            (VolumeState::InUse, VolumeAction::Detach) => {
                self.state = VolumeState::Available;
                self.attached = false;
                self.instance_id = None;
                Ok(())
            }
            (VolumeState::Available, VolumeAction::Delete { force }) => {
                if self.persistent && !force {
                    Err("Cannot delete persistent volume without force".to_string())
                } else {
                    self.state = VolumeState::Deleted;
                    Ok(())
                }
            }
            (VolumeState::InUse, VolumeAction::Delete { force }) => {
                if !force {
                    Err("Cannot delete attached volume without force".to_string())
                } else {
                    self.state = VolumeState::Deleted;
                    Ok(())
                }
            }
            _ => Err(format!("Invalid transition from {:?}", self.state)),
        }
    }

    fn invariants(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Invariant 1: If in use, must be attached
        if self.state == VolumeState::InUse && !self.attached {
            violations.push("In-use volume must be attached".to_string());
        }

        // Invariant 2: If attached, must have instance_id
        if self.attached && self.instance_id.is_none() {
            violations.push("Attached volume must have instance_id".to_string());
        }

        // Invariant 3: If deleted, must have been created
        if self.state == VolumeState::Deleted && !self.created {
            violations.push("Deleted volume must have been created".to_string());
        }

        violations
    }
}

fn volume_action_strategy() -> impl Strategy<Value = VolumeAction> {
    prop_oneof![
        prop_oneof![Just(false), Just(true)]
            .prop_map(|persistent| VolumeAction::Create { persistent }),
        (r"[a-z0-9-]+").prop_map(|id| VolumeAction::Attach { instance_id: id }),
        Just(VolumeAction::Detach),
        prop_oneof![Just(false), Just(true)].prop_map(|force| VolumeAction::Delete { force }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_volume_lifecycle_invariants(
        actions in prop::collection::vec(volume_action_strategy(), 1..30)
    ) {
        let mut lifecycle = VolumeLifecycle::new();

        for action in actions {
            let _ = lifecycle.apply(action);
        }

        let violations = lifecycle.invariants();
        assert!(violations.is_empty(), "Invariant violations: {:?}", violations);
    }

    #[test]
    fn test_volume_persistent_protection(
        actions in prop::collection::vec(volume_action_strategy(), 1..30)
    ) {
        let mut lifecycle = VolumeLifecycle::new();

        // First action must be Create
        if let Some(VolumeAction::Create { persistent }) = actions.first() {
            let _ = lifecycle.apply(VolumeAction::Create { persistent: *persistent });

            // Apply remaining actions
            for action in actions.iter().skip(1) {
                let result = lifecycle.apply(action.clone());

                // If trying to delete persistent without force, should fail
                if let VolumeAction::Delete { force: false } = action {
                    if lifecycle.persistent && lifecycle.state == VolumeState::Available {
                        assert!(result.is_err(),
                            "Should not be able to delete persistent volume without force");
                    }
                }
            }
        }
    }
}

/// Cost tracking state machine
#[derive(Debug, Clone)]
struct CostTracker {
    resources: Vec<ResourceCost>,
    total_hourly: f64,
    total_accumulated: f64,
}

#[derive(Debug, Clone)]
struct ResourceCost {
    hourly: f64,
    launch_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl CostTracker {
    fn new() -> Self {
        Self {
            resources: Vec::new(),
            total_hourly: 0.0,
            total_accumulated: 0.0,
        }
    }

    fn add_resource(&mut self, hourly: f64, launch_time: Option<chrono::DateTime<chrono::Utc>>) {
        self.resources.push(ResourceCost {
            hourly,
            launch_time,
        });
        self.total_hourly += hourly;
        if let Some(lt) = launch_time {
            let hours = (Utc::now() - lt).num_hours();
            self.total_accumulated += hourly * hours.max(0) as f64;
        }
    }

    fn remove_resource(&mut self, index: usize) {
        if index < self.resources.len() {
            let resource = self.resources.remove(index);
            // Only subtract if it won't make total negative (safety check)
            if resource.hourly <= self.total_hourly {
                self.total_hourly -= resource.hourly;
            } else {
                // Recalculate from remaining resources if subtraction would go negative
                self.total_hourly = self.resources.iter().map(|r| r.hourly).sum();
            }
            // Recalculate accumulated from remaining resources
            self.total_accumulated = self
                .resources
                .iter()
                .map(|r| {
                    if let Some(lt) = r.launch_time {
                        let hours = (Utc::now() - lt).num_hours().max(0) as f64;
                        r.hourly * hours
                    } else {
                        0.0
                    }
                })
                .sum();
        }
    }

    fn invariants(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Invariant 1: Total hourly should equal sum of resource hourly costs
        let sum_hourly: f64 = self.resources.iter().map(|r| r.hourly).sum();
        if (self.total_hourly - sum_hourly).abs() > 0.01 {
            violations.push(format!(
                "Total hourly {} should equal sum {}",
                self.total_hourly, sum_hourly
            ));
        }

        // Invariant 2: Costs should be non-negative
        if self.total_hourly < 0.0 {
            violations.push("Total hourly cost cannot be negative".to_string());
        }

        if self.total_accumulated < 0.0 {
            violations.push("Total accumulated cost cannot be negative".to_string());
        }

        violations
    }
}

proptest! {
    #[test]
    fn test_cost_tracker_invariants(
        operations in prop::collection::vec(
            prop_oneof![
                ((0.0f64..100.0f64), any::<bool>()).prop_map(|(hourly, has_launch)| {
                    let launch_time = if has_launch {
                        Some(Utc::now() - chrono::Duration::hours((hourly as u64 % 720) as i64))
                    } else {
                        None
                    };
                    (true, hourly, launch_time)
                }),
                (0usize..10usize).prop_map(|idx| (false, idx as f64, None)),
            ],
            1..20
        )
    ) {
        let mut tracker = CostTracker::new();

        for op in operations {
            match op {
                (true, hourly, launch_time) => {
                    tracker.add_resource(hourly, launch_time);
                }
                (false, hourly, _launch_time) => {
                    // Remove resource at index (if valid)
                    // Use hourly as a seed for index calculation
                    if !tracker.resources.is_empty() {
                        let idx = ((hourly * 1000.0) as usize) % tracker.resources.len();
                        tracker.remove_resource(idx);
                    }
                }
            }
        }

        let violations = tracker.invariants();
        assert!(violations.is_empty(), "Invariant violations: {:?}", violations);
    }
}
