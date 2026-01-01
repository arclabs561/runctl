# Critique of runctl Examples and Usage Patterns

This document provides a critical analysis of current examples and suggests improvements to prevent common mistakes.

## Critical Analysis

### Current Strengths ✅

1. **State Validation**: Excellent - prevents training on stopped instances
2. **Error Messages**: Clear and actionable
3. **Checkpoint Saving**: Automatic on stop
4. **Cost Tracking**: Built-in resource tracking
5. **SSM Validation**: Checks availability before use

### Current Weaknesses ⚠️

1. **No Training Detection**: Can start multiple training jobs on same instance
2. **No Default Checkpoint Interval**: Users must remember to specify
3. **No Cost Warnings**: Instances can run for days unnoticed
4. **No Terminate Confirmation**: Easy to accidentally delete resources
5. **Sync Code Flag Confusion**: `--sync-code=false` doesn't work as expected

## Footgun Analysis

### ✅ Prevented (8/10)

1. **Training on Stopped Instance** - ✅ Clear error with resolution
2. **Wrong Script Path** - ✅ Validates before syncing
3. **SSM Not Configured** - ✅ Checks and provides guidance
4. **Wrong Instance ID** - ✅ Format validation
5. **Code Sync** - ✅ Defaults to true
6. **Checkpoint Loss on Stop** - ✅ Auto-saves on stop
7. **Script Arguments** - ✅ Help text explains `--` separator
8. **Instance State** - ✅ Validates before operations

### ⚠️ Partially Prevented (2/10)

9. **Forgetting Checkpoints** - ⚠️ No default interval, no warnings
10. **Cost Accumulation** - ⚠️ Tracking exists but no warnings

## Edge Cases

### ✅ Handled Well

- Rapid state changes
- Training script early exit
- Checkpoint directory creation
- State validation during transitions

### ⚠️ Needs Improvement

- **Concurrent Training**: No detection of existing training
- **Cost Warnings**: No alerts for high costs
- **Terminate Safety**: No confirmation for destructive operations

## Critique of Training Script Example

### `training/train_with_checkpoints.py`

**Strengths**:
- ✅ Good checkpoint resume logic
- ✅ Signal handling for graceful shutdown
- ✅ Clear output and error handling
- ✅ Auto-resume from checkpoint directory

**Weaknesses**:
- ⚠️ Default checkpoint interval is 2 epochs (could be 1)
- ⚠️ No progress indicators for long training
- ⚠️ No validation of checkpoint format
- ⚠️ Hardcoded checkpoint directory default
- ⚠️ No warning if training long without checkpoints

**Suggestions**:
```python
# Better defaults
parser.add_argument("--checkpoint-interval", type=int, default=1, 
                   help="Save checkpoint every N epochs (default: 1)")

# Add progress indicator
if epochs > 5:
    from tqdm import tqdm
    epoch_iter = tqdm(range(start_epoch, total_epochs))
else:
    epoch_iter = range(start_epoch, total_epochs)

# Warn if long training without checkpoints
if checkpoint_interval > 5:
    print(f"WARNING: Checkpoint interval is {checkpoint_interval} epochs.")
    print("Consider smaller interval for long training runs.")
```

## Critique of Usage Examples

### Current Examples

**Good**:
- ✅ Show `--` separator usage
- ✅ Demonstrate common patterns
- ✅ Include wait flags

**Missing**:
- ⚠️ Don't emphasize safety features
- ⚠️ Don't show cost considerations
- ⚠️ Don't warn about common mistakes
- ⚠️ Don't show checkpoint best practices

### Improved Examples

**Before**:
```bash
runctl aws train i-xxx script.py --sync-code -- --epochs 10
```

**After** (with safety emphasis):
```bash
# ✅ GOOD: Includes checkpoint interval and wait flag
runctl aws train i-xxx script.py \
  --sync-code --wait \
  -- --epochs 10 --checkpoint-interval 2

# ⚠️ BAD: No checkpoint interval, no wait flag
# runctl aws train i-xxx script.py --sync-code -- --epochs 10
```

## Specific Improvements Needed

### 1. Training Detection ⚠️ HIGH PRIORITY

**Problem**: Can start multiple training jobs on same instance

**Current Behavior**:
- No check for existing training
- Both jobs start, may conflict

**Suggested Fix**:
```rust
// In train_on_instance, before starting training:
let check_training_cmd = format!(
    "if [ -f {}/training.pid ]; then \
     PID=$(cat {}/training.pid 2>/dev/null); \
     if ps -p $PID > /dev/null 2>&1; then \
         echo 'TRAINING_RUNNING:$PID'; \
     else \
         echo 'NO_TRAINING'; \
     fi; \
     else \
     echo 'NO_TRAINING'; \
     fi",
    project_dir, project_dir
);

match execute_ssm_command(&ssm_client, &options.instance_id, &check_training_cmd).await {
    Ok(output) => {
        if output.contains("TRAINING_RUNNING") {
            return Err(TrainctlError::Aws(format!(
                "Training already running on instance {}. \
                To start new training, either:\n\
                  1. Wait for current training to complete\n\
                  2. Stop current training: runctl aws stop {}\n\
                  3. Use --force to kill existing training (not recommended)",
                options.instance_id, options.instance_id
            )));
        }
    }
    Err(_) => {
        // If check fails, proceed (might be first training)
    }
}
```

### 2. Default Checkpoint Interval ⚠️ HIGH PRIORITY

**Problem**: Users forget to specify checkpoint interval

**Current Behavior**:
- No default, must specify `--checkpoint-interval`

**Suggested Fix**:
- Add default checkpoint interval (e.g., every 2 epochs)
- Warn if training > 1 hour without checkpoints
- Make checkpoint saving more automatic

### 3. Cost Warnings ⚠️ MEDIUM PRIORITY

**Problem**: Instances run for days, huge bills

**Current Behavior**:
- Cost tracking exists but no warnings

**Suggested Fix**:
```rust
// In resources list command:
if uptime_hours > 24 {
    println!("⚠️  WARNING: Instance {} has been running for {} hours (${:.2} total cost)",
             instance_id, uptime_hours, total_cost);
}

if total_cost > 10.0 {
    println!("⚠️  WARNING: Instance {} has accumulated ${:.2} in costs",
             instance_id, total_cost);
}
```

### 4. Terminate Confirmation ⚠️ MEDIUM PRIORITY

**Problem**: Easy to accidentally terminate instances

**Current Behavior**:
- No confirmation required

**Suggested Fix**:
```rust
// Before terminate:
if has_checkpoints {
    println!("⚠️  WARNING: Instance has checkpoints. Terminating will lose checkpoints.");
    println!("   Consider using 'stop' instead to preserve checkpoints.");
    if !force {
        // Require confirmation
    }
}
```

### 5. Better Sync Code Flag ⚠️ LOW PRIORITY

**Problem**: `--sync-code=false` doesn't work

**Current Behavior**:
- Flag exists but syntax is confusing

**Suggested Fix**:
- Add `--no-sync-code` flag
- Or make `--sync-code` accept boolean: `--sync-code=false`

## Recommendations Summary

### Must Have (High Priority)

1. ✅ **Training Detection**: Check for existing training before starting
2. ✅ **Default Checkpoint Interval**: Make checkpoint saving automatic
3. ✅ **Cost Warnings**: Alert on high costs or long uptime

### Should Have (Medium Priority)

4. ✅ **Terminate Confirmation**: Warn before destructive operations
5. ✅ **Better Defaults**: Make `--wait` default for interactive use
6. ✅ **Progress Indicators**: Show progress for long training

### Nice to Have (Low Priority)

7. ✅ **Auto-detect Missing `--`**: Warn if separator missing
8. ✅ **Training Validation**: Check script syntax before running
9. ✅ **Checkpoint Format Validation**: Verify checkpoint integrity

## Example Improvements

### Before (Current)

```bash
# Basic training
runctl aws train i-xxx script.py --sync-code -- --epochs 10

# With checkpoints
runctl aws train i-xxx script.py --sync-code -- --epochs 10 --checkpoint-interval 2
```

### After (Improved)

```bash
# Basic training (with safety defaults)
runctl aws train i-xxx script.py \
  --sync-code --wait \
  -- --epochs 10 --checkpoint-interval 1

# Long training (with explicit safety)
runctl aws train i-xxx script.py \
  --sync-code --wait \
  -- --epochs 100 --checkpoint-interval 5

# Check costs before long training
runctl resources list --platform aws
```

## Conclusion

**Overall Assessment**: ✅ **GOOD** - Prevents most common mistakes

**Score**: 8/10 footguns prevented, 4/5 edge cases handled

**Key Improvements Needed**:
1. Training detection (prevent concurrent training)
2. Default checkpoint interval
3. Cost warnings
4. Terminate confirmation

The tool is solid but could be more proactive about safety. Most issues are about defaults and warnings rather than core functionality.
