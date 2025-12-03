# Testing Guide

## Test Structure

```
tests/
├── integration_test.rs    # Integration tests (no external deps)
├── e2e/                   # End-to-end tests
│   ├── aws_resources_test.rs
│   ├── checkpoint_test.rs
│   ├── local_training_test.rs
│   └── README.md
└── README.md              # This file
```

## Running Tests

### Unit Tests (in src/)
```bash
cargo test --lib
```

### Integration Tests
```bash
cargo test --test integration_test
```

### E2E Tests (requires AWS)
```bash
TRAIN_OPS_E2E=1 cargo test --test aws_resources_test --features e2e
```

### All Tests
```bash
cargo test
```

## Test Coverage Goals

- [ ] Unit tests for all modules
- [ ] Integration tests for workflows
- [ ] E2E tests for AWS operations
- [ ] E2E tests for RunPod operations
- [ ] E2E tests for checkpoint management
- [ ] E2E tests for S3 operations

## Writing Tests

### Unit Tests
Place in module files with `#[cfg(test)]`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function() {
        // Test code
    }
}
```

### Integration Tests
Place in `tests/` directory:
```rust
#[test]
fn test_feature() {
    // Test code
}
```

### E2E Tests
Place in `tests/e2e/`:
```rust
#[tokio::test]
#[ignore] // Requires external resources
async fn test_aws_operation() {
    if !should_run_e2e() {
        return;
    }
    // Test with real AWS
}
```

## Test Utilities

- `tempfile` - Temporary directories/files
- `tokio-test` - Async test utilities
- `mockito` - HTTP mocking (for API tests)

## Continuous Integration

Tests run in CI with:
- Unit tests: Always
- Integration tests: Always
- E2E tests: Only with `TRAIN_OPS_E2E=1` and AWS credentials

