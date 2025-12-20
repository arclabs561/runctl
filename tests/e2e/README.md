# End-to-End Tests

## Overview

E2E tests verify runctl functionality with real AWS resources and actual system interactions.

## Running E2E Tests

### Prerequisites

1. **AWS Credentials**: Configure AWS credentials (via `~/.aws/credentials` or environment variables)
2. **Permissions**: Ensure your AWS user has permissions for:
   - EC2: `DescribeInstances`, `TerminateInstances` (for cleanup tests)
   - S3: `ListBucket`, `GetObject`, `PutObject` (for S3 tests)
   - SSM: `SendCommand`, `GetCommandInvocation` (for training tests)

### Running Tests

```bash
# Run all E2E tests (requires AWS credentials)
TRAIN_OPS_E2E=1 cargo test --test aws_resources_test --features e2e

# Run specific test
TRAIN_OPS_E2E=1 cargo test --test aws_resources_test test_list_aws_instances --features e2e

# Run without AWS (skips E2E tests)
cargo test
```

## Test Safety

All E2E tests:
- Use **dry-run mode** when possible
- **Clean up** resources they create
- Require **explicit opt-in** via `TRAIN_OPS_E2E=1`
- Are marked with `#[ignore]` by default

## Test Structure

- `aws_resources_test.rs` - AWS resource management tests
- `checkpoint_test.rs` - Checkpoint operations tests
- `local_training_test.rs` - Local training execution tests
- `resource_tracking_test.rs` - Resource tracking and cost awareness
- `safe_cleanup_test.rs` - Safe cleanup operations
- `resource_cleanup_test.rs` - Orphaned resource detection

## Best Practices

1. **Always use dry-run first** in tests
2. **Tag resources** created by tests for easy identification
3. **Clean up** in test teardown
4. **Use temporary resources** when possible
5. **Verify cleanup** in assertions

## CI/CD Integration

In CI, E2E tests should:
- Only run on main/master branch
- Use dedicated test AWS account
- Have resource quotas/limits
- Timeout after reasonable duration
- Report failures clearly

