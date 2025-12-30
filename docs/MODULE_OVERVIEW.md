# Module Overview

**Last Updated**: 2025-01-03

## Quick Reference

This document provides a quick overview of all modules in the runctl codebase.

## Core Modules

| Module | Lines | Purpose |
|--------|-------|---------|
| `lib.rs` | 33 | Library entry point, re-exports |
| `main.rs` | 339 | CLI entry point, command parsing |
| `config.rs` | 503 | Configuration management (TOML) |
| `error.rs` | ~200 | Custom error types (TrainctlError) |
| `error_helpers.rs` | ~100 | Error message helpers |
| `retry.rs` | ~200 | Retry policies (ExponentialBackoffPolicy) |
| `validation.rs` | ~100 | Input validation utilities |
| `utils.rs` | ~200 | General utilities (cost calculation, etc.) |

## AWS Module (`src/aws/`)

| Module | Lines | Purpose |
|--------|-------|---------|
| `mod.rs` | 383 | Command handling, CLI interface |
| `types.rs` | 127 | Shared type definitions |
| `helpers.rs` | 230 | Utility functions |
| `instance.rs` | 1274 | Instance lifecycle (create, start, stop, terminate) |
| `training.rs` | 333 | Training operations (train, sync, monitor) |
| `processes.rs` | 254 | Process monitoring |

## Resources Module (`src/resources/`)

| Module | Lines | Purpose |
|--------|-------|---------|
| `mod.rs` | 284 | Command enum, dispatcher |
| `types.rs` | 98 | Data structures |
| `json.rs` | 212 | JSON serialization |
| `aws.rs` | 686 | AWS listing and management |
| `runpod.rs` | 98 | RunPod listing |
| `local.rs` | 76 | Local process listing |
| `summary.rs` | 365 | Summary and insights |
| `export.rs` | 184 | Export to CSV/HTML |
| `watch.rs` | 49 | Watch mode |
| `cleanup.rs` | 336 | Cleanup operations |
| `utils.rs` | 18 | Utility functions |

## Provider System

| Module | Lines | Purpose |
|--------|-------|---------|
| `provider.rs` | ~200 | TrainingProvider trait definition |
| `providers/mod.rs` | ~50 | Provider registry |
| `providers/aws_provider.rs` | ~100 | AWS implementation (skeleton) |
| `providers/runpod_provider.rs` | ~100 | RunPod implementation (skeleton) |
| `providers/lyceum_provider.rs` | ~100 | Lyceum AI implementation (skeleton) |

**Status**: Defined but not yet fully integrated. See `docs/PROVIDER_TRAIT_DECISION.md`.

## Supporting Modules

| Module | Lines | Purpose |
|--------|-------|---------|
| `aws_utils.rs` | 407 | AWS utility functions (SSM, etc.) |
| `checkpoint.rs` | 517 | Checkpoint management |
| `dashboard.rs` | 654 | Interactive TUI dashboard (ratatui) |
| `data_transfer.rs` | 590 | Data transfer operations |
| `diagnostics.rs` | 458 | Resource usage diagnostics |
| `ebs.rs` | 1193 | EBS volume management |
| `ebs_optimization.rs` | ~200 | EBS optimization logic |
| `fast_data_loading.rs` | ~200 | Optimized data loading |
| `local.rs` | ~200 | Local training execution |
| `monitor.rs` | ~200 | Training monitoring |
| `resource_tracking.rs` | ~400 | ResourceTracker for cost awareness |
| `safe_cleanup.rs` | ~150 | Safe resource cleanup |
| `s3.rs` | 1297 | S3 operations (upload, download, sync) |
| `ssh_sync.rs` | 475 | SSH-based code synchronization |
| `training.rs` | ~300 | Training session tracking |

## Test Modules

| Module | Purpose |
|--------|---------|
| `tests/integration_test.rs` | Integration tests |
| `tests/integration_provider_tests.rs` | Provider trait tests |
| `tests/integration_concurrent_operations_tests.rs` | Concurrent operations |
| `tests/integration_resource_tracking_tests.rs` | Resource tracking |
| `tests/resource_tracker_unit_tests.rs` | ResourceTracker unit tests |
| `tests/resource_tracker_property_tests.rs` | Property-based tests |
| `tests/resource_tracker_refresh_tests.rs` | Cost refresh tests |
| `tests/resource_tracker_state_update_tests.rs` | State update tests |
| `tests/cost_calculation_tests.rs` | Cost calculation tests |
| `tests/error_message_tests.rs` | Error message tests |

## Statistics

- **Total Source Files**: ~40 Rust files
- **Total Test Files**: ~10 test files
- **Total Lines**: ~20,000+ lines of code
- **Test Coverage**: 29 passing tests
- **Largest Module**: `src/aws/instance.rs` (1274 lines)
- **Largest File**: `src/s3.rs` (1297 lines)

## Module Dependencies

```
main.rs
├── aws/ (modular)
├── resources/ (modular)
├── s3
├── checkpoint
├── config
├── dashboard
├── data_transfer
├── diagnostics
├── ebs
├── local
├── monitor
├── provider
├── providers/
├── resource_tracking
├── retry
├── safe_cleanup
├── ssh_sync
├── training
└── utils
```

## Key Patterns

1. **Error Handling**: All modules use `crate::error::Result<T>` or `TrainctlError`
2. **Retry Logic**: Cloud API calls use `ExponentialBackoffPolicy`
3. **Resource Tracking**: Resources registered with `ResourceTracker`
4. **Safe Cleanup**: Use `CleanupSafety` before deletion
5. **Modular Structure**: Large files split into focused modules

