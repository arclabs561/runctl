#!/bin/bash
set -euo pipefail

echo "=== Setting up IAM Role for SSM ==="

ROLE_NAME="runctl-ssm-role"
PROFILE_NAME="runctl-ssm-profile"
POLICY_ARN="arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"

# Create trust policy
cat > /tmp/ssm-trust-policy.json << 'TRUST'
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": {"Service": "ec2.amazonaws.com"},
    "Action": "sts:AssumeRole"
  }]
}
TRUST

# Create role (or update if exists)
if aws iam get-role --role-name "$ROLE_NAME" &>/dev/null; then
    echo "Role $ROLE_NAME already exists"
else
    echo "Creating IAM role: $ROLE_NAME"
    aws iam create-role \
        --role-name "$ROLE_NAME" \
        --assume-role-policy-document file:///tmp/ssm-trust-policy.json
fi

# Attach SSM policy
echo "Attaching SSM managed policy..."
aws iam attach-role-policy \
    --role-name "$ROLE_NAME" \
    --policy-arn "$POLICY_ARN" || echo "Policy may already be attached"

# Create instance profile
if aws iam get-instance-profile --instance-profile-name "$PROFILE_NAME" &>/dev/null; then
    echo "Instance profile $PROFILE_NAME already exists"
else
    echo "Creating instance profile: $PROFILE_NAME"
    aws iam create-instance-profile --instance-profile-name "$PROFILE_NAME"
fi

# Add role to profile
echo "Adding role to instance profile..."
aws iam add-role-to-instance-profile \
    --instance-profile-name "$PROFILE_NAME" \
    --role-name "$ROLE_NAME" 2>&1 | grep -v "EntityAlreadyExists" || true

echo ""
echo "✅ Setup complete!"
echo ""
echo "Usage:"
echo "  runctl aws create t3.micro --iam-instance-profile $PROFILE_NAME"
echo ""
echo "Verify:"
echo "  aws iam get-instance-profile --instance-profile-name $PROFILE_NAME"
