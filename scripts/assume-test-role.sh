#!/bin/bash
# Assume IAM role and export temporary credentials for testing
#
# Usage:
#   source scripts/assume-test-role.sh
#   # or
#   . scripts/assume-test-role.sh
#
# This will export AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, and AWS_SESSION_TOKEN
# as environment variables that will be used by the AWS SDK.

set -euo pipefail

# Configuration
ROLE_ARN="${TRAINCTL_TEST_ROLE_ARN:-arn:aws:iam::$(aws sts get-caller-identity --query Account --output text):role/runctl-test-role}"
SESSION_NAME="runctl-test-$(date +%s)"
DURATION_SECONDS="${TRAINCTL_TEST_SESSION_DURATION:-3600}"  # 1 hour default
EXTERNAL_ID="${TRAINCTL_TEST_EXTERNAL_ID:-runctl-test-env}"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Assuming IAM role for testing...${NC}"
echo "Role ARN: $ROLE_ARN"
echo "Session: $SESSION_NAME"
echo "Duration: $DURATION_SECONDS seconds ($(($DURATION_SECONDS / 60)) minutes)"
echo ""

# Assume the role
RESPONSE=$(aws sts assume-role \
  --role-arn "$ROLE_ARN" \
  --role-session-name "$SESSION_NAME" \
  --duration-seconds "$DURATION_SECONDS" \
  --external-id "$EXTERNAL_ID" \
  --output json)

if [ $? -ne 0 ]; then
  echo -e "${YELLOW}Error: Failed to assume role${NC}"
  echo "Make sure:"
  echo "  1. The role exists: aws iam get-role --role-name runctl-test-role"
  echo "  2. Your credentials have sts:AssumeRole permission"
  echo "  3. The role trust policy allows your account"
  exit 1
fi

# Extract credentials with validation
ACCESS_KEY=$(echo "$RESPONSE" | jq -r '.Credentials.AccessKeyId')
SECRET_KEY=$(echo "$RESPONSE" | jq -r '.Credentials.SecretAccessKey')
SESSION_TOKEN=$(echo "$RESPONSE" | jq -r '.Credentials.SessionToken')
EXPIRATION=$(echo "$RESPONSE" | jq -r '.Credentials.Expiration')

# Validate credentials were extracted
if [ -z "$ACCESS_KEY" ] || [ "$ACCESS_KEY" = "null" ] || \
   [ -z "$SECRET_KEY" ] || [ "$SECRET_KEY" = "null" ] || \
   [ -z "$SESSION_TOKEN" ] || [ "$SESSION_TOKEN" = "null" ]; then
  echo -e "${YELLOW}Error: Failed to extract credentials from response${NC}"
  echo "Response: $RESPONSE"
  exit 1
fi

# Export credentials
export AWS_ACCESS_KEY_ID="$ACCESS_KEY"
export AWS_SECRET_ACCESS_KEY="$SECRET_KEY"
export AWS_SESSION_TOKEN="$SESSION_TOKEN"

# Verify the credentials work
echo -e "${GREEN}✓ Credentials obtained${NC}"
echo ""
echo "Identity:"
if ! aws sts get-caller-identity --output table; then
  echo -e "${YELLOW}Warning: Could not verify credentials with get-caller-identity${NC}"
  echo "Credentials may be invalid or expired"
fi
echo ""
echo -e "${YELLOW}Credentials expire at: $EXPIRATION${NC}"

# Check expiration time
EXPIRATION_EPOCH=$(date -j -f "%Y-%m-%dT%H:%M:%S%z" "$EXPIRATION" +%s 2>/dev/null || date -d "$EXPIRATION" +%s 2>/dev/null || echo "0")
CURRENT_EPOCH=$(date +%s)
if [ "$EXPIRATION_EPOCH" -gt 0 ] && [ "$EXPIRATION_EPOCH" -lt "$CURRENT_EPOCH" ]; then
  echo -e "${RED}✗ WARNING: Credentials appear to be expired!${NC}"
fi
echo ""
echo -e "${GREEN}Ready to test runctl!${NC}"
echo ""
echo "Example commands:"
echo "  cargo run -- aws instances list"
echo "  cargo run -- aws create --instance-type t3.micro"
echo "  cargo run -- resources list"
echo ""
echo "To clear credentials:"
echo "  unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN"

