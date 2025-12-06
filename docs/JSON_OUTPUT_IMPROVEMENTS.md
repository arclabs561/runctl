# JSON Output Improvements

## Current State

trainctl supports `--output json` for structured output, but implementation is inconsistent across commands.

## Commands with JSON Support

### ✅ Implemented
- `resources list` - Returns JSON array of resources
- `resources summary` - Returns JSON summary
- `resources export` - Exports to JSON file

### ⚠️ Partial
- `aws create` - Returns instance info (but not consistently formatted)
- `aws train` - Returns status (but not consistently formatted)

### ❌ Missing
- `aws stop` - Text only
- `aws terminate` - Text only
- `aws monitor` - Text only (streaming)
- `aws processes` - Text only
- `checkpoint list` - Text only
- `checkpoint info` - Text only
- `s3` commands - Text only

## Recommended Improvements

### 1. Consistent JSON Structure

All commands should return consistent JSON:

```json
{
  "success": true,
  "data": { ... },
  "message": "Operation completed",
  "metadata": {
    "command": "aws create",
    "timestamp": "2025-01-01T12:00:00Z"
  }
}
```

### 2. Error Format

Errors should also be JSON:

```json
{
  "success": false,
  "error": {
    "type": "ResourceNotFound",
    "message": "Instance i-123 not found",
    "resource_type": "instance",
    "resource_id": "i-123"
  }
}
```

### 3. Streaming Commands

For streaming commands (monitor, watch), provide option for JSON lines:

```json
{"type": "log", "timestamp": "...", "message": "..."}
{"type": "checkpoint", "timestamp": "...", "path": "..."}
```

## Implementation Plan

### Phase 1: Core Infrastructure
1. Create `OutputFormatter` trait
2. Implement `JsonFormatter` and `TextFormatter`
3. Refactor commands to use formatters

### Phase 2: Command Updates
1. Update `aws` commands
2. Update `checkpoint` commands
3. Update `s3` commands

### Phase 3: Documentation
1. Document JSON schema
2. Add examples
3. Update Python wrapper

## Priority

**High Priority:**
- `aws create` - Most commonly used programmatically
- `aws train` - Needed for automation
- `resources list` - Already works, just needs consistency

**Medium Priority:**
- `checkpoint list` - Useful for checkpoint management
- `aws stop/terminate` - Useful for cleanup scripts

**Low Priority:**
- Streaming commands (monitor, watch) - Less common programmatic use

## Example Implementation

```rust
pub trait OutputFormatter {
    fn format_success<T: Serialize>(&self, data: T, message: &str) -> String;
    fn format_error(&self, error: &TrainctlError) -> String;
}

pub struct JsonFormatter;

impl OutputFormatter for JsonFormatter {
    fn format_success<T: Serialize>(&self, data: T, message: &str) -> String {
        json!({
            "success": true,
            "data": data,
            "message": message,
            "timestamp": Utc::now().to_rfc3339()
        }).to_string()
    }
    
    fn format_error(&self, error: &TrainctlError) -> String {
        json!({
            "success": false,
            "error": {
                "type": format!("{:?}", error),
                "message": error.to_string()
            }
        }).to_string()
    }
}
```

## Current Workaround

For commands without JSON support, use text parsing:

```python
import subprocess
import re

result = subprocess.run(
    ["trainctl", "aws", "create", "--instance-type", "g4dn.xlarge"],
    capture_output=True,
    text=True
)

# Parse instance ID from output
match = re.search(r'i-[a-z0-9]+', result.stdout)
if match:
    instance_id = match.group(0)
```

This is not ideal but works until JSON support is added.

