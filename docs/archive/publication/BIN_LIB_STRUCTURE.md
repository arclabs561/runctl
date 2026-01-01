# Binary/Library Structure Analysis

## Current State

### Configuration
- **Binary**: `src/main.rs` (declared in `Cargo.toml` as `[[bin]]`)
- **Library**: `src/lib.rs` (declared in `Cargo.toml` as `[lib]`)
- **Publishing**: Enabled (removed `publish = false`, added `authors` field)

### Structure Issue

The binary (`main.rs`) currently **duplicates all module declarations** instead of using the library:

```rust
// src/main.rs
mod aws;
mod config;
mod checkpoint;
// ... 28 total module declarations
```

Effects:
1. Compilation overhead: Modules are compiled twice (once for lib, once for bin)
2. Code duplication: Module paths are declared in both places
3. Maintenance burden: Adding a module requires updating both files

### What Should Happen

The binary should use the library:

```rust
// src/main.rs (ideal structure)
use runctl::*;  // or selective imports
mod docker_cli;  // Only binary-specific modules

// Then use: runctl::config::Config, etc.
```

## Why This Structure Makes Sense

### Having Both Bin and Lib

1. Library users: Other projects can import `runctl` as a dependency
   - Tests already do this: `use runctl::config::Config;`
   - Allows programmatic usage beyond CLI
   
2. Binary users: CLI users get `runctl` executable
   - Standard Rust pattern: `cargo install runctl` installs the binary
   - Library is available for those who want it

3. Separation of concerns:
   - Library = core functionality (reusable)
   - Binary = CLI interface (user-facing)

### Current Duplication

Problem: `main.rs` declares modules with `mod` instead of using the library.

Impact:
- Modules compiled twice (wasted compile time)
- Two separate compilation units sharing the same code
- Risk of divergence if someone forgets to update both

Evidence: Tests correctly use the library (`use runctl::config::Config`), but the binary doesn't.

## How It Currently Works

### Compilation Model

Rust treats `lib.rs` and `main.rs` as separate crate roots:
- `lib.rs` → library crate (compiled as `runc` library)
- `main.rs` → binary crate (compiled as `runctl` binary)

When `main.rs` declares `mod config`, it creates a **new** `config` module in the binary crate, separate from the library's `config` module.

### Current Flow

```
src/lib.rs
  └─ pub mod config;  → Library crate's config module

src/main.rs
  └─ mod config;       → Binary crate's config module (separate!)
     └─ use crate::config::Config;  → Uses binary's own config
```

Both compile the same `src/config.rs` file, but they're separate compilation units.

### Binary-Only Modules

`docker_cli` is correctly binary-only (not in `lib.rs`):
- CLI-specific command parsing
- Not needed by library users
- This pattern is correct

## Recommended Fix

### Option 1: Use Library (IMPLEMENTED)

Refactored `main.rs` to use the library:

```rust
// src/main.rs
mod docker_cli;  // Only binary-specific module

// All other modules accessed via runctl:: (the library)
use runctl::config::Config;
// ... etc
```

Benefits:
- Single compilation of shared modules
- Binary uses library (DRY principle)
- Easier maintenance

Changes made:
- Removed all duplicate `mod` declarations from `main.rs`
- Replaced `crate::` with `runctl::` in `main.rs`
- Updated `docker_cli.rs` to use `runctl::` instead of `crate::`
- Added `workflow` module to `lib.rs` (was missing)
- Kept only `mod docker_cli;` as binary-specific

### Option 2: Keep Current Structure (Not Recommended - Not Used)

If keeping duplication:
- Document why (if there's a reason)
- Accept the compile-time cost
- Ensure both stay in sync manually

## Publishing Readiness

### Publishing

1. Metadata complete:
   - `name`, `version`, `description`
   - `license`, `repository`, `homepage`
   - `authors`
   - `keywords`, `categories`
   - `readme`

2. Structure valid:
   - Both `[[bin]]` and `[lib]` are valid
   - Duplication doesn't prevent publishing
   - Tests use library correctly

3. To publish:
   ```bash
   cargo publish --dry-run  # Test first
   cargo publish            # Actually publish
   ```

### Considerations

- Version: Currently `0.1.0` - consider if this is appropriate for first publish
- Documentation: Library docs (`lib.rs` has usage examples)
- Dependencies: All dependencies are on crates.io (required for publishing)

## Summary

| Aspect | Status | Notes |
|--------|--------|-------|
| Bin/Lib structure | Valid | Both targets configured correctly |
| Publishing enabled | Ready | Removed `publish = false`, added `authors` |
| Module duplication | Fixed | Binary now uses library, no duplication |
| Binary-only modules | Correct | `docker_cli` correctly binary-only |
| Library usage | Good | Tests use library properly |

Status: Binary now uses the library. All modules compile once, and the binary references them via `runctl::` paths. Ready for publishing.

## Additional Improvements Made

### 1. Convenience Re-exports Added
Added commonly used types to `lib.rs` for easier library usage:
- `pub use config::Config;` - Most commonly used type
- `pub use aws::{CreateInstanceOptions, TrainInstanceOptions};` - AWS operation types
- `pub use resources::estimate_instance_cost;` - Utility function

### 2. Fixed SSH Sync Lifetime Issue
Fixed a pre-existing lifetime bug in `sync_code_shell()` where `ssh_process.stderr` was accessed after the process was moved into an async closure. The fix captures stderr before moving the process.

### 3. Library Structure Verification
- All public APIs used by binary are accessible
- Module organization follows Rust best practices
- Binary-only module (`docker_cli`) correctly isolated
- Library can be used independently of binary

