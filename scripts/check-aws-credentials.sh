#!/bin/bash
# Check AWS credentials and security posture
#
# This script helps identify if you're using root credentials or
# if your IAM setup follows security best practices.
#
# Usage:
#   ./scripts/check-aws-credentials.sh

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}=== AWS Credentials Security Check ===${NC}"
echo ""

# Check 1: Get current identity
echo -e "${YELLOW}[1/6] Checking current AWS identity...${NC}"
IDENTITY=$(aws sts get-caller-identity 2>/dev/null || echo "ERROR")

if [[ "$IDENTITY" == "ERROR" ]]; then
  echo -e "${RED}✗ No AWS credentials configured${NC}"
  echo "  Configure credentials with: aws configure"
  exit 1
fi

ARN=$(echo "$IDENTITY" | jq -r '.Arn' 2>/dev/null || echo "")
ACCOUNT_ID=$(echo "$IDENTITY" | jq -r '.Account' 2>/dev/null || echo "")
USER_ID=$(echo "$IDENTITY" | jq -r '.UserId' 2>/dev/null || echo "")

echo "  ARN: $ARN"
echo "  Account: $ACCOUNT_ID"
echo "  User ID: $USER_ID"
echo ""

# Check 2: Detect root user
echo -e "${YELLOW}[2/6] Checking for root user...${NC}"
if echo "$ARN" | grep -q ":root"; then
  echo -e "${RED}✗ CRITICAL: Using root credentials!${NC}"
  echo ""
  echo "  SECURITY RISK: Root credentials have unlimited access"
  echo "  ACTION REQUIRED:"
  echo "    1. Create an IAM user with minimal permissions"
  echo "    2. Use IAM roles for temporary credentials"
  echo "    3. Delete root access keys"
  echo ""
  echo "  See: docs/AWS_ROOT_CREDENTIALS_MIGRATION.md"
  ROOT_USER=true
else
  echo -e "${GREEN}✓ Not using root credentials${NC}"
  ROOT_USER=false
fi
echo ""

# Check 3: Detect IAM user vs role
echo -e "${YELLOW}[3/6] Checking credential type...${NC}"
if echo "$ARN" | grep -q "assumed-role"; then
  echo -e "${GREEN}✓ Using IAM role (temporary credentials)${NC}"
  echo "  This is the recommended approach"
  ROLE_USER=true
elif echo "$ARN" | grep -q ":user/"; then
  echo -e "${YELLOW}⚠ Using IAM user (long-term credentials)${NC}"
  echo "  Consider migrating to IAM roles for better security"
  ROLE_USER=false
else
  echo -e "${YELLOW}⚠ Unknown credential type${NC}"
  ROLE_USER=false
fi
echo ""

# Check 4: Check for access keys
echo -e "${YELLOW}[4/6] Checking for access keys...${NC}"
if [[ "$ROOT_USER" == "true" ]]; then
  ACCESS_KEYS=$(aws iam list-access-keys --user-name root 2>/dev/null || echo "[]")
  KEY_COUNT=$(echo "$ACCESS_KEYS" | jq '.AccessKeyMetadata | length' 2>/dev/null || echo "0")
  if [[ "$KEY_COUNT" -gt 0 ]]; then
    echo -e "${RED}✗ Root user has $KEY_COUNT access key(s)${NC}"
    echo "  CRITICAL: Delete root access keys immediately"
    echo "  Run: aws iam delete-access-key --user-name root --access-key-id <KEY_ID>"
  else
    echo -e "${GREEN}✓ No root access keys found${NC}"
  fi
else
  USER_NAME=$(echo "$ARN" | sed -n 's/.*:user\/\([^/]*\).*/\1/p')
  if [[ -n "$USER_NAME" ]]; then
    ACCESS_KEYS=$(aws iam list-access-keys --user-name "$USER_NAME" 2>/dev/null || echo "[]")
    KEY_COUNT=$(echo "$ACCESS_KEYS" | jq '.AccessKeyMetadata | length' 2>/dev/null || echo "0")
    if [[ "$KEY_COUNT" -eq 0 ]]; then
      echo -e "${GREEN}✓ No access keys found for IAM user${NC}"
    elif [[ "$KEY_COUNT" -eq 1 ]]; then
      echo -e "${GREEN}✓ IAM user has 1 access key (good)${NC}"
      echo "  Consider using IAM roles for temporary credentials (even better)"
    else
      echo -e "${YELLOW}⚠ IAM user has $KEY_COUNT access key(s)${NC}"
      echo "  Consider deleting unused keys and using IAM roles instead"
    fi
  else
    echo -e "${GREEN}✓ Using role (no access keys)${NC}"
  fi
fi
echo ""

# Check 5: Check MFA
echo -e "${YELLOW}[5/6] Checking MFA status...${NC}"
if [[ "$ROOT_USER" == "true" ]]; then
  MFA_DEVICES=$(aws iam list-mfa-devices --user-name root 2>/dev/null || echo "[]")
  MFA_COUNT=$(echo "$MFA_DEVICES" | jq '.MFADevices | length' 2>/dev/null || echo "0")
  if [[ "$MFA_COUNT" -gt 0 ]]; then
    echo -e "${GREEN}✓ Root user has MFA enabled${NC}"
  else
    echo -e "${RED}✗ Root user does NOT have MFA enabled${NC}"
    echo "  CRITICAL: Enable MFA for root user immediately"
    echo "  See: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_root-user.html"
  fi
else
  MFA_DEVICES=$(aws iam list-mfa-devices --user-name "$USER_NAME" 2>/dev/null || echo "[]")
  MFA_COUNT=$(echo "$MFA_DEVICES" | jq '.MFADevices | length' 2>/dev/null || echo "0")
  if [[ "$MFA_COUNT" -gt 0 ]]; then
    MFA_TYPE=$(echo "$MFA_DEVICES" | jq -r '.MFADevices[0].SerialNumber' 2>/dev/null | grep -oE "(u2f|mfa)" || echo "")
    if [[ "$MFA_TYPE" == "u2f" ]]; then
      echo -e "${GREEN}✓ IAM user has hardware MFA enabled (excellent)${NC}"
    else
      echo -e "${GREEN}✓ IAM user has MFA enabled${NC}"
    fi
  else
    echo -e "${RED}✗ IAM user does NOT have MFA enabled${NC}"
    echo "  CRITICAL: Enable MFA for admin user immediately"
    echo "  Run: aws iam create-virtual-mfa-device --virtual-mfa-device-name admin-mfa"
  fi
fi
echo ""

# Check 6: Check permissions
echo -e "${YELLOW}[6/6] Checking permissions...${NC}"
if [[ "$ROOT_USER" == "true" ]]; then
  echo -e "${RED}✗ Root user has unlimited permissions${NC}"
  echo "  This is extremely dangerous for daily use"
else
  # Check attached policies
  ATTACHED=$(aws iam list-attached-user-policies --user-name "$USER_NAME" --query 'AttachedPolicies[].PolicyName' --output text 2>/dev/null || echo "")
  INLINE=$(aws iam list-user-policies --user-name "$USER_NAME" --query 'PolicyNames' --output text 2>/dev/null || echo "")
  GROUPS=$(aws iam list-groups-for-user --user-name "$USER_NAME" --query 'Groups[].GroupName' --output text 2>/dev/null || echo "")
  
  if echo "$ATTACHED $INLINE $GROUPS" | grep -qiE "AdministratorAccess|PowerUser|FullAccess"; then
    echo -e "${RED}✗ User has admin-level permissions${NC}"
    echo "  Policies: $ATTACHED $INLINE"
    echo "  Groups: $GROUPS"
    echo "  CRITICAL: Consider using least-privilege permissions"
  elif [[ -n "$ATTACHED" ]] || [[ -n "$INLINE" ]] || [[ -n "$GROUPS" ]]; then
    echo -e "${YELLOW}⚠ User has custom policies${NC}"
    echo "  Attached: ${ATTACHED:-none}"
    echo "  Inline: ${INLINE:-none}"
    echo "  Groups: ${GROUPS:-none}"
    echo "  Review policies to ensure least-privilege"
  else
    echo -e "${GREEN}✓ No policies attached (may have group permissions)${NC}"
  fi
fi
echo ""

# Summary
echo "=========================================="
if [[ "$ROOT_USER" == "true" ]]; then
  echo -e "${RED}✗ CRITICAL SECURITY ISSUES FOUND${NC}"
  echo ""
  echo "You are using root credentials. This is extremely dangerous."
  echo ""
  echo "IMMEDIATE ACTIONS REQUIRED:"
  echo "  1. Create an IAM user with minimal permissions"
  echo "  2. Set up IAM roles for temporary credentials"
  echo "  3. Delete root access keys"
  echo "  4. Enable MFA on root account"
  echo ""
  echo "See: docs/AWS_ROOT_CREDENTIALS_MIGRATION.md"
  exit 1
else
  echo -e "${GREEN}✓ Credentials check passed${NC}"
  echo ""
  echo "Recommendations:"
  if [[ "$ROLE_USER" != "true" ]]; then
    echo "  - Consider migrating to IAM roles for temporary credentials"
    echo "  - Use scripts/assume-test-role.sh for testing"
  fi
  echo "  - Enable MFA on your IAM user"
  echo "  - Use least-privilege permissions"
  echo "  - Rotate credentials regularly"
  exit 0
fi

