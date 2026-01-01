# runctl Architecture

## Overview

`runctl` is a Rust-based CLI tool for ML training orchestration across multiple cloud providers (AWS, RunPod, Lyceum AI) with unified checkpoint management, cost tracking, and resource lifecycle management.

## Core Principles

1. Provider-Agnostic Design: `TrainingProvider` trait defined for future multi-cloud support (currently CLI uses direct implementations - see `docs/PROVIDER_TRAIT_DECISION.md`)
2. Error Handling: Use `TrainctlError` from `src/error.rs` for structured errors; `anyhow::Result` for CLI boundaries
3. Retry Logic: Use `ExponentialBackoffPolicy` from `src/retry.rs` for cloud API calls
4. Resource Tracking: Always register resources with `ResourceTracker` for cost awareness
5. Safe Cleanup: Use `CleanupSafety` before deleting resources
6. Pragmatic Evolution: Follow industry patterns (Terraform, Pulumi) - prepare abstractions but don't force migration until needed

## Module Structure

### Core Modules

```
src/
├── lib.rs                 # Library entry point, re-exports
├── main.rs               # CLI entry point
├── config.rs             # Configuration management (TOML)
├── error.rs              # Custom error types (TrainctlError)
├── error_helpers.rs      # Error message helpers
├── retry.rs              # Retry policies (ExponentialBackoffPolicy)
├── validation.rs         # Input validation utilities
└── utils.rs              # General utilities (cost calculation, etc.)
```

### AWS Module (Modular Structure)

```
src/aws/
├── mod.rs          # Command handling, CLI interface (383 lines)
├── types.rs        # Shared type definitions (127 lines)
├── helpers.rs      # Utility functions (230 lines)
├── instance.rs     # Instance lifecycle (1274 lines)
│   ├── create_instance
│   ├── create_spot_instance
│   ├── start_instance
│   ├── stop_instance
│   ├── terminate_instance
│   └── find_deep_learning_ami
├── training.rs     # Training operations (333 lines)
│   ├── train_on_instance
│   ├── sync_code_to_instance
│   └── monitor_instance
└── processes.rs    # Process monitoring (254 lines)
    └── show_processes
```

### Resources Module (Modular Structure)

```
src/resources/
├── mod.rs          # Command enum, dispatcher (284 lines)
├── types.rs        # Data structures (98 lines)
├── json.rs         # JSON serialization (212 lines)
├── aws.rs          # AWS listing (686 lines)
│   ├── list_resources
│   ├── list_aws_instances
│   ├── display_table_format
│   └── sync_resource_tracker_with_aws
├── runpod.rs       # RunPod listing (98 lines)
├── local.rs        # Local process listing (76 lines)
├── summary.rs      # Summary and insights (365 lines)
│   ├── show_summary
│   └── show_insights
├── export.rs       # Export functions (184 lines)
│   ├── export_resources
│   ├── generate_csv
│   └── generate_html
├── watch.rs        # Watch mode (49 lines)
├── cleanup.rs      # Cleanup operations (336 lines)
│   ├── cleanup_zombies
│   └── stop_all_instances
└── utils.rs        # Utility functions (18 lines)
```

### Provider System

```
src/
├── provider.rs          # TrainingProvider trait definition (industry pattern documentation)
└── providers/
    ├── mod.rs          # Provider registry (Terraform-style, implemented)
    ├── aws_provider.rs # AWS implementation (skeleton, reserved for future)
    ├── runpod_provider.rs # RunPod implementation (skeleton, reserved for future)
    └── lyceum_provider.rs # Lyceum AI implementation (skeleton, reserved for future)
```

**Architecture Pattern**: Follows industry patterns from Terraform (plugin registry), Pulumi (component model), and Kubernetes (CRD extensibility). The "defined but unused" pattern is common in mature tools during evolution.

**Current Status**: Provider trait system is defined with `ProviderRegistry` implemented, but CLI currently uses direct AWS implementation. This follows the pragmatic pattern where abstractions are prepared but not forced until multi-cloud support is actually needed.

**See**: `docs/PROVIDER_TRAIT_DECISION.md` for detailed rationale and industry context.

### Supporting Modules

```
src/
├── aws_utils.rs         # AWS utility functions (SSM, etc.)
├── checkpoint.rs        # Checkpoint management
├── dashboard.rs         # Interactive TUI dashboard (ratatui)
├── data_transfer.rs     # Data transfer operations
├── diagnostics.rs       # Resource usage diagnostics
├── ebs.rs              # EBS volume management
├── ebs_optimization.rs  # EBS optimization logic
├── fast_data_loading.rs # Optimized data loading
├── local.rs            # Local training execution
├── monitor.rs          # Training monitoring
├── resource_tracking.rs # ResourceTracker for cost awareness
├── safe_cleanup.rs     # Safe resource cleanup
├── s3.rs               # S3 operations (upload, download, sync)
├── ssh_sync.rs         # SSH-based code synchronization
└── training.rs         # Training session tracking
```

## Error Handling

### Dual Error System

The codebase uses a dual error system following Rust best practices:

- **Library Code** (`crate::error::Result<T>`): Structured errors with `TrainctlError` enum
- **CLI Code** (`anyhow::Result<T>`): Context-rich errors for user-facing code

### Error Types

All library errors use `TrainctlError` from `src/error.rs`:

```rust
pub enum TrainctlError {
    Config(ConfigError),
    Aws(String),
    S3(String),
    Resource { resource_type, resource_id, ... },
    Validation { field, reason },
    // ... more variants
}
```

### Error Boundary Conversion

CLI code converts library errors to `anyhow::Error` while preserving context:

```rust
// In main.rs - preserves error chain
.map_err(anyhow::Error::from)  // ✅ Preserves context
// NOT: .map_err(|e| anyhow::anyhow!("{}", e))  // ❌ Loses context
```

### Error Helpers

`src/error_helpers.rs` provides helper functions for rich error messages:
- `resource_not_found_with_suggestions`
- `validation_error_with_examples`
- `cloud_provider_error_with_troubleshooting`
- `config_error_with_fix`

### Usage Pattern

```rust
// Library code
use crate::error::{Result, TrainctlError};

fn example() -> Result<()> {
    Err(TrainctlError::ResourceNotFound {
        resource_type: "instance".to_string(),
        resource_id: "i-123".to_string(),
    })
}

// CLI code
pub async fn handle_command(...) -> anyhow::Result<()> {
    example().await.map_err(anyhow::Error::from)?;
    Ok(())
}
```

## Retry Logic

Use `ExponentialBackoffPolicy` for cloud API calls:

```rust
use crate::retry::{RetryPolicy, ExponentialBackoffPolicy};

let retry = ExponentialBackoffPolicy::for_cloud_api();
let result = retry.execute_with_retry(|| async {
    // Operation that might fail
}).await?;
```

## Resource Tracking

`ResourceTracker` provides cost awareness and resource lifecycle management:

```rust
use crate::resource_tracking::ResourceTracker;

let tracker = ResourceTracker::new();
tracker.register(resource_status).await?;
let running = tracker.get_running().await;
let total_cost = tracker.get_total_cost().await;
tracker.refresh_costs().await; // Update accumulated costs
```

## Safe Cleanup

Use `CleanupSafety` before deleting resources:

```rust
use crate::safe_cleanup::CleanupSafety;

let safety = CleanupSafety::new();
if safety.is_safe_to_delete(&resource_id).await? {
    // Proceed with deletion
}
```

## Testing

### Test Structure

```
tests/
├── integration_test.rs              # Integration tests
├── integration_provider_tests.rs    # Provider trait tests
├── integration_concurrent_operations_tests.rs
├── integration_resource_tracking_tests.rs
├── resource_tracker_unit_tests.rs
├── resource_tracker_property_tests.rs
├── resource_tracker_refresh_tests.rs
├── resource_tracker_state_update_tests.rs
├── cost_calculation_tests.rs
└── error_message_tests.rs
```

### Running Tests

```bash
# Unit tests
cargo test --lib

# Integration tests
cargo test --test integration_test

# E2E tests (requires AWS credentials)
TRAINCTL_E2E=1 cargo test --features e2e
```

## Code Style

- Use `crate::error::Result<T>` for library code
- Use `anyhow::Result<T>` for binary/CLI code (main.rs, aws.rs, ebs.rs, etc.)
- Prefer `?` operator over `.unwrap()` or `.expect()`
- Use `tracing::info!`, `warn!`, `error!` for logging
- Add `#[instrument]` to async functions for tracing

## File Organization

### Large Files Split

- `aws.rs` (2689 lines) → `src/aws/` (6 modules)
- `resources.rs` (2287 lines) → `src/resources/` (11 modules)

### Remaining Large Files

- `s3.rs` (1297 lines) - S3 operations
- `ebs.rs` (1193 lines) - EBS volume management
- `aws/instance.rs` (1274 lines) - Could be split further if needed
- `dashboard.rs` (654 lines) - Interactive dashboard
- `data_transfer.rs` (590 lines) - Data transfer operations

## Documentation

- User Guides: `docs/README.md`, `docs/EXAMPLES.md`
- Architecture: `docs/ARCHITECTURE.md`, `docs/PROVIDER_ARCHITECTURE.md`
- Implementation: `docs/IMPLEMENTATION_PLAN.md`
- Security: `docs/SECURITY_QUICK_START.md`, `docs/AWS_SECURITY_BEST_PRACTICES.md`
- Testing: `docs/TESTING.md`, `docs/E2E_TESTS.md`

## Development Workflow

1. Error Handling: Use `TrainctlError` for structured errors
2. Retry Logic: Wrap cloud API calls with `ExponentialBackoffPolicy`
3. Resource Tracking: Register resources with `ResourceTracker`
4. **Testing**: Add unit tests for new functions, integration tests for workflows
5. **Documentation**: Update relevant docs when adding features

## Key Design Decisions

1. **Provider Trait**: Defined with `ProviderRegistry` implemented, but CLI uses direct implementations until multi-cloud is needed (see `docs/PROVIDER_TRAIT_DECISION.md` for industry context)
2. **Error Types**: Dual system - `TrainctlError` for library code, `anyhow::Error` for CLI with context-preserving conversion
3. **Retry Logic**: Exponential backoff with jitter for cloud APIs, uses `IsRetryable` trait
4. **Resource Tracking**: Automatic cost calculation and state management with lazy updates
5. **Modular Structure**: Large files split into focused modules (aws/, resources/)
6. **Pragmatic Evolution**: Follows industry patterns - prepare abstractions but don't force migration (similar to Terraform, Pulumi evolution)

## Future Improvements

- Full provider trait integration
- Split remaining large files if they grow
- Enhanced observability (metrics, tracing)
- More comprehensive E2E test coverage

