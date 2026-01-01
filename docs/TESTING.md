# Testing runctl

## Test Structure

Test coverage across multiple levels:

### Unit Tests

Located in module files with `#[cfg(test)]` blocks:
- `src/resource_tracking.rs` - ResourceTracker unit tests
- `src/error.rs` - Error handling tests
- `src/retry.rs` - Retry policy tests
- `src/validation.rs` - Input validation tests

### Integration Tests

Located in `tests/` directory:

| Test File | Purpose |
|-----------|---------|
| `integration_test.rs` | Core integration tests |
| `integration_provider_tests.rs` | Provider trait system tests |
| `integration_concurrent_operations_tests.rs` | Concurrent resource operations |
| `integration_resource_tracking_tests.rs` | Resource tracking and cleanup |
| `resource_tracker_unit_tests.rs` | ResourceTracker detailed tests |
| `resource_tracker_property_tests.rs` | Property-based tests (proptest) |
| `resource_tracker_refresh_tests.rs` | Cost refresh functionality |
| `resource_tracker_state_update_tests.rs` | State update operations |
| `cost_calculation_tests.rs` | Cost calculation logic |
| `error_message_tests.rs` | Error message helpers |

## Quick Test Commands

```bash
# Build
cargo build

# Check compilation
cargo check

# Run all tests
cargo test

# Run only library tests
cargo test --lib

# Run specific test file
cargo test --test integration_test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test --lib test_name
```

## Test Categories

### 1. Unit Tests

Test individual functions and modules in isolation:

```bash
cargo test --lib
```

Coverage:
- ResourceTracker operations
- Cost calculation
- Error handling
- Retry logic
- Input validation

### 2. Integration Tests

Test interactions between modules:

```bash
cargo test --test integration_test
```

**Coverage**:
- Provider trait system
- Concurrent operations
- Resource tracking workflows
- Cleanup safety

### 3. Property-Based Tests

Test invariants with generated inputs:

```bash
cargo test --lib resource_tracker_property
```

**Coverage**:
- Cost calculation properties
- State transition properties
- Resource tracking invariants

### 4. End-to-End Tests

Test full workflows (requires AWS credentials):

```bash
TRAINCTL_E2E=1 cargo test --features e2e
```

**Coverage**:
- Full training workflows
- Resource lifecycle
- Checkpoint management
- Cost tracking

## Test Statistics

- **Total Tests**: 29 passing
- **Unit Tests**: ~15
- **Integration Tests**: ~10
- **Property Tests**: ~4
- **E2E Tests**: Optional (requires credentials)

## Running Tests

### All Tests

```bash
cargo test
```

### Specific Test Suite

```bash
# Integration tests
cargo test --test integration_test

# Resource tracking tests
cargo test --test integration_resource_tracking_tests

# Property tests
cargo test --lib resource_tracker_property
```

### With Verbose Output

```bash
cargo test -- --nocapture --test-threads=1
```

### Specific Test

```bash
cargo test test_name
```

## Test Environment

### Unit Tests

No external dependencies required. Run anywhere:

```bash
cargo test --lib
```

### Integration Tests

May require:
- Mock AWS SDK (if implemented)
- Test fixtures
- Temporary directories

### E2E Tests

Requires:
- AWS credentials (via environment or IAM role)
- `TRAINCTL_E2E=1` environment variable
- `--features e2e` flag

See `docs/AWS_TESTING_SETUP.md` for E2E test setup.

## Test Coverage Areas

### Tested Areas

- ResourceTracker operations
- Cost calculation
- Error handling
- Retry logic
- Input validation
- State transitions

### ⚠️ Needs More Coverage

- AWS operations (requires mocking or E2E)
- S3 operations
- EBS operations
- Dashboard functionality
- Training workflows

## Writing New Tests

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function() {
        // Test implementation
    }
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_integration() {
    // Test implementation
}
```

### Property Test Example

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_property(input in 0..100u64) {
        // Property test implementation
    }
}
```

## Test Best Practices

1. **Isolation**: Each test should be independent
2. **Naming**: Use descriptive test names
3. **Assertions**: Use clear assertion messages
4. **Fixtures**: Use test fixtures for complex setup
5. **Mocking**: Mock external dependencies when possible
6. **Coverage**: Aim for high coverage of critical paths

## Known Limitations

1. **AWS Operations**: Most AWS tests require actual credentials or mocking
2. **E2E Tests**: Require AWS setup (see `docs/AWS_TESTING_SETUP.md`)
3. **RunPod Tests**: Require API key and runpodctl
4. **Checkpoint Tests**: May require PyTorch files for full testing

## Continuous Integration

Tests run automatically on:
- Pull requests
- Pushes to main branch
- Manual workflow triggers

See `.github/workflows/` for CI configuration.

## Next Steps

1. Add AWS SDK mocking for unit tests
2. Expand E2E test coverage
3. Add performance benchmarks
4. Add fuzzing for input validation
5. Increase property test coverage
