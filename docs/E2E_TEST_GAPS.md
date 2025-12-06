# E2E Test Coverage Gaps

## Current E2E Tests

We have **16+ E2E tests** but they're mostly **basic smoke tests**:

### ✅ What's Tested
- Listing AWS instances
- Resource summaries
- Persistent volume creation
- Volume lifecycle (create → snapshot → delete)
- Instance termination safety
- Cost threshold checks
- Basic resource tracking

### ❌ What's Missing (Critical Gaps)

#### 1. **Full Training Workflow**
```rust
// MISSING: Complete end-to-end training
#[test]
async fn test_full_training_workflow() {
    // 1. Create instance
    // 2. Sync code
    // 3. Start training
    // 4. Monitor logs
    // 5. Verify training runs
    // 6. Check checkpoint creation
    // 7. Cleanup
}
```

#### 2. **Code Syncing**
```rust
// MISSING: Verify code actually syncs correctly
#[test]
async fn test_code_sync() {
    // 1. Create test project structure
    // 2. Sync to instance
    // 3. Verify files exist on instance
    // 4. Verify excludes work (no .git, etc.)
}
```

#### 3. **Dependency Installation**
```rust
// MISSING: Verify dependencies install correctly
#[test]
async fn test_dependency_installation() {
    // 1. Create instance with requirements.txt
    // 2. Start training
    // 3. Verify packages installed
    // 4. Verify training can import packages
}
```

#### 4. **S3 Data Transfer**
```rust
// MISSING: Verify S3 data transfer works
#[test]
async fn test_s3_data_transfer() {
    // 1. Upload test data to S3
    // 2. Transfer to instance
    // 3. Verify data on instance
    // 4. Verify training can access data
}
```

#### 5. **Checkpoint Management**
```rust
// MISSING: Verify checkpoint upload/download
#[test]
async fn test_checkpoint_workflow() {
    // 1. Create instance and train
    // 2. Generate checkpoint
    // 3. Upload to S3
    // 4. Download checkpoint
    // 5. Verify checkpoint integrity
}
```

#### 6. **Error Handling**
```rust
// MISSING: Test error scenarios
#[test]
async fn test_training_failure_handling() {
    // 1. Start training with invalid script
    // 2. Verify error is caught
    // 3. Verify instance cleanup
}
```

#### 7. **Multi-Instance Scenarios**
```rust
// MISSING: Test multiple instances
#[test]
async fn test_multi_instance_workflow() {
    // 1. Create multiple instances
    // 2. Train on each
    // 3. Verify isolation
    // 4. Cleanup all
}
```

## Recommended E2E Test Suite

### Priority 1: Critical Workflows
1. ✅ Full training workflow (create → sync → train → monitor → cleanup)
2. ✅ Code syncing verification
3. ✅ Dependency installation
4. ✅ S3 data transfer

### Priority 2: Important Features
5. ✅ Checkpoint upload/download
6. ✅ Error handling and recovery
7. ✅ Resource cleanup verification

### Priority 3: Edge Cases
8. ✅ Multi-instance scenarios
9. ✅ Spot instance interruption handling
10. ✅ EBS volume pre-warming

## Test Structure Recommendation

```rust
tests/e2e/
├── aws_resources_test.rs          # ✅ Existing (basic)
├── checkpoint_test.rs            # ✅ Existing (basic)
├── training_workflow_test.rs      # ❌ NEW - Full workflow
├── code_sync_test.rs              # ❌ NEW - Code syncing
├── dependency_test.rs              # ❌ NEW - Dependencies
├── s3_transfer_test.rs            # ❌ NEW - S3 operations
└── error_handling_test.rs         # ❌ NEW - Error scenarios
```

## Running Comprehensive E2E Tests

```bash
# Run all E2E tests (takes ~10-15 minutes, costs ~$1-2)
TRAINCTL_E2E=1 cargo test --features e2e -- --ignored

# Run specific workflow test
TRAINCTL_E2E=1 cargo test --test training_workflow_test --features e2e -- --ignored

# Run with cleanup verification
TRAINCTL_E2E=1 TRAINCTL_VERIFY_CLEANUP=1 cargo test --features e2e -- --ignored
```

## Cost Considerations

- **Current tests**: ~$0.10-1.00 per run
- **Full workflow tests**: ~$1-3 per run (creates real instances)
- **Recommendation**: Run full suite in CI only, use basic tests for development

