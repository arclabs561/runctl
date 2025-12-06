#!/bin/bash
# Setup CloudTrail for audit logging
#
# This script creates a CloudTrail trail to log all API calls for security auditing.
#
# Usage:
#   ./scripts/setup-cloudtrail.sh [bucket-name]

set -euo pipefail

# Configuration
BUCKET_NAME="${1:-trainctl-cloudtrail-logs-$(date +%s)}"
TRAIL_NAME="trainctl-audit-trail"
REGION="${AWS_DEFAULT_REGION:-us-east-1}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}Setting up CloudTrail for audit logging...${NC}"
echo "Trail name: $TRAIL_NAME"
echo "S3 bucket: $BUCKET_NAME"
echo "Region: $REGION"
echo ""

# Check if trail already exists
EXISTING=$(aws cloudtrail get-trail --name "$TRAIL_NAME" 2>/dev/null || echo "")
if [[ -n "$EXISTING" ]]; then
  echo -e "${YELLOW}Trail $TRAIL_NAME already exists${NC}"
  IS_LOGGING=$(aws cloudtrail get-trail-status --name "$TRAIL_NAME" --query 'IsLogging' --output text 2>/dev/null || echo "false")
  if [[ "$IS_LOGGING" == "true" ]]; then
    echo -e "${GREEN}✓ Trail is already logging${NC}"
    exit 0
  else
    echo -e "${YELLOW}Trail exists but is not logging. Starting logging...${NC}"
    aws cloudtrail start-logging --name "$TRAIL_NAME"
    echo -e "${GREEN}✓ Logging started${NC}"
    exit 0
  fi
fi

# Step 1: Create S3 bucket for logs
echo -e "${YELLOW}[1/4] Creating S3 bucket for CloudTrail logs...${NC}"
if aws s3 ls "s3://$BUCKET_NAME" 2>/dev/null; then
  echo -e "${YELLOW}Bucket $BUCKET_NAME already exists${NC}"
else
  if [[ "$REGION" == "us-east-1" ]]; then
    aws s3 mb "s3://$BUCKET_NAME" --region "$REGION"
  else
    aws s3 mb "s3://$BUCKET_NAME" --region "$REGION"
  fi
  echo -e "${GREEN}✓ Bucket created${NC}"
fi

# Step 2: Set bucket policy for CloudTrail
echo -e "${YELLOW}[2/4] Setting bucket policy for CloudTrail...${NC}"
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
cat > /tmp/cloudtrail-bucket-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "AWSCloudTrailAclCheck",
      "Effect": "Allow",
      "Principal": {
        "Service": "cloudtrail.amazonaws.com"
      },
      "Action": "s3:GetBucketAcl",
      "Resource": "arn:aws:s3:::$BUCKET_NAME",
      "Condition": {
        "StringEquals": {
          "AWS:SourceArn": "arn:aws:cloudtrail:$REGION:$ACCOUNT_ID:trail/$TRAIL_NAME"
        }
      }
    },
    {
      "Sid": "AWSCloudTrailWrite",
      "Effect": "Allow",
      "Principal": {
        "Service": "cloudtrail.amazonaws.com"
      },
      "Action": "s3:PutObject",
      "Resource": "arn:aws:s3:::$BUCKET_NAME/*",
      "Condition": {
        "StringEquals": {
          "AWS:SourceArn": "arn:aws:cloudtrail:$REGION:$ACCOUNT_ID:trail/$TRAIL_NAME",
          "s3:x-amz-acl": "bucket-owner-full-control"
        }
      }
    }
  ]
}
EOF

aws s3api put-bucket-policy --bucket "$BUCKET_NAME" --policy file:///tmp/cloudtrail-bucket-policy.json
echo -e "${GREEN}✓ Bucket policy set${NC}"

# Step 3: Create CloudTrail trail
echo -e "${YELLOW}[3/4] Creating CloudTrail trail...${NC}"
aws cloudtrail create-trail \
  --name "$TRAIL_NAME" \
  --s3-bucket-name "$BUCKET_NAME" \
  --is-multi-region-trail \
  --enable-log-file-validation \
  --include-global-service-events

echo -e "${GREEN}✓ Trail created${NC}"

# Step 4: Start logging
echo -e "${YELLOW}[4/4] Starting CloudTrail logging...${NC}"
aws cloudtrail start-logging --name "$TRAIL_NAME"
echo -e "${GREEN}✓ Logging started${NC}"

echo ""
echo "=========================================="
echo -e "${GREEN}✓ CloudTrail setup complete!${NC}"
echo ""
echo "Trail name: $TRAIL_NAME"
echo "S3 bucket: $BUCKET_NAME"
echo "Status: Logging all API calls"
echo ""
echo "View logs:"
echo "  aws s3 ls s3://$BUCKET_NAME/"
echo ""
echo "Check status:"
echo "  aws cloudtrail get-trail-status --name $TRAIL_NAME"

