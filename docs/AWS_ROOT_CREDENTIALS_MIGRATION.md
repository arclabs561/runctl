# AWS Root Credentials Migration Guide

## ⚠️ CRITICAL: Root Credentials Security Risk

If you're using AWS root credentials, you're at **extreme risk**:

- **Unlimited access** to all AWS resources
- **Cannot be restricted** by IAM policies
- **Attractive target** for attackers
- **No audit trail** separation
- **Account compromise** = total loss

## Immediate Actions

### 1. Check Your Current Credentials

```bash
# Run the security check script
./scripts/check-aws-credentials.sh

# Or manually check
aws sts get-caller-identity
```

**If ARN contains `:root`**, you're using root credentials. **STOP** and follow this guide.

### 2. Enable MFA on Root Account (If Not Already)

```bash
# This must be done via AWS Console
# Go to: IAM → Users → Root user → Security credentials
# Enable MFA device
```

**MFA is REQUIRED** for root account security.

### 3. Delete Root Access Keys

```bash
# List root access keys
aws iam list-access-keys --user-name root

# Delete each key (replace KEY_ID)
aws iam delete-access-key --user-name root --access-key-id <KEY_ID>
```

**CRITICAL**: Never use root credentials for programmatic access.

## Migration Path: Root → IAM User → IAM Role

### Step 1: Create IAM User for Daily Use

Create a dedicated IAM user with minimal permissions:

```bash
# Create IAM user
aws iam create-user \
  --user-name runctl-admin \
  --tags Key=Purpose,Value=runctl-cli

# Create access key for the user
aws iam create-access-key --user-name runctl-admin
```

**Save the access key ID and secret** - you'll need them to configure AWS CLI.

### Step 2: Attach Minimal Permissions Policy

Create a policy with only the permissions runctl needs:

```bash
cat > /tmp/runctl-user-policy.json <<'EOF'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "EC2Operations",
      "Effect": "Allow",
      "Action": [
        "ec2:Describe*",
        "ec2:RunInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:RebootInstances",
        "ec2:TerminateInstances",
        "ec2:CreateTags",
        "ec2:DescribeTags"
      ],
      "Resource": "*"
    },
    {
      "Sid": "EBSOperations",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeVolumes",
        "ec2:CreateVolume",
        "ec2:AttachVolume",
        "ec2:DetachVolume",
        "ec2:DeleteVolume",
        "ec2:CreateSnapshot",
        "ec2:DescribeSnapshots",
        "ec2:DeleteSnapshot"
      ],
      "Resource": "*"
    },
    {
      "Sid": "S3Operations",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket",
        "s3:HeadBucket"
      ],
      "Resource": "*"
    },
    {
      "Sid": "SSMOperations",
      "Effect": "Allow",
      "Action": [
        "ssm:SendCommand",
        "ssm:GetCommandInvocation",
        "ssm:DescribeInstanceInformation",
        "ssm:ListCommandInvocations"
      ],
      "Resource": "*"
    },
    {
      "Sid": "STSAssumeRole",
      "Effect": "Allow",
      "Action": "sts:AssumeRole",
      "Resource": "arn:aws:iam::*:role/runctl-*"
    }
  ]
}
EOF

# Create the policy
aws iam create-policy \
  --policy-name runctl-user-policy \
  --policy-document file:///tmp/runctl-user-policy.json

# Attach to user
aws iam attach-user-policy \
  --user-name runctl-admin \
  --policy-arn arn:aws:iam::$(aws sts get-caller-identity --query Account --output text):policy/runctl-user-policy
```

### Step 3: Configure AWS CLI with IAM User Credentials

```bash
# Configure AWS CLI with new IAM user credentials
aws configure

# Enter:
# - AWS Access Key ID: (from Step 1)
# - AWS Secret Access Key: (from Step 1)
# - Default region: us-east-1
# - Default output format: json

# Verify it works
aws sts get-caller-identity
# Should show: arn:aws:iam::ACCOUNT:user/runctl-admin
```

### Step 4: Enable MFA on IAM User

```bash
# Create virtual MFA device
aws iam create-virtual-mfa-device \
  --virtual-mfa-device-name runctl-admin-mfa \
  --outfile QRCode.png \
  --bootstrap-method QRCodePNG

# Enable MFA (requires MFA codes from your device)
aws iam enable-mfa-device \
  --user-name runctl-admin \
  --serial-number arn:aws:iam::ACCOUNT:mfa/runctl-admin-mfa \
  --authentication-code-1 <CODE1> \
  --authentication-code-2 <CODE2>
```

### Step 5: Set Up IAM Role for Temporary Credentials (Recommended)

For even better security, use IAM roles with temporary credentials:

```bash
# Use the existing test role setup
./scripts/setup-test-role.sh

# Or create a production role
# (modify scripts/setup-test-role.sh for production use)
```

Then use temporary credentials:

```bash
# Assume role and get temporary credentials
source scripts/assume-test-role.sh

# Verify
aws sts get-caller-identity
# Should show: arn:aws:sts::ACCOUNT:assumed-role/runctl-test-role/...
```

### Step 6: Verify Migration

```bash
# Run security check
./scripts/check-aws-credentials.sh

# Should show:
# ✓ Not using root credentials
# ✓ Using IAM user/role
```

## Best Practices Going Forward

### ✅ DO:

1. **Use IAM roles** for temporary credentials (best)
2. **Use IAM users** with minimal permissions (good)
3. **Enable MFA** on all accounts
4. **Rotate credentials** every 90 days
5. **Use separate accounts** for production/testing
6. **Monitor usage** with CloudTrail
7. **Tag resources** for isolation

### ❌ DON'T:

1. **Never use root credentials** for programmatic access
2. **Never share credentials** between users
3. **Never commit credentials** to git
4. **Never use admin permissions** for daily tasks
5. **Never skip MFA** on root account
6. **Never use long-term credentials** when roles are available

## For CI/CD (GitHub Actions)

### Option 1: OIDC (Recommended)

Use GitHub's OIDC provider to assume IAM roles directly:

```yaml
- name: Configure AWS credentials
  uses: aws-actions/configure-aws-credentials@v4
  with:
    role-to-assume: arn:aws:iam::ACCOUNT:role/github-actions-role
    aws-region: us-east-1
```

**No secrets needed** - GitHub assumes the role directly.

### Option 2: GitHub Secrets (Less Secure)

If OIDC isn't available, use GitHub Secrets:

1. Create IAM user for CI/CD
2. Create access keys
3. Add to GitHub Secrets: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`
4. Rotate every 90 days

**Not recommended** - use OIDC instead.

## Security Checklist

- [ ] Root access keys deleted
- [ ] MFA enabled on root account
- [ ] IAM user created with minimal permissions
- [ ] AWS CLI configured with IAM user credentials
- [ ] MFA enabled on IAM user
- [ ] IAM role set up for temporary credentials
- [ ] `check-aws-credentials.sh` passes
- [ ] CloudTrail enabled for audit logging
- [ ] Credentials rotated regularly (90 days)

## References

- [AWS Root User Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/root-user-best-practices.html)
- [IAM Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [Temporary Security Credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp.html)
- [GitHub Actions OIDC](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)

## Summary

**Root credentials = Extreme risk**

**Migration path:**
1. Root → IAM User (immediate)
2. IAM User → IAM Role (recommended)
3. IAM Role → OIDC for CI/CD (best)

**Time to migrate:** Do it now. Every day with root credentials is a risk.

