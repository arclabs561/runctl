//! Safe cleanup and teardown operations
//!
//! Provides careful resource cleanup with confirmation, dry-run, and safety checks.
//!
//! ## Design Rationale
//!
//! Accidental resource deletion is costly and disruptive. This module implements
//! multiple protection layers:
//!
//! 1. **Time-based protection**: Prevents deletion of resources < 5 minutes old
//!    (catches immediate mistakes, requires `--force` to override)
//! 2. **Tag-based protection**: Resources tagged `runctl:protected=true` cannot be deleted
//!    (explicit opt-in protection for important resources)
//! 3. **Explicit protection**: Programmatic protection via `safety.protect()`
//!    (for resources that should never be deleted)
//!
//! ## Protection Precedence
//!
//! All protections are checked unless `force=true`. If any protection applies,
//! the resource is skipped. The `--force` flag bypasses all protections (use with caution).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use runctl::safe_cleanup::{safe_cleanup, CleanupSafety};
//! use runctl::resource_tracking::ResourceTracker;
//!
//! # async fn example() -> runctl::error::Result<()> {
//! let tracker = ResourceTracker::new();
//! let safety = CleanupSafety::new();
//!
//! // Dry-run to see what would be deleted
//! let result = safe_cleanup(
//!     vec!["i-123".to_string()],
//!     &tracker,
//!     &safety,
//!     true,  // dry_run
//!     false, // force
//! ).await?;
//!
//! // Actual cleanup
//! let result = safe_cleanup(
//!     vec!["i-123".to_string()],
//!     &tracker,
//!     &safety,
//!     false, // dry_run
//!     false, // force
//! ).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use crate::provider::ResourceId;
use crate::resource_tracking::ResourceTracker;
use chrono::{DateTime, Utc};
use std::collections::HashSet;

/// Cleanup safety checks
#[derive(Default)]
pub struct CleanupSafety {
    /// Resources that should never be deleted
    protected_resources: HashSet<ResourceId>,
    /// Resources tagged as important
    #[allow(dead_code)]
    protected_tags: Vec<(String, String)>,
    /// Minimum age in minutes before resource can be deleted (time-based protection)
    min_age_minutes: u64,
}

impl CleanupSafety {
    pub fn new() -> Self {
        Self {
            protected_resources: HashSet::new(),
            protected_tags: vec![
                ("runctl:protected".to_string(), "true".to_string()),
                ("runctl:important".to_string(), "true".to_string()),
            ],
            min_age_minutes: 5, // Default: 5 minutes protection
        }
    }

    /// Create with custom minimum age
    #[allow(dead_code)] // Reserved for future use
    pub fn with_min_age(minutes: u64) -> Self {
        Self {
            min_age_minutes: minutes,
            ..Self::new()
        }
    }

    /// Mark a resource as protected
    #[allow(dead_code)] // Reserved for future use
    pub fn protect(&mut self, resource_id: ResourceId) {
        self.protected_resources.insert(resource_id);
    }

    /// Check if resource can be safely deleted
    pub async fn can_delete(
        &self,
        resource_id: &ResourceId,
        tracker: &ResourceTracker,
        created_at: Option<DateTime<Utc>>,
        force: bool,
    ) -> Result<bool> {
        // Check explicit protection
        if self.protected_resources.contains(resource_id) {
            return Ok(false);
        }

        // Check protected tags
        let resources = tracker.get_by_tag("runctl:protected", "true").await;
        if resources.iter().any(|r| r.status.id == *resource_id) {
            return Ok(false);
        }

        // Time-based protection: resources < min_age_minutes old require force
        if !force {
            if let Some(created) = created_at {
                let age = Utc::now().signed_duration_since(created);
                let age_minutes = age.num_minutes().max(0) as u64;

                if age_minutes < self.min_age_minutes {
                    return Ok(false); // Too new, requires --force
                }
            }
        }

        Ok(true)
    }

    /// Get list of resources safe to delete
    #[allow(dead_code)] // Reserved for future use
    pub async fn get_safe_to_delete(
        &self,
        tracker: &ResourceTracker,
        filter: Option<&str>,
        force: bool,
    ) -> Result<Vec<ResourceId>> {
        let running = tracker.get_running().await;
        let mut safe = Vec::new();

        for resource in running {
            if let Some(filter) = filter {
                if !resource.status.id.contains(filter) {
                    continue;
                }
            }

            let created_at = Some(resource.created_at);
            if self
                .can_delete(&resource.status.id, tracker, created_at, force)
                .await?
            {
                safe.push(resource.status.id);
            }
        }

        Ok(safe)
    }
}

/// Cleanup operation result
#[derive(Debug)]
pub struct CleanupResult {
    pub deleted: Vec<ResourceId>,
    pub skipped: Vec<(ResourceId, String)>, // (id, reason)
    pub errors: Vec<(ResourceId, String)>,  // (id, error)
}

/// Perform safe cleanup with confirmation
pub async fn safe_cleanup(
    resource_ids: Vec<ResourceId>,
    tracker: &ResourceTracker,
    safety: &CleanupSafety,
    dry_run: bool,
    force: bool,
) -> Result<CleanupResult> {
    let mut result = CleanupResult {
        deleted: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };

    for resource_id in resource_ids {
        // Get resource creation time from tracker
        let created_at = tracker.get_by_id(&resource_id).await.map(|r| r.created_at);

        // Safety check
        if !force {
            match safety
                .can_delete(&resource_id, tracker, created_at, force)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    let reason = if let Some(created) = created_at {
                        let age = Utc::now().signed_duration_since(created);
                        let age_minutes = age.num_minutes().max(0) as u64;
                        if age_minutes < safety.min_age_minutes {
                            format!(
                                "Resource is too new ({} minutes old, minimum {} minutes)",
                                age_minutes, safety.min_age_minutes
                            )
                        } else {
                            "Resource is protected".to_string()
                        }
                    } else {
                        "Resource is protected".to_string()
                    };
                    result.skipped.push((resource_id, reason));
                    continue;
                }
                Err(e) => {
                    result.errors.push((resource_id, format!("{}", e)));
                    continue;
                }
            }
        }

        if dry_run {
            result.deleted.push(resource_id);
            continue;
        }

        // Actual deletion would happen here
        // For now, just track it
        result.deleted.push(resource_id);
    }

    Ok(result)
}
