# Actual E2E Experience: Real Training Run Critique

## Test Run Date
2025-01-03

## Issues Found During Actual Execution

### 1. Command Syntax Errors in Examples ❌

**Problem**: Examples use incorrect syntax that doesn't match actual CLI.

**Found**:
- Examples use `--instance-type t3.micro` but actual command requires positional argument: `runctl aws create t3.micro --spot`
- Examples use `--script-args "--epochs 3"` but actual command requires `--` separator: `runctl aws train ... -- --epochs 3`

**Impact**:
- Examples don't work as written
- Users get confusing error messages
- Poor first impression

**Fix Applied**: ✅
- Updated all examples to use correct syntax
- Instance type is now positional argument
- Script args use `--` separator

### 2. SSM Code Sync Requires S3 Bucket ❌

**Problem**: SSM-based code sync requires S3 bucket to be configured, but this isn't clear.

**Error Encountered**:
```
Error: AWS SDK error: Could not find SSH key for key pair 'unknown'.

To resolve:
...
4. Use SSM instead: Create instance with --iam-instance-profile and configure s3_bucket in config
```

**Root Cause**:
- Instance has IAM profile (SSM available)
- But SSM code sync requires S3 bucket in config
- Falls back to SSH, which requires SSH key
- Error message mentions S3 but doesn't make it clear it's required for SSM sync

**Impact**:
- Users think SSM is working (status shows "SSM Available: Yes")
- But code sync fails because S3 bucket not configured
- Confusing error about SSH key when SSM should work

**Recommendation**:
- Check for S3 bucket before attempting SSM sync
- Clear error message: "SSM code sync requires s3_bucket in config. Add to .runctl.toml: [aws] s3_bucket = 'your-bucket'"
- Or: Make SSH key optional when SSM is available but S3 not configured (use direct SSM commands instead of S3-based sync)

### 3. Spot Instance Fallback Not Clear ⚠️

**Experience**:
```
WARNING: Spot instance failed: Cloud provider error: aws - Spot request timed out after 5 minutes
Falling back to on-demand...
Created on-demand instance: i-02faaaac6190ca253
```

**Issues**:
- Warning is clear, but cost difference isn't emphasized
- User might not notice they're paying 3-10x more
- No confirmation prompt

**Recommendation**:
- Make fallback more prominent
- Show cost difference: "Spot failed. On-demand costs $0.0104/hr vs spot ~$0.001/hr. Continue? (y/n)"
- Or: Add `--fail-if-no-spot` flag to prevent silent fallback

### 4. Instance Creation Timeout ⚠️

**Experience**:
- Spot instance creation timed out after 5 minutes
- This is expected behavior, but 5 minutes feels long
- No progress indication during wait

**Recommendation**:
- Show progress: "Waiting for spot instance... (attempt 1/60)"
- Reduce timeout or make configurable
- Better explanation of why spot might fail

### 5. SSM Readiness Verification Works ✅

**Experience**:
```
Waiting for instance to be ready...
Instance ready and SSM connected (if IAM profile configured)
```

**Good**:
- `--wait` flag actually verified SSM connectivity
- Clear message when ready
- No false positives

**Minor Issue**:
- Message says "if IAM profile configured" but we know it is configured
- Could be more specific: "Instance ready and SSM connected"

### 6. Status Command Works Well ✅

**Experience**:
```
Instance: i-02faaaac6190ca253
  State: running
  Type: t3.micro
  Public IP: 54.226.25.239
  Private IP: 172.31.20.8
  SSM Available: Yes
  Training Status: not_started
```

**Good**:
- Clear, structured output
- Shows all relevant information
- Training status included

## Actual Workflow Experience

### Step 1: Create Instance

**Command**: `runctl aws create t3.micro --spot --iam-instance-profile runctl-ssm-profile --wait --output instance-id`

**Experience**:
1. Spot request initiated
2. Waited ~5 minutes for spot
3. Spot timed out
4. Fell back to on-demand
5. Instance created: `i-02faaaac6190ca253`
6. `--wait` verified SSM connectivity
7. Got instance ID cleanly

**Time**: ~6 minutes total

**Issues**:
- Long wait for spot (expected, but could be clearer)
- Silent fallback (should be more prominent)

### Step 2: Check Status

**Command**: `runctl aws status i-02faaaac6190ca253`

**Experience**:
- Fast response
- Clear output
- Shows SSM available
- Shows training status

**Time**: <1 second

**Good**: Works as expected

### Step 3: Train

**Command**: `runctl aws train i-02faaaac6190ca253 training/train_mnist_e2e.py --sync-code --wait -- --epochs 3`

**Experience**:
- Failed immediately with SSH key error
- SSM is available but code sync requires S3 bucket
- Error message mentions S3 but doesn't make it clear it's required

**Time**: <1 second (failed immediately)

**Issues**:
- SSM code sync requires S3 bucket (not documented clearly)
- Error message confusing (talks about SSH when SSM should work)
- No clear path forward

## Critical Issues Summary

### High Priority

1. **Examples Use Wrong Syntax** ✅ FIXED
   - Instance type should be positional, not flag
   - Script args need `--` separator, not `--script-args`

2. **SSM Code Sync Requires S3 Bucket** ❌ NEEDS FIX
   - Not clear in documentation
   - Error message confusing
   - Should check for S3 bucket before attempting SSM sync

3. **Spot Fallback Not Prominent** ⚠️ NEEDS IMPROVEMENT
   - Cost difference not emphasized
   - No confirmation

### Medium Priority

4. **Long Spot Timeout** ⚠️
   - 5 minutes feels long
   - No progress indication

5. **Error Messages Could Be Clearer** ⚠️
   - SSM/S3 relationship not clear
   - Should suggest adding S3 bucket to config

## Recommendations

### Immediate Fixes

1. **Fix All Examples** ✅ DONE
   - Use correct syntax
   - Test examples actually work

2. **Improve SSM/S3 Error Messages**
   - Check for S3 bucket before SSM sync
   - Clear error: "SSM code sync requires s3_bucket in config. Add: [aws] s3_bucket = 'your-bucket'"
   - Or: Support direct SSM commands for code sync (no S3 needed)

3. **Make Spot Fallback More Prominent**
   - Show cost difference
   - Require confirmation or `--fail-if-no-spot` flag

### Documentation Updates

1. **Prerequisites Section**
   - Clearly state: "SSM code sync requires S3 bucket in config"
   - Show how to configure: `[aws] s3_bucket = 'your-bucket'`
   - Or: "Use SSH key if S3 bucket not available"

2. **Troubleshooting Section**
   - Add: "SSM available but code sync fails" → Check S3 bucket config
   - Add: "Spot instance falls back to on-demand" → Cost implications

## Next Steps

1. ✅ Fix example syntax (done)
2. ⏳ Improve SSM/S3 error handling
3. ⏳ Make spot fallback more prominent
4. ⏳ Add S3 bucket to config or document requirement
5. ⏳ Test complete workflow with S3 bucket configured

