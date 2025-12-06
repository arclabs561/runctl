# SSM Quick Start Guide

## Overview

Systems Manager (SSM) enables secure command execution on EC2 instances without SSH keys. This guide shows how to set up SSM for trainctl.

## Quick Setup

### 1. Create IAM Role and Instance Profile

Run the setup script (one-time):

```bash
./scripts/setup-ssm-role.sh
```

This creates:
- IAM role: `trainctl-ssm-role`
- Instance profile: `trainctl-ssm-profile`
- Attaches `AmazonSSMManagedInstanceCore` policy

### 2. Create Instance with SSM

```bash
trainctl aws create t3.micro --iam-instance-profile trainctl-ssm-profile
```

### 3. Use SSM Features

Once the instance has SSM configured, you can use:

```bash
# Process monitoring (no SSH needed)
trainctl aws processes <instance-id>

# Training execution (uses SSM instead of SSH)
trainctl aws train <instance-id> train.py --sync-code

# All SSM-based commands work automatically
```

## Manual Setup

If you prefer to set up manually:

```bash
# 1. Create trust policy
cat > trust-policy.json << 'EOF'
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": {"Service": "ec2.amazonaws.com"},
    "Action": "sts:AssumeRole"
  }]
}
EOF

# 2. Create IAM role
aws iam create-role \
    --role-name trainctl-ssm-role \
    --assume-role-policy-document file://trust-policy.json

# 3. Attach SSM policy
aws iam attach-role-policy \
    --role-name trainctl-ssm-role \
    --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore

# 4. Create instance profile
aws iam create-instance-profile --instance-profile-name trainctl-ssm-profile

# 5. Add role to profile
aws iam add-role-to-instance-profile \
    --instance-profile-name trainctl-ssm-profile \
    --role-name trainctl-ssm-role
```

## Verify Setup

```bash
# Check instance profile exists
aws iam get-instance-profile --instance-profile-name trainctl-ssm-profile

# Check role has SSM policy
aws iam list-attached-role-policies --role-name trainctl-ssm-role

# Check SSM connectivity (after instance is running)
aws ssm describe-instance-information \
    --filters "Key=InstanceIds,Values=i-1234567890abcdef0"
```

## Benefits

- **No SSH keys needed**: More secure, no key management
- **Automatic**: SSM agent pre-installed on Amazon Linux/Ubuntu AMIs
- **Audit trail**: All commands logged in CloudTrail
- **Works through VPN**: No need for public IPs or security group rules
- **Session Manager**: Can also use AWS Console Session Manager

## Troubleshooting

### SSM Not Working

1. **Check IAM profile attached**:
   ```bash
   aws ec2 describe-instances --instance-ids i-xxx \
       --query 'Reservations[0].Instances[0].IamInstanceProfile'
   ```

2. **Check SSM agent status** (via SSH if available):
   ```bash
   sudo systemctl status amazon-ssm-agent
   ```

3. **Wait for agent**: SSM agent may take 1-2 minutes after instance start

4. **Check permissions**: Ensure role has `AmazonSSMManagedInstanceCore` policy

### Fallback to SSH

If SSM is not available, trainctl will:
- Use SSH if `--key-name` is provided
- Show helpful error messages with setup instructions

## Current Auth Status

Your current AWS identity:
- **User**: `admin` (AIDAXOZXBE6RHJ5ZKZG6O)
- **Account**: 512827140002
- **Permissions**: Can create IAM roles, EC2 instances, SSM commands

## Next Steps

1. ✅ Run `./scripts/setup-ssm-role.sh` (one-time setup)
2. ✅ Create instances with `--iam-instance-profile trainctl-ssm-profile`
3. ✅ Use SSM features (processes, training, monitoring)

