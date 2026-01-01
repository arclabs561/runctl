# SSM Quick Start

AWS Systems Manager (SSM) for secure command execution on EC2 instances without SSH keys.

## Setup

```bash
# One-time setup
./scripts/setup-ssm-role.sh

# Create instance with SSM
runctl aws create t3.micro --iam-instance-profile runctl-ssm-profile
```

## Usage

Commands use SSM automatically when the instance has an IAM profile with SSM permissions:

```bash
runctl aws processes <instance-id>
runctl aws train <instance-id> train.py --sync-code
```

## Verify

```bash
aws iam get-instance-profile --instance-profile-name runctl-ssm-profile
aws ssm describe-instance-information --filters "Key=InstanceIds,Values=i-xxx"
```

## Troubleshooting

- Check IAM profile attached: `aws ec2 describe-instances --instance-ids i-xxx --query 'Reservations[0].Instances[0].IamInstanceProfile'`
- Check SSM agent: `sudo systemctl status amazon-ssm-agent` (via SSH)
- Wait 1-2 minutes after instance start for agent to connect
- Ensure role has `AmazonSSMManagedInstanceCore` policy

## Notes

- No SSH keys required
- SSM agent pre-installed on Amazon Linux/Ubuntu AMIs
- Commands logged in CloudTrail
- Works through VPN (no public IPs required)

