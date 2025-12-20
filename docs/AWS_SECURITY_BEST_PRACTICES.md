# AWS Security Best Practices for runctl

## Overview

This document consolidates AWS security best practices specifically for runctl usage, based on AWS recommendations and security research.

## Root Credentials: Never Use

### Why Root Credentials Are Dangerous

1. **Unlimited Access**: Root user has full administrative access to all AWS resources
2. **Cannot Be Restricted**: Root user actions cannot be constrained by IAM policies
3. **Attractive Target**: Root credentials are prime targets for attackers
4. **No Audit Separation**: All actions appear as root, making auditing difficult
5. **Account Compromise**: If root is compromised, entire account is at risk

### Root Credential Best Practices

✅ **DO:**
- Enable MFA on root account (REQUIRED)
- Use root only for account-level operations (billing, account recovery)
- Store root password securely (password manager)
- Use group email for root account (not individual)
- Implement multi-person approval for root sign-in

❌ **DON'T:**
- Never use root for programmatic access
- Never share root credentials
- Never commit root credentials to git
- Never use root for daily operations
- Never skip MFA on root account

## IAM Users vs Roles

### IAM Users (Long-term Credentials)

**Use for:**
- Human users who need persistent access
- Service accounts that cannot use roles
- Legacy systems that don't support roles

**Security considerations:**
- Credentials don't expire automatically
- Must be manually rotated
- Can be compromised if leaked
- Require MFA for security

### IAM Roles (Temporary Credentials) ⭐ RECOMMENDED

**Use for:**
- CI/CD pipelines
- EC2 instances
- Lambda functions
- Temporary access scenarios
- Testing environments

**Advantages:**
- Credentials automatically expire (1 hour default)
- No credential storage needed
- Automatic rotation
- Better audit trail
- Least privilege by design

## Least Privilege Principle

### What It Means

Grant only the **minimum permissions** needed to perform a task.

### How to Apply

1. **Start with no permissions** (default)
2. **Grant only what's needed** for specific tasks
3. **Test with minimal permissions** first
4. **Add permissions incrementally** if needed
5. **Review and remove** unused permissions regularly

### Example: runctl Permissions

**What runctl needs:**
- EC2: Describe, Create, Start, Stop, Terminate instances
- EBS: Describe, Create, Attach, Detach, Delete volumes
- S3: GetObject, PutObject, DeleteObject, ListBucket
- SSM: SendCommand, GetCommandInvocation

**What runctl does NOT need:**
- IAM: Create users, roles, policies
- Organizations: Account management
- Billing: Cost management
- CloudFormation: Stack management

## Temporary Credentials Best Practices

### For Local Development

```bash
# Use IAM role assumption
source scripts/assume-test-role.sh

# Credentials expire in 1 hour
# Re-run script to refresh
```

### For CI/CD

```yaml
# Use OIDC (recommended)
- uses: aws-actions/configure-aws-credentials@v4
  with:
    role-to-assume: arn:aws:iam::ACCOUNT:role/github-actions-role
    aws-region: us-east-1
```

### Session Duration

- **Default**: 1 hour (maximum for AssumeRole)
- **For testing**: 1 hour is sufficient
- **For production**: Use shorter durations when possible
- **For long tasks**: Re-assume role periodically

## Resource Tagging and Isolation

### Why Tag Resources

- **Cost tracking**: Identify resources by project
- **Security isolation**: Prevent cross-project access
- **Compliance**: Meet organizational requirements
- **Automation**: Enable policy-based controls

### runctl Tagging Strategy

```json
{
  "Environment": "test|production",
  "Project": "project-name",
  "CreatedBy": "runctl",
  "runctl:created": "2025-12-04",
  "runctl:project": "project-name"
}
```

### Policy-Based Isolation

```json
{
  "Effect": "Deny",
  "Action": ["ec2:RunInstances", "ec2:TerminateInstances"],
  "Resource": "*",
  "Condition": {
    "StringNotEquals": {
      "aws:RequestTag/Environment": "test"
    }
  }
}
```

## Permission Boundaries

### What They Are

Permission boundaries set the **maximum permissions** an IAM entity can have, regardless of attached policies.

### Why Use Them

- **Prevent privilege escalation**: Even if policy is misconfigured
- **Enforce organizational limits**: Prevent accidental over-permissioning
- **Defense in depth**: Multiple layers of security

### Example for runctl

```json
{
  "Effect": "Allow",
  "Action": ["ec2:*", "s3:*", "ssm:*"],
  "Resource": "*",
  "Condition": {
    "StringEquals": {
      "aws:RequestedRegion": ["us-east-1", "us-west-2"]
    }
  }
},
{
  "Effect": "Deny",
  "Action": ["iam:*", "organizations:*"],
  "Resource": "*"
}
```

## Monitoring and Auditing

### CloudTrail

Enable CloudTrail to log all API calls:

```bash
# Enable CloudTrail
aws cloudtrail create-trail \
  --name runctl-audit-trail \
  --s3-bucket-name runctl-audit-logs
```

### GuardDuty

Enable GuardDuty to detect:
- Root credential usage
- Unauthorized API calls
- Suspicious activity

### Alarms

Set up CloudWatch alarms for:
- Root user API calls
- Unusual API activity
- Failed authentication attempts

## Credential Rotation

### IAM User Credentials

- **Rotate every 90 days** (AWS recommendation)
- **Create new key before deleting old** (avoid downtime)
- **Test new credentials** before removing old
- **Delete old keys immediately** after verification

### IAM Role Credentials

- **Automatic rotation** (no action needed)
- **Expire after session duration** (default 1 hour)
- **Re-assume role** to refresh

## Multi-Factor Authentication (MFA)

### When to Use MFA

✅ **REQUIRED:**
- Root account (mandatory)
- IAM users with admin permissions
- Production accounts

✅ **RECOMMENDED:**
- All IAM users
- Console access
- Sensitive operations

### MFA Types

1. **Virtual MFA**: Mobile app (Google Authenticator, Authy)
2. **Hardware MFA**: Physical device (YubiKey)
3. **SMS MFA**: Text message (less secure)

**Recommendation**: Use virtual or hardware MFA.

## Account Structure

### Separate Accounts

Use separate AWS accounts for:
- **Production**: Live workloads
- **Development**: Development/testing
- **Staging**: Pre-production testing
- **Sandbox**: Experimentation

### Benefits

- **Isolation**: Prevent cross-environment access
- **Cost tracking**: Separate billing
- **Compliance**: Meet regulatory requirements
- **Security**: Limit blast radius

## Summary: Security Checklist

### Immediate Actions

- [ ] Delete root access keys
- [ ] Enable MFA on root account
- [ ] Create IAM user with minimal permissions
- [ ] Configure AWS CLI with IAM user
- [ ] Enable MFA on IAM user
- [ ] Set up IAM role for temporary credentials

### Ongoing Practices

- [ ] Rotate credentials every 90 days
- [ ] Review and remove unused permissions
- [ ] Enable CloudTrail logging
- [ ] Enable GuardDuty
- [ ] Tag all resources
- [ ] Use permission boundaries
- [ ] Monitor for root credential usage
- [ ] Review access logs regularly

### For CI/CD

- [ ] Use OIDC for GitHub Actions (recommended)
- [ ] Or use IAM role with temporary credentials
- [ ] Never hardcode credentials in workflows
- [ ] Use GitHub Secrets if OIDC unavailable
- [ ] Rotate CI/CD credentials regularly

## References

- [AWS Root User Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/root-user-best-practices.html)
- [IAM Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [Security Best Practices in IAM](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [Temporary Security Credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp.html)
- [GitHub Actions OIDC](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)

