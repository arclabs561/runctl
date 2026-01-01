# Refinement Findings and Fixes

## Issues Found and Fixed

### 1. Training Wait Timeout Error Message Calculation Bug ✅

**Problem**: Error message shows wrong timeout duration
- `max_checks = 3600` (2 second intervals = 7200 seconds = 2 hours)
- Error message calculation: `max_checks * 2 / 60 = 120 minutes` (wrong!)
- Should be: `max_checks * 2 / 60 = 120 minutes` but that's still wrong - should be `(max_checks * check_interval.as_secs()) / 60`

**Fix**: Correct the calculation to use actual check interval

### 2. Training Wait Timeout Not Configurable ⚠️

**Problem**: Hardcoded 2-hour timeout with no way to configure
- `max_checks = 3600` is hardcoded
- No CLI flag or config option to adjust
- Some training jobs may need longer timeouts

**Recommendation**: 
- Add `--timeout` flag to `aws train` command
- Or make it configurable via `.runctl.toml`
- Default to 2 hours is reasonable, but allow override

### 3. Spot Monitoring Not Automatically Started ⚠️

**Problem**: Spot monitoring exists but must be manually started
- `monitor_spot_interruption()` function exists
- Not called automatically when training starts on spot instance
- Users must manually start monitoring

**Current State**: 
- Training starts on spot instance
- No automatic monitoring
- If interruption occurs, no graceful shutdown

**Fix Needed**: 
- Detect if instance is spot instance
- Automatically start monitoring in background task
- Handle monitoring errors gracefully

### 4. Dependency Installation Progress Not Shown ⚠️

**Problem**: Dependency installation can take 5-10 minutes with no feedback
- Command runs via SSM with no progress indication
- Users don't know if it's working or hung
- No way to see pip/uv output

**Current State**:
```rust
let setup_cmd = format!(
    "cd {} && \
    if [ -f requirements.txt ]; then \
        echo 'Installing dependencies...' && \
        uv pip install -r requirements.txt || pip install -r requirements.txt; \
    fi",
    project_dir
);
```

**Recommendation**:
- Stream output from dependency installation
- Show progress bar or spinner
- Or at minimum, show "Installing dependencies..." message that updates

### 5. S3 Temporary File Cleanup ✅

**Status**: Already implemented correctly
- Cleanup happens in `sync_code_via_ssm()` after download
- Uses `delete_object()` with error handling
- Warns if cleanup fails but doesn't fail the operation
- Good: Best-effort cleanup that doesn't block success

### 6. Error Message Terminology Inconsistency ⚠️

**Problem**: Mix of "SSM" and "Systems Manager" terminology
- Some messages say "SSM"
- Some say "Systems Manager"
- Help text says "Systems Manager (SSM)"

**Recommendation**: 
- Standardize on "SSM" (shorter, more common)
- Or use "AWS Systems Manager (SSM)" on first mention, then "SSM"

### 7. Timeout Constants Documentation ⚠️

**Problem**: Constants exist but not well documented
- `SSM_COMMAND_MAX_ATTEMPTS = 60` (what's the actual timeout?)
- `INSTANCE_WAIT_MAX_ATTEMPTS = 60` (5 minutes with 5s intervals)
- `VOLUME_ATTACH_MAX_ATTEMPTS = 30` (1 minute with 2s intervals)

**Recommendation**:
- Add comments explaining actual timeout durations
- Consider making some configurable
- Document in module-level docs

### 8. Instance Type Validation ⚠️

**Problem**: Only checks prefix, not full validation
- Checks `starts_with("g")` or `starts_with("p")` for GPU
- No validation that instance type is valid AWS type
- Could accept invalid types like "garbage123"

**Current State**:
```rust
let is_gpu = options.instance_type.starts_with("g")
    || options.instance_type.starts_with("p")
    || options.instance_type.contains("gpu");
```

**Recommendation**:
- Validate against known instance type patterns
- Or let AWS API validate (it will fail with clear error)
- Current approach is acceptable (fail fast at AWS API level)

## Priority Fixes

### High Priority
1. ✅ Fix training timeout error message calculation
2. ⚠️ Add spot monitoring automatic start
3. ⚠️ Add dependency installation progress indication

### Medium Priority
4. ⚠️ Standardize error message terminology
5. ⚠️ Document timeout constants
6. ⚠️ Consider making training timeout configurable

### Low Priority
7. ⚠️ Instance type validation (current approach acceptable)

