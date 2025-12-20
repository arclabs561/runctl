# E2E Test Guide

## Overview

End-to-end tests verify runctl functionality with real AWS resources. They are opt-in and require explicit environment variable to run.

## Running E2E Tests

### Prerequisites

1. **AWS Credentials**: Configure AWS credentials
   ```bash
   aws configure
   # Or set environment variables:
   export AWS_ACCESS_KEY_ID=...
   export AWS_SECRET_ACCESS_KEY=...
   export AWS_DEFAULT_REGION=us-east-1
   ```

2. **Permissions**: Your AWS user needs:
   - EC2: `CreateVolume`, `DeleteVolume`, `AttachVolume`, `DetachVolume`, `CreateSnapshot`, `DeleteSnapshot`, `Describe*`
   - EC2: `RunInstances`, `TerminateInstances`, `DescribeInstances`
   - S3: `ListBucket`, `GetObject`, `PutObject` (for data transfer tests)

### Running Tests

```bash
# Run all E2E tests
TRAINCTL_E2E=1 cargo test --features e2e -- --ignored

# Run specific test suite
TRAINCTL_E2E=1 cargo test --test persistent_storage_e2e_test --features e2e -- --ignored

# Run specific test
TRAINCTL_E2E=1 cargo test --test persistent_storage_e2e_test --features e2e -- --ignored test_persistent_volume_creation_and_tagging

# List all E2E tests (without running)
cargo test --features e2e -- --list --ignored
```

### Without E2E Flag

```bash
# Regular tests (E2E tests are skipped)
cargo test

# E2E tests are marked with #[ignore] and check for TRAINCTL_E2E
# They will print a message and return early if flag is not set
```

## Test Suites

### `persistent_storage_e2e_test.rs`
Tests persistent volume functionality:
- ✅ Persistent volume creation and tagging
- ✅ Protection from deletion
- ✅ Survival across instance termination
- ✅ Cleanup behavior (skips persistent)

**Cost**: ~$0.10-0.50 per run

### `resource_safety_e2e_test.rs`
Tests resource safety and edge cases:
- ✅ AZ validation for volume attachment
- ✅ Snapshot dependencies
- ✅ Attached volume deletion protection

**Cost**: ~$0.20-1.00 per run

### `ebs_lifecycle_e2e_test.rs`
Tests complete EBS workflows:
- ✅ Complete lifecycle (create → snapshot → delete)
- ✅ Persistent vs ephemeral behavior

**Cost**: ~$0.10-0.30 per run

### `aws_resources_e2e_test.rs`
Tests AWS resource management:
- ✅ Instance listing
- ✅ Resource summary
- ✅ Zombie detection
- ✅ Cleanup dry-run

**Cost**: ~$0.00 (read-only operations)

## Test Safety

All E2E tests:
1. **Tag resources** with `runctl:test=<uuid>` for identification
2. **Clean up** resources they create in teardown
3. **Use small resources** (1 GB volumes, minimal instances) to minimize cost
4. **Have timeouts** to prevent hanging
5. **Require explicit opt-in** via `TRAINCTL_E2E=1`

## Cost Management

### Estimated Costs

- **Volume creation**: ~$0.10/GB/month (gp3), but only charged while volume exists
- **Snapshot creation**: ~$0.05/GB/month, but only charged while snapshot exists
- **Instance creation**: Varies by type (tests use minimal instances when possible)
- **Data transfer**: Usually free within same region

### Cost Optimization

1. **Use small volumes**: Tests use 1 GB volumes
2. **Quick cleanup**: Tests delete resources immediately after verification
3. **Read-only tests**: Some tests only read, no cost
4. **Regional**: All resources in same region to avoid transfer costs

### Monitoring Costs

```bash
# Check current AWS costs
aws ce get-cost-and-usage \
    --time-period Start=$(date -u -d '1 day ago' +%Y-%m-%d),End=$(date -u +%Y-%m-%d) \
    --granularity DAILY \
    --metrics BlendedCost

# List test resources (should be empty after tests)
aws ec2 describe-volumes --filters "Name=tag:runctl:test,Values=*"
aws ec2 describe-snapshots --filters "Name=tag:runctl:test,Values=*"
```

## Test Patterns

### Pattern 1: Resource Creation and Cleanup

```rust
#[tokio::test]
#[ignore]
async fn test_example() {
    require_e2e!();
    
    let client = Ec2Client::new(&aws_config);
    let test_tag = test_tag(); // Unique tag for this test
    
    // Create resource
    let resource = client.create_volume()...
        .tag_specifications(/* tag with test_tag */)
        .send().await?;
    
    let resource_id = resource.volume_id()?;
    
    // Test logic
    // ...
    
    // Cleanup
    client.delete_volume()
        .volume_id(&resource_id)
        .send().await?;
}
```

### Pattern 2: Verification Before Cleanup

```rust
// Verify resource exists
let describe = client.describe_volumes()
    .volume_ids(&volume_id)
    .send().await?;

assert!(!describe.volumes().is_empty(), "Resource should exist");

// Cleanup
client.delete_volume().volume_id(&volume_id).send().await?;
```

### Pattern 3: Waiting for State Changes

```rust
let mut attempts = 0;
loop {
    sleep(Duration::from_secs(2)).await;
    attempts += 1;
    
    let describe = client.describe_volumes()
        .volume_ids(&volume_id)
        .send().await?;
    
    let volume = describe.volumes().first()?;
    let state = volume.state()?;
    
    if state == "available" {
        break;
    }
    
    if attempts > 30 {
        panic!("Timeout waiting for state change");
    }
}
```

## Troubleshooting

### Tests Fail with "Access Denied"

Check AWS credentials and permissions:
```bash
aws sts get-caller-identity
aws ec2 describe-volumes
```

### Tests Hang

Tests have timeouts, but if they hang:
1. Check AWS service status
2. Verify network connectivity
3. Check for rate limiting

### Resources Not Cleaned Up

If tests fail mid-run, resources may be left:
```bash
# List test resources
aws ec2 describe-volumes --filters "Name=tag:runctl:test,Values=*"

# Manual cleanup
aws ec2 delete-volume --volume-id vol-xxx
```

### Cost Concerns

If costs are higher than expected:
1. Check for orphaned resources
2. Verify tests are cleaning up
3. Use smaller test resources
4. Run tests less frequently

## CI/CD Integration

In CI, E2E tests should:
1. Only run on main/master branch
2. Use dedicated test AWS account
3. Have resource quotas/limits
4. Timeout after reasonable duration (e.g., 10 minutes)
5. Report failures clearly

Example GitHub Actions:
```yaml
- name: Run E2E tests
  env:
    TRAINCTL_E2E: 1
    AWS_ACCESS_KEY_ID: ${{ secrets.AWS_TEST_ACCESS_KEY_ID }}
    AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_TEST_SECRET_ACCESS_KEY }}
  run: |
    cargo test --features e2e -- --ignored
```

## Best Practices

1. **Always tag test resources** with `runctl:test=<uuid>`
2. **Clean up in teardown** even if test fails
3. **Use small resources** to minimize cost
4. **Add timeouts** to prevent hanging
5. **Verify cleanup** in assertions when possible
6. **Document expected costs** in test comments
7. **Use dry-run mode** when possible for expensive operations

