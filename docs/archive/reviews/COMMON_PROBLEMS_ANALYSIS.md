# Common Problems Analysis: E2E Workflow Issues

## Research Methodology

This document analyzes common problems that will occur when using runctl workflows based on:
1. Code review of error handling paths
2. Example workflows and their failure modes
3. Edge cases in instance creation, training, and monitoring
4. Real-world usage patterns

## Critical Failure Modes

### 1. Instance Creation Failures

#### Problem: Spot Instance Capacity Issues
**When it happens:**
- High demand for spot instances
- Requested instance type unavailable in region
- Spot price exceeds on-demand price

**Current behavior:**
- Falls back to on-demand (unless `--no-fallback`)
- Silent fallback can surprise users expecting spot pricing

**User impact:**
- Unexpected costs (on-demand is 3-10x more expensive)
- No warning about fallback
- No way to know actual instance type until after creation

**Recommendation:**
- Add `--fail-if-no-spot` flag to prevent silent fallback
- Show pricing comparison before creation
- Warn when fallback occurs

#### Problem: SSM Not Ready After Instance Creation
**When it happens:**
- Instance reaches "running" state but SSM agent not ready
- IAM instance profile attached but SSM agent not started
- Network connectivity issues to SSM endpoints

**Current behavior:**
- `--wait` flag waits for instance running + 30 seconds
- But SSM may take 60-90 seconds to be ready
- Training fails with unclear error

**User impact:**
- Training commands fail immediately after `--wait` completes
- Confusing error messages about SSM connectivity
- Users think instance is ready when it's not

**Recommendation:**
- `--wait` should verify SSM connectivity, not just instance state
- Add `runctl aws wait` command that checks SSM specifically
- Better error messages explaining SSM readiness

### 2. Training Execution Failures

#### Problem: Script Path Resolution
**When it happens:**
- Script in subdirectory (e.g., `training/train_mnist_e2e.py`)
- Project root detection finds wrong directory
- Script path doesn't match synced code structure

**Current behavior:**
- Calculates relative path from project root
- Fails if script not under detected project root
- Error message unclear about what went wrong

**User impact:**
- Training fails with "Script not under project root"
- Users don't understand why path resolution failed
- No guidance on how to fix

**Recommendation:**
- Show detected project root in error messages
- Suggest using absolute paths if detection fails
- Validate script path before syncing code

#### Problem: Dependency Installation Blocks Training
**When it happens:**
- `requirements.txt` exists with large packages (torch, tensorflow)
- Installation takes 5-10 minutes
- No progress indication

**Current behavior:**
- Blocks training start until dependencies installed
- No output during installation
- Training appears to hang

**User impact:**
- Poor UX: appears frozen for 5-10 minutes
- No way to cancel or check progress
- Users think something is broken

**Recommendation:**
- Stream pip output in real-time
- Make dependency installation optional (let training script handle it)
- Support pre-warmed AMIs with dependencies

#### Problem: Training Script Not Found on Instance
**When it happens:**
- Code sync fails silently
- Script path incorrect after sync
- Project structure not preserved

**Current behavior:**
- Training command fails with "file not found"
- No indication that code sync may have failed
- Error doesn't suggest checking if files were synced

**User impact:**
- Confusing error: script exists locally but not on instance
- No way to verify what was actually synced
- Users don't know if sync failed or path is wrong

**Recommendation:**
- Verify script exists on instance before starting training
- Show what files were synced
- Better error messages with troubleshooting steps

### 3. Training Completion Detection Issues

#### Problem: False Positives in Completion Detection
**When it happens:**
- Training script crashes but creates `training_complete.txt` anyway
- Process dies but PID file still exists
- Log contains "COMPLETE" but training actually failed

**Current behavior:**
- Checks multiple heuristics (marker file, PID, log)
- But doesn't verify training actually succeeded
- No exit code checking

**User impact:**
- `--wait` returns success when training actually failed
- Users think training succeeded when it didn't
- Wasted compute time

**Recommendation:**
- Check training script exit code
- Verify checkpoints were actually created
- Add `--verify-success` flag that does deeper validation

#### Problem: Training Never Completes (Hangs)
**When it happens:**
- Training script has infinite loop
- Script waiting for input
- Resource exhaustion (OOM, disk full)

**Current behavior:**
- `--wait` times out after 2 hours
- No indication of what's wrong
- Instance keeps running, accumulating costs

**User impact:**
- Costs continue even though training isn't progressing
- No way to detect hung training
- Manual intervention required

**Recommendation:**
- Add progress detection (checkpoints should update periodically)
- Timeout with clear message
- Option to kill hung training automatically

### 4. Code Sync Failures

#### Problem: SSM Sync Requires S3 Bucket
**When it happens:**
- Instance has IAM profile but no S3 bucket configured
- S3 bucket doesn't exist or wrong permissions
- SSM sync fails, falls back to SSH

**Current behavior:**
- Falls back to SSH if SSM sync fails
- But SSH may not be configured (no key, no public IP)
- Error message unclear about what failed

**User impact:**
- Sync fails with confusing error
- Users don't know if they need S3 bucket or SSH key
- No clear path forward

**Recommendation:**
- Validate S3 bucket before attempting SSM sync
- Clear error messages explaining requirements
- Suggest alternatives (SSH key, or configure S3)

#### Problem: Large Codebases Timeout
**When it happens:**
- Project has large files (data, models, checkpoints)
- Sync takes longer than timeout
- Partial sync leaves instance in broken state

**Current behavior:**
- Sync may timeout
- No indication of what was synced
- Training starts with incomplete code

**User impact:**
- Training fails with missing files
- No way to know what was actually synced
- Users think code is there when it's not

**Recommendation:**
- Show sync progress
- Resume interrupted syncs
- Verify critical files after sync

### 5. Instance State Issues

#### Problem: Instance Terminated During Training
**When it happens:**
- Spot instance interrupted
- Manual termination
- Account limits exceeded

**Current behavior:**
- `--wait` may not detect termination
- Training appears to hang
- No indication instance is gone

**User impact:**
- Wasted time waiting for training that will never complete
- No way to know instance was terminated
- Confusing error messages

**Recommendation:**
- Check instance state periodically during `--wait`
- Detect spot interruptions
- Clear error messages when instance terminates

#### Problem: Instance Not in Expected State
**When it happens:**
- Instance stopped (not terminated)
- Instance in "pending" state for long time
- Instance in "stopping" state

**Current behavior:**
- Commands may fail with unclear errors
- No state validation before operations
- Users don't know why commands fail

**User impact:**
- Confusing errors about instance state
- No guidance on how to fix
- Users try same command repeatedly

**Recommendation:**
- Validate instance state before operations
- Suggest appropriate actions (start, wait, etc.)
- Show current state in error messages

## Edge Cases

### 1. Project Root Detection Edge Cases

**Problems:**
- Multiple project markers in nested directories
- Script outside project directory
- No project markers (just files in current dir)

**Current behavior:**
- Uses first marker found walking up directory tree
- May find wrong project root
- Fails if script not under detected root

**Impact:**
- Code syncs to wrong location
- Training can't find script
- Inconsistent behavior

### 2. Script Arguments Parsing

**Problems:**
- Script args with spaces not quoted properly
- Special characters in arguments
- Arguments that look like runctl flags

**Current behavior:**
- Joins args with spaces
- No quoting or escaping
- May break with special characters

**Impact:**
- Training fails with argument parsing errors
- No way to pass complex arguments
- Users must work around limitations

### 3. Concurrent Operations

**Problems:**
- Multiple training jobs on same instance
- Code sync while training running
- Instance termination during sync

**Current behavior:**
- No locking or coordination
- Operations may interfere with each other
- Race conditions possible

**Impact:**
- Unpredictable behavior
- Data corruption possible
- No way to prevent conflicts

## Recommendations by Priority

### High Priority

1. **SSM Readiness Verification in --wait**
   - Don't just wait for instance running
   - Verify SSM connectivity before returning
   - Clear error if SSM never becomes ready

2. **Better Error Messages**
   - Show detected project root in errors
   - Explain what failed and why
   - Provide actionable next steps

3. **Training Completion Validation**
   - Check exit code, not just markers
   - Verify checkpoints were created
   - Detect hung training

### Medium Priority

4. **Progress Indication**
   - Show dependency installation progress
   - Stream sync progress
   - Indicate training is progressing (not hung)

5. **State Validation**
   - Check instance state before operations
   - Suggest fixes for common state issues
   - Prevent operations on wrong state instances

6. **Argument Handling**
   - Proper quoting/escaping for script args
   - Support complex arguments
   - Clear separation of runctl vs script args

### Low Priority

7. **Concurrent Operation Protection**
   - Lock files or coordination
   - Prevent conflicting operations
   - Better error messages for conflicts

8. **Sync Verification**
   - Verify critical files after sync
   - Show what was synced
   - Resume interrupted syncs

## Testing Recommendations

1. **Test spot instance fallback scenarios**
   - What happens when spot unavailable?
   - Is fallback clearly communicated?
   - Are costs as expected?

2. **Test SSM readiness edge cases**
   - Instance running but SSM not ready
   - SSM becomes ready after long delay
   - SSM never becomes ready

3. **Test training completion detection**
   - Script crashes but creates marker
   - Training hangs indefinitely
   - Training completes but checkpoints missing

4. **Test code sync edge cases**
   - Large codebases
   - Nested project structures
   - Missing files

5. **Test argument parsing**
   - Arguments with spaces
   - Special characters
   - Arguments that look like flags

