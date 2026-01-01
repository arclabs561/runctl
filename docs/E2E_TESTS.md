# End-to-End (E2E) Tests

## Overview

E2E tests are located in `tests/e2e/` and test interactions with AWS resources and system components.

## Test Structure

```
tests/e2e/
├── README.md                    # E2E test documentation
├── aws_resources_test.rs        # AWS resource management tests
├── checkpoint_test.rs           # Checkpoint operations tests
├── local_training_test.rs       # Local training execution tests
├── resource_tracking_test.rs    # Resource tracking & cost awareness (NEW)
└── safe_cleanup_test.rs         # Safe cleanup operations (NEW)
```

## Running E2E Tests

### Prerequisites

1. AWS Credentials: Configure AWS credentials
   ```bash
   aws configure
   # Or set environment variables:
   export AWS_ACCESS_KEY_ID=...
   export AWS_SECRET_ACCESS_KEY=...
   ```

2. Permissions: Your AWS user needs:
   - EC2: `DescribeInstances`, `CreateVolume`, `AttachVolume`, `TerminateInstances`
   - S3: `ListBucket`, `GetObject`, `PutObject`
   - SSM: `SendCommand`, `GetCommandInvocation`

### Running Tests

```bash
# Run all E2E tests (requires explicit opt-in)
TRAINCTL_E2E=1 cargo test --test aws_resources_test --features e2e

# Run specific test
TRAINCTL_E2E=1 cargo test --test resource_tracking_test test_resource_tracking --features e2e

# Run without AWS (skips E2E tests)
cargo test

# Run all tests including E2E (if enabled)
TRAINCTL_E2E=1 cargo test --features e2e
```

## Test Safety Features

All E2E tests:
- Require explicit opt-in via `TRAINCTL_E2E=1` environment variable
- Use dry-run mode when possible
- Clean up resources they create
- Are marked with `#[ignore]` by default
- Check for AWS credentials before running

## New E2E Tests Added

### Resource Tracking Tests (`resource_tracking_test.rs`)
- `test_resource_tracking` - Basic resource registration and tracking
- `test_cost_tracking` - Cost accumulation across multiple resources
- `test_resource_filtering_by_tag` - Tag-based resource filtering

### Safe Cleanup Tests (`safe_cleanup_test.rs`)
- `test_protected_resources` - Protection mechanism for important resources
- `test_dry_run_cleanup` - Dry-run mode for safe testing
- `test_cleanup_with_protection` - Cleanup with protected resources

## Test Configuration

Tests check for `TRAINCTL_E2E` or `CI` environment variables:

```rust
fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}
```

## CI/CD Integration

For CI/CD pipelines:

```yaml
# Example GitHub Actions
- name: Run E2E tests
  env:
    TRAINCTL_E2E: 1
    AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
    AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
  run: cargo test --features e2e
```

## Best Practices

1. Always use dry-run first in tests
2. Tag resources created by tests (`runctl:test=true`)
3. Clean up in test teardown (even on failure)
4. Use temporary resources when possible
5. Verify cleanup in assertions
6. Time out tests after reasonable duration

## Test Coverage

### Current Coverage
- ✅ AWS resource listing
- ✅ Resource summary generation
- ✅ Zombie detection
- ✅ Checkpoint operations
- ✅ Local training execution
- ✅ Resource tracking (NEW)
- ✅ Safe cleanup (NEW)

### Planned Coverage
- ⏳ EBS volume operations
- ⏳ Data transfer operations
- ⏳ Fast data loading
- ⏳ Retry logic
- ⏳ Graceful shutdown

## Troubleshooting

### Tests skip automatically
- Solution: Set `TRAINCTL_E2E=1` environment variable

### AWS permission errors
- Solution: Check IAM permissions for your AWS user

### Tests create resources but don't clean up
- Solution: Check test teardown logic, ensure cleanup runs even on failure

### Tests timeout
- Solution: Increase timeout or check network connectivity

