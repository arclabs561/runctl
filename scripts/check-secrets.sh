#!/bin/bash
# Secret scanning script for runctl
#
# Checks for common secret patterns and verifies .gitignore is working
#
# Usage:
#   ./scripts/check-secrets.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Scanning for potential secrets...${NC}"
echo ""

ISSUES=0

# Check 1: AWS Access Keys
echo -e "${YELLOW}[1/8] Checking for AWS access keys...${NC}"
if git grep -E "AKIA[0-9A-Z]{16}" -- . :!scripts/check-secrets.sh :!docs/ 2>/dev/null | grep -v "^Binary" | head -5; then
  echo -e "${RED}✗ Found potential AWS access keys${NC}"
  ISSUES=$((ISSUES + 1))
else
  echo -e "${GREEN}✓ No AWS access keys found${NC}"
fi
echo ""

# Check 2: AWS Secret Keys
echo -e "${YELLOW}[2/8] Checking for AWS secret keys...${NC}"
if git grep -E "[0-9a-zA-Z/+]{40}" -- . :!scripts/check-secrets.sh :!docs/ 2>/dev/null | grep -v "^Binary" | head -5; then
  echo -e "${YELLOW}⚠ Found potential secret patterns (may be false positives)${NC}"
else
  echo -e "${GREEN}✓ No obvious secret keys found${NC}"
fi
echo ""

# Check 3: Private Keys
echo -e "${YELLOW}[3/8] Checking for private keys...${NC}"
if git grep -E "BEGIN.*PRIVATE KEY|BEGIN RSA PRIVATE KEY|BEGIN EC PRIVATE KEY" -- . :!scripts/check-secrets.sh :!docs/ :!.github/ :!tests/ 2>/dev/null | grep -v "^Binary" | grep -v "grep -E"; then
  echo -e "${RED}✗ Found private keys${NC}"
  ISSUES=$((ISSUES + 1))
else
  echo -e "${GREEN}✓ No private keys found${NC}"
fi
echo ""

# Check 4: Hardcoded API keys
echo -e "${YELLOW}[4/8] Checking for hardcoded API keys...${NC}"
# Check for common API key patterns but exclude documentation
if git grep -iE "api[_-]?key\s*=\s*[\"'][^\"']{10,}" -- . :!scripts/check-secrets.sh :!docs/ :!README.md 2>/dev/null | grep -v "api_key = None" | grep -v "Option<String>" | head -5; then
  echo -e "${YELLOW}⚠ Found potential hardcoded API keys (review manually)${NC}"
else
  echo -e "${GREEN}✓ No hardcoded API keys found${NC}"
fi
echo ""

# Check 5: Environment files
echo -e "${YELLOW}[5/8] Checking for .env files in git...${NC}"
if git ls-files | grep -E "\.env$|\.env\."; then
  echo -e "${RED}✗ Found .env files in git${NC}"
  ISSUES=$((ISSUES + 1))
else
  echo -e "${GREEN}✓ No .env files tracked${NC}"
fi
echo ""

# Check 6: Credential files
echo -e "${YELLOW}[6/8] Checking for credential files...${NC}"
if git ls-files | grep -E "\.(pem|key|p12|pfx|credential|secret)$"; then
  echo -e "${RED}✗ Found credential files in git${NC}"
  ISSUES=$((ISSUES + 1))
else
  echo -e "${GREEN}✓ No credential files tracked${NC}"
fi
echo ""

# Check 7: .gitignore coverage
echo -e "${YELLOW}[7/8] Checking .gitignore coverage...${NC}"
MISSING_IGNORES=0
for pattern in ".env" ".pem" ".key" "credentials" "*.secret" ".runctl.toml"; do
  if ! grep -q "$pattern" .gitignore 2>/dev/null; then
    echo -e "${YELLOW}⚠ .gitignore missing: $pattern${NC}"
    MISSING_IGNORES=$((MISSING_IGNORES + 1))
  fi
done
if [ $MISSING_IGNORES -eq 0 ]; then
  echo -e "${GREEN}✓ .gitignore covers common secret patterns${NC}"
fi
echo ""

# Check 8: Config files with secrets
echo -e "${YELLOW}[8/8] Checking config file handling...${NC}"
if grep -q "\.runctl\.toml" .gitignore; then
  echo -e "${GREEN}✓ .runctl.toml is in .gitignore${NC}"
else
  echo -e "${YELLOW}⚠ .runctl.toml not in .gitignore${NC}"
  ISSUES=$((ISSUES + 1))
fi
echo ""

# Summary
echo "=========================================="
if [ $ISSUES -eq 0 ]; then
  echo -e "${GREEN}✓ No secrets found!${NC}"
  echo ""
  echo "Recommendations:"
  echo "  1. Set up gitleaks or similar for automated scanning"
  echo "  2. Use pre-commit hooks to prevent secret commits"
  echo "  3. Review GitHub Actions secrets if using CI/CD"
  exit 0
else
  echo -e "${RED}✗ Found $ISSUES potential issue(s)${NC}"
  echo ""
  echo "CRITICAL: Review the issues above before pushing to public repo!"
  exit 1
fi

