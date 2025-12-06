#!/bin/bash
# Test script to verify AWS authentication and permissions
#
# Usage:
#   source scripts/assume-test-role.sh
#   ./scripts/test-auth.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Testing AWS authentication and permissions...${NC}"
echo ""

# Check if credentials are set
if [ -z "${AWS_ACCESS_KEY_ID:-}" ] || [ -z "${AWS_SECRET_ACCESS_KEY:-}" ] || [ -z "${AWS_SESSION_TOKEN:-}" ]; then
  echo -e "${RED}✗ Error: AWS credentials not set${NC}"
  echo "Run: source scripts/assume-test-role.sh"
  exit 1
fi

# Test 1: Verify identity
echo -e "${YELLOW}[1/6] Testing identity...${NC}"
IDENTITY=$(aws sts get-caller-identity --output json)
echo "Identity:"
echo "$IDENTITY" | jq '.'
ROLE_ARN=$(echo "$IDENTITY" | jq -r '.Arn')
if [[ "$ROLE_ARN" == *"trainctl-test-role"* ]]; then
  echo -e "${GREEN}✓ Using test role${NC}"
else
  echo -e "${RED}✗ Warning: Not using test role (using: $ROLE_ARN)${NC}"
fi
echo ""

# Set default region
export AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-us-east-1}"

# Test 2: EC2 permissions
echo -e "${YELLOW}[2/6] Testing EC2 permissions...${NC}"
if aws ec2 describe-instances --max-items 1 --region "$AWS_DEFAULT_REGION" &>/dev/null; then
  echo -e "${GREEN}✓ Can describe instances${NC}"
else
  echo -e "${RED}✗ Cannot describe instances${NC}"
fi
if aws ec2 describe-instance-types --instance-types t3.micro --region "$AWS_DEFAULT_REGION" &>/dev/null; then
  echo -e "${GREEN}✓ Can describe instance types${NC}"
else
  echo -e "${RED}✗ Cannot describe instance types${NC}"
fi
echo ""

# Test 3: EBS permissions
echo -e "${YELLOW}[3/6] Testing EBS permissions...${NC}"
if aws ec2 describe-volumes --max-items 1 --region "$AWS_DEFAULT_REGION" &>/dev/null; then
  echo -e "${GREEN}✓ Can describe volumes${NC}"
else
  echo -e "${RED}✗ Cannot describe volumes${NC}"
fi
echo ""

# Test 4: S3 permissions
echo -e "${YELLOW}[4/6] Testing S3 permissions...${NC}"
# ListAllMyBuckets should be denied (good - isolation working)
if aws s3api list-buckets &>/dev/null; then
  echo -e "${YELLOW}⚠ Can list all buckets (may be OK if no sensitive buckets)${NC}"
  # Try to find test buckets
  TEST_BUCKETS=$(aws s3api list-buckets --query 'Buckets[?starts_with(Name, `trainctl-test-`)].Name' --output text 2>/dev/null || echo "")
else
  echo -e "${GREEN}✓ Cannot list all buckets (good - isolation working)${NC}"
  # We can't list buckets, so we can't find test buckets this way
  # This is expected and correct behavior
  TEST_BUCKETS=""
fi

# If we found test buckets, verify access
if [ -n "$TEST_BUCKETS" ]; then
  FIRST_BUCKET=$(echo "$TEST_BUCKETS" | awk '{print $1}')
  if aws s3 ls "s3://$FIRST_BUCKET" &>/dev/null; then
    echo -e "${GREEN}✓ Can access test bucket: $FIRST_BUCKET${NC}"
  else
    echo -e "${RED}✗ Cannot access test bucket${NC}"
  fi
else
  echo -e "${YELLOW}⚠ Cannot enumerate test buckets (expected if ListAllMyBuckets denied)${NC}"
  echo "  This is correct - S3 isolation is working"
fi
echo ""

# Test 5: SSM permissions
echo -e "${YELLOW}[5/6] Testing SSM permissions...${NC}"
if aws ssm describe-instance-information --max-items 1 &>/dev/null; then
  echo -e "${GREEN}✓ Can describe SSM instance information${NC}"
else
  echo -e "${YELLOW}⚠ Cannot describe SSM instances (may not have any managed instances)${NC}"
fi
echo ""

# Test 6: Verify denied permissions (IAM)
echo -e "${YELLOW}[6/6] Testing permission boundary (should deny IAM)...${NC}"
if aws iam list-users &>/dev/null; then
  echo -e "${RED}✗ WARNING: Can access IAM (permission boundary may not be working)${NC}"
else
  echo -e "${GREEN}✓ IAM access correctly denied (permission boundary working)${NC}"
fi
echo ""

# Summary
echo -e "${GREEN}Authentication test complete!${NC}"
echo ""
echo "If all tests passed, you can now use trainctl:"
echo "  cargo run -- aws instances list"
echo "  cargo run -- aws create --instance-type t3.micro"

