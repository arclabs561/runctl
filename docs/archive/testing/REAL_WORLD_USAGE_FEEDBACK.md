# Real-World Usage Feedback

This document contains feedback from actually using `runctl` to train models, documenting what works well, pain points, and areas for improvement.

## Test Run: Basic Training Workflow

### Setup

**Commands Attempted:**
```bash
# 1. Initialize config
runctl init
# ✅ Works! Creates .runctl.toml with defaults

# 2. Create instance (on-demand, since spot failed)
runctl aws create t3.micro --wait
# ✅ Works! Created: i-05dc9003955afc989

# 3. Train using direct command
runctl aws train i-05dc9003955afc989 training/train_mnist_e2e.py --sync-code --script-args "--epochs 3"
```

### Critical Bug Found: `runctl init` Panic

**Issue**: `runctl init` command panics with clap error:
```
Mismatch between definition and access of `output`. Could not downcast...
```

**Root Cause**: Conflict between global `--output` flag (for JSON output) and `Init` command's `--output` flag (for config file path).

**Fix Applied**: Renamed `Init` command's flag to `--config-path` to avoid conflict.

**Impact**: **BLOCKER** - Users cannot initialize config, preventing first-time setup.

**Status**: ✅ **FIXED**

### Real Training Run Experience

#### ✅ What Works Well

1. **Instance Creation**
   - ✅ On-demand creation works perfectly
   - ✅ `--wait` flag properly waits for instance readiness
   - ✅ Clear output: "Created on-demand instance: i-xxx"
   - ✅ Good feedback: "Instance ready and SSM connected"
   - ✅ Resources list shows all instances with costs

2. **Resource Visibility**
   - ✅ `runctl resources list` is excellent
   - ✅ Shows instance type, state, pricing, uptime
   - ✅ Shows public/private IPs
   - ✅ Cost tracking per instance
   - ✅ Clear formatting

3. **Training Script**
   - ✅ `train_mnist_e2e.py` is perfect for testing
   - ✅ Fast, no dependencies, clear output
   - ✅ Proper checkpoint saving

4. **Config System**
   - ✅ Sensible defaults in `.runctl.toml`
   - ✅ All sections pre-configured
   - ✅ Easy to understand structure

#### ⚠️ Issues Found During Real Usage

1. **Spot Instance Error Messages**
   - **Problem**: Generic "service error" doesn't help
   - **Example**: `Error: AWS SDK error: Failed to request spot instance: service error`
   - **Impact**: User doesn't know if it's capacity, pricing, or permissions
   - **Fix Applied**: Enhanced error messages with specific guidance based on error type
   - **Status**: ✅ **IMPROVED**

2. **Command Syntax Confusion**
   - **Problem**: Tried `--script-args "--epochs 3"` but that's not the syntax
   - **Correct**: `-- --epochs 3` (double dash separator)
   - **Impact**: Confusing for new users - not obvious from help text
   - **Suggestion**: 
     - Make help text clearer about `--` separator
     - Show more examples in help
     - Consider `--args` flag as alias for clarity

3. **SSM Connectivity Issues**
   - **Problem**: Monitor command fails with "SSM error: service error"
   - **Observation**: Instance created but SSM not ready yet
   - **Impact**: Can't monitor training immediately after creation
   - **Suggestion**: 
     - `--wait` should verify SSM connectivity, not just instance state
     - Better error message explaining SSM readiness timing
     - Add `runctl aws wait-ssm <instance-id>` command

3. **Error Recovery**
   - **Question**: What happens if training fails mid-way?
   - **Question**: Can user resume or must start over?
   - **Pending**: Test failure scenarios

4. **Cost Visibility**
   - ✅ Good: Resources list shows costs
   - ⚠️ Missing: No total cost summary

## Test Run 2: Complete Training Workflow with SSM

### Setup

**Instance Created:**
```bash
runctl aws create t3.micro --iam-instance-profile runctl-ssm-profile --wait
# ✅ Created: i-03d5ddb8c7783f963
# ✅ SSM connected successfully
```

**Training Command:**
```bash
runctl aws train i-03d5ddb8c7783f963 training/train_mnist_e2e.py --sync-code --wait -- --epochs 1
```

### ✅ What Worked Perfectly

1. **Instance Creation with IAM Profile**
   - ✅ `--iam-instance-profile` flag works correctly
   - ✅ `--wait` properly waits for SSM connectivity (not just instance state)
   - ✅ Clear feedback: "Instance ready and SSM connected"
   - ✅ No SSH key required when using SSM

2. **Code Sync via SSM**
   - ✅ Code sync worked flawlessly via SSM
   - ✅ Verification message: "Code sync verified: script and directories found"
   - ✅ Fast and reliable (no SSH key management needed)

3. **Training Execution**
   - ✅ Training started successfully
   - ✅ Script arguments passed correctly with `--` separator
   - ✅ Training completed and created checkpoints
   - ✅ Completion marker (`training_complete.txt`) created

4. **Completion Detection**
   - ✅ `--wait` flag correctly detected training completion
   - ✅ Multiple heuristics working (marker file, PID status)
   - ✅ Clear success message: "Training completed successfully"

5. **Monitoring**
   - ✅ `runctl aws monitor` shows training logs correctly
   - ✅ Logs are readable and well-formatted
   - ✅ Shows checkpoint creation and validation

### ⚠️ Observations and Potential Improvements

1. **Exit Code Capture**
   - **Observation**: Training script uses `sys.exit(0)` but `training_exit_code.txt` is not created
   - **Current Behavior**: Completion detection checks for `training_exit_code.txt` but script doesn't create it
   - **Impact**: Exit code checking in completion detection may not work for all scripts
   - **Suggestion**: 
     - Document that scripts should create `training_exit_code.txt` if they want exit code validation
     - Or: Modify training command to capture exit code automatically
     - Or: Use `$?` in bash wrapper to capture exit code

2. **Training Script Template**
   - **Observation**: `train_mnist_e2e.py` is a good example but not documented as a template
   - **Suggestion**: 
     - Create `training/template.py` with best practices
     - Document completion marker creation
     - Document exit code file creation
     - Show SIGTERM handling for graceful shutdown

3. **Checkpoint Verification**
   - **Observation**: Completion detection doesn't verify checkpoints were actually created
   - **Current Behavior**: Checks for marker file and PID, but doesn't validate checkpoints exist
   - **Impact**: Training could "complete" but checkpoints might be missing
   - **Suggestion**: 
     - Add optional `--verify-checkpoints` flag
     - Check that checkpoint directory contains files
     - Warn if checkpoints are missing

4. **Training Log Location**
   - **Observation**: Logs are in `/home/ec2-user/runctl/training.log`
   - **Current Behavior**: Hardcoded path, not configurable
   - **Suggestion**: 
     - Make log path configurable via `.runctl.toml`
     - Or: Use project directory for logs

5. **Multiple Training Runs**
   - **Observation**: Running training twice on same instance works, but logs may be overwritten
   - **Suggestion**: 
     - Add timestamp to log files
     - Or: Create log directory with timestamps
     - Or: Append to existing log with separator

### 🎯 Overall Assessment

**Excellent Experience**: The complete workflow works smoothly with SSM:
- ✅ Instance creation with IAM profile
- ✅ Code sync via SSM (fast, reliable)
- ✅ Training execution and monitoring
- ✅ Completion detection
- ✅ Clear feedback and error messages

**Key Strengths**:
1. SSM integration is seamless and secure
2. `--wait` flag makes workflows simple
3. Completion detection is robust (multiple heuristics)
4. Error messages are helpful and actionable

**Minor Improvements Needed**:
1. Exit code capture automation
2. Checkpoint verification option
3. Training script template/documentation
4. Configurable log paths
   - ⚠️ Missing: No cost alerts or limits
   - **Suggestion**: Add `runctl resources cost` command

5. **Instance State Management**
   - ✅ Good: `--wait` works well
   - ⚠️ Missing: No way to check if instance is "ready" without creating
   - **Suggestion**: `runctl aws wait <instance-id>` command

#### 🔍 Areas Tested

- [x] Config initialization
- [x] Instance creation (on-demand)
- [x] Instance creation (spot - failed, but error improved)
- [x] Resource listing
- [ ] Code sync to instance
- [ ] Training execution
- [ ] Training monitoring
- [ ] Checkpoint saving
- [ ] Instance lifecycle (stop/start)
- [ ] Error recovery

#### 🚀 What's Great

1. **Command Structure**: Intuitive and well-organized
2. **Help Text**: Comprehensive and helpful
3. **Resource Visibility**: Excellent cost and status tracking
4. **Safety Features**: Blocks mass instance creation (>50)
5. **Defaults**: Sensible defaults throughout

#### 💡 Improvement Opportunities

1. **Better Error Messages**
   - Parse AWS errors and provide specific guidance
   - Show actionable next steps
   - Include relevant AWS documentation links

2. **Progress Indicators**
   - Show progress during instance creation
   - Show progress during code sync
   - Show training progress in real-time

3. **Workflow Commands**
   - `workflow train` is great but needs better error handling
   - Add `workflow resume` for failed workflows
   - Add `workflow cleanup` for orphaned resources

4. **Cost Management**
   - Add cost limits with auto-stop
   - Show cost estimates before creation
   - Daily/weekly cost summaries

5. **Monitoring**
   - Better real-time training metrics
   - GPU utilization tracking
   - Training progress visualization

## Next Steps

1. ✅ Fix `init` command panic
2. ✅ Improve spot instance error messages
3. [ ] Test complete training workflow end-to-end
4. [ ] Test failure scenarios and recovery
5. [ ] Measure performance (sync time, etc.)
6. [ ] Test with real ML workloads (PyTorch, etc.)

## Real-World Critique

### What Makes runctl Good

1. **Unified Interface**: One tool for all cloud providers
2. **Cost Awareness**: Built-in cost tracking
3. **Safety First**: Prevents accidental mass creation
4. **Good Defaults**: Works out of the box
5. **Clear Commands**: Intuitive command structure

### What Could Be Better

1. **Error Messages**: Need more specificity and actionability
2. **Progress Feedback**: Need more visibility into long operations
3. **Workflow Resilience**: Better handling of partial failures
4. **Cost Controls**: Need limits and alerts
5. **Documentation**: More real-world examples

### Overall Assessment

**Strengths**: Solid foundation, good architecture, cost-aware, safe defaults

**Weaknesses**: Error handling could be better, needs more progress feedback, workflow resilience

**Verdict**: **Good tool with room for polish**. The core functionality works well, but the user experience could be smoother, especially around errors and long-running operations.
