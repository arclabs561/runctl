#!/bin/bash
# Verify that the test role setup is correct and secure
#
# Usage:
#   ./scripts/verify-setup.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

ROLE_NAME="trainctl-test-role"
POLICY_NAME="trainctl-test-policy"
BOUNDARY_NAME="trainctl-test-boundary"
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

echo -e "${GREEN}Verifying trainctl test role setup...${NC}"
echo ""

ERRORS=0
WARNINGS=0

# Check 1: Role exists
echo -e "${YELLOW}[1/8] Checking if role exists...${NC}"
if aws iam get-role --role-name "$ROLE_NAME" &>/dev/null; then
  echo -e "${GREEN}✓ Role exists${NC}"
else
  echo -e "${RED}✗ Role does not exist${NC}"
  ERRORS=$((ERRORS + 1))
fi
echo ""

# Check 2: Trust policy allows account root
echo -e "${YELLOW}[2/8] Checking trust policy...${NC}"
TRUST_POLICY=$(aws iam get-role --role-name "$ROLE_NAME" --query 'Role.AssumeRolePolicyDocument' --output json 2>/dev/null || echo "{}")
if echo "$TRUST_POLICY" | jq -e ".Statement[0].Principal.AWS | contains(\"arn:aws:iam::${ACCOUNT_ID}:root\")" &>/dev/null; then
  echo -e "${GREEN}✓ Trust policy allows account root${NC}"
else
  echo -e "${RED}✗ Trust policy may not allow account root${NC}"
  ERRORS=$((ERRORS + 1))
fi
if echo "$TRUST_POLICY" | jq -e '.Statement[0].Condition.StringEquals."sts:ExternalId" | contains("trainctl-test-env")' &>/dev/null; then
  echo -e "${GREEN}✓ ExternalId condition present${NC}"
else
  echo -e "${YELLOW}⚠ ExternalId condition missing (less secure)${NC}"
  WARNINGS=$((WARNINGS + 1))
fi
echo ""

# Check 3: Permissions policy attached
echo -e "${YELLOW}[3/8] Checking permissions policy...${NC}"
if aws iam get-role-policy --role-name "$ROLE_NAME" --policy-name "$POLICY_NAME" &>/dev/null; then
  echo -e "${GREEN}✓ Permissions policy attached${NC}"
  
  # Check for deny statement
  POLICY_DOC=$(aws iam get-role-policy --role-name "$ROLE_NAME" --policy-name "$POLICY_NAME" --query 'PolicyDocument' --output json)
  if echo "$POLICY_DOC" | jq -e '.Statement[] | select(.Effect == "Deny")' &>/dev/null; then
    echo -e "${GREEN}✓ Deny statement present (protects production)${NC}"
  else
    echo -e "${YELLOW}⚠ No deny statement found (production resources not protected)${NC}"
    WARNINGS=$((WARNINGS + 1))
  fi
else
  echo -e "${RED}✗ Permissions policy not attached${NC}"
  ERRORS=$((ERRORS + 1))
fi
echo ""

# Check 4: Permission boundary attached
echo -e "${YELLOW}[4/8] Checking permission boundary...${NC}"
BOUNDARY_ARN=$(aws iam get-role --role-name "$ROLE_NAME" --query 'Role.PermissionsBoundary.PermissionsBoundaryArn' --output text 2>/dev/null || echo "")
if [ -n "$BOUNDARY_ARN" ] && [ "$BOUNDARY_ARN" != "None" ]; then
  echo -e "${GREEN}✓ Permission boundary attached: $BOUNDARY_ARN${NC}"
  
  # Check boundary denies IAM
  BOUNDARY_VERSION=$(aws iam get-policy --policy-arn "$BOUNDARY_ARN" --query 'Policy.DefaultVersionId' --output text 2>/dev/null || echo "")
  if [ -n "$BOUNDARY_VERSION" ]; then
    BOUNDARY_DOC=$(aws iam get-policy-version --policy-arn "$BOUNDARY_ARN" --version-id "$BOUNDARY_VERSION" --query 'PolicyVersion.Document' --output json 2>/dev/null || echo "{}")
    if echo "$BOUNDARY_DOC" | jq -e '.Statement[] | select(.Effect == "Deny" and (.Action[]? | contains("iam")))' &>/dev/null; then
      echo -e "${GREEN}✓ Boundary denies IAM access${NC}"
    else
      echo -e "${YELLOW}⚠ Boundary may not deny IAM access${NC}"
      WARNINGS=$((WARNINGS + 1))
    fi
  fi
else
  echo -e "${YELLOW}⚠ Permission boundary not attached (less secure)${NC}"
  WARNINGS=$((WARNINGS + 1))
fi
echo ""

# Check 5: Can assume role
echo -e "${YELLOW}[5/8] Testing role assumption...${NC}"
if aws sts assume-role \
  --role-arn "arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}" \
  --role-session-name "verify-test-$(date +%s)" \
  --external-id "trainctl-test-env" \
  --duration-seconds 900 \
  --query 'Credentials.AccessKeyId' \
  --output text &>/dev/null; then
  echo -e "${GREEN}✓ Can assume role${NC}"
else
  echo -e "${RED}✗ Cannot assume role${NC}"
  ERRORS=$((ERRORS + 1))
fi
echo ""

# Check 6: Test S3 bucket exists
echo -e "${YELLOW}[6/8] Checking for test S3 buckets...${NC}"
TEST_BUCKETS=$(aws s3api list-buckets --query 'Buckets[?starts_with(Name, `trainctl-test-`)].Name' --output text 2>/dev/null || echo "")
if [ -n "$TEST_BUCKETS" ]; then
  BUCKET_COUNT=$(echo "$TEST_BUCKETS" | wc -w | tr -d ' ')
  echo -e "${GREEN}✓ Found $BUCKET_COUNT test bucket(s)${NC}"
else
  echo -e "${YELLOW}⚠ No test buckets found (this is OK)${NC}"
fi
echo ""

# Check 7: Verify permissions work with assumed role
echo -e "${YELLOW}[7/8] Testing permissions with assumed role...${NC}"
TEMP_CREDS=$(aws sts assume-role \
  --role-arn "arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}" \
  --role-session-name "verify-perms-$(date +%s)" \
  --external-id "trainctl-test-env" \
  --duration-seconds 900 \
  --output json 2>/dev/null)

if [ -n "$TEMP_CREDS" ]; then
  export AWS_ACCESS_KEY_ID=$(echo "$TEMP_CREDS" | jq -r '.Credentials.AccessKeyId')
  export AWS_SECRET_ACCESS_KEY=$(echo "$TEMP_CREDS" | jq -r '.Credentials.SecretAccessKey')
  export AWS_SESSION_TOKEN=$(echo "$TEMP_CREDS" | jq -r '.Credentials.SessionToken')
  export AWS_DEFAULT_REGION=us-east-1
  
  # Test EC2
  if aws ec2 describe-instances --max-items 1 --region us-east-1 &>/dev/null; then
    echo -e "${GREEN}✓ EC2 describe works${NC}"
  else
    echo -e "${RED}✗ EC2 describe failed${NC}"
    ERRORS=$((ERRORS + 1))
  fi
  
  # Test IAM is denied
  if aws iam list-users &>/dev/null; then
    echo -e "${RED}✗ IAM access not denied (security issue!)${NC}"
    ERRORS=$((ERRORS + 1))
  else
    echo -e "${GREEN}✓ IAM access correctly denied${NC}"
  fi
  
  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
else
  echo -e "${RED}✗ Could not assume role for permission test${NC}"
  ERRORS=$((ERRORS + 1))
fi
echo ""

# Check 8: Verify tags on role
echo -e "${YELLOW}[8/8] Checking role tags...${NC}"
ROLE_TAGS=$(aws iam list-role-tags --role-name "$ROLE_NAME" --query 'Tags' --output json 2>/dev/null || echo "[]")
if echo "$ROLE_TAGS" | jq -e '.[] | select(.Key == "Environment" and .Value == "test")' &>/dev/null; then
  echo -e "${GREEN}✓ Role has Environment=test tag${NC}"
else
  echo -e "${YELLOW}⚠ Role missing Environment=test tag${NC}"
  WARNINGS=$((WARNINGS + 1))
fi
echo ""

# Summary
echo "=========================================="
if [ $ERRORS -eq 0 ] && [ $WARNINGS -eq 0 ]; then
  echo -e "${GREEN}✓ All checks passed!${NC}"
  exit 0
elif [ $ERRORS -eq 0 ]; then
  echo -e "${YELLOW}⚠ Setup complete with $WARNINGS warning(s)${NC}"
  exit 0
else
  echo -e "${RED}✗ Setup has $ERRORS error(s) and $WARNINGS warning(s)${NC}"
  exit 1
fi

