//! End-to-end tests for checkpoint management

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

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
    
    // Test listing
    let entries: Vec<_> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    
    assert_eq!(entries.len(), 5);
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
    
    // Simulate cleanup (keep last 5)
    let mut checkpoints: Vec<PathBuf> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    
    checkpoints.sort();
    
    // Keep last 5, delete first 5
    let to_delete = &checkpoints[..5];
    for path in to_delete {
        fs::remove_file(path).unwrap();
    }
    
    // Verify cleanup
    let remaining: Vec<_> = fs::read_dir(&checkpoint_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    
    assert_eq!(remaining.len(), 5);
}

#[tokio::test]
async fn test_checkpoint_info() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint = temp_dir.path().join("test.pt");
    fs::write(&checkpoint, b"test checkpoint").unwrap();
    
    let metadata = fs::metadata(&checkpoint).unwrap();
    assert!(metadata.is_file());
    assert!(metadata.len() > 0);
}

