# AI Tool Usability Analysis

## Overview

This document analyzes runctl's usability for AI coding assistants (Cursor, GitHub Copilot, etc.) and identifies areas for improvement.

## Current State

### ✅ Strengths

1. **Structured Error Messages**: Many errors include "To resolve:" sections with actionable steps
2. **JSON Output Support**: Some commands support `--json` flag for machine-readable output
3. **Non-Interactive by Default**: Most commands don't require user input
4. **Clear Command Structure**: Commands follow predictable patterns (`runctl <platform> <action>`)
5. **Help Text**: Commands have detailed help with examples

### ⚠️ Gaps and Issues

#### 1. Inconsistent JSON Output

**Problem**: JSON output is not consistently available across all commands.

**Current State**:
- ✅ `aws train` - Has `output_format` parameter
- ✅ `aws instances list` - Returns structured `InstanceInfo`
- ❌ `aws create` - No JSON output
- ❌ `aws stop/start/terminate` - No JSON output
- ❌ `aws ebs *` - No JSON output
- ❌ `aws monitor` - No JSON output
- ❌ `resources list` - No JSON output
- ❌ `s3 *` - Partial JSON (only upload/download results)

**Impact**: AI tools can't reliably parse command outputs programmatically.

**Recommendation**: Add `--json` flag to all commands that produce output.

#### 2. Inconsistent Exit Codes

**Problem**: Exit codes may not be consistent for error scenarios.

**Current State**: 
- Uses Rust's `Result` types, but exit codes may vary
- No explicit exit code handling in `main.rs`

**Impact**: AI tools can't reliably detect success/failure.

**Recommendation**: 
- Explicit exit codes: 0 = success, 1 = user error, 2 = system error
- Document exit codes in help text

#### 3. Mixed Output Formats

**Problem**: Some commands mix human-readable and machine-readable output.

**Example**:
```rust
println!("Created instance: {}", instance_id);  // Human-readable
// But no JSON alternative
```

**Impact**: AI tools must parse unstructured text.

**Recommendation**: 
- Always provide JSON alternative
- Use structured types for all outputs

#### 4. Missing Command Aliases

**Problem**: Long command names are verbose for AI tools.

**Current State**:
- `runctl aws instances list` - No alias
- `runctl aws train` - No alias
- `runctl resources list` - No alias

**Impact**: AI tools generate verbose commands.

**Recommendation**: Add common aliases:
- `runctl aws ls` → `runctl aws instances list`
- `runctl aws train` → `runctl aws t` (or keep as-is, it's short)
- `runctl resources ls` → `runctl resources list`

#### 5. Inconsistent Error Message Format

**Problem**: Error messages vary in structure and actionability.

**Good Example**:
```rust
TrainctlError::Aws(format!(
    "Instance {} not found.\n\n\
    To resolve:\n\
      1. Verify instance ID: runctl resources list --platform aws\n\
      2. Check if instance was terminated: aws ec2 describe-instances --instance-ids {}\n\
      ...",
    instance_id, instance_id
))
```

**Bad Example**:
```rust
Err(TrainctlError::Aws("Instance not found".to_string()))
```

**Impact**: AI tools can't reliably extract actionable information.

**Recommendation**: Standardize error message format with "To resolve:" sections.

#### 6. No Machine-Readable Status Codes

**Problem**: Status information is only in human-readable format.

**Example**: `runctl aws instances list` shows:
```
Instance i-123: running (g4dn.xlarge) - $0.50/hr
```

**Impact**: AI tools must parse text to extract status.

**Recommendation**: Add JSON output with structured status:
```json
{
  "instances": [
    {
      "id": "i-123",
      "state": "running",
      "type": "g4dn.xlarge",
      "cost_per_hour": 0.50
    }
  ]
}
```

#### 7. Missing Validation Feedback

**Problem**: Validation errors don't always provide clear guidance.

**Current State**: Some validation errors are good, but inconsistent.

**Recommendation**: All validation errors should:
- Show the invalid value
- Show valid examples
- Provide command to check valid values

#### 8. No Dry-Run Mode

**Problem**: AI tools can't preview what commands will do.

**Current State**: 
- ✅ `resources cleanup` has `--dry-run`
- ❌ Most other commands don't

**Impact**: AI tools can't safely test commands.

**Recommendation**: Add `--dry-run` to destructive commands:
- `aws terminate --dry-run`
- `aws ebs delete --dry-run`
- `aws stop --dry-run` (show what would stop)

#### 9. Inconsistent Progress Indicators

**Problem**: Progress indicators vary across commands.

**Current State**:
- Some commands use `indicatif::ProgressBar`
- Others use `println!`
- Some have no progress indication

**Impact**: AI tools can't reliably detect when operations complete.

**Recommendation**: 
- Use consistent progress indicators
- In JSON mode, emit progress as structured events
- Always indicate completion clearly

#### 10. No Command Completion/Validation

**Problem**: No way to validate commands before execution.

**Example**: AI tool wants to check if `runctl aws train i-123 train.py` is valid.

**Recommendation**: Add `--validate` flag:
```bash
runctl aws train i-123 train.py --validate
# Output: Valid command. Would train on instance i-123 with script train.py
```

## Recommendations Priority

### High Priority (AI Tool Blockers)

1. **Add JSON output to all commands** - Critical for programmatic access
2. **Standardize exit codes** - Essential for error detection
3. **Consistent error message format** - Needed for actionable error handling

### Medium Priority (UX Improvements)

4. **Add command aliases** - Reduces verbosity
5. **Add dry-run mode** - Enables safe testing
6. **Structured status output** - Better parsing

### Low Priority (Nice to Have)

7. **Command validation mode** - Helpful but not critical
8. **Progress event streaming** - Advanced feature

## Implementation Plan

### Phase 1: JSON Output Everywhere

1. Add `--json` flag to all commands
2. Create serializable result types for all commands
3. Update help text to document JSON output

### Phase 2: Exit Code Standardization

1. Define exit code constants
2. Map error types to exit codes
3. Document in help text

### Phase 3: Error Message Standardization

1. Create error message template
2. Update all error messages to use template
3. Ensure all errors have "To resolve:" sections

### Phase 4: Command Aliases

1. Add aliases for common commands
2. Update help text
3. Document in quick reference

## Examples for AI Tools

### Good Command (JSON + Clear Exit Code)

```bash
runctl aws create g4dn.xlarge --spot --json
# Exit code: 0
# Output: {"instance_id": "i-123", "state": "pending", "cost_per_hour": 0.50}
```

### Bad Command (No JSON, Unclear Exit)

```bash
runctl aws create g4dn.xlarge --spot
# Exit code: ? (may vary)
# Output: "Created spot instance: i-123" (unstructured)
```

## Conclusion

runctl is **partially ready** for AI tool integration. The main blockers are:

1. **Inconsistent JSON output** - Most critical
2. **Unclear exit codes** - Important for error handling
3. **Mixed output formats** - Makes parsing unreliable

With these improvements, runctl would be **excellent** for AI tool integration.

