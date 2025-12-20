# UX and Functionality Review

**Date**: 2025-01-XX  
**Scope**: Complete review of runctl command UX, functionality, and command combinations

## Executive Summary

runctl provides a comprehensive CLI for ML training orchestration. The tool is functional but has several UX inconsistencies and opportunities for improvement in command combinations, output formats, and error handling.

## Command Structure Analysis

### Top-Level Commands

| Command | Purpose | Status | JSON Support |
|---------|---------|--------|--------------|
| `aws` | AWS EC2 operations | ✅ Primary | ⚠️ Partial |
| `resources` | Cross-platform resource management | ✅ Good | ✅ Yes |
| `checkpoint` | Checkpoint operations | ✅ Good | ❌ No |
| `s3` | S3 operations | ✅ Good | ❌ No |
| `monitor` | Training monitoring | ✅ Good | ❌ N/A (streaming) |
| `local` | Local training | ✅ Good | ❌ No |
| `runpod` | RunPod operations | ⚠️ Experimental | ❌ No |
| `status` | Quick status overview | ⚠️ Unclear | ⚠️ Partial |
| `transfer` | Data transfer | ⚠️ Overlaps with s3 | ❌ No |
| `dashboard` | Interactive dashboard | ✅ New | ❌ N/A (TUI) |
| `init` | Initialize configuration | ✅ Good | ❌ N/A |

### Command Organization Issues

#### 1. Overlapping Functionality

**Problem**: `s3` and `transfer` commands overlap:
- `s3 upload` vs `transfer <local> <s3://bucket/key>`
- `s3 download` vs `transfer <s3://bucket/key> <local>`
- `s3 sync` vs `transfer` with sync options

**Impact**: Users confused about which to use

**Recommendation**:
- Document when to use each
- Or consolidate: make `transfer` the primary, `s3` as convenience aliases
- Or: `s3` for S3-specific operations, `transfer` for cross-platform

#### 2. Platform-Specific vs Generic

**Problem**: Mix of platform-specific and generic commands:
- `aws create` vs `resources list` (AWS-specific vs generic)
- `aws train` vs `local train` (platform-specific training)
- `aws monitor` vs `monitor` (overlapping)

**Current State**: 
- ✅ Works but inconsistent
- ⚠️ No clear pattern

**Recommendation**: 
- Keep current structure (it's intuitive)
- But improve consistency in flags and output

#### 3. Status Command Ambiguity

**Problem**: `runctl status` purpose unclear:
- What does it show vs `resources list`?
- What does it show vs `resources summary`?
- Is it a quick summary or detailed?

**Current Implementation**: Calls `resources::show_quick_status()`

**Recommendation**:
- Make it truly "quick" (1-2 lines: "3 instances running, $12.50/hour")
- Keep `resources list` for detailed view
- Keep `resources summary` for cost breakdown

## Output Format Consistency

### Current State

#### ✅ Commands with JSON Support
- `resources list --output json` - Full JSON support
- `resources summary --output json` - Full JSON support
- `status --output json` - Partial (uses resources summary)

#### ⚠️ Commands with Partial JSON
- `aws create` - Returns instance ID but not structured JSON
- `aws train` - Returns status but not structured JSON

#### ❌ Commands without JSON
- `aws stop` - Text only
- `aws terminate` - Text only
- `aws monitor` - Text only (streaming)
- `aws processes` - Text only
- `checkpoint list` - Text only
- `checkpoint info` - Text only
- `s3 upload` - Text only
- `s3 download` - Text only
- `transfer` - Text only
- `local train` - Text only

### Issues

1. **Inconsistent JSON Structure**
   - `resources list` returns: `{"aws": {"instances": [...]}}`
   - `resources summary` returns: `{"aws_instances": [...], "total_cost_estimate": ...}`
   - No standard format

2. **Error Output Not JSON**
   - Errors always text, even with `--output json`
   - Python wrapper can't parse errors

3. **Missing JSON for Common Operations**
   - `aws create` - Most commonly used programmatically
   - `aws train` - Needed for automation
   - `checkpoint list` - Useful for checkpoint management

### Recommendations

**High Priority**:
1. Add JSON output to `aws create`:
   ```json
   {
     "success": true,
     "instance_id": "i-123",
     "instance_type": "g4dn.xlarge",
     "public_ip": "1.2.3.4",
     "cost_per_hour": 0.526
   }
   ```

2. Add JSON output to `aws train`:
   ```json
   {
     "success": true,
     "training_started": true,
     "log_path": "/home/ubuntu/project/training.log",
     "pid": 12345
   }
   ```

3. Standardize JSON structure across all commands

**Medium Priority**:
4. Add JSON to `checkpoint list`
5. Add JSON to `aws stop/terminate`
6. Make errors JSON when `--output json`

## Error Handling and Messages

### Current State

**Good**:
- Uses `TrainctlError` for structured errors
- Some commands have helpful suggestions (e.g., `local train` suggests similar files)

**Issues**:
1. **Inconsistent Error Format**
   - Some use `anyhow::bail!` (text only)
   - Some use `TrainctlError` (structured)
   - No consistent format

2. **Missing Suggestions**
   - Most errors don't suggest fixes
   - No "Did you mean...?" for typos
   - No common error patterns documented

3. **Error Messages Not User-Friendly**
   - Technical error messages (e.g., AWS SDK errors)
   - Missing context (what was the user trying to do?)
   - No recovery suggestions

### Recommendations

1. **Standardize Error Format**
   ```rust
   // Always use TrainctlError in library code
   // Convert to user-friendly messages at CLI boundary
   ```

2. **Add Error Suggestions**
   - "Instance not found" → "Did you mean: i-123...?"
   - "Permission denied" → "Check AWS credentials: aws sts get-caller-identity"
   - "SSM not ready" → "Wait 30 seconds and try again"

3. **Improve Error Context**
   - Show what command was being executed
   - Show relevant parameters
   - Show recovery steps

## Command Combinations and Workflows

### Common Workflows

#### 1. Create and Train (Most Common)

**Current**:
```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')
runctl aws train $INSTANCE_ID training/train.py --sync-code
```

**Issues**:
- Requires parsing instance ID from text output
- Two separate commands
- No single command to do both

**Recommendation**:
- Add `--auto-train` flag to `aws create`:
  ```bash
  runctl aws create --spot --instance-type g4dn.xlarge \
    --auto-train training/train.py \
    --sync-code
  ```
- Or improve JSON output so instance ID is easily extractable

#### 2. Monitor Training

**Current**:
```bash
runctl aws train $INSTANCE_ID training/train.py
runctl aws monitor $INSTANCE_ID --follow
```

**Issues**:
- Monitor doesn't auto-detect instance logs
- Requires knowing instance ID
- No way to monitor multiple instances

**Recommendation**:
- Add `--monitor` flag to `aws train` to auto-start monitoring
- Add `resources monitor` to monitor all instances

#### 3. Checkpoint Management

**Current**:
```bash
runctl checkpoint list checkpoints/
runctl checkpoint upload i-123 checkpoints/latest.pt
runctl checkpoint download checkpoint-id
```

**Issues**:
- No automatic checkpoint detection
- No integration with training (auto-upload)
- Manual checkpoint management

**Recommendation**:
- Add `--auto-upload` to `aws train` for checkpoint auto-upload
- Add `checkpoint sync <instance-id>` to auto-detect and upload
- Add `checkpoint resume <checkpoint-id> --instance <new-instance>`

#### 4. Resource Management

**Current**:
```bash
runctl resources list
runctl resources stop-all
runctl resources cleanup
```

**Issues**:
- `stop-all` vs `cleanup` distinction unclear
- No filtering by project/user in stop-all
- No cost warnings before stop-all

**Recommendation**:
- Clarify: `stop-all` = pause instances, `cleanup` = delete orphaned resources
- Add `--project` filter to `stop-all`
- Add cost warning: "Stopping 5 instances will save $X/hour"

#### 5. Data Transfer

**Current**:
```bash
# Option 1: s3 command
runctl s3 upload local/ s3://bucket/data/

# Option 2: transfer command
runctl transfer local/ s3://bucket/data/
```

**Issues**:
- Overlap between `s3` and `transfer`
- When to use which?
- No clear examples

**Recommendation**:
- Document: Use `s3` for S3-specific operations, `transfer` for cross-platform
- Or consolidate into single command
- Add examples to help text

### Missing Workflow Features

1. **Quick Start Command**
   - Single command: create + train + monitor
   - `runctl aws quick-start --instance-type g4dn.xlarge --script train.py`

2. **Batch Operations**
   - Stop all instances in a project
   - Cleanup old resources
   - Cost analysis by project

3. **Checkpoint Auto-Sync**
   - Auto-upload checkpoints during training
   - Auto-download on instance termination
   - Resume from latest checkpoint

## Output Formatting Issues

### Text Output

**Issues**:
1. **Inconsistent Table Format**
   - Some use `comfy-table`, others use manual formatting
   - Column widths vary
   - Headers sometimes missing

2. **Missing Information**
   - `resources list` doesn't show costs by default
   - `checkpoint list` doesn't show sizes
   - `aws processes` doesn't show GPU usage

3. **No Progress Indicators**
   - `aws create` - Silent during creation
   - `s3 upload` - Has progress bar (good!)
   - `transfer` - No progress indicator
   - `aws train` - No progress for code sync

### Recommendations

1. **Standardize Table Format**
   - Use consistent column widths
   - Always show headers
   - Use consistent separators

2. **Add Progress Indicators**
   - Show progress for long operations
   - Use `--quiet` flag to suppress
   - Show ETA for transfers

3. **Improve Information Density**
   - Show costs in `resources list`
   - Show sizes in `checkpoint list`
   - Show GPU usage in `aws processes`

## Help Text Quality

### Current State

**Good**:
- Most commands have help text
- Some have examples (e.g., `aws create`)

**Issues**:
1. **Missing Examples**
   - Most commands lack examples
   - No "See also" references
   - No common use cases

2. **Inconsistent Detail**
   - Some commands very detailed (e.g., `aws create`)
   - Others minimal (e.g., `checkpoint list`)

3. **No Workflow Documentation**
   - Help text doesn't show how commands work together
   - No "Common workflows" section

### Recommendations

1. **Add Examples to All Commands**
   ```rust
   /// Examples:
   ///   runctl aws create g4dn.xlarge --spot
   ///   runctl aws create p3.2xlarge --data-volume-size 500
   ```

2. **Add "See Also" Sections**
   - Link related commands
   - Show workflow examples

3. **Improve Descriptions**
   - More context about when to use
   - Common pitfalls
   - Performance considerations

## Configuration Management

### Current State

**Commands**:
- `runctl init` - Creates `.runctl.toml`

**Issues**:
1. **No Config Viewing**
   - No `runctl config show`
   - No way to see effective config
   - No config validation

2. **Config File Location Unclear**
   - Current dir vs `~/.config/runctl/config.toml`
   - Precedence not documented

3. **No Config Editing**
   - Must edit file manually
   - No `runctl config set <key> <value>`

### Recommendations

1. **Add Config Commands**
   ```bash
   runctl config show          # Show effective config
   runctl config set aws.region us-west-2
   runctl config validate      # Check config is valid
   ```

2. **Document Config Precedence**
   - Command-line flags > `.runctl.toml` > `~/.config/runctl/config.toml` > defaults

3. **Add Config Validation**
   - Validate on load
   - Show helpful error messages
   - Suggest fixes

## Specific Command Issues

### `aws create`

**Good**:
- ✅ Comprehensive options
- ✅ Auto-detects Deep Learning AMI
- ✅ Good help text with examples
- ✅ Safety checks (warns if many instances)

**Issues**:
- ❌ No progress indicator
- ❌ No JSON output
- ❌ Instance ID extraction requires parsing
- ❌ No `--wait` flag to wait for ready state

**Recommendations**:
- Add progress indicator
- Add JSON output with instance details
- Add `--wait` flag
- Add `--json` alias for `--output json`

### `aws train`

**Good**:
- ✅ Code sync works well
- ✅ S3 data transfer
- ✅ Background execution

**Issues**:
- ❌ No checkpoint auto-upload
- ❌ No training status polling
- ❌ No auto-monitor option
- ❌ No JSON output

**Recommendations**:
- Add `--auto-upload-checkpoints`
- Add `--monitor` flag
- Add JSON output
- Add training status endpoint

### `resources list`

**Good**:
- ✅ Multiple platforms
- ✅ Good filtering options
- ✅ JSON support
- ✅ Watch mode

**Issues**:
- ❌ Costs not shown by default
- ❌ No sort options in help
- ❌ No export to CSV
- ❌ Format options unclear

**Recommendations**:
- Show costs by default
- Add sort examples to help
- Add CSV export
- Clarify format options

### `checkpoint list`

**Good**:
- ✅ Lists checkpoints
- ✅ Sorted by date

**Issues**:
- ❌ No size information
- ❌ No age information
- ❌ No filtering options
- ❌ No JSON output

**Recommendations**:
- Show checkpoint size
- Show checkpoint age
- Add `--filter` option
- Add JSON output

### `s3 upload`

**Good**:
- ✅ Progress bar
- ✅ s5cmd optimization
- ✅ Recursive support

**Issues**:
- ❌ No resume for interrupted uploads
- ❌ No parallel uploads for directories
- ❌ No checksum verification
- ❌ No JSON output

**Recommendations**:
- Add resume capability
- Add parallel uploads
- Add checksum verification
- Add JSON output for status

### `transfer`

**Issues**:
- ❌ Unclear when to use vs `s3`
- ❌ No examples in help
- ❌ No progress for large transfers
- ❌ Options not well documented

**Recommendations**:
- Add examples to help
- Clarify vs `s3` commands
- Add progress indicator
- Document all options

### `status`

**Issues**:
- ❌ Purpose unclear
- ❌ What does it show?
- ❌ How is it different from `resources list`?

**Current Implementation**: Calls `resources::show_quick_status()`

**Recommendation**:
- Make it truly "quick" (1-2 lines)
- Show: running count, total cost, recent checkpoints
- Keep detailed views in `resources list`

## Workflow Improvements

### Suggested New Commands

1. **`runctl aws quick-start`**
   ```bash
   runctl aws quick-start \
     --instance-type g4dn.xlarge \
     --script training/train.py \
     --data-s3 s3://bucket/data/ \
     --monitor
   ```
   - Create instance + train + monitor in one command

2. **`runctl resources cost`**
   ```bash
   runctl resources cost
   runctl resources cost --project my-project
   runctl resources cost --by-user
   ```
   - Show cost breakdown
   - Projected costs
   - Cost by project/user

3. **`runctl checkpoint auto`**
   ```bash
   runctl checkpoint auto --enable
   runctl checkpoint auto --interval 300  # Upload every 5 min
   ```
   - Enable auto-upload
   - Configure intervals
   - Monitor and upload

4. **`runctl config`**
   ```bash
   runctl config show
   runctl config set aws.region us-west-2
   runctl config validate
   ```
   - View/edit configuration
   - Show effective config
   - Validate config

### Suggested Command Enhancements

1. **Batch Operations**
   ```bash
   runctl resources stop-all --project myproject
   runctl resources cleanup --older-than 24h
   runctl resources cost --project myproject
   ```

2. **Checkpoint Workflow**
   ```bash
   runctl checkpoint sync <instance-id>  # Auto-detect and upload
   runctl checkpoint resume <checkpoint-id> --instance <new-instance>
   ```

3. **Auto-Monitor**
   ```bash
   runctl aws train <instance-id> <script> --monitor
   ```

## Consistency Issues

### Naming Conventions

**Good**:
- ✅ `--instance-type`, `--instance-id` (consistent)
- ✅ `--force` for destructive operations (consistent)

**Issues**:
- ⚠️ `--script` vs `--file` (inconsistent)
- ⚠️ `--data-s3` vs `--s3-data` (inconsistent)
- ⚠️ `--sync-code` vs `--no-sync-code` (double negative)

**Recommendations**:
- Standardize: always use `--script` for training scripts
- Standardize: always use `--data-s3` for S3 data paths
- Change: `--sync-code` to `--no-sync-code` (default: sync)

### Flag Patterns

**Good**:
- ✅ `--force` for destructive operations
- ✅ `--dry-run` for preview

**Missing**:
- ❌ `--yes` for confirmations
- ❌ `--quiet` for less output
- ❌ `--json` alias for `--output json`

**Recommendations**:
- Add `--yes` flag
- Add `--quiet` flag
- Add `--json` alias

### Output Patterns

**Issues**:
- ⚠️ Inconsistent: Some commands show tables, others show lists
- ⚠️ Inconsistent: Some show costs, others don't
- ⚠️ Inconsistent: JSON format varies

**Recommendations**:
- Standardize table format
- Always show costs in resource lists
- Standardize JSON structure

## Testing UX

### Missing UX Tests

1. **Error Message Clarity**
   - Are errors helpful?
   - Do they suggest fixes?
   - Are they actionable?

2. **Command Discoverability**
   - Can users find commands?
   - Is help text useful?
   - Are examples clear?

3. **Workflow Completeness**
   - Do commands work together?
   - Are workflows smooth?
   - Are there gaps?

4. **Output Readability**
   - Is output easy to parse?
   - Is JSON consistent?
   - Are tables readable?

### Suggested UX Tests

1. **New User Test**
   - Can a new user complete common tasks?
   - How long does it take?
   - What errors do they hit?

2. **Error Recovery Test**
   - Can users recover from errors?
   - Are error messages helpful?
   - Do suggestions work?

3. **Workflow Test**
   - Do command combinations work smoothly?
   - Are there missing steps?
   - Are there redundant steps?

4. **Output Parsing Test**
   - Is JSON output consistent?
   - Can it be parsed reliably?
   - Are edge cases handled?

## Priority Recommendations

### High Priority (Immediate)

1. **Fix JSON Output Consistency**
   - Add JSON to `aws create`
   - Add JSON to `aws train`
   - Standardize JSON structure
   - Make errors JSON when `--output json`

2. **Improve Error Messages**
   - Add suggestions
   - Add context
   - Make errors actionable

3. **Clarify Command Overlap**
   - Document `s3` vs `transfer`
   - Document `status` vs `resources list`
   - Add examples

4. **Add Progress Indicators**
   - `aws create` - Show progress
   - `aws train` - Show code sync progress
   - `transfer` - Show transfer progress

### Medium Priority (Short-term)

1. **Add Config Commands**
   - `runctl config show`
   - `runctl config set`
   - `runctl config validate`

2. **Improve Help Text**
   - Add examples to all commands
   - Add "See also" sections
   - Add workflow examples

3. **Enhance Workflows**
   - Add `--auto-train` to `aws create`
   - Add `--monitor` to `aws train`
   - Add checkpoint auto-upload

4. **Add Missing JSON**
   - `checkpoint list`
   - `aws stop/terminate`
   - `s3` commands

### Low Priority (Long-term)

1. **New Commands**
   - `runctl aws quick-start`
   - `runctl resources cost`
   - `runctl checkpoint auto`

2. **Advanced Features**
   - Batch operations
   - Multi-instance monitoring
   - Cost projections

3. **UX Polish**
   - Better table formatting
   - More progress indicators
   - Better error recovery

## Conclusion

runctl is a functional and powerful tool, but has several UX inconsistencies that could be improved. The highest priority items are:

1. **JSON output consistency** - Critical for programmatic use
2. **Error message quality** - Critical for user experience
3. **Command clarity** - Important for discoverability
4. **Workflow completeness** - Important for efficiency

Most issues are straightforward to fix and would significantly improve the user experience.
