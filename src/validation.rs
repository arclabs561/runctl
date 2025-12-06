//! Input validation utilities
//!
//! Provides validation functions for user inputs to prevent
//! invalid data from causing runtime errors.

use crate::error::{Result, TrainctlError};

/// Validate EC2 instance ID format
///
/// Instance IDs must start with "i-" followed by 17 hexadecimal characters.
pub fn validate_instance_id(instance_id: &str) -> Result<()> {
    if !instance_id.starts_with("i-") {
        return Err(TrainctlError::Validation {
            field: "instance_id".to_string(),
            reason: format!("Instance ID must start with 'i-', got: {}", instance_id),
        });
    }

    if instance_id.len() < 10 || instance_id.len() > 19 {
        return Err(TrainctlError::Validation {
            field: "instance_id".to_string(),
            reason: format!(
                "Instance ID must be 10-19 characters, got: {} (len: {})",
                instance_id,
                instance_id.len()
            ),
        });
    }

    // Check that remaining characters are alphanumeric
    let id_part = &instance_id[2..];
    if !id_part.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(TrainctlError::Validation {
            field: "instance_id".to_string(),
            reason: format!(
                "Instance ID must contain only alphanumeric characters after 'i-', got: {}",
                instance_id
            ),
        });
    }

    Ok(())
}

/// Validate EBS volume ID format
///
/// Volume IDs must start with "vol-" followed by hexadecimal characters.
#[allow(dead_code)] // Reserved for future volume ID validation
pub fn validate_volume_id(volume_id: &str) -> Result<()> {
    if !volume_id.starts_with("vol-") {
        return Err(TrainctlError::Validation {
            field: "volume_id".to_string(),
            reason: format!("Volume ID must start with 'vol-', got: {}", volume_id),
        });
    }

    if volume_id.len() < 15 || volume_id.len() > 21 {
        return Err(TrainctlError::Validation {
            field: "volume_id".to_string(),
            reason: format!(
                "Volume ID must be 15-21 characters, got: {} (len: {})",
                volume_id,
                volume_id.len()
            ),
        });
    }

    Ok(())
}

/// Validate snapshot ID format
///
/// Snapshot IDs must start with "snap-" followed by hexadecimal characters.
#[allow(dead_code)] // Reserved for future snapshot ID validation
pub fn validate_snapshot_id(snapshot_id: &str) -> Result<()> {
    if !snapshot_id.starts_with("snap-") {
        return Err(TrainctlError::Validation {
            field: "snapshot_id".to_string(),
            reason: format!("Snapshot ID must start with 'snap-', got: {}", snapshot_id),
        });
    }

    if snapshot_id.len() < 16 || snapshot_id.len() > 22 {
        return Err(TrainctlError::Validation {
            field: "snapshot_id".to_string(),
            reason: format!(
                "Snapshot ID must be 16-22 characters, got: {} (len: {})",
                snapshot_id,
                snapshot_id.len()
            ),
        });
    }

    Ok(())
}

/// Validate project name
///
/// Project names should be alphanumeric with hyphens/underscores, max 64 chars.
/// Validate project name format
///
/// Currently unused but kept for future validation needs.
#[allow(dead_code)]
pub fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(TrainctlError::Validation {
            field: "project_name".to_string(),
            reason: "Project name cannot be empty".to_string(),
        });
    }

    if name.len() > 64 {
        return Err(TrainctlError::Validation {
            field: "project_name".to_string(),
            reason: format!(
                "Project name must be <= 64 characters, got: {} (len: {})",
                name,
                name.len()
            ),
        });
    }

    // Allow alphanumeric, hyphens, underscores, dots
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(TrainctlError::Validation {
            field: "project_name".to_string(),
            reason: format!("Project name can only contain alphanumeric characters, hyphens, underscores, and dots, got: {}", name),
        });
    }

    Ok(())
}

/// Validate path for security (prevent path traversal)
///
/// Checks that path doesn't contain ".." or other dangerous patterns.
pub fn validate_path(path: &str) -> Result<()> {
    if path.contains("..") {
        return Err(TrainctlError::Validation {
            field: "path".to_string(),
            reason: "Path cannot contain '..' (path traversal not allowed)".to_string(),
        });
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(TrainctlError::Validation {
            field: "path".to_string(),
            reason: "Path cannot contain null bytes".to_string(),
        });
    }

    Ok(())
}

/// Validate S3 path format
///
/// S3 paths must be in format s3://bucket/key
pub fn validate_s3_path(s3_path: &str) -> Result<()> {
    if !s3_path.starts_with("s3://") {
        return Err(TrainctlError::Validation {
            field: "s3_path".to_string(),
            reason: format!("S3 path must start with 's3://', got: {}", s3_path),
        });
    }

    let without_prefix = &s3_path[5..];
    if without_prefix.is_empty() {
        return Err(TrainctlError::Validation {
            field: "s3_path".to_string(),
            reason: "S3 path must include bucket name".to_string(),
        });
    }

    let parts: Vec<&str> = without_prefix.splitn(2, '/').collect();
    if parts[0].is_empty() {
        return Err(TrainctlError::Validation {
            field: "s3_path".to_string(),
            reason: "S3 bucket name cannot be empty".to_string(),
        });
    }

    // Validate bucket name (simplified - AWS has more complex rules)
    if parts[0].len() < 3 || parts[0].len() > 63 {
        return Err(TrainctlError::Validation {
            field: "s3_path".to_string(),
            reason: format!(
                "S3 bucket name must be 3-63 characters, got: {} (len: {})",
                parts[0],
                parts[0].len()
            ),
        });
    }

    Ok(())
}

/// Validate volume size (in GB)
///
/// Volume sizes must be between 1 GB and 16384 GB (16 TB).
#[allow(dead_code)] // Reserved for future volume size validation
pub fn validate_volume_size(size_gb: i32) -> Result<()> {
    if size_gb < 1 {
        return Err(TrainctlError::Validation {
            field: "volume_size".to_string(),
            reason: format!("Volume size must be at least 1 GB, got: {}", size_gb),
        });
    }

    if size_gb > 16384 {
        return Err(TrainctlError::Validation {
            field: "volume_size".to_string(),
            reason: format!(
                "Volume size must be at most 16384 GB (16 TB), got: {}",
                size_gb
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_instance_id() {
        assert!(validate_instance_id("i-1234567890abcdef0").is_ok());
        assert!(validate_instance_id("i-0abcdef1234567890").is_ok());
        assert!(validate_instance_id("i-123").is_err()); // Too short
        assert!(validate_instance_id("vol-123").is_err()); // Wrong prefix
        assert!(validate_instance_id("invalid").is_err()); // No prefix
    }

    #[test]
    fn test_validate_volume_id() {
        assert!(validate_volume_id("vol-1234567890abcdef0").is_ok());
        assert!(validate_volume_id("vol-123").is_err()); // Too short
        assert!(validate_volume_id("i-1234567890abcdef0").is_err()); // Wrong prefix
    }

    #[test]
    fn test_validate_project_name() {
        assert!(validate_project_name("my-project").is_ok());
        assert!(validate_project_name("my_project").is_ok());
        assert!(validate_project_name("project123").is_ok());
        assert!(validate_project_name("").is_err()); // Empty
        assert!(validate_project_name(&"a".repeat(65)).is_err()); // Too long
        assert!(validate_project_name("project/name").is_err()); // Invalid char
    }

    #[test]
    fn test_validate_path() {
        assert!(validate_path("/valid/path").is_ok());
        assert!(validate_path("valid/path").is_ok());
        assert!(validate_path("../invalid").is_err()); // Path traversal
        assert!(validate_path("valid/../invalid").is_err()); // Path traversal
        assert!(validate_path("valid\0path").is_err()); // Null byte
    }

    #[test]
    fn test_validate_s3_path() {
        assert!(validate_s3_path("s3://bucket/key").is_ok());
        assert!(validate_s3_path("s3://bucket").is_ok());
        assert!(validate_s3_path("s3://").is_err()); // No bucket
        assert!(validate_s3_path("invalid").is_err()); // Wrong format
    }

    #[test]
    fn test_validate_volume_size() {
        assert!(validate_volume_size(1).is_ok());
        assert!(validate_volume_size(100).is_ok());
        assert!(validate_volume_size(16384).is_ok());
        assert!(validate_volume_size(0).is_err()); // Too small
        assert!(validate_volume_size(16385).is_err()); // Too large
    }
}
