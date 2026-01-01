# Security Quick Start

## Setup

```bash
# Check current status
./scripts/check-aws-credentials.sh

# Use temporary credentials (recommended)
source scripts/assume-test-role.sh

# Enable CloudTrail
./scripts/setup-cloudtrail.sh

# Set up OIDC for CI/CD
./scripts/setup-github-oidc.sh
```

## Checklist

- [ ] MFA enabled on admin user
- [ ] Only 1 access key
- [ ] CloudTrail enabled
- [ ] Using IAM roles for development (optional)
- [ ] OIDC for CI/CD (optional)

## See Also

- [AWS_SECURITY_BEST_PRACTICES.md](AWS_SECURITY_BEST_PRACTICES.md)
- `scripts/check-aws-credentials.sh`
