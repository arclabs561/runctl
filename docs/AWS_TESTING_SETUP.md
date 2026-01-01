# AWS Testing Setup with Temporary Credentials

Set up isolated AWS testing environments using temporary credentials and IAM roles.

## Overview

Instead of long-term access keys, use:
- IAM Roles with least-privilege permissions
- Temporary credentials via AWS STS (Security Token Service)
- Resource tagging for isolation
- **Permission boundaries** for additional protection

## Prerequisites

- AWS CLI configured with credentials that have IAM permissions
- `jq` installed (for JSON parsing in scripts)
- Basic understanding of IAM roles and policies

## Step 1: Create Test IAM Role

Create a dedicated IAM role for testing with minimal permissions:

```bash
# Create the role trust policy (allows EC2 and your account to assume it)
cat > /tmp/test-role-trust-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::YOUR-ACCOUNT-ID:root"
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

# Create the role
aws iam create-role \
  --role-name runctl-test-role \
  --assume-role-policy-document file:///tmp/test-role-trust-policy.json \
  --description "Testing role for runctl CLI tool"
```

## Step 2: Create Least-Privilege Permissions Policy

Create a policy that grants only the minimum permissions needed:

```bash
cat > /tmp/test-role-policy.json <<EOF
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
          "aws:RequestedRegion": "us-east-1"
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
      "Sid": "DenyProductionResources",
      "Effect": "Deny",
      "Action": "*",
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

# Attach the policy to the role
aws iam put-role-policy \
  --role-name runctl-test-role \
  --policy-name runctl-test-policy \
  --policy-document file:///tmp/test-role-policy.json
```

## Step 3: Create Permission Boundary (Optional but Recommended)

Add a permission boundary to prevent privilege escalation:

```bash
cat > /tmp/test-permission-boundary.json <<EOF
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
          "aws:RequestedRegion": ["us-east-1", "us-west-2"]
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

# Create the boundary policy
aws iam create-policy \
  --policy-name runctl-test-boundary \
  --policy-document file:///tmp/test-permission-boundary.json \
  --description "Permission boundary for runctl test role"

# Attach boundary to role
aws iam put-role-permissions-boundary \
  --role-name runctl-test-role \
  --permissions-boundary arn:aws:iam::YOUR-ACCOUNT-ID:policy/runctl-test-boundary
```

## Step 4: Create Test S3 Bucket

Create a dedicated test bucket:

```bash
# Create bucket with test tag
aws s3 mb s3://runctl-test-$(date +%s) \
  --region us-east-1

# Tag it for isolation
aws s3api put-bucket-tagging \
  --bucket runctl-test-* \
  --tagging 'TagSet=[{Key=Environment,Value=test},{Key=Purpose,Value=testing}]'
```

## Step 5: Assume Role and Get Temporary Credentials

Use the provided script to assume the role and export credentials:

```bash
# Make the script executable
chmod +x scripts/assume-test-role.sh

# Assume role and export credentials
source scripts/assume-test-role.sh

# Verify credentials
aws sts get-caller-identity
```

The script will:
1. Call `sts:AssumeRole` to get temporary credentials
2. Export them as environment variables
3. Set expiration time (default: 1 hour)
4. Display the session expiration time

## Step 6: Verify Setup

Before testing, verify the setup is correct:

```bash
# Verify all components are configured correctly
./scripts/verify-setup.sh
```

This checks:
- Role exists and has correct trust policy
- Permissions policy is attached
- Permission boundary is attached
- Role can be assumed
- Permissions work correctly

## Step 7: Test runctl with Temporary Credentials

Once credentials are exported, test the CLI:

```bash
# Run comprehensive test suite
./scripts/run-all-tests.sh

# Or run individual tests:
./scripts/test-auth.sh                    # Basic authentication
./scripts/test-security-boundaries.sh      # Security verification
./scripts/test-runctl-integration.sh     # runctl integration
```

To test manually:

```bash
# List instances (should work)
cargo run -- resources list

# Create a test instance (will be tagged with Environment=test)
cargo run -- aws create --instance-type t3.micro --project-name test-project

# Verify the instance was created with test tag
aws ec2 describe-instances \
  --filters "Name=tag:Environment,Values=test" \
  --query 'Reservations[*].Instances[*].[InstanceId,Tags[?Key==`Name`].Value|[0]]' \
  --output table
```

## Step 7: Verify Credential Expiration

Test that credentials expire correctly:

```bash
# Wait for expiration (or manually expire by removing env vars)
unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN

# Try to use CLI (should fail with credential error)
cargo run -- aws instances list
```

## Cleanup

When done testing:

```bash
# Terminate all test instances
cargo run -- resources terminate-all --force

# Delete test S3 bucket
aws s3 rb s3://runctl-test-* --force

# Delete IAM role and policies
aws iam delete-role-policy \
  --role-name runctl-test-role \
  --policy-name runctl-test-policy

aws iam delete-role \
  --role-name runctl-test-role

aws iam delete-policy \
  --policy-arn arn:aws:iam::YOUR-ACCOUNT-ID:policy/runctl-test-boundary
```

## Security Best Practices

1. **Never commit credentials**: Temporary credentials are in environment variables only
2. **Use short session durations**: Default 1 hour, adjust based on test needs
3. **Tag all resources**: Enforce tagging in policies to isolate test resources
4. **Monitor usage**: Enable CloudTrail to log all API calls
5. **Rotate regularly**: Re-assume role periodically during long test sessions
6. **Use separate accounts**: For production testing, use a separate AWS account

## Troubleshooting

### "Access Denied" errors
- Verify the role trust policy allows your principal
- Check that the role has the necessary permissions
- Ensure resources are tagged with `Environment=test`

### Credentials expired
- Re-run `assume-test-role.sh` to get new credentials
- Check session duration (max 1 hour for AssumeRole)

### Cannot assume role
- Verify your base credentials have `sts:AssumeRole` permission
- Check the role trust policy matches your account ID
- Ensure ExternalId condition matches (if used)

## References

- [AWS IAM Best Practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
- [Temporary Security Credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp.html)
- [Using Temporary Credentials](https://docs.aws.amazon.com/IAM/latest/UserGuide/id_credentials_temp_use-resources.html)

