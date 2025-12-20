#!/bin/bash
# Automated setup script for runctl test IAM role
#
# This script creates:
# 1. IAM role with trust policy
# 2. Permissions policy with least-privilege access
# 3. Permission boundary (optional)
# 4. Test S3 bucket
#
# Usage:
#   ./scripts/setup-test-role.sh [--account-id ACCOUNT_ID] [--region REGION]

set -euo pipefail

# Configuration
ACCOUNT_ID="${1:-$(aws sts get-caller-identity --query Account --output text)}"
REGION="${2:-us-east-1}"
ROLE_NAME="runctl-test-role"
POLICY_NAME="runctl-test-policy"
BOUNDARY_NAME="runctl-test-boundary"
BUCKET_NAME="runctl-test-$(date +%s)"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}Setting up runctl test environment...${NC}"
echo "Account ID: $ACCOUNT_ID"
echo "Region: $REGION"
echo ""

# Step 1: Create trust policy
echo -e "${YELLOW}[1/5] Creating role trust policy...${NC}"
cat > /tmp/runctl-trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::${ACCOUNT_ID}:root"
      },
      "Action": "sts:AssumeRole",
      "Condition": {
        "StringEquals": {
          "sts:ExternalId": "runctl-test-env"
        }
      }
    }
  ]
}
EOF

# Step 2: Create role
echo -e "${YELLOW}[2/5] Creating IAM role...${NC}"
if aws iam get-role --role-name "$ROLE_NAME" &>/dev/null; then
  echo "Role already exists, updating trust policy..."
  aws iam update-assume-role-policy \
    --role-name "$ROLE_NAME" \
    --policy-document file:///tmp/runctl-trust-policy.json
else
  aws iam create-role \
    --role-name "$ROLE_NAME" \
    --assume-role-policy-document file:///tmp/runctl-trust-policy.json \
    --description "Testing role for runctl CLI tool" \
    --tags Key=Purpose,Value=testing Key=Environment,Value=test
fi

# Step 3: Create permissions policy
echo -e "${YELLOW}[3/5] Creating permissions policy...${NC}"
cat > /tmp/runctl-permissions-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "EC2InstanceManagement",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeInstances",
        "ec2:DescribeInstanceStatus",
        "ec2:DescribeImages",
        "ec2:DescribeInstanceTypes",
        "ec2:RunInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:RebootInstances",
        "ec2:TerminateInstances",
        "ec2:CreateTags",
        "ec2:DescribeTags",
        "ec2:DescribeSecurityGroups",
        "ec2:DescribeKeyPairs"
      ],
      "Resource": "*",
      "Condition": {
        "StringEquals": {
          "aws:RequestedRegion": "$REGION"
        }
      }
    },
    {
      "Sid": "EBSVolumeManagement",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeVolumes",
        "ec2:DescribeVolumeStatus",
        "ec2:CreateVolume",
        "ec2:AttachVolume",
        "ec2:DetachVolume",
        "ec2:DeleteVolume",
        "ec2:CreateSnapshot",
        "ec2:DescribeSnapshots",
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
      "Resource": [
        "arn:aws:s3:::runctl-test-*",
        "arn:aws:s3:::runctl-test-*/*"
      ]
    },
    {
      "Sid": "SSMOperations",
      "Effect": "Allow",
      "Action": [
        "ssm:SendCommand",
        "ssm:GetCommandInvocation",
        "ssm:DescribeInstanceInformation",
        "ssm:ListCommandInvocations"
      ],
      "Resource": "*"
    },
    {
      "Sid": "DenyProductionModifications",
      "Effect": "Deny",
      "Action": [
        "ec2:RunInstances",
        "ec2:TerminateInstances",
        "ec2:CreateVolume",
        "ec2:DeleteVolume",
        "ec2:CreateSnapshot",
        "ec2:DeleteSnapshot"
      ],
      "Resource": "*",
      "Condition": {
        "StringNotEquals": {
          "aws:RequestTag/Environment": "test"
        }
      }
    }
  ]
}
EOF

aws iam put-role-policy \
  --role-name "$ROLE_NAME" \
  --policy-name "$POLICY_NAME" \
  --policy-document file:///tmp/runctl-permissions-policy.json

# Step 4: Create permission boundary (optional but recommended)
echo -e "${YELLOW}[4/5] Creating permission boundary...${NC}"
cat > /tmp/runctl-boundary-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "ec2:*",
        "s3:*",
        "ssm:*",
        "logs:*"
      ],
      "Resource": "*",
      "Condition": {
        "StringEquals": {
          "aws:RequestedRegion": ["$REGION", "us-west-2"]
        }
      }
    },
    {
      "Effect": "Deny",
      "Action": [
        "iam:*",
        "organizations:*",
        "account:*",
        "sts:GetFederationToken",
        "sts:GetSessionToken"
      ],
      "Resource": "*"
    }
  ]
}
EOF

if aws iam get-policy --policy-arn "arn:aws:iam::${ACCOUNT_ID}:policy/${BOUNDARY_NAME}" &>/dev/null; then
  echo "Boundary policy exists, updating..."
  aws iam create-policy-version \
    --policy-arn "arn:aws:iam::${ACCOUNT_ID}:policy/${BOUNDARY_NAME}" \
    --policy-document file:///tmp/runctl-boundary-policy.json \
    --set-as-default
else
  aws iam create-policy \
    --policy-name "$BOUNDARY_NAME" \
    --policy-document file:///tmp/runctl-boundary-policy.json \
    --description "Permission boundary for runctl test role"
fi

aws iam put-role-permissions-boundary \
  --role-name "$ROLE_NAME" \
  --permissions-boundary "arn:aws:iam::${ACCOUNT_ID}:policy/${BOUNDARY_NAME}"

# Step 5: Create test S3 bucket
echo -e "${YELLOW}[5/5] Creating test S3 bucket...${NC}"
aws s3 mb "s3://${BUCKET_NAME}" --region "$REGION" || true

aws s3api put-bucket-tagging \
  --bucket "$BUCKET_NAME" \
  --tagging 'TagSet=[{Key=Environment,Value=test},{Key=Purpose,Value=testing}]' || true

# Summary
echo ""
echo -e "${GREEN}✓ Setup complete!${NC}"
echo ""
echo "Created resources:"
echo "  - IAM Role: $ROLE_NAME"
echo "  - Permissions Policy: $POLICY_NAME"
echo "  - Permission Boundary: $BOUNDARY_NAME"
echo "  - S3 Bucket: $BUCKET_NAME"
echo ""
echo "Next steps:"
echo "  1. Source the assume role script:"
echo "     source scripts/assume-test-role.sh"
echo ""
echo "  2. Test the CLI:"
echo "     cargo run -- aws instances list"
echo ""
echo "  3. Cleanup when done:"
echo "     ./scripts/cleanup-test-role.sh"

