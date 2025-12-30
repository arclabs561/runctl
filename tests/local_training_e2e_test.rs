//! End-to-end tests for local training execution

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_python_script_execution() {
    let temp_dir = TempDir::new().unwrap();
    let script = temp_dir.path().join("test_script.py");

    // Create a simple test script
    fs::write(
        &script,
        r#"
#!/usr/bin/env python3
import sys
print("Test script executed")
sys.exit(0)
"#,
    )
    .unwrap();

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
    }

    // Test execution
    let output = Command::new("python3")
        .arg(&script)
        .output()
        .expect("Failed to execute script");

    assert!(
        output.status.success(),
        "Script should execute successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Test script executed"),
        "Should see expected output"
    );
}

#[test]
fn test_uv_detection() {
    // Check if uv is available
    let has_uv = Command::new("which")
        .arg("uv")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Log the result (test passes regardless since uv is optional)
    if has_uv {
        println!("uv is available");
    } else {
        println!("WARNING: uv is not available (optional)");
    }
}

#[test]
fn test_python_available() {
    // Verify Python is available for training
    let has_python3 = Command::new("which")
        .arg("python3")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    assert!(
        has_python3,
        "python3 should be available for local training"
    );

    // Test Python version
    let output = Command::new("python3")
        .arg("--version")
        .output()
        .expect("Failed to check Python version");

    assert!(output.status.success());
    let version = String::from_utf8_lossy(&output.stdout);
    println!("Python version: {}", version.trim());
}
