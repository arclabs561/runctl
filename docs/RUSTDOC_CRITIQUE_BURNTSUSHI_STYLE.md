# Rustdoc Critique: Burntsushi's Perspective

## Overview

This critique evaluates the rustdoc documentation from Andrew Gallant's (burntsushi) perspective, focusing on upfront position statements, design rationale, practical examples, and clear technical communication without validation language.

## Burntsushi's Documentation Principles

1. **Upfront position statements**: Clearly declare boundaries and context before details
2. **Layered analysis**: Split "what and why" from "how"
3. **Concrete examples**: Reproducible, practical code examples
4. **Design rationale**: Explain WHY decisions were made, not just WHAT exists
5. **Meta-commentary**: Mix practical guidelines with architectural context
6. **Direct technical language**: No validation phrases, just facts
7. **Containment for complexity**: Use footnotes or asides for nuanced details

## Module-by-Module Critique

### `src/error.rs` - ⚠️ Needs Improvement

**Current State:**
```rust
//! Error types for runctl
//!
//! This module provides structured error handling with retry awareness
//! and clear error categorization.
```

**Issues:**
- No upfront statement about error handling philosophy
- Doesn't explain the relationship between `TrainctlError` and `ConfigError`
- Missing guidance on when to use which error variant
- No explanation of retry awareness design
- Doesn't address error conversion patterns (library vs CLI)

**Burntsushi-style Improvement:**
```rust
//! Error types for runctl
//!
//! This module defines the error handling strategy for runctl. There are two
//! error types: `TrainctlError` (main error enum) and `ConfigError` (configuration-specific).
//!
//! ## Error Handling Philosophy
//!
//! Library code uses `crate::error::Result<T>` which returns `TrainctlError`.
//! CLI code uses `anyhow::Result<T>` for top-level error handling. The conversion
//! happens at the CLI boundary using `anyhow::Error::from` to preserve error chains.
//!
//! ## Retry Awareness
//!
//! Errors implement `IsRetryable` to indicate whether an operation should be retried.
//! The `RetryPolicy` in `src/retry.rs` uses this to determine retry behavior.
//! Only `CloudProvider`, `Io`, and `Retryable` variants are retryable by default.
//!
//! ## When to Use Which Error
//!
//! - `ConfigError`: Configuration parsing and validation issues
//! - `CloudProvider`: Generic cloud API failures (provider-agnostic)
//! - `Aws`/`S3`/`Ssm`: AWS-specific errors (use when AWS context matters)
//! - `ResourceNotFound`/`ResourceExists`: Resource lifecycle errors
//! - `Validation`: Input validation failures
//!
//! ## Error Conversion
//!
//! When converting to `anyhow::Error`, use `anyhow::Error::from` (not string conversion)
//! to preserve error context and chains for better debugging.
```

### `src/retry.rs` - ⚠️ Good Examples, Missing Rationale

**Current State:**
- Has usage examples (good)
- Lists available policies
- Missing: WHY exponential backoff, WHY jitter, WHY these constants

**Issues:**
- Doesn't explain the design rationale for exponential backoff
- No discussion of jitter purpose (thundering herd prevention)
- Constants are defined but not explained
- Missing guidance on when to use which policy

**Burntsushi-style Improvement:**
```rust
//! Retry logic with exponential backoff
//!
//! Provides retry policies for handling transient failures in cloud API calls.
//!
//! ## Design Rationale
//!
//! Cloud APIs can fail transiently due to rate limiting, network issues, or
//! temporary service unavailability. Exponential backoff with jitter prevents
//! thundering herd problems when multiple clients retry simultaneously.
//!
//! The default policy uses:
//! - 5 attempts for cloud APIs (higher than default 3 due to cloud API volatility)
//! - Exponential backoff: 100ms → 200ms → 400ms → 800ms → 1600ms (capped at 30s)
//! - 10% jitter to randomize retry timing across clients
//!
//! ## When to Retry
//!
//! Only errors implementing `IsRetryable` are retried. Non-retryable errors
//! (e.g., validation errors, authentication failures) fail immediately.
//!
//! ## Policy Selection
//!
//! - `for_cloud_api()`: Use for AWS EC2, S3, SSM calls (5 attempts)
//! - `new(n)`: Custom attempts for specific use cases
//! - `NoRetryPolicy`: For operations that must not be retried (e.g., resource deletion)
```

### `src/resource_tracking.rs` - ⚠️ Too Minimal

**Current State:**
```rust
//! Resource tracking and cost awareness
//!
//! Tracks what resources exist, what's running, and resource usage
//! to enable cost awareness and safe cleanup.
```

**Issues:**
- Doesn't explain the design (in-memory vs persistent)
- No guidance on when to register resources
- Doesn't explain the relationship between `ResourceStatus` and `TrackedResource`
- Missing discussion of cost calculation approach

**Burntsushi-style Improvement:**
```rust
//! Resource tracking and cost awareness
//!
//! Tracks resource lifecycle and usage to enable cost awareness and safe cleanup.
//!
//! ## Design
//!
//! `ResourceTracker` maintains an in-memory map of resources. It's designed for
//! single-process use (CLI tool). Resources are registered when created and
//! updated as their state changes.
//!
//! ## Cost Calculation
//!
//! Accumulated cost is calculated on-demand when accessing resources via
//! `get_running()`, `get_by_id()`, etc. This ensures costs are always current
//! based on `launch_time` and `cost_per_hour`, without requiring periodic
//! background tasks.
//!
//! ## Resource Lifecycle
//!
//! - `ResourceStatus`: Provider-agnostic resource state (from `provider` module)
//! - `TrackedResource`: Internal tracking with usage history and accumulated cost
//! - Resources are registered via `register()` when created
//! - State updates via `update_state()` as resources transition
//! - Usage metrics added via `update_usage()` for monitoring
//!
//! ## Thread Safety
//!
//! Uses `Arc<Mutex<HashMap>>` for thread-safe access. All methods are async
//! to avoid blocking on the mutex.
```

### `src/provider.rs` - ✅ Good, Could Be More Upfront

**Current State:**
- Has architecture pattern explanation
- Documents current status
- Has future evolution path
- Good example code

**Minor Issues:**
- Could be more upfront about trade-offs
- Could explain why this pattern was chosen over alternatives

**Suggested Addition:**
```rust
//! Provider-agnostic trait definitions for cloud training platforms
//!
//! ## Position Statement
//!
//! This trait system is **defined but not yet used** by the CLI. The CLI currently
//! uses direct implementations (`aws::handle_command()`, etc.). This is intentional
//! technical debt - see rationale below.
//!
//! ## Why This Approach?
//!
//! **Alternative 1**: Force migration now
//! - Pro: Consistent abstraction
//! - Con: High risk (incomplete implementations), breaks working code
//!
//! **Alternative 2**: Delete trait system
//! - Pro: No unused code
//! - Con: Harder to add multi-cloud support later
//!
//! **Chosen Approach**: Keep trait, don't force migration
//! - Pro: Future-ready, low risk, follows industry patterns (Terraform, Pulumi)
//! - Con: Some unused code (documented and marked)
//!
//! [rest of existing documentation...]
```

### `src/config.rs` - ⚠️ Too Minimal

**Current State:**
```rust
//! Configuration management
//!
//! Handles loading and parsing of `.runctl.toml` configuration files.
//! Provides defaults and validation for all configuration options.
```

**Issues:**
- Doesn't explain the config file structure
- No guidance on defaults vs required fields
- Missing validation approach explanation
- Doesn't explain the relationship between config sections

**Burntsushi-style Improvement:**
```rust
//! Configuration management
//!
//! Handles loading and parsing of `.runctl.toml` configuration files.
//!
//! ## Configuration Philosophy
//!
//! All configuration is optional - runctl works with sensible defaults.
//! Configuration files are discovered automatically (current directory → home directory).
//! Environment variables can override config values (see individual field docs).
//!
//! ## Config Structure
//!
//! - `[aws]`: AWS-specific settings (region, instance types, spot pricing)
//! - `[runpod]`: RunPod API configuration
//! - `[local]`: Local execution settings
//! - `[checkpoint]`: Checkpoint management defaults
//! - `[monitoring]`: Logging and monitoring configuration
//!
//! ## Defaults
//!
//! If a config file doesn't exist, `Config::load()` returns defaults suitable
//! for basic usage. AWS defaults to `us-east-1`, `t3.medium` instances.
//! RunPod defaults to RTX 4080 SUPER with 30GB disk.
//!
//! ## Validation
//!
//! Validation happens on load. Invalid values return `ConfigError::InvalidValue`
//! with a reason. Missing required fields return `ConfigError::MissingField`.
//! The config file path is included in error messages for debugging.
```

### `src/aws/helpers.rs` - ⚠️ Missing Context

**Current State:**
```rust
//! Helper functions for AWS operations
//!
//! Utility functions used across AWS modules for common operations
//! like user identification, project name derivation, and resource status conversion.
```

**Issues:**
- Doesn't explain when to use helpers vs direct AWS SDK calls
- No guidance on the conversion function's purpose
- Missing discussion of fallback behavior

**Burntsushi-style Improvement:**
```rust
//! Helper functions for AWS operations
//!
//! Utility functions used across AWS modules for common operations.
//!
//! ## When to Use Helpers vs Direct SDK Calls
//!
//! These helpers provide:
//! - **Abstraction**: Convert AWS-specific types to provider-agnostic types
//! - **Consistency**: Standardized tagging, naming, and status conversion
//! - **Fallback logic**: Auto-detection when config values aren't set
//!
//! Use helpers when you need provider-agnostic types (`ResourceStatus`) or
//! consistent behavior (project names, user IDs). Use direct SDK calls when
//! you need AWS-specific features not in the abstraction.
//!
//! ## Key Functions
//!
//! - `ec2_instance_to_resource_status()`: Converts AWS EC2 instances to
//!   provider-agnostic `ResourceStatus` for use with `ResourceTracker`
//! - `get_user_id()`: Returns user ID with fallback chain (config → env → "unknown")
//! - `get_project_name()`: Derives project name from config or current directory
```

### `src/safe_cleanup.rs` - ✅ Good Examples, Missing Rationale

**Current State:**
- Has usage examples (good)
- Lists protection mechanisms
- Missing: WHY these protection mechanisms exist

**Suggested Addition:**
```rust
//! Safe cleanup and teardown operations
//!
//! Provides careful resource cleanup with confirmation, dry-run, and safety checks.
//!
//! ## Design Rationale
//!
//! Accidental resource deletion is costly and disruptive. This module implements
//! multiple protection layers:
//!
//! 1. **Time-based protection**: Prevents deletion of resources < 5 minutes old
//!    (catches immediate mistakes, requires `--force` to override)
//! 2. **Tag-based protection**: Resources tagged `runctl:protected=true` cannot be deleted
//!    (explicit opt-in protection for important resources)
//! 3. **Explicit protection**: Programmatic protection via `safety.protect()`
//!    (for resources that should never be deleted)
//!
//! ## Protection Precedence
//!
//! All protections are checked unless `force=true`. If any protection applies,
//! the resource is skipped. The `--force` flag bypasses all protections (use with caution).
```

## General Patterns Missing

### 1. Upfront Position Statements

Most modules jump into "what" without stating "why" or "when". Burntsushi would start with:

```rust
//! [Module purpose]
//!
//! [Upfront statement about design decisions, trade-offs, or philosophy]
//!
//! [Then details...]
```

### 2. Design Rationale

Many modules explain WHAT exists but not WHY. Examples:
- Why exponential backoff? (Cloud API volatility)
- Why in-memory tracking? (CLI tool, not a service)
- Why provider trait unused? (Pragmatic technical debt)

### 3. When to Use Guidance

Missing guidance on:
- When to use `TrainctlError::Aws` vs `TrainctlError::CloudProvider`
- When to use helpers vs direct SDK calls
- When to use which retry policy

### 4. Error Handling Philosophy

The error handling strategy (library vs CLI) is not documented upfront. Burntsushi would explain:
- Library code uses `crate::error::Result`
- CLI code uses `anyhow::Result`
- Conversion happens at boundaries
- Why this split exists

### 5. Concrete Examples

Some modules have examples (good), but they could be more practical:
- Show error handling patterns
- Show retry usage in context
- Show resource tracking lifecycle

## Recommendations

### High Priority

1. **Add upfront position statements** to all module docs explaining design decisions
2. **Document error handling philosophy** in `error.rs` module doc
3. **Explain retry design rationale** in `retry.rs` (why exponential backoff, jitter, constants)
4. **Document resource tracking design** in `resource_tracking.rs` (in-memory vs persistent, cost calculation)

### Medium Priority

5. **Add "when to use" guidance** for error variants, helper functions, retry policies
6. **Explain config philosophy** in `config.rs` (optional vs required, defaults, validation)
7. **Document provider trait trade-offs** more explicitly in `provider.rs`

### Low Priority

8. **Add more concrete examples** showing real-world usage patterns
9. **Document thread safety** where relevant (resource_tracking, etc.)
10. **Add footnotes** for complex design decisions

## Example: Complete Rewrite of `error.rs` Module Doc

```rust
//! Error types for runctl
//!
//! This module defines the error handling strategy for runctl. There are two
//! error types: `TrainctlError` (main error enum) and `ConfigError` (configuration-specific).
//!
//! ## Error Handling Philosophy
//!
//! Library code uses `crate::error::Result<T>` which returns `TrainctlError`.
//! CLI code uses `anyhow::Result<T>` for top-level error handling. The conversion
//! happens at the CLI boundary using `anyhow::Error::from` to preserve error chains.
//!
//! This split exists because:
//! - Library code benefits from structured error types for programmatic handling
//! - CLI code benefits from `anyhow`'s context chains and user-friendly display
//! - Conversion preserves full error information (not just strings)
//!
//! ## Retry Awareness
//!
//! Errors implement `IsRetryable` to indicate whether an operation should be retried.
//! The `RetryPolicy` in `src/retry.rs` uses this to determine retry behavior.
//! Only `CloudProvider`, `Io`, and `Retryable` variants are retryable by default.
//!
//! Non-retryable errors (e.g., `Validation`, `Config`) fail immediately to avoid
//! wasting time on operations that cannot succeed.
//!
//! ## When to Use Which Error
//!
//! - `ConfigError`: Configuration parsing and validation issues
//!   - Use when config file is malformed or missing required fields
//!   - Automatically converted to `TrainctlError::Config` via `#[from]`
//!
//! - `CloudProvider`: Generic cloud API failures (provider-agnostic)
//!   - Use for provider-agnostic errors that could occur with any cloud
//!   - Retryable by default
//!
//! - `Aws`/`S3`/`Ssm`: AWS-specific errors
//!   - Use when AWS-specific context matters for debugging
//!   - `Aws` is retryable (wrapped in `CloudProvider` internally)
//!
//! - `ResourceNotFound`/`ResourceExists`: Resource lifecycle errors
//!   - Use when resources don't exist or already exist
//!   - Not retryable (idempotency issues, not transient failures)
//!
//! - `Validation`: Input validation failures
//!   - Use for user input validation (instance IDs, paths, etc.)
//!   - Not retryable (invalid input won't become valid)
//!
//! ## Error Conversion
//!
//! When converting to `anyhow::Error`, use `anyhow::Error::from` (not string conversion)
//! to preserve error context and chains for better debugging.
//!
//! ```rust,no_run
//! // Good: preserves error chain
//! .map_err(anyhow::Error::from)
//!
//! // Bad: loses error context
//! .map_err(|e| anyhow::anyhow!("{}", e))
//! ```
```

## Improvements Made ✅

Based on this critique, the following modules have been updated to follow burntsushi's style:

1. **`src/error.rs`**: Added error handling philosophy, retry awareness explanation, and "when to use" guidance for each error variant
2. **`src/retry.rs`**: Added design rationale (why exponential backoff, jitter, constants), policy selection guidance
3. **`src/resource_tracking.rs`**: Added design explanation (in-memory vs persistent), cost calculation approach, resource lifecycle, thread safety
4. **`src/config.rs`**: Added configuration philosophy, config structure, defaults, and validation approach
5. **`src/aws/helpers.rs`**: Added "when to use helpers vs direct SDK calls" guidance and key function explanations
6. **`src/safe_cleanup.rs`**: Added design rationale for protection mechanisms and protection precedence
7. **`src/provider.rs`**: Added upfront position statement with trade-off analysis

## Remaining Opportunities

- Add more concrete examples showing error handling patterns
- Document thread safety in other modules where relevant
- Add footnotes for complex design decisions
- Improve function-level documentation with rationale

## Conclusion

The documentation has been significantly improved with upfront position statements, design rationale, and "when to use" guidance. The updated modules now follow burntsushi's style:

1. ✅ **Upfront context** about design decisions
2. ✅ **Rationale** for architectural choices
3. ✅ **Practical guidance** on when to use what
4. ✅ **Direct technical language** without validation phrases
5. ✅ **Containment** for complex topics (trade-offs, alternatives)

The documentation is now more educational and self-explanatory, following burntsushi's patterns of clear, upfront communication with design rationale.

