# Testing Strategy for trainctl

## Overview

trainctl uses a comprehensive testing strategy combining unit tests, integration tests, property-based tests, and end-to-end tests to ensure reliability and correctness.

## Test Pyramid

```
        /\
       /  \  E2E Tests (16 tests)
      /----\
     /      \  Integration Tests (10+ tests)
    /--------\
   /          \  Property-Based Tests (30+ tests)
  /------------\
 /              \  Unit Tests (20+ tests)
/----------------\
```

## Test Types

### 1. Unit Tests

**Location**: `src/*.rs` (in `#[cfg(test)]` modules)

**Purpose**: Test individual functions in isolation

**Examples**:
- `format_duration` - Duration formatting
- `calculate_accumulated_cost` - Cost calculations
- `is_old_instance` - Instance age detection
- Config loading and saving

**Run**: `cargo test --lib`

### 2. Integration Tests

**Location**: `tests/integration_test.rs`, `tests/unit_tests.rs`, `tests/module_unit_tests.rs`

**Purpose**: Test module interactions and workflows

**Examples**:
- Config roundtrip serialization
- Checkpoint operations
- S3 path parsing
- Cost estimation consistency

**Run**: `cargo test --test integration_test`

### 3. Property-Based Tests

**Location**: `tests/property_tests.rs`, `tests/integration_property_tests.rs`, etc.

**Purpose**: Generate random inputs and verify properties hold

**Framework**: `proptest`

**Examples**:
- Duration formatting never negative
- Cost calculation monotonicity
- S3 path parsing consistency
- Config validation properties

**Run**: `cargo test --test property_tests`

### 4. Stateful Property Tests

**Location**: `tests/stateful_property_tests.rs`

**Purpose**: Test state machines and complex sequences

**Examples**:
- Resource lifecycle state transitions
- Volume lifecycle state machine
- Cost tracker invariants

**Run**: `cargo test --test stateful_property_tests`

### 5. E2E Tests

**Location**: `tests/*_e2e_test.rs`

**Purpose**: Test with real AWS resources

**Examples**:
- Persistent volume creation and protection
- Resource safety edge cases
- Instance termination with volumes
- Cost threshold warnings

**Run**: `TRAINCTL_E2E=1 cargo test --features e2e -- --ignored`

## Property-Based Testing Patterns

### Basic Property Tests

```rust
proptest! {
    #[test]
    fn test_property(input in strategy()) {
        let result = function(input);
        prop_assert!(property_holds(result));
    }
}
```

### Stateful Property Tests

```rust
proptest! {
    #[test]
    fn test_state_machine(
        operations in prop::collection::vec(action_strategy(), 1..50)
    ) {
        let mut state = initial_state();
        for op in operations {
            state.apply(op);
            prop_assert!(state.invariants_hold());
        }
    }
}
```

## Test Coverage by Module

### Core Modules
- ✅ `utils.rs` - 100% property test coverage
- ✅ `config.rs` - Unit + property tests
- ✅ `error.rs` - Error conversion and display tests
- ✅ `retry.rs` - Backoff calculation properties
- ⚠️ `resources.rs` - Cost estimation tests (needs more)
- ⚠️ `data_transfer.rs` - Path parsing tests (needs implementation tests)

### Provider Modules
- ⚠️ `providers/aws_provider.rs` - Stub implementations (needs mocking)
- ⚠️ `providers/runpod_provider.rs` - Stub implementations
- ⚠️ `providers/lyceum_provider.rs` - Stub implementations

### EBS Module
- ✅ Volume size validation
- ✅ AZ format validation
- ✅ Tag validation
- ✅ Persistent volume protection (E2E)

## Running Tests

### Fast Tests (Unit + Integration)
```bash
cargo test --lib --test integration_test
```

### Property Tests
```bash
cargo test --test property_tests
cargo test --test stateful_property_tests
cargo test --test integration_property_tests
```

### All Tests
```bash
cargo test
```

### E2E Tests (requires AWS)
```bash
TRAINCTL_E2E=1 cargo test --features e2e -- --ignored
```

## Test Best Practices

### Property-Based Testing
1. **Define clear properties** - What should always be true?
2. **Use appropriate strategies** - Generate valid inputs
3. **Test invariants** - Properties that hold in all states
4. **Shrink failures** - Let proptest find minimal cases

### Stateful Testing
1. **Model state explicitly** - Track all relevant state
2. **Define valid transitions** - Only allow valid operations
3. **Check invariants** - Verify after each operation
4. **Test sequences** - Not just single operations

### E2E Testing
1. **Tag resources** - Use `trainctl:test=<uuid>` tags
2. **Clean up** - Always clean up in teardown
3. **Use small resources** - Minimize cost
4. **Opt-in** - Require explicit `TRAINCTL_E2E=1`

## Coverage Goals

- **Unit Tests**: 100% of public functions
- **Property Tests**: All pure functions
- **Stateful Tests**: All state machines
- **E2E Tests**: All critical workflows

## Future Improvements

1. **Mock AWS SDK** - Use `mockall` for provider testing
2. **Async Property Tests** - Test retry logic with proptest
3. **Snapshot Tests** - CLI output validation
4. **Performance Tests** - Benchmark critical paths
5. **Fuzz Tests** - Input validation fuzzing

