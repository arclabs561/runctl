# AWS Security Recommendations for Your Account

**Generated**: Based on your current AWS configuration  
**Account**: 512827140002  
**User**: admin

## Executive Summary

Based on the analysis of your AWS configuration, here are the prioritized security recommendations:

### Critical (Do Immediately)
1. 🔴 **Enable MFA** - **CRITICAL**: Admin user with AdministratorAccess has NO MFA enabled
2. 🔴 **Delete unused access key** - Key `AKIAXXXXXXXXXXXXXXXX` from 2024 is not in use, delete it [REDACTED]
3. 🔴 **Create limited-permission user** - Admin user has FULL AdministratorAccess, create runctl-specific user

### High Priority (Do This Week)
4. ✅ **Migrate to IAM roles** - Use temporary credentials instead of long-term keys
5. ✅ **Apply least privilege** - Create specific policies for runctl operations
6. ✅ **Enable CloudTrail** - Enable audit logging for all API calls

### Medium Priority (Do This Month)
7. ✅ **Set up OIDC for CI/CD** - Use GitHub Actions OIDC instead of secrets
8. ✅ **Implement permission boundaries** - Add extra security layer
9. ✅ **Tag resources** - Ensure all resources are properly tagged

## Detailed Analysis

### Current Configuration

**User**: `admin`  
**Type**: IAM User (not root - good!)  
**Group**: `admingroup`  
**Permissions**: 🔴 **AdministratorAccess** (FULL admin access - unlimited permissions)  
**Access Keys**: 2 keys configured [REDACTED]
  - `AKIAXXXXXXXXXXXXXXXX` (2025-12-02) - **CURRENTLY IN USE** [REDACTED]
  - `AKIAXXXXXXXXXXXXXXXX` (2024-02-15) - **UNUSED - CAN BE DELETED** [REDACTED]  
**MFA**: ❌ **NOT ENABLED** (CRITICAL - admin user with no MFA)

### Permission Analysis

To check your exact permissions:

```bash
# List all policies attached to admin user
aws iam list-attached-user-policies --user-name admin
aws iam list-user-policies --user-name admin

# List groups admin belongs to
aws iam list-groups-for-user --user-name admin

# For each group, check policies
aws iam list-attached-group-policies --group-name <group-name>
```

**Action Required**: Review these policies to determine if admin-level access is necessary.

## Specific Recommendations

### 1. Enable MFA on Admin User

**Why**: MFA adds a critical security layer, especially for admin accounts.

**How**:
```bash
# Create virtual MFA device
aws iam create-virtual-mfa-device \
  --virtual-mfa-device-name admin-mfa \
  --outfile QRCode.png \
  --bootstrap-method QRCodePNG

# Scan QR code with authenticator app (Google Authenticator, Authy)
# Then enable MFA
aws iam enable-mfa-device \
  --user-name admin \
  --serial-number arn:aws:iam::512827140002:mfa/admin-mfa \
  --authentication-code-1 <CODE1> \
  --authentication-code-2 <CODE2>
```

**Priority**: Critical

### 2. Create Limited-Permission User for runctl

**Current Risk**: Admin user has `AdministratorAccess` via `admingroup` - **FULL UNLIMITED ACCESS**

**This is extremely dangerous** because:
- Any compromise = total account compromise
- No audit trail separation
- Violates least-privilege principle
- No way to limit damage if credentials leak

**Recommended Action**:

1. **Create runctl-specific policy**:
```bash
# Use the policy from scripts/setup-test-role.sh
# Modify for production use (remove test tag requirements)
```

2. **Create dedicated IAM user for runctl**:
```bash
aws iam create-user --user-name runctl-user

# Attach runctl-specific policy
aws iam attach-user-policy \
  --user-name runctl-user \
  --policy-arn arn:aws:iam::512827140002:policy/runctl-policy
```

3. **Use new user for runctl operations**:
```bash
aws configure --profile runctl
# Enter runctl-user credentials
export AWS_PROFILE=runctl

# Or update default profile
aws configure
# Enter runctl-user credentials
```

4. **Keep admin user for admin tasks only**:
   - Use admin user only when you need full admin access
   - Use runctl-user for daily runctl operations
   - This limits blast radius if credentials are compromised

**Priority**: Critical (do this week)

### 3. Migrate to IAM Roles (Temporary Credentials)

**Why**: Temporary credentials are more secure than long-term access keys.

**Current Setup**: You already have `runctl-test-role` configured.

**Action**:
```bash
# Use existing test role for development
source scripts/assume-test-role.sh

# Verify you're using role
aws sts get-caller-identity
# Should show: arn:aws:sts::512827140002:assumed-role/runctl-test-role/...
```

**For Production**: Create a production role with appropriate permissions:
```bash
# Copy and modify scripts/setup-test-role.sh
# Remove test tag requirements
# Adjust permissions for production needs
```

**Priority**: High

### 4. Delete Unused Access Key

**Current**: 2 access keys for admin user [REDACTED]
- `AKIAXXXXXXXXXXXXXXXX` (2025-12-02) - **IN USE** (configured in ~/.aws/credentials) [REDACTED]
- `AKIAXXXXXXXXXXXXXXXX` (2024-02-15) - **UNUSED** (can be deleted) [REDACTED]

**Action**:
```bash
# Delete the unused 2024 key
aws iam delete-access-key \
  --user-name admin \
  --access-key-id <ACCESS_KEY_ID>  # [REDACTED - use actual key ID]
```

**Why**: Having unused access keys increases attack surface. If the old key was ever compromised, it could still be used.

**Best Practice**: Use only 1 access key per user. Keep second key only during rotation.

**Priority**: Critical (immediate action)

### 5. Enable CloudTrail

**Why**: Audit logging is essential for security and compliance.

**Check Status**:
```bash
aws cloudtrail describe-trails
```

**Enable if not enabled**:
```bash
# Create S3 bucket for logs
aws s3 mb s3://runctl-cloudtrail-logs-$(date +%s)

# Create trail
aws cloudtrail create-trail \
  --name runctl-audit-trail \
  --s3-bucket-name runctl-cloudtrail-logs-*

# Start logging
aws cloudtrail start-logging --name runctl-audit-trail
```

**Priority**: High

### 6. Set Up OIDC for GitHub Actions

**Why**: Eliminates need to store AWS credentials in GitHub Secrets.

**Current**: GitHub Actions references `${{ secrets.AWS_ACCESS_KEY_ID }}`

**Better Approach**: Use OIDC

```yaml
# .github/workflows/test.yml
- name: Configure AWS credentials
  uses: aws-actions/configure-aws-credentials@v4
  with:
    role-to-assume: arn:aws:iam::512827140002:role/github-actions-role
    aws-region: us-east-1
```

**Setup Required**:
1. Create IAM role for GitHub Actions
2. Configure GitHub as OIDC provider in AWS
3. Update workflow to use OIDC

**Priority**: Medium (if using CI/CD)

### 7. Implement Permission Boundaries

**Why**: Prevents privilege escalation even if policies are misconfigured.

**Action**: Already implemented in `runctl-test-role`. Apply to production role as well.

**Priority**: Medium

### 8. Enable GuardDuty

**Why**: Detects suspicious activity, including root credential usage.

**Check**:
```bash
aws guardduty list-detectors
```

**Enable**:
```bash
aws guardduty create-detector --enable
```

**Priority**: Medium

## Migration Plan

### Phase 1: Immediate (Today)
1. ✅ Enable MFA on admin user
2. ✅ Review admin user permissions
3. ✅ Delete unused access keys

### Phase 2: This Week
4. ✅ Create runctl-specific IAM user
5. ✅ Migrate to using IAM roles for development
6. ✅ Enable CloudTrail

### Phase 3: This Month
7. ✅ Set up OIDC for GitHub Actions
8. ✅ Create production IAM role
9. ✅ Enable GuardDuty
10. ✅ Implement resource tagging strategy

## Security Checklist

### Immediate Actions
- [ ] Enable MFA on admin user
- [ ] Review admin user permissions
- [ ] Delete unused access keys
- [ ] Verify CloudTrail is enabled

### Short-term (This Week)
- [ ] Create runctl-specific IAM user
- [ ] Create runctl-specific policy (least privilege)
- [ ] Migrate to IAM roles for development
- [ ] Test with temporary credentials

### Long-term (This Month)
- [ ] Set up OIDC for CI/CD
- [ ] Create production IAM role
- [ ] Enable GuardDuty
- [ ] Implement comprehensive tagging
- [ ] Set up CloudWatch alarms

## Commands Reference

### Check Current Status
```bash
# Run comprehensive check
./scripts/check-aws-credentials.sh

# Check specific items
aws iam get-user --user-name admin
aws iam list-attached-user-policies --user-name admin
aws iam list-mfa-devices --user-name admin
aws iam list-access-keys --user-name admin
aws cloudtrail describe-trails
```

### Security Improvements
```bash
# Enable MFA (see section 1 above)
# Create runctl user (see section 2 above)
# Use temporary credentials
source scripts/assume-test-role.sh
```

## Next Steps

1. **Run the check script**: `./scripts/check-aws-credentials.sh`
2. **Review this document**: Understand your current state
3. **Prioritize actions**: Start with Critical items
4. **Implement gradually**: Don't break existing workflows
5. **Test thoroughly**: Verify each change works

## Questions to Answer

Before making changes, answer:

1. **Does admin user need full admin access?**
   - If yes: Keep but enable MFA and use roles for daily work
   - If no: Create limited-permission user for runctl

2. **Are both access keys in use?**
   - Check AWS CLI config: `cat ~/.aws/credentials`
   - Check environment variables: `env | grep AWS`
   - Delete unused keys

3. **Is CloudTrail enabled?**
   - Check: `aws cloudtrail describe-trails`
   - Enable if not: Critical for security

4. **Do you use GitHub Actions?**
   - If yes: Set up OIDC (more secure than secrets)
   - If no: Skip OIDC setup

## Summary

**Current State**: Using IAM user (good), but may have admin permissions (risky)

**Target State**: 
- IAM roles for temporary credentials
- Least-privilege permissions
- MFA enabled
- CloudTrail logging
- OIDC for CI/CD

**Timeline**: 
- Critical items: Today
- High priority: This week
- Medium priority: This month

