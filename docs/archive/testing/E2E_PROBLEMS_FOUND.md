# E2E Problems Found: Real-World Issues

## Testing Methodology

After implementing the improvements (--wait flags, structured output, workflow commands), I analyzed the code paths and identified common problems that will occur in real usage.

## Critical Issues Found

### 1. SSM Readiness False Positives

**Problem**: `--wait` flag says "Instance ready and SSM connected" but SSM may not actually be ready.

**Root Cause**: 
- `wait_for_instance_running()` waits 30 seconds after instance is "running"
- But doesn't verify SSM connectivity
- SSM can take 60-90 seconds to be ready
- Training commands fail immediately after `--wait` completes

**Impact**:
- Users think instance is ready when it's not
- Training fails with confusing SSM errors
- Poor developer experience

**Fix Applied**: 
- Modified `wait_for_instance_running()` to actually test SSM connectivity
- Retries SSM command up to 20 times (60 seconds max)
- More reliable than fixed delay

### 2. Training Script Path Issues

**Problem**: Scripts in subdirectories fail with "Script not under project root".

**Root Cause**:
- Project root detection may find wrong directory
- Script path resolution fails if script not under detected root
- Error message doesn't show what was detected

**Example Failure**:
```bash
$ runctl aws train i-123 training/train_mnist_e2e.py
Error: Script training/train_mnist_e2e.py is not under project root /Users/arc/Documents/dev
```

**Impact**:
- Common case (scripts in subdirectories) fails
- Users don't understand why
- No guidance on how to fix

**Recommendation**:
- Show detected project root in error
- Suggest using absolute path if detection fails
- Validate before syncing code

### 3. Dependency Installation Blocks Training

**Problem**: Training appears to hang for 5-10 minutes while installing dependencies.

**Root Cause**:
- `requirements.txt` triggers pip install before training
- Large packages (torch ~2GB) take time to download
- No progress indication
- Blocks training start

**Impact**:
- Poor UX: appears frozen
- No way to cancel
- Users think something is broken

**Current Behavior**:
- Installation happens synchronously
- No output during installation
- Training waits for completion

**Recommendation**:
- Stream pip output in real-time
- Make dependency installation optional
- Support pre-warmed AMIs

### 4. Spot Instance Fallback Surprises

**Problem**: Users request spot instances but get on-demand without warning.

**Root Cause**:
- Spot request fails → silently falls back to on-demand
- No indication of fallback
- Costs 3-10x more than expected

**Impact**:
- Unexpected costs
- No way to know actual instance type
- Users think they got spot pricing

**Current Behavior**:
- Prints "WARNING: Spot instance failed: ..."
- Prints "Falling back to on-demand..."
- But easy to miss in output

**Recommendation**:
- Make fallback more prominent
- Show cost difference
- Add `--fail-if-no-spot` flag

### 5. Training Completion False Positives

**Problem**: `--wait` returns success when training actually failed.

**Root Cause**:
- Checks for `training_complete.txt` marker
- But script may create marker even on failure
- Doesn't check exit code
- Doesn't verify checkpoints were created

**Impact**:
- Users think training succeeded when it failed
- Wasted compute time
- No indication of actual failure

**Current Behavior**:
- Checks marker file, PID status, log content
- But doesn't verify training actually succeeded
- No exit code checking

**Recommendation**:
- Check training script exit code
- Verify checkpoints were created
- Add `--verify-success` flag

### 6. Code Sync Failures Are Silent

**Problem**: Code sync fails but training starts anyway (with missing files).

**Root Cause**:
- Sync errors are logged but don't fail training
- Training starts even if sync incomplete
- No verification that files were synced

**Impact**:
- Training fails with "file not found"
- Users don't know sync failed
- Confusing error messages

**Current Behavior**:
- Sync errors return error, but training may continue
- No verification of what was synced
- Error messages don't suggest checking sync

**Recommendation**:
- Verify critical files after sync
- Show what was synced
- Fail training if sync fails

### 7. Instance State Not Validated

**Problem**: Commands fail on instances in wrong state (stopped, terminating).

**Root Cause**:
- No state validation before operations
- Commands assume instance is running
- Errors are unclear about state issues

**Impact**:
- Confusing errors
- No guidance on how to fix
- Users try same command repeatedly

**Current Behavior**:
- Some commands check state, others don't
- Inconsistent error messages
- No suggestions for state issues

**Recommendation**:
- Validate state before all operations
- Suggest appropriate actions (start, wait, etc.)
- Show current state in errors

## Workflow-Specific Issues

### Workflow Train Command Issues

**Problem**: `runctl workflow train` may fail in several ways:

1. **Instance creation fails** → No cleanup, user left with partial state
2. **Training fails** → Instance still running, accumulating costs
3. **No way to resume** → Must start over if workflow fails mid-way

**Current Behavior**:
- Workflow creates instance, trains, but doesn't handle failures well
- No cleanup on failure
- No way to resume

**Recommendation**:
- Add cleanup on failure
- Support `--resume` flag to continue from failure point
- Better error handling and rollback

## Edge Cases

### 1. Project Root Detection

**Problems**:
- Multiple markers in nested directories → finds wrong root
- Script outside project → fails
- No markers → falls back to script directory (may be wrong)

**Impact**: Code syncs to wrong location, training can't find script

### 2. Script Arguments

**Problems**:
- Arguments with spaces not quoted
- Special characters break parsing
- Arguments that look like runctl flags

**Impact**: Training fails with argument parsing errors

### 3. Concurrent Operations

**Problems**:
- Multiple training jobs on same instance
- Code sync while training running
- Instance termination during sync

**Impact**: Unpredictable behavior, possible data corruption

## Recommendations

### Immediate Fixes Needed

1. **SSM Readiness Verification** ✅ FIXED
   - Now actually tests SSM connectivity
   - More reliable than fixed delay

2. **Better Error Messages**
   - Show detected project root
   - Explain what failed and why
   - Provide actionable next steps

3. **Training Completion Validation**
   - Check exit code
   - Verify checkpoints
   - Detect hung training

### Medium Priority

4. **Progress Indication**
   - Stream dependency installation
   - Show sync progress
   - Indicate training is progressing

5. **State Validation**
   - Check instance state before operations
   - Suggest fixes
   - Prevent wrong-state operations

6. **Argument Handling**
   - Proper quoting/escaping
   - Support complex arguments
   - Clear separation of args

### Testing Needed

1. Test with actual AWS account
2. Test spot instance fallback scenarios
3. Test SSM readiness edge cases
4. Test training completion detection
5. Test code sync edge cases
6. Test argument parsing edge cases

