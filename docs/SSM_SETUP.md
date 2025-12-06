# SSM (Systems Manager) Setup Guide

## Issue Found During Testing

Some instances may not have SSM connectivity, which is required for:
- Process monitoring (`trainctl aws processes`)
- Training execution (`trainctl aws train`)
- Code syncing (when using SSM)
- Command execution on instances

## Root Cause

SSM requires:
1. **IAM Instance Profile** with SSM permissions
2. **SSM Agent** installed and running on the instance
3. **Network connectivity** to SSM endpoints

## Solution

### Option 1: Use IAM Instance Profile (Recommended)

When creating instances, ensure they have an IAM instance profile with SSM permissions:

```bash
# Create IAM role for SSM
aws iam create-role \
    --role-name EC2-SSM-Role \
    --assume-role-policy-document '{
        "Version": "2012-10-17",
        "Statement": [{
            "Effect": "Allow",
            "Principal": {"Service": "ec2.amazonaws.com"},
            "Action": "sts:AssumeRole"
        }]
    }'

# Attach SSM managed policy
aws iam attach-role-policy \
    --role-name EC2-SSM-Role \
    --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore

# Create instance profile
aws iam create-instance-profile --instance-profile-name EC2-SSM-Profile
aws iam add-role-to-instance-profile \
    --instance-profile-name EC2-SSM-Profile \
    --role-name EC2-SSM-Role
```

Then use when creating instances:
```bash
trainctl aws create t3.micro --iam-instance-profile EC2-SSM-Profile
```

### Option 2: Use SSH Fallback

If SSM is not available, trainctl should fall back to SSH. Ensure:
- SSH key is configured (`--key-name`)
- Security group allows SSH (port 22)
- Instance has public IP or VPN access

### Option 3: Install SSM Agent Manually

For existing instances without SSM:

```bash
# On Amazon Linux 2023
sudo yum install -y amazon-ssm-agent
sudo systemctl enable amazon-ssm-agent
sudo systemctl start amazon-ssm-agent

# On Ubuntu
sudo snap install amazon-ssm-agent --classic
sudo snap start amazon-ssm-agent
```

## Current Implementation

trainctl currently:
- ✅ Prefers SSM when IAM instance profile is detected
- ✅ Falls back to SSH when SSM unavailable
- ⚠️ May fail if neither SSM nor SSH is configured

## Recommendations

1. **Add IAM Instance Profile Support**
   - Add `--iam-instance-profile` flag to `aws create`
   - Auto-attach SSM managed policy
   - Document in help text

2. **Improve Error Messages**
   - Detect SSM unavailability
   - Suggest SSH fallback
   - Provide setup instructions

3. **Auto-Configure SSM**
   - Create IAM role automatically if not exists
   - Attach to instances by default
   - Make SSM the default (more secure than SSH)

## Testing

To test SSM connectivity:
```bash
# Check if instance has SSM
aws ssm describe-instance-information \
    --filters "Key=InstanceIds,Values=i-1234567890abcdef0"

# Send test command
aws ssm send-command \
    --instance-ids i-1234567890abcdef0 \
    --document-name "AWS-RunShellScript" \
    --parameters "commands=[\"echo 'SSM working'\"]"
```

## Status

- **Current**: SSM works when IAM role configured
- **Issue**: Some instances don't have IAM role
- **Workaround**: Use SSH fallback
- **Future**: Auto-configure SSM for all instances

