#!/bin/bash
# Cleanup script for trainctl test IAM role and resources
#
# Usage:
#   ./scripts/cleanup-test-role.sh [--force]

set -euo pipefail

FORCE="${1:-}"

ROLE_NAME="trainctl-test-role"
POLICY_NAME="trainctl-test-policy"
BOUNDARY_NAME="trainctl-test-boundary"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

if [ "$FORCE" != "--force" ]; then
  echo -e "${YELLOW}This will delete:${NC}"
  echo "  - IAM Role: $ROLE_NAME"
  echo "  - Permissions Policy: $POLICY_NAME"
  echo "  - Permission Boundary: $BOUNDARY_NAME"
  echo "  - All test S3 buckets (trainctl-test-*)"
  echo ""
  read -p "Are you sure? (type 'yes' to confirm): " confirm
  if [ "$confirm" != "yes" ]; then
    echo "Cancelled."
    exit 0
  fi
fi

echo -e "${YELLOW}Cleaning up test resources...${NC}"

# Delete role policy
if aws iam get-role-policy --role-name "$ROLE_NAME" --policy-name "$POLICY_NAME" &>/dev/null; then
  echo "Deleting role policy..."
  aws iam delete-role-policy --role-name "$ROLE_NAME" --policy-name "$POLICY_NAME"
fi

# Remove permission boundary
if aws iam get-role --role-name "$ROLE_NAME" &>/dev/null; then
  echo "Removing permission boundary..."
  aws iam delete-role-permissions-boundary --role-name "$ROLE_NAME" || true
fi

# Delete role
if aws iam get-role --role-name "$ROLE_NAME" &>/dev/null; then
  echo "Deleting IAM role..."
  aws iam delete-role --role-name "$ROLE_NAME"
fi

# Delete boundary policy (need to delete all versions first)
if aws iam get-policy --policy-arn "arn:aws:iam::$(aws sts get-caller-identity --query Account --output text):policy/${BOUNDARY_NAME}" &>/dev/null; then
  echo "Deleting permission boundary policy..."
  POLICY_ARN="arn:aws:iam::$(aws sts get-caller-identity --query Account --output text):policy/${BOUNDARY_NAME}"
  
  # List and delete all policy versions
  for version in $(aws iam list-policy-versions --policy-arn "$POLICY_ARN" --query 'Versions[?IsDefaultVersion==`false`].VersionId' --output text); do
    aws iam delete-policy-version --policy-arn "$POLICY_ARN" --version-id "$version" || true
  done
  
  aws iam delete-policy --policy-arn "$POLICY_ARN" || true
fi

# Delete test S3 buckets
echo "Finding test S3 buckets..."
# Need to use credentials that can list buckets
if [ -n "${AWS_ACCESS_KEY_ID:-}" ]; then
  TEST_BUCKETS=$(aws s3api list-buckets --query 'Buckets[?starts_with(Name, `trainctl-test-`)].Name' --output text 2>/dev/null || echo "")
  if [ -n "$TEST_BUCKETS" ]; then
    for bucket in $TEST_BUCKETS; do
      if [ -n "$bucket" ]; then
        echo "Deleting bucket: $bucket"
        aws s3 rb "s3://$bucket" --force 2>/dev/null || true
      fi
    done
  else
    echo "No test buckets found"
  fi
else
  echo "Skipping bucket cleanup (no credentials - may need to assume role first)"
fi

echo ""
echo -e "${GREEN}✓ Cleanup complete!${NC}"

