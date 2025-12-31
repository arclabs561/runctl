//! Tests for training error handling scenarios
//!
//! Tests various error cases in the training workflow:
//! - Invalid script paths
//! - Scripts outside project root
//! - Missing dependencies
//! - SSM/SSH fallback scenarios

use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_get_script_relative_path_valid() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let script = project_root.join("training").join("train.py");

    std::fs::create_dir_all(script.parent().unwrap()).unwrap();
    std::fs::write(&script, "print('hello')\n").unwrap();

    let relative = runctl::utils::get_script_relative_path(&script, project_root).unwrap();
    assert_eq!(relative, PathBuf::from("training/train.py"));
}

#[test]
fn test_get_script_relative_path_invalid() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let script = PathBuf::from("/completely/different/path/train.py");

    let result = runctl::utils::get_script_relative_path(&script, project_root);
    assert!(
        result.is_err(),
        "Should fail when script is outside project root"
    );

    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("not under project root"),
        "Error should mention project root"
    );
}

#[test]
fn test_find_project_root_with_script_at_root() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create marker at root
    std::fs::write(project_root.join("requirements.txt"), "torch\n").unwrap();

    // Script at root level
    let script = project_root.join("train.py");
    std::fs::write(&script, "print('hello')\n").unwrap();

    let found_root = runctl::utils::find_project_root(script.parent().unwrap());
    assert_eq!(found_root, project_root);

    // Should be able to get relative path (empty or just filename)
    let relative = runctl::utils::get_script_relative_path(&script, &found_root).unwrap();
    assert_eq!(relative, PathBuf::from("train.py"));
}

#[test]
fn test_find_project_root_deep_nesting() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create marker at root
    std::fs::write(project_root.join("Cargo.toml"), "[package]\n").unwrap();

    // Deep nesting
    let deep_path = project_root
        .join("src")
        .join("training")
        .join("models")
        .join("experiments");
    std::fs::create_dir_all(&deep_path).unwrap();

    let script = deep_path.join("train.py");
    std::fs::write(&script, "print('hello')\n").unwrap();

    let found_root = runctl::utils::find_project_root(script.parent().unwrap());
    assert_eq!(found_root, project_root);

    let relative = runctl::utils::get_script_relative_path(&script, &found_root).unwrap();
    assert!(relative.to_string_lossy().contains("train.py"));
}

#[test]
fn test_find_project_root_no_markers_uses_start_path() {
    let temp_dir = TempDir::new().unwrap();
    let subdir = temp_dir.path().join("some").join("deep").join("path");
    std::fs::create_dir_all(&subdir).unwrap();

    // No markers anywhere
    let found_root = runctl::utils::find_project_root(&subdir);
    assert_eq!(
        found_root, subdir,
        "Should return starting path when no markers found"
    );
}

#[test]
fn test_project_root_detection_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create marker
    std::fs::write(project_root.join("requirements.txt"), "torch\n").unwrap();

    // Multiple scripts at different levels
    let script1 = project_root.join("train1.py");
    let script2 = project_root.join("training").join("train2.py");
    let script3 = project_root.join("src").join("train3.py");

    std::fs::create_dir_all(script2.parent().unwrap()).unwrap();
    std::fs::create_dir_all(script3.parent().unwrap()).unwrap();

    std::fs::write(&script1, "print('1')\n").unwrap();
    std::fs::write(&script2, "print('2')\n").unwrap();
    std::fs::write(&script3, "print('3')\n").unwrap();

    // All should find the same root
    let root1 = runctl::utils::find_project_root(script1.parent().unwrap());
    let root2 = runctl::utils::find_project_root(script2.parent().unwrap());
    let root3 = runctl::utils::find_project_root(script3.parent().unwrap());

    assert_eq!(root1, project_root);
    assert_eq!(root2, project_root);
    assert_eq!(root3, project_root);

    // All should be able to get relative paths
    assert!(runctl::utils::get_script_relative_path(&script1, &root1).is_ok());
    assert!(runctl::utils::get_script_relative_path(&script2, &root2).is_ok());
    assert!(runctl::utils::get_script_relative_path(&script3, &root3).is_ok());
}
