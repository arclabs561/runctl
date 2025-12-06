# Security Quick Start

## Current Status

Run `./scripts/check-aws-credentials.sh` to check your current security status.

## Quick Improvements

### 1. Use Temporary Credentials (Recommended)
```bash
source scripts/assume-test-role.sh
# Now using temporary credentials (expire in 1 hour)
```

### 2. Enable CloudTrail
```bash
./scripts/setup-cloudtrail.sh
```

### 3. Set Up OIDC for CI/CD
```bash
./scripts/setup-github-oidc.sh
```

## Security Checklist

- [ ] MFA enabled on admin user ✅
- [ ] Only 1 access key ✅
- [ ] CloudTrail enabled (run setup-cloudtrail.sh)
- [ ] Using IAM roles for development (optional)
- [ ] OIDC for CI/CD (optional)

## See Also

- `docs/AWS_SECURITY_RECOMMENDATIONS.md` - Detailed recommendations
- `docs/AWS_SECURITY_REVIEW_RESULTS.md` - Current status
- `scripts/check-aws-credentials.sh` - Security check script
