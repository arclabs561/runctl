# Examples

Example scripts demonstrating runctl workflows.

## Quick Start

```bash
# Complete workflow (recommended)
./examples/complete_workflow.sh

# Quick test
./examples/quick_test.sh

# Workflow command
./examples/workflow_train_example.sh
```

## Prerequisites

```bash
cargo build --release
aws configure
aws sts get-caller-identity
```

## Customization

```bash
INSTANCE_TYPE=g4dn.xlarge ./examples/complete_workflow.sh
USE_SPOT=false ./examples/complete_workflow.sh
TRAINING_SCRIPT=training/train_mnist.py ./examples/complete_workflow.sh
EPOCHS=10 ./examples/complete_workflow.sh
```

## Script Details

### `complete_workflow.sh`

Prerequisites validation, error handling with cleanup, configurable via environment variables.

```bash
./examples/complete_workflow.sh
```

Environment variables:
- `INSTANCE_TYPE`: EC2 instance type (default: `t3.micro`)
- `USE_SPOT`: Use spot instances (default: `true`)
- `TRAINING_SCRIPT`: Path to training script (default: `training/train_mnist_e2e.py`)
- `EPOCHS`: Number of training epochs (default: `3`)

### `quick_test.sh`

Minimal example for quick testing.

```bash
./examples/quick_test.sh
```

### `workflow_train_example.sh`

Demonstrates the high-level `runctl workflow train` command.

```bash
./examples/workflow_train_example.sh
```

## Best Practices

```bash
# Always use --wait
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)

# Use structured output (not grep)
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)

# Always cleanup
trap 'runctl aws terminate $INSTANCE_ID --force || true' EXIT

# Use proper error handling
set -euo pipefail
```

## Troubleshooting

- Permission denied: `chmod +x examples/*.sh`
- AWS credentials: `aws configure && aws sts get-caller-identity`
- Instance limit: `aws ec2 describe-account-attributes`
- SSM not available: `aws ssm describe-instance-information --instance-ids $INSTANCE_ID`

## Cost

- Spot instances: 50-90% discount, can be interrupted
- t3.micro: ~$0.01/hr (free tier eligible)
- g4dn.xlarge: ~$0.50/hr (GPU)

