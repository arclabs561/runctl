# E2E Testing Status

## Current Status

We have **13 E2E test files** covering various aspects of trainctl, but **we haven't actually run a full training workflow end-to-end** yet.

## Available E2E Tests

### Core Workflow Tests
- `tests/training_workflow_e2e_test.rs` - Full training workflow (create → sync → train → verify → cleanup)
- `tests/e2e/full_training_e2e_test.rs` - Comprehensive training test with Python script
- `tests/local_training_e2e_test.rs` - Local training execution

### Resource Management Tests
- `tests/aws_resources_e2e_test.rs` - AWS resource listing and management
- `tests/ebs_lifecycle_e2e_test.rs` - EBS volume lifecycle
- `tests/ebs_persistent_test.rs` - Persistent volume tests
- `tests/persistent_storage_e2e_test.rs` - Storage persistence tests
- `tests/resource_cleanup_e2e_test.rs` - Resource cleanup
- `tests/resource_safety_e2e_test.rs` - Safety checks
- `tests/resource_tracking_e2e_test.rs` - Resource tracking
- `tests/safe_cleanup_e2e_test.rs` - Safe cleanup operations
- `tests/instance_termination_e2e_test.rs` - Instance termination safety

### Other Tests
- `tests/checkpoint_e2e_test.rs` - Checkpoint operations
- `tests/cost_threshold_e2e_test.rs` - Cost threshold checks
- `tests/e2e/secret_scanning_test.rs` - Secret scanning

## Test Scripts

### Manual E2E Test
- `scripts/test_full_training.sh` - Complete manual E2E test script
  - Creates instance
  - Sets up training
  - Runs training
  - Verifies results
  - Cleans up

### Local Test Script
- `test_training_script.py` - Simple Python training script for local testing
  - Creates test dataset
  - Trains minimal model
  - Saves checkpoints
  - Validates training

## Running Tests

### Local Testing (No AWS Required)
```bash
# Test training script locally
python3 test_training_script.py

# Run unit tests
cargo test --lib

# Run integration tests (no AWS)
cargo test --test integration_test
```

### E2E Testing (Requires AWS)
```bash
# Set environment variable to enable E2E tests
export TRAINCTL_E2E=1

# Run specific E2E test
cargo test --test training_workflow_e2e_test --features e2e -- --ignored

# Run all E2E tests
TRAINCTL_E2E=1 cargo test --features e2e -- --ignored

# Run manual E2E test script
./scripts/test_full_training.sh
```

## What's Missing

### ❌ Not Yet Tested End-to-End

1. **Full Training Workflow with trainctl CLI**
   - Create instance via `trainctl aws create`
   - Sync code via `trainctl aws train --sync-code`
   - Start training via `trainctl aws train`
   - Monitor via `trainctl aws monitor`
   - Verify checkpoints created
   - Cleanup via `trainctl aws terminate`

2. **Code Syncing Verification**
   - Verify files actually sync to instance
   - Verify exclusions work (.git, checkpoints, etc.)
   - Verify incremental vs full sync

3. **Dependency Installation**
   - Verify requirements.txt is installed
   - Verify training can import packages
   - Verify auto-installed services (uv, python, etc.)

4. **S3 Data Transfer**
   - Upload test data to S3
   - Transfer to instance via `--data-s3`
   - Verify data accessible on instance
   - Verify training can access data

5. **Checkpoint Management**
   - Verify checkpoints created during training
   - Upload checkpoints to S3
   - Download checkpoints
   - Resume from checkpoint

## Next Steps

1. **Fix remaining test compilation errors** ✅ (in progress)
2. **Run local test script** ✅ (works)
3. **Run E2E test with actual trainctl CLI** (TODO)
4. **Verify code syncing works** (TODO)
5. **Test with real training script** (TODO)
6. **Test checkpoint upload/download** (TODO)

## Test Cost Estimates

- **t3.micro**: ~$0.01/hour (free tier eligible)
- **Full E2E test**: ~$0.01-0.05 per run (5-10 minutes)
- **With GPU (g4dn.xlarge)**: ~$0.50-2.00 per run

## Safety Features

All E2E tests:
- ✅ Require explicit opt-in (`TRAINCTL_E2E=1`)
- ✅ Are marked with `#[ignore]` by default
- ✅ Clean up resources they create
- ✅ Use smallest instance types when possible
- ✅ Check for AWS credentials before running

