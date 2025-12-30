# AWS Full Test Results

## Test Date
2025-12-30

## Objective
Test the complete runctl workflow with the MNIST training example on AWS EC2.

## Setup Completed

### 1. SSM Configuration
- Created IAM role: `runctl-ssm-role`
- Created instance profile: `runctl-ssm-profile`
- Attached `AmazonSSMManagedInstanceCore` policy
- Verified setup with `aws iam get-instance-profile`

### 2. Instance Creation
- Created instance: `i-0c3d55601ac7bd81a`
- Instance type: `g4dn.xlarge` (GPU instance)
- IAM instance profile: `runctl-ssm-profile` (SSM enabled)
- Status: Instance created successfully
- SSM connectivity: Verified online after ~2 minutes

## Findings

### What Works

1. **SSM Setup**: Successfully created IAM role and instance profile
2. **Instance Creation**: `runctl aws create` works with `--iam-instance-profile` flag
3. **SSM Connectivity**: Instance registered with SSM and shows as "Online"
4. **SSM Command Execution**: Can execute commands via SSM (tested basic commands)

### What Needs Improvement

1. **Code Syncing via SSM**: 
   - Current implementation only supports SSH-based code syncing
   - When instance has SSM but no SSH key, code sync fails
   - Error: "Could not find SSH key for key pair 'unknown'"
   - **Recommendation**: Implement SSM-based code syncing using S3 or SSM file transfer

2. **Training Execution**:
   - Training command execution supports SSM (code checks for IAM instance profile)
   - But fails because code sync happens first and requires SSH
   - **Workaround**: Use `--sync-code false` and manually transfer code, or use S3

## Current Workflow Limitations

When using SSM (no SSH keys):
- ✅ Instance creation works
- ✅ SSM connectivity works
- ✅ Command execution via SSM works
- ❌ Code syncing fails (requires SSH)
- ❌ Training workflow incomplete (depends on code sync)

## Recommended Next Steps

### Short-term Workaround
1. Use S3 for code transfer:
   ```bash
   # Upload code to S3
   tar -czf code.tar.gz training/ examples/
   aws s3 cp code.tar.gz s3://bucket/code.tar.gz
   
   # On instance, download and extract
   aws ssm send-command --instance-ids i-xxx \
     --document-name "AWS-RunShellScript" \
     --parameters 'commands=["aws s3 cp s3://bucket/code.tar.gz . && tar -xzf code.tar.gz"]'
   ```

2. Or use `--sync-code false` and manually set up code

### Long-term Solution
Implement SSM-based code syncing:
- Use S3 as intermediate storage
- Or use SSM file transfer capabilities
- Or implement direct SSM-based tar/untar

## Test Commands Used

```bash
# Setup SSM
./scripts/setup-ssm-role.sh

# Create instance with SSM
./target/release/runctl aws create g4dn.xlarge \
  --spot \
  --iam-instance-profile runctl-ssm-profile

# Verify SSM
aws ssm describe-instance-information \
  --filters "Key=InstanceIds,Values=i-xxx"

# Attempt training (fails at code sync)
./target/release/runctl aws train i-xxx \
  training/train_mnist.py \
  --sync-code \
  -- --epochs 3
```

## Conclusion

The infrastructure works correctly:
- SSM setup is functional
- Instance creation with SSM works
- SSM command execution works

The main gap is **code syncing via SSM**. This is a known limitation that should be addressed to fully support SSM-only workflows (no SSH keys required).

For now, users can:
1. Use SSH keys (traditional approach)
2. Use S3 for code transfer (workaround)
3. Wait for SSM-based code syncing implementation

