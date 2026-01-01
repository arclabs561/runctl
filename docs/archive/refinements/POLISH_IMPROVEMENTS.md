# Polish Improvements Needed

## Overview

This document identifies areas where runctl could be more polished and production-ready.

## 1. JSON Output Consistency

### Current State
- ✅ `aws train` - Has `output_format` parameter
- ✅ `aws instances list` - Returns `InstanceInfo` (serializable)
- ❌ Most other commands - No JSON output

### Needed
- Add `--json` flag to ALL commands
- Ensure all result types implement `Serialize`
- Document JSON schema for each command

### Commands Missing JSON
- `aws create`
- `aws stop/start/terminate`
- `aws ebs *` (all EBS commands)
- `aws monitor`
- `resources list/cleanup/stop-all`
- `s3 list/watch/review` (only upload/download have JSON)

## 2. Exit Code Standardization

### Current State
- Uses Rust's default exit codes (may vary)
- No explicit exit code handling

### Needed
- Define exit code constants:
  - `0` = Success
  - `1` = User error (invalid input, validation failure)
  - `2` = System error (AWS API failure, network error)
  - `3` = Configuration error (missing config, invalid credentials)
- Map all error types to appropriate exit codes
- Document in help text

## 3. Error Message Standardization

### Current State
- Some errors have "To resolve:" sections (good!)
- Others are just error strings (bad)
- Format varies across modules

### Needed
- Standardize all error messages to include:
  1. Clear error description
  2. "To resolve:" section with numbered steps
  3. Relevant command examples
  4. Links to documentation (if applicable)

### Examples

**Good** (from `src/aws/training.rs`):
```rust
TrainctlError::Aws(format!(
    "SSM command failed and SSH fallback not available.\n\n\
    SSM error: {}\n\n\
    To resolve:\n\
      1. Check SSM connectivity: aws ssm describe-instance-information --instance-ids {}\n\
      2. Verify IAM role has SSM permissions\n\
      3. If SSH is desired, provide SSH key when creating instance and ensure port 22 is open",
    e, options.instance_id
))
```

**Needs Improvement** (many places):
```rust
Err(TrainctlError::Aws("Instance not found".to_string()))
```

## 4. Command Aliases

### Current State
- Very few aliases
- Long command names for common operations

### Needed
Add aliases for:
- `runctl aws instances list` → `runctl aws ls` or `runctl aws list`
- `runctl aws instances` → `runctl aws instances` (already short)
- `runctl resources list` → `runctl resources ls` or `runctl res ls`
- `runctl aws monitor` → `runctl aws watch` (already has alias)
- `runctl aws stop` → `runctl aws pause` (already has alias)

## 5. Dry-Run Mode

### Current State
- ✅ `resources cleanup` has `--dry-run`
- ❌ Most destructive commands don't

### Needed
Add `--dry-run` to:
- `aws terminate` - Show what would be terminated
- `aws ebs delete` - Show what would be deleted
- `aws stop` - Show what would be stopped
- `resources stop-all` - Already has it
- `s3 cleanup` - Show what would be cleaned

## 6. Progress Indicators

### Current State
- Inconsistent across commands
- Some use `indicatif::ProgressBar`
- Others use `println!`
- Some have no progress indication

### Needed
- Use consistent progress indicators for all long-running operations
- In JSON mode, emit progress as structured events:
  ```json
  {"type": "progress", "message": "Creating instance...", "percent": 50}
  ```
- Always indicate completion clearly

## 7. Command Validation

### Current State
- No way to validate commands before execution
- Errors only appear after execution starts

### Needed
- Add `--validate` flag to commands:
  ```bash
  runctl aws train i-123 train.py --validate
  # Output: Valid command. Would train on instance i-123 with script train.py
  ```
- Check:
  - Instance exists and is in correct state
  - Script exists and is accessible
  - Required dependencies are available
  - Permissions are sufficient

## 8. Structured Status Output

### Current State
- Status information is human-readable only
- AI tools must parse text

### Needed
- JSON output with structured status:
  ```json
  {
    "instances": [
      {
        "id": "i-123",
        "state": "running",
        "type": "g4dn.xlarge",
        "cost_per_hour": 0.50,
        "uptime_seconds": 3600,
        "public_ip": "1.2.3.4",
        "private_ip": "10.0.0.1"
      }
    ]
  }
  ```

## 9. Configuration Validation

### Current State
- Configuration errors appear at runtime
- No way to validate config file

### Needed
- Add `runctl config validate` command
- Check:
  - Config file syntax
  - Required fields present
  - AWS credentials valid
  - S3 bucket accessible
  - Region valid

## 10. Command Completion

### Current State
- No shell completion scripts
- No tab completion

### Needed
- Generate completion scripts for:
  - bash
  - zsh
  - fish
- Use `clap_complete` crate
- Document installation in README

## 11. Logging Levels

### Current State
- Uses `tracing` but levels may not be consistent
- No clear way to control verbosity

### Needed
- Add `--verbose` / `-v` flag (multiple levels)
- Add `--quiet` / `-q` flag
- Document log levels:
  - `ERROR` - Only errors
  - `WARN` - Warnings and errors
  - `INFO` - Normal operation (default)
  - `DEBUG` - Detailed debugging
  - `TRACE` - Very detailed

## 12. Timeout Handling

### Current State
- Some operations may hang indefinitely
- No timeout configuration

### Needed
- Add `--timeout` flag to long-running commands
- Default timeouts for common operations:
  - Instance creation: 5 minutes
  - Training: No timeout (user must interrupt)
  - Code sync: 10 minutes
- Document in help text

## 13. Retry Logic Visibility

### Current State
- Retry logic exists but is invisible to users
- No indication when retries are happening

### Needed
- Show retry attempts in progress output
- In JSON mode, emit retry events:
  ```json
  {"type": "retry", "attempt": 2, "max_attempts": 3, "error": "Connection timeout"}
  ```

## 14. Resource Naming

### Current State
- Resources use default AWS names
- No consistent naming scheme

### Needed
- Add `--name` or `--tag` flag to resource creation
- Use consistent naming: `runctl-<project>-<resource-type>-<id>`
- Document naming conventions

## 15. Output Formatting

### Current State
- Mixed formatting styles
- Some commands use tables, others use lists

### Needed
- Consistent formatting:
  - Lists: Use tables where appropriate
  - Status: Use consistent symbols (✅ ❌ ⚠️)
  - Colors: Use consistent color scheme
- Add `--format` flag: `table`, `json`, `yaml`, `plain`

## Priority Ranking

### Critical (Blockers)
1. JSON output consistency
2. Exit code standardization
3. Error message standardization

### High (Major UX Issues)
4. Command aliases
5. Dry-run mode
6. Structured status output
7. Progress indicators

### Medium (Nice to Have)
8. Command validation
9. Configuration validation
10. Shell completion
11. Logging levels

### Low (Future Enhancements)
12. Timeout handling
13. Retry visibility
14. Resource naming
15. Output formatting options

