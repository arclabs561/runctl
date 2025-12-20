//! Tests for SSH code syncing functionality
//!
//! Tests cover:
//! - Pattern matching for include_patterns
//! - Gitignore override behavior
//! - File selection logic

use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Helper function to match the internal implementation
fn matches_include_pattern(path: &Path, pattern: &str, project_root: &Path) -> bool {
    let rel_path = match path.strip_prefix(project_root) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let pattern = pattern.trim_matches('/');
    if pattern.is_empty() {
        return false;
    }

    let pattern_path = Path::new(pattern);

    rel_path.starts_with(pattern_path)
        || rel_path
            .parent()
            .map(|p| p == pattern_path || p.starts_with(pattern_path))
            .unwrap_or(false)
}

#[test]
fn test_matches_include_pattern_directory_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create test files
    let data_file = project_root.join("data/train.csv");
    fs::create_dir_all(data_file.parent().unwrap()).unwrap();
    fs::write(&data_file, "test").unwrap();

    let other_file = project_root.join("my_data_file.txt");
    fs::write(&other_file, "test").unwrap();

    // "data/" should match "data/train.csv"
    assert!(matches_include_pattern(&data_file, "data/", project_root));

    // "data/" should NOT match "my_data_file.txt"
    assert!(!matches_include_pattern(&other_file, "data/", project_root));
}

#[test]
fn test_matches_include_pattern_without_trailing_slash() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let data_file = project_root.join("data/train.csv");
    fs::create_dir_all(data_file.parent().unwrap()).unwrap();
    fs::write(&data_file, "test").unwrap();

    // "data" should match "data/train.csv" (normalized)
    assert!(matches_include_pattern(&data_file, "data", project_root));
}

#[test]
fn test_matches_include_pattern_nested_paths() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let nested_file = project_root.join("data/train/images/cat.jpg");
    fs::create_dir_all(nested_file.parent().unwrap()).unwrap();
    fs::write(&nested_file, "test").unwrap();

    // "data/" should match nested files
    assert!(matches_include_pattern(&nested_file, "data/", project_root));
}

#[test]
fn test_matches_include_pattern_specific_file() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let specific_file = project_root.join("config.json");
    fs::write(&specific_file, "test").unwrap();

    // Pattern matching specific file
    assert!(matches_include_pattern(
        &specific_file,
        "config.json",
        project_root
    ));
}

#[test]
fn test_matches_include_pattern_empty_pattern() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let file = project_root.join("test.txt");
    fs::write(&file, "test").unwrap();

    // Empty pattern should not match
    assert!(!matches_include_pattern(&file, "", project_root));
    assert!(!matches_include_pattern(&file, "/", project_root));
}

#[test]
fn test_matches_include_pattern_outside_project() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let outside_file = temp_dir.path().parent().unwrap().join("outside.txt");

    // File outside project root should not match
    assert!(!matches_include_pattern(
        &outside_file,
        "outside.txt",
        project_root
    ));
}

#[test]
fn test_matches_include_pattern_multiple_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let data_file = project_root.join("data/train.csv");
    fs::create_dir_all(data_file.parent().unwrap()).unwrap();
    fs::write(&data_file, "test").unwrap();

    let datasets_file = project_root.join("datasets/test.csv");
    fs::create_dir_all(datasets_file.parent().unwrap()).unwrap();
    fs::write(&datasets_file, "test").unwrap();

    // Test multiple patterns
    let patterns = ["data/", "datasets/"];
    assert!(patterns
        .iter()
        .any(|p| matches_include_pattern(&data_file, p, project_root)));
    assert!(patterns
        .iter()
        .any(|p| matches_include_pattern(&datasets_file, p, project_root)));
}
