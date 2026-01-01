//! Resource tracking and cost awareness
//!
//! Tracks resource lifecycle and usage to enable cost awareness and safe cleanup.
//!
//! ## Design
//!
//! `ResourceTracker` maintains an in-memory map of resources. It's designed for
//! single-process use (CLI tool). Resources are registered when created and
//! updated as their state changes.
//!
//! ## Cost Calculation
//!
//! Accumulated cost is calculated on-demand when accessing resources via
//! `get_running()`, `get_by_id()`, etc. This ensures costs are always current
//! based on `launch_time` and `cost_per_hour`, without requiring periodic
//! background tasks.
//!
//! ## Resource Lifecycle
//!
//! - `ResourceStatus`: Provider-agnostic resource state (from `provider` module)
//! - `TrackedResource`: Internal tracking with usage history and accumulated cost
//! - Resources are registered via `register()` when created
//! - State updates via `update_state()` as resources transition
//! - Usage metrics added via `update_usage()` for monitoring
//!
//! ## Thread Safety
//!
//! Uses `Arc<Mutex<HashMap>>` for thread-safe access. All methods are async
//! to avoid blocking on the mutex.

use crate::error::{Result, TrainctlError};
use crate::provider::{ResourceId, ResourceState, ResourceStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Resource usage metrics at a point in time
///
/// Captures CPU, memory, GPU, and network usage for a resource at a specific
/// timestamp. Used to track resource utilization over time and calculate costs.
///
/// ## Usage
///
/// Metrics are typically collected periodically (e.g., every 5 seconds) and
/// added to a resource's usage history via `ResourceTracker::update_usage()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub gpu_utilization: Option<f64>,
    pub network_in_mb: f64,
    pub network_out_mb: f64,
    pub timestamp: DateTime<Utc>,
}

/// Tracked resource with usage history and cost information
///
/// Extends `ResourceStatus` with tracking-specific information:
/// - Creation timestamp for cost calculation
/// - Usage history for monitoring and analysis
/// - Accumulated cost based on runtime
/// - Tags for organization and filtering
///
/// ## Cost Calculation
///
/// `accumulated_cost` is calculated on-demand when accessing resources. It's
/// based on the resource's `cost_per_hour` and time elapsed since `created_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedResource {
    pub status: ResourceStatus,
    pub created_at: DateTime<Utc>,
    pub usage_history: Vec<ResourceUsage>,
    pub accumulated_cost: f64,
    pub tags: HashMap<String, String>,
}

/// Resource tracker for cost awareness and lifecycle management
///
/// Maintains an in-memory registry of resources with their states, usage history,
/// and accumulated costs. Designed for single-process use (CLI tool).
///
/// ## Thread Safety
///
/// Uses `Arc<Mutex<HashMap>>` internally for thread-safe access. All methods
/// are async to avoid blocking on the mutex.
///
/// ## Resource Lifecycle
///
/// 1. **Register**: Resources are registered when created via `register()`
/// 2. **Update State**: State changes are tracked via `update_state()`
/// 3. **Update Usage**: Usage metrics are added via `update_usage()`
/// 4. **Query**: Access resources via `get_running()`, `get_by_id()`, etc.
///
/// ## Cost Calculation
///
/// Accumulated cost is calculated on-demand when accessing resources. This
/// ensures costs are always current based on `launch_time` and `cost_per_hour`,
/// without requiring periodic background tasks.
///
/// ## Examples
///
/// ```rust,no_run
/// use runctl::resource_tracking::ResourceTracker;
/// use runctl::provider::{ResourceStatus, ResourceState};
///
/// # async fn example() -> runctl::error::Result<()> {
/// let tracker = ResourceTracker::new();
///
/// // Register a new resource
/// let status = ResourceStatus {
///     id: "i-123".to_string(),
///     state: ResourceState::Running,
///     // ... other fields
/// };
/// tracker.register(status).await?;
///
/// // Get all running resources with costs
/// let running = tracker.get_running().await;
/// # Ok(())
/// # }
/// ```
pub struct ResourceTracker {
    resources: Arc<Mutex<HashMap<ResourceId, TrackedResource>>>,
}

impl ResourceTracker {
    pub fn new() -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a new resource
    ///
    /// The resource's accumulated_cost will be automatically calculated
    /// when it is accessed via get_running(), get_by_id(), etc.
    pub async fn register(&self, status: ResourceStatus) -> Result<()> {
        let mut resources = self.resources.lock().await;

        if resources.contains_key(&status.id) {
            return Err(TrainctlError::ResourceExists {
                resource_type: "resource".to_string(),
                resource_id: status.id.clone(),
            });
        }

        // Convert tags from Vec<(String, String)> to HashMap
        // Note: We clone here because status.tags is needed for ResourceStatus.
        // The optimization to use into_iter() would require restructuring to extract
        // tags first, which adds complexity. Cloning is acceptable here since tags
        // are typically small (<15 items based on research).
        let tags: HashMap<String, String> = status.tags.iter().cloned().collect();

        resources.insert(
            status.id.clone(),
            TrackedResource {
                status,
                created_at: Utc::now(),
                usage_history: Vec::new(),
                accumulated_cost: 0.0,
                tags,
            },
        );

        Ok(())
    }

    /// Update resource status and usage
    ///
    /// This adds a usage record to the resource's history.
    /// Note: This does not update accumulated_cost - that is calculated
    /// automatically when the resource is accessed via get_running(), get_by_id(), etc.
    pub async fn update_usage(&self, resource_id: &ResourceId, usage: ResourceUsage) -> Result<()> {
        let mut resources = self.resources.lock().await;

        let resource =
            resources
                .get_mut(resource_id)
                .ok_or_else(|| TrainctlError::ResourceNotFound {
                    resource_type: "resource".to_string(),
                    resource_id: resource_id.clone(),
                })?;

        resource.usage_history.push(usage);

        // Keep only last 1000 usage records
        if resource.usage_history.len() > 1000 {
            resource.usage_history.remove(0);
        }

        Ok(())
    }

    /// Update resource state (e.g., when instance is stopped/started)
    ///
    /// Updates the state of a tracked resource. This is useful when resources
    /// change state outside of the normal sync process.
    ///
    /// # Arguments
    /// * `resource_id` - The ID of the resource to update
    /// * `new_state` - The new state of the resource
    ///
    /// # Errors
    /// Returns `ResourceNotFound` if the resource doesn't exist in the tracker.
    pub async fn update_state(
        &self,
        resource_id: &ResourceId,
        new_state: crate::provider::ResourceState,
    ) -> Result<()> {
        let mut resources = self.resources.lock().await;

        let resource =
            resources
                .get_mut(resource_id)
                .ok_or_else(|| TrainctlError::ResourceNotFound {
                    resource_type: "resource".to_string(),
                    resource_id: resource_id.clone(),
                })?;

        resource.status.state = new_state;
        // Update cost since state affects cost calculation
        Self::update_resource_cost(resource);

        Ok(())
    }

    /// Calculate accumulated cost for a resource based on launch time
    ///
    /// For running resources, calculates cost from launch_time to now.
    /// For stopped/terminated resources, preserves the existing accumulated_cost
    /// (doesn't reset to 0.0) since those costs were already incurred.
    fn calculate_accumulated_cost_for_resource(resource: &TrackedResource) -> f64 {
        use crate::utils::calculate_accumulated_cost;
        if matches!(
            resource.status.state,
            ResourceState::Running | ResourceState::Starting
        ) {
            // Running: calculate cost from launch_time to now
            calculate_accumulated_cost(resource.status.cost_per_hour, resource.status.launch_time)
        } else {
            // Stopped/Terminated: preserve existing accumulated cost
            // (don't reset to 0.0 - those costs were already incurred)
            resource.accumulated_cost
        }
    }

    /// Update accumulated cost for a resource in-place
    fn update_resource_cost(resource: &mut TrackedResource) {
        resource.accumulated_cost = Self::calculate_accumulated_cost_for_resource(resource);
    }

    /// Get all running resources with automatically updated costs
    ///
    /// The accumulated_cost for each resource is recalculated based on
    /// launch time and current time before returning.
    pub async fn get_running(&self) -> Vec<TrackedResource> {
        let mut resources = self.resources.lock().await;
        resources
            .values_mut()
            .filter(|r| {
                matches!(
                    r.status.state,
                    ResourceState::Running | ResourceState::Starting
                )
            })
            .map(|r| {
                // Update accumulated cost before returning
                Self::update_resource_cost(r);
                r.clone()
            })
            .collect()
    }

    /// Get total cost of all resources with automatically updated costs
    ///
    /// All resource costs are recalculated before summing.
    pub async fn get_total_cost(&self) -> f64 {
        let mut resources = self.resources.lock().await;
        resources
            .values_mut()
            .map(|r| {
                // Update accumulated cost before summing
                Self::update_resource_cost(r);
                r.accumulated_cost
            })
            .sum()
    }

    /// Refresh costs for all resources
    ///
    /// Manually recalculates accumulated_cost for all resources.
    /// This is useful for periodic updates or before generating reports.
    /// Note: Costs are automatically updated when accessing resources via
    /// get_running(), get_by_id(), etc., so this is usually not necessary.
    pub async fn refresh_costs(&self) {
        let mut resources = self.resources.lock().await;
        for resource in resources.values_mut() {
            Self::update_resource_cost(resource);
        }
    }

    /// Get resources by tag with automatically updated costs
    ///
    /// The accumulated_cost for each matching resource is recalculated.
    pub async fn get_by_tag(&self, key: &str, value: &str) -> Vec<TrackedResource> {
        let mut resources = self.resources.lock().await;
        resources
            .values_mut()
            .filter(|r| r.tags.get(key).map(|v| v == value).unwrap_or(false))
            .map(|r| {
                // Update accumulated cost before returning
                Self::update_resource_cost(r);
                r.clone()
            })
            .collect()
    }

    /// Get resource by ID with automatically updated cost
    ///
    /// The accumulated_cost is recalculated based on launch time and current time.
    pub async fn get_by_id(&self, resource_id: &ResourceId) -> Option<TrackedResource> {
        let mut resources = self.resources.lock().await;
        resources.get_mut(resource_id).map(|r| {
            // Update accumulated cost before returning
            Self::update_resource_cost(r);
            r.clone()
        })
    }

    /// Check if resource exists
    pub async fn exists(&self, resource_id: &ResourceId) -> bool {
        let resources = self.resources.lock().await;
        resources.contains_key(resource_id)
    }

    /// Remove resource (after cleanup)
    pub async fn remove(&self, resource_id: &ResourceId) -> Result<()> {
        let mut resources = self.resources.lock().await;
        resources
            .remove(resource_id)
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "resource".to_string(),
                resource_id: resource_id.clone(),
            })?;
        Ok(())
    }
}

impl Default for ResourceTracker {
    fn default() -> Self {
        Self::new()
    }
}
