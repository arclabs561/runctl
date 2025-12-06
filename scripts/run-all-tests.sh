#!/bin/bash
# Run all AWS testing scripts in sequence
#
# This script runs:
# 1. Setup verification
# 2. Authentication tests
# 3. Security boundary tests
# 4. trainctl integration tests
#
# Usage:
#   ./scripts/run-all-tests.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  trainctl AWS Testing Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Test 1: Verify setup
echo -e "${YELLOW}[Test 1/4] Verifying setup...${NC}"
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if ./scripts/verify-setup.sh; then
  echo -e "${GREEN}✓ Setup verification passed${NC}"
  PASSED_TESTS=$((PASSED_TESTS + 1))
else
  echo -e "${RED}✗ Setup verification failed${NC}"
  FAILED_TESTS=$((FAILED_TESTS + 1))
  echo ""
  echo -e "${YELLOW}Note: Run ./scripts/setup-test-role.sh first if setup is incomplete${NC}"
fi
echo ""

# Test 2: Assume role and test authentication
echo -e "${YELLOW}[Test 2/4] Testing authentication...${NC}"
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if source scripts/assume-test-role.sh > /dev/null 2>&1 && ./scripts/test-auth.sh; then
  echo -e "${GREEN}✓ Authentication tests passed${NC}"
  PASSED_TESTS=$((PASSED_TESTS + 1))
else
  echo -e "${RED}✗ Authentication tests failed${NC}"
  FAILED_TESTS=$((FAILED_TESTS + 1))
fi
echo ""

# Test 3: Security boundaries
echo -e "${YELLOW}[Test 3/4] Testing security boundaries...${NC}"
TOTAL_TESTS=$((TOTAL_TESTS + 1))
# Re-assume role in case previous test cleared env
if source scripts/assume-test-role.sh > /dev/null 2>&1 && ./scripts/test-security-boundaries.sh; then
  echo -e "${GREEN}✓ Security boundary tests passed${NC}"
  PASSED_TESTS=$((PASSED_TESTS + 1))
else
  echo -e "${RED}✗ Security boundary tests failed${NC}"
  FAILED_TESTS=$((FAILED_TESTS + 1))
fi
echo ""

# Test 4: trainctl integration
echo -e "${YELLOW}[Test 4/4] Testing trainctl integration...${NC}"
TOTAL_TESTS=$((TOTAL_TESTS + 1))
# Re-assume role
if source scripts/assume-test-role.sh > /dev/null 2>&1 && ./scripts/test-trainctl-integration.sh; then
  echo -e "${GREEN}✓ trainctl integration tests passed${NC}"
  PASSED_TESTS=$((PASSED_TESTS + 1))
else
  echo -e "${RED}✗ trainctl integration tests failed${NC}"
  FAILED_TESTS=$((FAILED_TESTS + 1))
fi
echo ""

# Final summary
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  Test Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Total tests: $TOTAL_TESTS"
echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
if [ $FAILED_TESTS -gt 0 ]; then
  echo -e "${RED}Failed: $FAILED_TESTS${NC}"
else
  echo -e "${GREEN}Failed: $FAILED_TESTS${NC}"
fi
echo ""

if [ $FAILED_TESTS -eq 0 ]; then
  echo -e "${GREEN}✓ All tests passed!${NC}"
  echo ""
  echo "The AWS testing setup is working correctly:"
  echo "  ✓ IAM role configured properly"
  echo "  ✓ Authentication working"
  echo "  ✓ Security boundaries enforced"
  echo "  ✓ trainctl integration verified"
  exit 0
else
  echo -e "${RED}✗ Some tests failed${NC}"
  echo ""
  echo "Review the failures above and:"
  echo "  1. Check setup: ./scripts/verify-setup.sh"
  echo "  2. Re-run setup if needed: ./scripts/setup-test-role.sh"
  echo "  3. Check credentials: source scripts/assume-test-role.sh"
  exit 1
fi

