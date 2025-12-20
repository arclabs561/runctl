#!/bin/bash
# Setup GitHub OIDC for CI/CD (if possible)
#
# This script helps set up OIDC provider for GitHub Actions.
# Note: Some steps require manual configuration in AWS Console.
#
# Usage:
#   ./scripts/setup-github-oidc.sh [github-org] [github-repo]

set -euo pipefail

# Configuration
GITHUB_ORG="${1:-arclabs561}"
GITHUB_REPO="${2:-runctl}"
OIDC_PROVIDER_NAME="token.actions.githubusercontent.com"
ROLE_NAME="github-actions-role"
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}Setting up GitHub OIDC for CI/CD...${NC}"
echo "GitHub org: $GITHUB_ORG"
echo "GitHub repo: $GITHUB_REPO"
echo "Account: $ACCOUNT_ID"
echo ""

# Step 1: Check if OIDC provider exists
echo -e "${YELLOW}[1/4] Checking for existing OIDC provider...${NC}"
EXISTING_PROVIDER=$(aws iam list-open-id-connect-providers --query "OpenIDConnectProviderList[?contains(Arn, 'token.actions.githubusercontent.com')].Arn" --output text 2>/dev/null || echo "")

if [[ -n "$EXISTING_PROVIDER" ]]; then
  echo -e "${GREEN}✓ OIDC provider already exists: $EXISTING_PROVIDER${NC}"
  PROVIDER_ARN="$EXISTING_PROVIDER"
else
  echo -e "${YELLOW}OIDC provider not found${NC}"
  echo ""
  echo "To create OIDC provider, run this command:"
  echo ""
  echo "aws iam create-open-id-connect-provider \\"
  echo "  --url https://token.actions.githubusercontent.com \\"
  echo "  --client-id-list sts.amazonaws.com \\"
  echo "  --thumbprint-list 6938fd4d98bab03faadb97b34396831e3780aea1"
  echo ""
  echo "Or create it via AWS Console:"
  echo "  IAM → Identity providers → Add provider → OpenID Connect"
  echo "  Provider URL: https://token.actions.githubusercontent.com"
  echo "  Audience: sts.amazonaws.com"
  echo ""
  read -p "Press enter after creating the provider, or Ctrl+C to cancel..."
  PROVIDER_ARN=$(aws iam list-open-id-connect-providers --query "OpenIDConnectProviderList[?contains(Arn, 'token.actions.githubusercontent.com')].Arn" --output text)
fi

# Step 2: Create trust policy for GitHub Actions
echo -e "${YELLOW}[2/4] Creating trust policy for GitHub Actions role...${NC}"
cat > /tmp/github-oidc-trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Federated": "${PROVIDER_ARN}"
      },
      "Action": "sts:AssumeRoleWithWebIdentity",
      "Condition": {
        "StringEquals": {
          "token.actions.githubusercontent.com:aud": "sts.amazonaws.com"
        },
        "StringLike": {
          "token.actions.githubusercontent.com:sub": "repo:${GITHUB_ORG}/${GITHUB_REPO}:*"
        }
      }
    }
  ]
}
EOF

# Step 3: Create IAM role for GitHub Actions
echo -e "${YELLOW}[3/4] Creating IAM role for GitHub Actions...${NC}"
EXISTING_ROLE=$(aws iam get-role --role-name "$ROLE_NAME" 2>/dev/null || echo "")
if [[ -z "$EXISTING_ROLE" ]]; then
  aws iam create-role \
    --role-name "$ROLE_NAME" \
    --assume-role-policy-document file:///tmp/github-oidc-trust-policy.json \
    --description "Role for GitHub Actions to access AWS resources"
  echo -e "${GREEN}✓ Role created${NC}"
else
  echo -e "${YELLOW}Role already exists, updating trust policy...${NC}"
  aws iam update-assume-role-policy \
    --role-name "$ROLE_NAME" \
    --policy-document file:///tmp/github-oidc-trust-policy.json
  echo -e "${GREEN}✓ Trust policy updated${NC}"
fi

# Step 4: Attach permissions (use existing runctl-test-policy or create new)
echo -e "${YELLOW}[4/4] Attaching permissions...${NC}"
# Check if runctl-test-policy exists
TEST_POLICY_ARN="arn:aws:iam::${ACCOUNT_ID}:policy/runctl-test-policy"
EXISTING_POLICY=$(aws iam get-policy --policy-arn "$TEST_POLICY_ARN" 2>/dev/null || echo "")

if [[ -n "$EXISTING_POLICY" ]]; then
  echo -e "${YELLOW}Using existing runctl-test-policy${NC}"
  aws iam attach-role-policy \
    --role-name "$ROLE_NAME" \
    --policy-arn "$TEST_POLICY_ARN"
  echo -e "${GREEN}✓ Policy attached${NC}"
else
  echo -e "${YELLOW}runctl-test-policy not found${NC}"
  echo "You can:"
  echo "  1. Run scripts/setup-test-role.sh first to create the policy"
  echo "  2. Or manually attach appropriate policies to $ROLE_NAME"
fi

echo ""
echo "=========================================="
echo -e "${GREEN}✓ GitHub OIDC setup complete!${NC}"
echo ""
echo "Role ARN: arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}"
echo ""
echo "Next steps:"
echo "  1. Update .github/workflows/*.yml to use OIDC:"
echo "     - uses: aws-actions/configure-aws-credentials@v4"
echo "       with:"
echo "         role-to-assume: arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}"
echo "         aws-region: us-east-1"
echo ""
echo "  2. Remove AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY from GitHub Secrets"
echo ""
echo "  3. Test the workflow"

