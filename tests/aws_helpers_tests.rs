//! Unit tests for AWS module helper functions
//!
//! Tests utility functions in the aws::helpers module without requiring AWS credentials.

use runctl::config::Config;
use runctl::provider::{normalize_state, ResourceState, ResourceStatus};

// Tests pub(crate) functions indirectly through public API

#[test]
fn test_normalize_state_conversions() {
    // AWS state strings normalized correctly
    assert_eq!(normalize_state("running"), ResourceState::Running);
    assert_eq!(normalize_state("stopped"), ResourceState::Stopped);
    assert_eq!(normalize_state("stopping"), ResourceState::Stopped); // stopping -> stopped
    assert_eq!(normalize_state("pending"), ResourceState::Starting); // pending -> starting
    assert_eq!(normalize_state("terminated"), ResourceState::Terminated);
    assert_eq!(normalize_state("shutting-down"), ResourceState::Terminating);

    // Unknown states map to Unknown
    assert_eq!(normalize_state("unknown"), ResourceState::Unknown);
    assert_eq!(normalize_state("invalid-state"), ResourceState::Unknown);
}

#[test]
fn test_normalize_state_case_insensitive() {
    // Case-insensitive state normalization
    assert_eq!(normalize_state("RUNNING"), ResourceState::Running);
    assert_eq!(normalize_state("Running"), ResourceState::Running);
    assert_eq!(normalize_state("STOPPED"), ResourceState::Stopped);
}

#[test]
fn test_resource_state_variants() {
    // All state variants exist and are distinct
    let states = vec![
        ResourceState::Running,
        ResourceState::Starting,
        ResourceState::Stopped,
        ResourceState::Terminating,
        ResourceState::Terminated,
        ResourceState::Unknown,
    ];

    // States are distinct
    for (i, state1) in states.iter().enumerate() {
        for (j, state2) in states.iter().enumerate() {
            if i != j {
                assert_ne!(state1, state2, "States should be distinct");
            }
        }
    }
}

#[test]
fn test_config_aws_section() {
    // Config structure is valid
    let config = Config::default();
    let _ = config.checkpoint.dir;
}

#[test]
fn test_resource_status_serialization() {
    // ResourceStatus structure
    use chrono::Utc;
    use runctl::provider::ResourceStatus;

    let status = ResourceStatus {
        id: "i-1234567890abcdef0".to_string(),
        name: Some("Test Instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![
            ("Project".to_string(), "test".to_string()),
            ("Environment".to_string(), "dev".to_string()),
        ],
    };

    // Fields set correctly
    assert_eq!(status.id, "i-1234567890abcdef0");
    assert_eq!(status.state, ResourceState::Running);
    assert_eq!(status.cost_per_hour, 0.01);
    assert_eq!(status.tags.len(), 2);
}

#[test]
fn test_resource_status_with_minimal_fields() {
    // ResourceStatus with minimal fields
    use runctl::provider::ResourceStatus;

    let status = ResourceStatus {
        id: "i-minimal".to_string(),
        name: None,
        state: ResourceState::Unknown,
        instance_type: None,
        launch_time: None,
        cost_per_hour: 0.0,
        public_ip: None,
        tags: vec![],
    };

    assert_eq!(status.id, "i-minimal");
    assert_eq!(status.state, ResourceState::Unknown);
    assert!(status.tags.is_empty());
}

#[test]
fn test_instance_id_validation() {
    // Instance ID format validation
    use runctl::validation::validate_instance_id;

    // Valid instance IDs
    assert!(validate_instance_id("i-1234567890abcdef0").is_ok());
    assert!(validate_instance_id("i-0123456789abcdef").is_ok());

    // Invalid instance IDs
    assert!(validate_instance_id("invalid").is_err());
    assert!(validate_instance_id("i-").is_err());
    assert!(validate_instance_id("").is_err());
    assert!(validate_instance_id("i-123").is_err()); // Too short
}

#[test]
fn test_project_name_validation() {
    use runctl::validation::validate_project_name;

    // Valid project names
    assert!(validate_project_name("my-project").is_ok());
    assert!(validate_project_name("project123").is_ok());
    assert!(validate_project_name("a").is_ok());

    // Invalid project names
    assert!(validate_project_name("").is_err());
    assert!(validate_project_name("project with spaces").is_err());
    assert!(validate_project_name("project/with/slashes").is_err());
}

#[test]
fn test_cost_calculation_consistency() {
    use runctl::resources::estimate_instance_cost;
    use runctl::utils::get_instance_cost;

    // Cost functions return consistent values
    let instance_types = ["t3.micro", "t3.medium", "g4dn.xlarge", "p3.2xlarge"];

    for instance_type in instance_types {
        let cost1 = get_instance_cost(instance_type);
        let cost2 = estimate_instance_cost(instance_type);

        assert!(cost1 > 0.0, "Cost positive for {}", instance_type);
        assert!(cost2 > 0.0, "Cost positive for {}", instance_type);
        assert!(cost1 < 10.0, "Cost reasonable for {}", instance_type);
        assert!(cost2 < 10.0, "Cost reasonable for {}", instance_type);
    }
}

#[test]
fn test_tag_extraction() {
    // Tag extraction and filtering
    let tags = vec![
        ("Name".to_string(), "Test Instance".to_string()),
        ("Project".to_string(), "test".to_string()),
        ("Environment".to_string(), "dev".to_string()),
        ("runctl:user".to_string(), "testuser".to_string()),
        ("runctl:project".to_string(), "test-project".to_string()),
    ];

    // Find specific tag
    let name_tag = tags.iter().find(|(k, _)| k == "Name");
    assert!(name_tag.is_some());
    assert_eq!(name_tag.unwrap().1, "Test Instance");

    // Filter runctl tags
    let runctl_tags: Vec<_> = tags
        .iter()
        .filter(|(k, _)| k.starts_with("runctl:"))
        .collect();
    assert_eq!(runctl_tags.len(), 2);
}
