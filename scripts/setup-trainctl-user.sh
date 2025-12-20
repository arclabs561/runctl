#!/bin/bash
# Create a limited-permission IAM user for runctl operations
#
# This creates a dedicated user with only the permissions runctl needs,
# following the principle of least privilege.
#
# Usage:
#   ./scripts/setup-runctl-user.sh [user-name]

set -euo pipefail

# Configuration
USER_NAME="${1:-runctl-user}"
POLICY_NAME="runctl-user-policy"
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
REGION="${AWS_DEFAULT_REGION:-us-east-1}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}Creating runctl-specific IAM user...${NC}"
echo "User name: $USER_NAME"
echo "Account: $ACCOUNT_ID"
echo ""

# Check if user already exists
EXISTING=$(aws iam get-user --user-name "$USER_NAME" 2>/dev/null || echo "")
if [[ -n "$EXISTING" ]]; then
  echo -e "${YELLOW}User $USER_NAME already exists${NC}"
  read -p "Continue anyway? (yes/no): " confirm
  if [[ "$confirm" != "yes" ]]; then
    exit 0
  fi
fi

# Step 1: Create IAM user
echo -e "${YELLOW}[1/5] Creating IAM user...${NC}"
if [[ -z "$EXISTING" ]]; then
  aws iam create-user \
    --user-name "$USER_NAME" \
    --tags Key=Purpose,Value=runctl Key=CreatedBy,Value=setup-script
  echo -e "${GREEN}✓ User created${NC}"
else
  echo -e "${YELLOW}User already exists, skipping creation${NC}"
fi

# Step 2: Create policy with runctl permissions
echo -e "${YELLOW}[2/5] Creating permissions policy...${NC}"
cat > /tmp/runctl-user-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "EC2Operations",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeInstances",
        "ec2:DescribeInstanceStatus",
        "ec2:DescribeInstanceTypes",
        "ec2:DescribeImages",
        "ec2:DescribeSecurityGroups",
        "ec2:DescribeKeyPairs",
        "ec2:DescribeTags",
        "ec2:RunInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:RebootInstances",
        "ec2:TerminateInstances",
        "ec2:CreateTags",
        "ec2:DescribeVpcs",
        "ec2:DescribeSubnets"
      ],
      "Resource": "*"
    },
    {
      "Sid": "EBSOperations",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeVolumes",
        "ec2:DescribeVolumeStatus",
        "ec2:DescribeSnapshots",
        "ec2:CreateVolume",
        "ec2:AttachVolume",
        "ec2:DetachVolume",
        "ec2:DeleteVolume",
        "ec2:CreateSnapshot",
        "ec2:DeleteSnapshot",
        "ec2:ModifyVolumeAttribute"
      ],
      "Resource": "*"
    },
    {
      "Sid": "S3Operations",
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket",
        "s3:HeadBucket",
        "s3:GetBucketLocation"
      ],
      "Resource": "*"
    },
    {
      "Sid": "SSMOperations",
      "Effect": "Allow",
      "Action": [
        "ssm:SendCommand",
        "ssm:GetCommandInvocation",
        "ssm:DescribeInstanceInformation",
        "ssm:ListCommandInvocations",
        "ssm:DescribeInstanceProperties"
      ],
      "Resource": "*"
    },
    {
      "Sid": "STSAssumeRole",
      "Effect": "Allow",
      "Action": "sts:AssumeRole",
      "Resource": "arn:aws:iam::${ACCOUNT_ID}:role/runctl-*"
    }
  ]
}
EOF

# Check if policy exists
POLICY_ARN="arn:aws:iam::${ACCOUNT_ID}:policy/${POLICY_NAME}"
EXISTING_POLICY=$(aws iam get-policy --policy-arn "$POLICY_ARN" 2>/dev/null || echo "")
if [[ -z "$EXISTING_POLICY" ]]; then
  aws iam create-policy \
    --policy-name "$POLICY_NAME" \
    --policy-document file:///tmp/runctl-user-policy.json \
    --description "Least-privilege policy for runctl operations"
  echo -e "${GREEN}✓ Policy created${NC}"
else
  echo -e "${YELLOW}Policy already exists, updating...${NC}"
  # Get default version
  DEFAULT_VERSION=$(aws iam get-policy --policy-arn "$POLICY_ARN" --query 'Policy.DefaultVersionId' --output text)
  # Create new version
  aws iam create-policy-version \
    --policy-arn "$POLICY_ARN" \
    --policy-document file:///tmp/runctl-user-policy.json \
    --set-as-default
  # Delete old version if not default
  echo -e "${GREEN}✓ Policy updated${NC}"
fi

# Step 3: Attach policy to user
echo -e "${YELLOW}[3/5] Attaching policy to user...${NC}"
aws iam attach-user-policy \
  --user-name "$USER_NAME" \
  --policy-arn "$POLICY_ARN"
echo -e "${GREEN}✓ Policy attached${NC}"

# Step 4: Create access key
echo -e "${YELLOW}[4/5] Creating access key...${NC}"
KEY_OUTPUT=$(aws iam create-access-key --user-name "$USER_NAME")
ACCESS_KEY=$(echo "$KEY_OUTPUT" | jq -r '.AccessKey.AccessKeyId')
SECRET_KEY=$(echo "$KEY_OUTPUT" | jq -r '.AccessKey.SecretAccessKey')

echo -e "${GREEN}✓ Access key created${NC}"
echo ""
echo -e "${YELLOW}⚠️  IMPORTANT: Save these credentials securely${NC}"
echo "Access Key ID: $ACCESS_KEY"
echo "Secret Access Key: $SECRET_KEY"
echo ""

# Step 5: Recommend MFA
echo -e "${YELLOW}[5/5] Security recommendations...${NC}"
echo -e "${GREEN}✓ User created successfully${NC}"
echo ""
echo "Next steps:"
echo "  1. Enable MFA on this user:"
echo "     aws iam create-virtual-mfa-device --virtual-mfa-device-name ${USER_NAME}-mfa"
echo ""
echo "  2. Configure AWS CLI with new credentials:"
echo "     aws configure --profile runctl"
echo "     # Enter Access Key ID and Secret Access Key above"
echo ""
echo "  3. Use the new profile:"
echo "     export AWS_PROFILE=runctl"
echo "     # Or: aws --profile runctl <command>"
echo ""
echo "  4. Test the new user:"
echo "     aws --profile runctl sts get-caller-identity"

