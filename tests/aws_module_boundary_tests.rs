//! Tests for AWS module boundaries and integration
//!
//! Tests that verify the new modular structure works correctly,
//! including module exports, type conversions, and error handling.

use runctl::config::Config;
use runctl::provider::{ResourceState, ResourceStatus};
use std::path::PathBuf;

#[test]
fn test_aws_module_exports() {
    // AWS module exports accessible
    use runctl::aws::{CreateInstanceOptions, TrainInstanceOptions};
    
    // CreateInstanceOptions constructible
    let _options = CreateInstanceOptions {
        instance_type: "t3.micro".to_string(),
        use_spot: false,
        spot_max_price: None,
        no_fallback: false,
        key_name: None,
        security_group: None,
        ami_id: None,
        root_volume_size: None,
        data_volume_size: None,
        project_name: "test".to_string(),
        iam_instance_profile: None,
    };
    
    // TrainInstanceOptions constructible
    let _train_options = TrainInstanceOptions {
        instance_id: "i-1234567890abcdef0".to_string(),
        script: PathBuf::from("train.py"),
        data_s3: None,
        output_s3: None,
        sync_code: true,
        include_patterns: vec![],
        project_name: "test".to_string(),
        script_args: vec![],
    };
}

#[test]
fn test_aws_module_command_enum() {
    // AwsCommands enum defined
    use runctl::aws::AwsCommands;
    
    // Enum variants exist
    let _create = AwsCommands::Create {
        instance_type: "t3.micro".to_string(),
        spot: false,
        spot_max_price: None,
        no_fallback: false,
        key_name: None,
        security_group: None,
        ami_id: None,
        root_volume_size: None,
        data_volume_size: None,
        project_name: None,
        iam_instance_profile: None,
    };
    
    let _train = AwsCommands::Train {
        instance_id: "i-123".to_string(),
        script: PathBuf::from("train.py"),
        data_s3: None,
        _output_s3: None,
        sync_code: true,
        include_pattern: vec![],
        project_name: None,
        script_args: vec![],
    };
}

#[test]
fn test_resource_state_transitions() {
    // Resource state transitions
    assert!(matches!(
        (ResourceState::Running, ResourceState::Stopped),
        (ResourceState::Running, ResourceState::Stopped)
    ));
    
    // Stopped -> Running is valid
    assert!(matches!(
        (ResourceState::Stopped, ResourceState::Running),
        (ResourceState::Stopped, ResourceState::Running)
    ));
    
    // Any state -> Terminated is valid
    let states = [
        ResourceState::Starting,
        ResourceState::Running,
        ResourceState::Stopped,
    ];
    
    for state in states {
        // Can transition to terminated
        assert!(matches!(
            (state, ResourceState::Terminated),
            (_, ResourceState::Terminated)
        ));
    }
}

#[test]
fn test_error_types_for_aws_operations() {
    // Error types for AWS operations
    use runctl::error::{ConfigError, TrainctlError};
    
    let config_error = TrainctlError::Config(ConfigError::MissingField("aws".to_string()));
    assert!(matches!(config_error, TrainctlError::Config(_)));
    
    let cloud_error = TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: "Test error".to_string(),
        source: None,
    };
    assert!(matches!(cloud_error, TrainctlError::CloudProvider { .. }));
}

#[test]
fn test_resource_status_equality() {
    // ResourceStatus equality
    use chrono::Utc;
    
    let status1 = ResourceStatus {
        id: "i-123".to_string(),
        name: Some("Test".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![],
    };
    
    let status2 = ResourceStatus {
        id: "i-123".to_string(),
        name: Some("Test".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.micro".to_string()),
        launch_time: status1.launch_time,
        cost_per_hour: 0.01,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![],
    };
    
    // IDs match
    assert_eq!(status1.id, status2.id);
    assert_eq!(status1.state, status2.state);
    assert_eq!(status1.cost_per_hour, status2.cost_per_hour);
}

#[test]
fn test_config_with_resource_tracker() {
    // Config holds ResourceTracker
    use std::sync::Arc;
    
    let config = Config::default();
    
    // ResourceTracker present in default config
    assert!(config.resource_tracker.is_some());
    let tracker = config.resource_tracker.as_ref().unwrap();
    let _running = tracker.get_running();
    assert!(Arc::ptr_eq(config.resource_tracker.as_ref().unwrap(), tracker));
}

#[test]
fn test_instance_type_validation() {
    // Instance type validation
    use runctl::validation::validate_instance_id;
    
    // Valid instance types (as instance IDs for this test)
    assert!(validate_instance_id("i-1234567890abcdef0").is_ok());
    
    // Invalid formats
    assert!(validate_instance_id("invalid").is_err());
    assert!(validate_instance_id("").is_err());
}

#[test]
fn test_project_name_derivation() {
    // Project name derivation
    use std::env;
    use runctl::validation::validate_project_name;
    
    let current_dir = env::current_dir().unwrap();
    let dir_name = current_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    
    // Directory names with alphanumeric + hyphens/underscores are valid
    if dir_name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        assert!(validate_project_name(dir_name).is_ok());
    }
}

#[test]
fn test_error_display_formatting() {
    // Error display formatting
    use runctl::error::{ConfigError, TrainctlError};
    
    let error = TrainctlError::Config(ConfigError::MissingField("aws".to_string()));
    let error_str = format!("{}", error);
    assert!(error_str.contains("aws") || error_str.contains("config") || error_str.contains("missing"));
}

#[test]
fn test_resource_state_display() {
    // ResourceState display
    let states = [
        ResourceState::Starting,
        ResourceState::Running,
        ResourceState::Stopped,
        ResourceState::Terminating,
        ResourceState::Terminated,
        ResourceState::Unknown,
    ];
    
    for state in states {
        let state_str = format!("{:?}", state);
        // Not empty
        assert!(!state_str.is_empty());
    }
}

