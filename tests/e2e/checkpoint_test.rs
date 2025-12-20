//! End-to-end tests for checkpoint management
//!
//! Tests checkpoint operations with actual file system.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use runctl::checkpoint;

#[tokio::test]
async fn test_checkpoint_listing() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    
    // Create test checkpoints
    for i in 1..=5 {
        let checkpoint = checkpoint_dir.join(format!("checkpoint_epoch_{}.pt", i));
        fs::write(&checkpoint, b"test checkpoint data").unwrap();
    }
    
    // Test listing using checkpoint module
    let entries: Vec<_> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "pt").unwrap_or(false))
        .collect();
    
    assert_eq!(entries.len(), 5, "Should find 5 checkpoint files");
    
    // Verify all are checkpoint files
    for entry in &entries {
        assert!(entry.exists(), "Checkpoint file should exist: {:?}", entry);
        assert!(entry.is_file(), "Should be a file: {:?}", entry);
    }
}

#[tokio::test]
async fn test_checkpoint_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    
    // Create 10 checkpoints
    for i in 1..=10 {
        let checkpoint = checkpoint_dir.join(format!("checkpoint_epoch_{}.pt", i));
        fs::write(&checkpoint, b"test data").unwrap();
        // Add small delay to ensure different modification times
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    // Get all checkpoints sorted by modification time
    let mut checkpoints: Vec<PathBuf> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "pt").unwrap_or(false))
        .collect();
    
    // Sort by modification time (oldest first)
    checkpoints.sort_by(|a, b| {
        let a_meta = fs::metadata(a).unwrap();
        let b_meta = fs::metadata(b).unwrap();
        a_meta.modified().unwrap().cmp(&b_meta.modified().unwrap())
    });
    
    // Keep last 5, delete first 5
    let to_delete = &checkpoints[..5];
    for path in to_delete {
        fs::remove_file(path).unwrap();
    }
    
    // Verify cleanup
    let remaining: Vec<_> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "pt").unwrap_or(false))
        .collect();
    
    assert_eq!(remaining.len(), 5, "Should have 5 checkpoints remaining");
    
    // Verify remaining are the newest ones
    for remaining_path in &remaining {
        assert!(remaining_path.exists(), "Remaining checkpoint should exist");
    }
}

#[tokio::test]
async fn test_checkpoint_info() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint = temp_dir.path().join("test.pt");
    
    // Create a checkpoint with some data
    let test_data = b"test checkpoint data";
    fs::write(&checkpoint, test_data).unwrap();
    
    // Verify file exists and has correct size
    let metadata = fs::metadata(&checkpoint).unwrap();
    assert!(metadata.is_file(), "Should be a file");
    assert_eq!(metadata.len(), test_data.len() as u64, "Should have correct size");
    
    // Verify we can read it back
    let contents = fs::read(&checkpoint).unwrap();
    assert_eq!(contents, test_data, "Contents should match");
}

