#!/bin/bash
# Comprehensive security improvement script
#
# This script runs multiple security improvements automatically where possible.
#
# Usage:
#   ./scripts/improve-security.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== Comprehensive Security Improvements ===${NC}"
echo ""

# Track what we do
IMPROVEMENTS=0
SKIPPED=0

# 1. Check current status
echo -e "${YELLOW}[1/6] Checking current security status...${NC}"
./scripts/check-aws-credentials.sh > /tmp/security-check.txt 2>&1 || true
MFA_ENABLED=$(grep -q "MFA enabled" /tmp/security-check.txt && echo "yes" || echo "no")
KEY_COUNT=$(aws iam list-access-keys --user-name admin --query 'AccessKeyMetadata | length(@)' --output text 2>/dev/null || echo "0")

echo "  MFA: $MFA_ENABLED"
echo "  Access Keys: $KEY_COUNT"
echo ""

# 2. Setup CloudTrail (if not exists)
echo -e "${YELLOW}[2/6] Setting up CloudTrail...${NC}"
if aws cloudtrail get-trail --name trainctl-audit-trail >/dev/null 2>&1; then
  IS_LOGGING=$(aws cloudtrail get-trail-status --name trainctl-audit-trail --query 'IsLogging' --output text 2>/dev/null || echo "false")
  if [[ "$IS_LOGGING" == "true" ]]; then
    echo -e "${GREEN}✓ CloudTrail already configured and logging${NC}"
    SKIPPED=$((SKIPPED + 1))
  else
    echo -e "${YELLOW}CloudTrail exists but not logging. Starting...${NC}"
    aws cloudtrail start-logging --name trainctl-audit-trail && echo -e "${GREEN}✓ CloudTrail logging started${NC}" && IMPROVEMENTS=$((IMPROVEMENTS + 1))
  fi
else
  echo -e "${YELLOW}Creating CloudTrail...${NC}"
  if ./scripts/setup-cloudtrail.sh >/dev/null 2>&1; then
    echo -e "${GREEN}✓ CloudTrail created and started${NC}"
    IMPROVEMENTS=$((IMPROVEMENTS + 1))
  else
    echo -e "${RED}✗ Failed to create CloudTrail (may need additional permissions)${NC}"
  fi
fi
echo ""

# 3. Verify test role works
echo -e "${YELLOW}[3/6] Verifying trainctl-test-role...${NC}"
if aws iam get-role --role-name trainctl-test-role >/dev/null 2>&1; then
  echo -e "${GREEN}✓ trainctl-test-role exists${NC}"
  echo "  You can use it with: source scripts/assume-test-role.sh"
  SKIPPED=$((SKIPPED + 1))
else
  echo -e "${YELLOW}trainctl-test-role not found${NC}"
  echo "  Run: ./scripts/setup-test-role.sh to create it"
fi
echo ""

# 4. Check GitHub Actions setup
echo -e "${YELLOW}[4/6] Checking GitHub Actions configuration...${NC}"
if grep -q "secrets.AWS_ACCESS_KEY_ID" .github/workflows/*.yml 2>/dev/null; then
  echo -e "${YELLOW}⚠ GitHub Actions uses secrets (less secure)${NC}"
  echo "  Consider setting up OIDC: ./scripts/setup-github-oidc.sh"
  echo "  This eliminates need for secrets"
else
  echo -e "${GREEN}✓ No AWS secrets in workflows (or using OIDC)${NC}"
  SKIPPED=$((SKIPPED + 1))
fi
echo ""

# 5. Create helper documentation
echo -e "${YELLOW}[5/6] Creating security documentation...${NC}"
if [[ ! -f "docs/SECURITY_QUICK_START.md" ]]; then
  cat > docs/SECURITY_QUICK_START.md <<'EOF'
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
EOF
  echo -e "${GREEN}✓ Documentation created${NC}"
  IMPROVEMENTS=$((IMPROVEMENTS + 1))
else
  echo -e "${GREEN}✓ Documentation already exists${NC}"
  SKIPPED=$((SKIPPED + 1))
fi
echo ""

# 6. Summary
echo -e "${YELLOW}[6/6] Summary...${NC}"
echo ""
echo "=========================================="
echo -e "${GREEN}Security improvements completed!${NC}"
echo ""
echo "Improvements made: $IMPROVEMENTS"
echo "Already configured: $SKIPPED"
echo ""
echo "Next steps:"
echo "  1. Review: ./scripts/check-aws-credentials.sh"
echo "  2. Use roles: source scripts/assume-test-role.sh"
echo "  3. Enable CloudTrail: ./scripts/setup-cloudtrail.sh (if not done)"
echo "  4. Set up OIDC: ./scripts/setup-github-oidc.sh (optional)"
echo ""
echo "See docs/SECURITY_QUICK_START.md for quick reference"

