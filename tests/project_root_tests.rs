//! Tests for project root detection utility

use runctl::error::Result;
use runctl::utils::find_project_root;
use runctl::utils::get_script_relative_path;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_find_project_root_with_requirements_txt() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let subdir = project_root.join("training");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create requirements.txt in root
    std::fs::write(project_root.join("requirements.txt"), "torch\n").unwrap();

    // Create script in subdirectory
    let script_path = subdir.join("train.py");
    std::fs::write(&script_path, "print('hello')\n").unwrap();

    // Should find root from script directory
    let found_root = find_project_root(&subdir);
    assert_eq!(found_root, project_root);
}

#[test]
fn test_find_project_root_with_git() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let subdir = project_root.join("src").join("training");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create .git in root
    std::fs::create_dir(project_root.join(".git")).unwrap();

    // Should find root from deep subdirectory
    let found_root = find_project_root(&subdir);
    assert_eq!(found_root, project_root);
}

#[test]
fn test_find_project_root_no_markers() {
    let temp_dir = TempDir::new().unwrap();
    let subdir = temp_dir.path().join("some").join("deep").join("path");
    std::fs::create_dir_all(&subdir).unwrap();

    // No markers, should return starting path
    let found_root = find_project_root(&subdir);
    assert_eq!(found_root, subdir);
}

#[test]
fn test_get_script_relative_path_valid() {
    let project_root = PathBuf::from("/project");
    let script_path = PathBuf::from("/project/training/train.py");

    let relative = get_script_relative_path(&script_path, &project_root).unwrap();
    assert_eq!(relative, PathBuf::from("training/train.py"));
}

#[test]
fn test_get_script_relative_path_invalid() {
    let project_root = PathBuf::from("/project");
    let script_path = PathBuf::from("/other/train.py");

    let result = get_script_relative_path(&script_path, &project_root);
    assert!(result.is_err());
}

#[test]
fn test_find_project_root_prefers_git() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let subdir = project_root.join("training");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create both .git and requirements.txt
    std::fs::create_dir(project_root.join(".git")).unwrap();
    std::fs::write(project_root.join("requirements.txt"), "torch\n").unwrap();

    // Should find root (both markers present)
    let found_root = find_project_root(&subdir);
    assert_eq!(found_root, project_root);
}

#[test]
fn test_find_project_root_nested_markers() {
    let temp_dir = TempDir::new().unwrap();
    let outer = temp_dir.path();
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();

    // Create markers in both
    std::fs::write(outer.join("requirements.txt"), "torch\n").unwrap();
    std::fs::write(inner.join("requirements.txt"), "numpy\n").unwrap();

    let script_in_inner = inner.join("train.py");
    std::fs::write(&script_in_inner, "print('hello')\n").unwrap();

    // Should find inner (closest marker) when no .git exists
    let found_root = find_project_root(script_in_inner.parent().unwrap());
    assert_eq!(found_root, inner);
}

#[test]
fn test_find_project_root_prioritizes_git() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let subdir = repo_root.join("src").join("ml");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create .git at repo root (most authoritative)
    std::fs::create_dir(repo_root.join(".git")).unwrap();

    // Create requirements.txt in subdirectory (should be ignored)
    std::fs::write(subdir.join("requirements.txt"), "numpy\n").unwrap();

    let script_in_subdir = subdir.join("train.py");
    std::fs::write(&script_in_subdir, "print('hello')\n").unwrap();

    // Should find repo root (with .git), NOT subdirectory (with requirements.txt)
    let found_root = find_project_root(script_in_subdir.parent().unwrap());
    assert_eq!(
        found_root, repo_root,
        "Should prioritize .git over nested requirements.txt"
    );
}

#[test]
fn test_find_project_root_src_ml_scenario() {
    // This test simulates the reported issue: src/ml/requirements.txt
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let src_ml = repo_root.join("src").join("ml");
    std::fs::create_dir_all(&src_ml).unwrap();

    // Create .git at repo root
    std::fs::create_dir(repo_root.join(".git")).unwrap();

    // Create requirements.txt in src/ml (the problematic marker)
    std::fs::write(src_ml.join("requirements.txt"), "torch\n").unwrap();

    let script = src_ml.join("train.py");
    std::fs::write(&script, "print('hello')\n").unwrap();

    // Should find repo root, NOT src/ml
    let found_root = find_project_root(script.parent().unwrap());
    assert_eq!(
        found_root, repo_root,
        "Should find repo root with .git, not src/ml with requirements.txt"
    );
}
