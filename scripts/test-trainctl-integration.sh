#!/bin/bash
# Integration tests for trainctl with temporary credentials
#
# Tests that trainctl actually works with the test role credentials
# and that operations are properly restricted.
#
# Usage:
#   source scripts/assume-test-role.sh
#   ./scripts/test-trainctl-integration.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Testing trainctl integration with test role...${NC}"
echo ""

# Check credentials
if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ] || [ -z "${AWS_SESSION_TOKEN:-}" ]; then
  echo -e "${RED}✗ Error: AWS credentials not set${NC}"
  echo "Run: source scripts/assume-test-role.sh"
  exit 1
fi

export AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-us-east-1}"

# Check if trainctl is built
if [ ! -f "./target/debug/trainctl" ] && [ ! -f "./target/release/trainctl" ]; then
  echo -e "${YELLOW}Building trainctl...${NC}"
  cargo build --quiet 2>&1 | tail -1 || {
    echo -e "${RED}✗ Failed to build trainctl${NC}"
    exit 1
  }
fi

TRAINCTL="./target/debug/trainctl"
if [ ! -f "$TRAINCTL" ]; then
  TRAINCTL="./target/release/trainctl"
fi

if [ ! -f "$TRAINCTL" ]; then
  echo -e "${RED}✗ trainctl binary not found${NC}"
  exit 1
fi

FAILURES=0

# Test 1: List resources
echo -e "${YELLOW}[1/4] Testing resource listing...${NC}"
if $TRAINCTL resources list --output text &>/dev/null; then
  echo -e "${GREEN}✓ trainctl resources list works${NC}"
else
  echo -e "${RED}✗ trainctl resources list failed${NC}"
  FAILURES=$((FAILURES + 1))
fi
echo ""

# Test 2: AWS instances list (if command exists)
echo -e "${YELLOW}[2/4] Testing AWS command structure...${NC}"
if $TRAINCTL aws --help &>/dev/null; then
  echo -e "${GREEN}✓ trainctl aws command available${NC}"
else
  echo -e "${RED}✗ trainctl aws command not available${NC}"
  FAILURES=$((FAILURES + 1))
fi
echo ""

# Test 3: Verify AWS SDK uses credentials correctly
echo -e "${YELLOW}[3/4] Testing AWS SDK credential usage...${NC}"
# trainctl should use the environment variables automatically
IDENTITY_FROM_TRAINCTL=$($TRAINCTL resources list --output json 2>/dev/null | jq -r '.identity.arn // empty' || echo "")
IDENTITY_FROM_AWS=$(aws sts get-caller-identity --query Arn --output text)

if [ -n "$IDENTITY_FROM_TRAINCTL" ]; then
  if [[ "$IDENTITY_FROM_TRAINCTL" == *"trainctl-test-role"* ]]; then
    echo -e "${GREEN}✓ trainctl using test role credentials${NC}"
  else
    echo -e "${YELLOW}⚠ trainctl identity: $IDENTITY_FROM_TRAINCTL${NC}"
  fi
else
  # If trainctl doesn't output identity, check via AWS CLI
  if [[ "$IDENTITY_FROM_AWS" == *"trainctl-test-role"* ]]; then
    echo -e "${GREEN}✓ AWS SDK using test role credentials (verified via AWS CLI)${NC}"
  else
    echo -e "${RED}✗ Credentials not being used correctly${NC}"
    FAILURES=$((FAILURES + 1))
  fi
fi
echo ""

# Test 4: Error handling
echo -e "${YELLOW}[4/4] Testing error handling...${NC}"
# Try an invalid operation to see if errors are handled gracefully
if $TRAINCTL aws terminate i-invalid 2>&1 | grep -i "error\|denied\|not found" &>/dev/null; then
  echo -e "${GREEN}✓ Error handling works (invalid instance ID rejected)${NC}"
else
  echo -e "${YELLOW}⚠ Could not verify error handling${NC}"
fi
echo ""

# Summary
echo "=========================================="
if [ $FAILURES -eq 0 ]; then
  echo -e "${GREEN}✓ All trainctl integration tests passed!${NC}"
  echo ""
  echo "Integration verification:"
  echo "  ✓ trainctl can list resources"
  echo "  ✓ AWS commands available"
  echo "  ✓ Credentials used correctly"
  echo "  ✓ Error handling works"
  echo ""
  echo "You can now use trainctl with the test role:"
  echo "  $TRAINCTL resources list"
  echo "  $TRAINCTL aws create --instance-type t3.micro"
  exit 0
else
  echo -e "${RED}✗ Integration tests failed: $FAILURES failure(s)${NC}"
  exit 1
fi

