//! Tests for Docker error handling scenarios
//!
//! Tests various Docker-related error cases:
//! - Docker build failures
//! - ECR authentication failures
//! - Container execution failures
//! - Missing Dockerfile handling

use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_detect_dockerfile_missing() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // No Dockerfile should return None
    let result = runctl::docker::detect_dockerfile(project_root);
    assert!(
        result.is_none(),
        "Should return None when no Dockerfile exists"
    );
}

#[test]
fn test_detect_dockerfile_standard() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create standard Dockerfile
    let dockerfile = project_root.join("Dockerfile");
    std::fs::write(&dockerfile, "FROM python:3.10\n").unwrap();

    let result = runctl::docker::detect_dockerfile(project_root);
    assert!(result.is_some(), "Should detect standard Dockerfile");
    assert_eq!(result.unwrap(), dockerfile);
}

#[test]
fn test_detect_dockerfile_train_variant() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create Dockerfile.train
    let dockerfile = project_root.join("Dockerfile.train");
    std::fs::write(&dockerfile, "FROM python:3.10\n").unwrap();

    let result = runctl::docker::detect_dockerfile(project_root);
    assert!(result.is_some(), "Should detect Dockerfile.train");
    assert_eq!(result.unwrap(), dockerfile);
}

#[test]
fn test_detect_dockerfile_in_docker_dir() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create docker/Dockerfile
    let docker_dir = project_root.join("docker");
    std::fs::create_dir_all(&docker_dir).unwrap();
    let dockerfile = docker_dir.join("Dockerfile");
    std::fs::write(&dockerfile, "FROM python:3.10\n").unwrap();

    let result = runctl::docker::detect_dockerfile(project_root);
    assert!(result.is_some(), "Should detect docker/Dockerfile");
    assert_eq!(result.unwrap(), dockerfile);
}

#[test]
fn test_detect_dockerfile_precedence() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create multiple Dockerfiles - standard should take precedence
    let standard = project_root.join("Dockerfile");
    let train = project_root.join("Dockerfile.train");
    let docker_dir = project_root.join("docker");
    std::fs::create_dir_all(&docker_dir).unwrap();
    let docker_subdir = docker_dir.join("Dockerfile");

    std::fs::write(&standard, "FROM python:3.10\n").unwrap();
    std::fs::write(&train, "FROM python:3.9\n").unwrap();
    std::fs::write(&docker_subdir, "FROM python:3.8\n").unwrap();

    let result = runctl::docker::detect_dockerfile(project_root);
    assert!(result.is_some(), "Should detect a Dockerfile");
    // Should prefer standard Dockerfile
    assert_eq!(result.unwrap(), standard);
}
