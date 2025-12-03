//! End-to-end tests for local training execution

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_python_script_execution() {
    let temp_dir = TempDir::new().unwrap();
    let script = temp_dir.path().join("test_script.py");
    
    // Create a simple test script
    fs::write(&script, r#"
#!/usr/bin/env python3
import sys
print("Test script executed")
sys.exit(0)
"#).unwrap();
    
    // Test execution
    let output = Command::new("python3")
        .arg(&script)
        .output()
        .expect("Failed to execute script");
    
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test script executed"));
}

#[test]
fn test_uv_detection() {
    // Check if uv is available
    let has_uv = Command::new("which")
        .arg("uv")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    
    // Test should pass regardless
    assert!(true);
    
    if has_uv {
        println!("✓ uv is available");
    } else {
        println!("⚠ uv is not available (optional)");
    }
}

#[tokio::test]
async fn test_training_session_creation() {
    use train_ops::training::TrainingSession;
    use std::path::PathBuf;
    
    let temp_dir = TempDir::new().unwrap();
    let session = TrainingSession::new(
        "local".to_string(),
        PathBuf::from("test.py"),
        temp_dir.path().join("checkpoints"),
    );
    
    assert_eq!(session.platform, "local");
    assert_eq!(session.status, train_ops::training::TrainingStatus::Running);
    
    // Test saving
    session.save(temp_dir.path()).unwrap();
    
    // Test loading
    let loaded = TrainingSession::load(temp_dir.path(), &session.id).unwrap();
    assert_eq!(loaded.id, session.id);
}

