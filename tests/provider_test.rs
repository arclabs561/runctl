//! Tests for provider trait and implementations

use chrono::Utc;
use runctl::provider::*;
use std::path::PathBuf;

#[test]
fn test_normalize_state() {
    // Test state normalization
    assert_eq!(normalize_state("running"), ResourceState::Running);
    assert_eq!(normalize_state("active"), ResourceState::Running);
    assert_eq!(normalize_state("ready"), ResourceState::Running);

    assert_eq!(normalize_state("pending"), ResourceState::Starting);
    assert_eq!(normalize_state("starting"), ResourceState::Starting);
    assert_eq!(normalize_state("provisioning"), ResourceState::Starting);

    assert_eq!(normalize_state("stopped"), ResourceState::Stopped);
    assert_eq!(normalize_state("stopping"), ResourceState::Stopped);

    assert_eq!(normalize_state("terminating"), ResourceState::Terminating);
    assert_eq!(normalize_state("shutting-down"), ResourceState::Terminating);

    assert_eq!(normalize_state("terminated"), ResourceState::Terminated);

    assert!(matches!(normalize_state("error"), ResourceState::Error(_)));
    assert!(matches!(normalize_state("failed"), ResourceState::Error(_)));

    assert_eq!(normalize_state("unknown-state"), ResourceState::Unknown);
}

#[test]
fn test_resource_status() {
    let status = ResourceStatus {
        id: "test-id".to_string(),
        name: Some("test-instance".to_string()),
        state: ResourceState::Running,
        instance_type: Some("t3.medium".to_string()),
        launch_time: Some(Utc::now()),
        cost_per_hour: 0.0416,
        public_ip: Some("1.2.3.4".to_string()),
        tags: vec![("Name".to_string(), "test".to_string())],
    };

    assert_eq!(status.id, "test-id");
    assert_eq!(status.state, ResourceState::Running);
    assert!(status.cost_per_hour > 0.0);
}

#[test]
fn test_training_job() {
    let job = TrainingJob {
        script: PathBuf::from("train.py"),
        args: vec!["--epochs".to_string(), "10".to_string()],
        data_source: Some("s3://bucket/data".to_string()),
        output_dest: Some("s3://bucket/output".to_string()),
        checkpoint_dir: Some(PathBuf::from("checkpoints")),
        environment: vec![("CUDA_VISIBLE_DEVICES".to_string(), "0".to_string())],
    };

    assert_eq!(job.script, PathBuf::from("train.py"));
    assert_eq!(job.args.len(), 2);
    assert!(job.data_source.is_some());
    assert!(job.output_dest.is_some());
}

#[test]
fn test_create_resource_options() {
    let options = CreateResourceOptions {
        use_spot: true,
        spot_max_price: Some("0.1".to_string()),
        disk_gb: Some(100),
        tags: vec![("Name".to_string(), "test".to_string())],
        ..Default::default()
    };

    assert!(options.use_spot);
    assert_eq!(options.disk_gb, Some(100));
    assert_eq!(options.tags.len(), 1);
}
