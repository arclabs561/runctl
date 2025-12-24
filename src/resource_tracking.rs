//! Resource tracking and cost awareness
//!
//! Tracks what resources exist, what's running, and resource usage
//! to enable cost awareness and safe cleanup.

use crate::error::{Result, TrainctlError};
use crate::provider::{ResourceId, ResourceState, ResourceStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Resource usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub gpu_utilization: Option<f64>,
    pub network_in_mb: f64,
    pub network_out_mb: f64,
    pub timestamp: DateTime<Utc>,
}

/// Tracked resource with usage history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedResource {
    pub status: ResourceStatus,
    pub created_at: DateTime<Utc>,
    pub usage_history: Vec<ResourceUsage>,
    pub accumulated_cost: f64,
    pub tags: HashMap<String, String>,
}

/// Resource tracker for cost awareness
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
    pub async fn register(&self, status: ResourceStatus) -> Result<()> {
        let mut resources = self.resources.lock().await;

        if resources.contains_key(&status.id) {
            return Err(TrainctlError::ResourceExists {
                resource_type: "resource".to_string(),
                resource_id: status.id.clone(),
            });
        }

        // Convert tags from Vec<(String, String)> to HashMap
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

    /// Get all running resources
    pub async fn get_running(&self) -> Vec<TrackedResource> {
        let resources = self.resources.lock().await;
        resources
            .values()
            .filter(|r| {
                matches!(
                    r.status.state,
                    ResourceState::Running | ResourceState::Starting
                )
            })
            .cloned()
            .collect()
    }

    /// Get total cost of all resources
    pub async fn get_total_cost(&self) -> f64 {
        let resources = self.resources.lock().await;
        resources.values().map(|r| r.accumulated_cost).sum()
    }

    /// Get resources by tag
    pub async fn get_by_tag(&self, key: &str, value: &str) -> Vec<TrackedResource> {
        let resources = self.resources.lock().await;
        resources
            .values()
            .filter(|r| r.tags.get(key).map(|v| v == value).unwrap_or(false))
            .cloned()
            .collect()
    }

    /// Get resource by ID
    pub async fn get_by_id(&self, resource_id: &ResourceId) -> Option<TrackedResource> {
        let resources = self.resources.lock().await;
        resources.get(resource_id).cloned()
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
