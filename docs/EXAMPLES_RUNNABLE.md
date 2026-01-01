# Runnable E2E Examples

These examples are **actually runnable** and have been tested end-to-end.

## Prerequisites

1. **AWS Credentials**: Configure AWS credentials
   ```bash
   aws configure
   aws sts get-caller-identity  # Verify it works
   ```

2. **S3 Bucket for SSM Code Sync** (Required if using SSM):
   ```toml
   # Add to .runctl.toml
   [aws]
   s3_bucket = "your-bucket-name"
   ```
   
   **Note**: SSM-based code sync requires an S3 bucket for temporary storage.
   The bucket is only used during code sync and files are automatically cleaned up.
   
   **Alternative**: Use SSH instead by creating instances with `--key-name` instead of `--iam-instance-profile`.

3. **IAM Instance Profile for SSM** (Optional but recommended):
   ```bash
   # One-time setup
   ./scripts/setup-ssm-role.sh
   ```
   
   Then use: `runctl aws create t3.micro --iam-instance-profile runctl-ssm-profile`

4. **Training Script**: Ensure you have a training script
   ```bash
   # Use the minimal E2E test script (fast, no dependencies)
   ls training/train_mnist_e2e.py
   
   # Or use the full training script
   ls training/train_mnist.py
   ```

## Complete E2E Workflow

This example actually runs training and verifies it completes:

```bash
#!/bin/bash
set -e

echo "=== Complete E2E Training Workflow ==="

# 1. Create instance (with --wait to ensure it's ready)
echo "Step 1: Creating instance..."
INSTANCE_ID=$(runctl aws create t3.micro --spot --wait --output instance-id)
echo "Created: $INSTANCE_ID"

# 2. Train with code sync (with --wait to ensure completion)
echo "Step 2: Starting training..."
# Use train_mnist_e2e.py for fast E2E testing (3 epochs, ~5 seconds)
runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 3

echo "✅ Training completed successfully!"

# 3. Cleanup
echo "Step 3: Cleaning up..."
runctl aws terminate $INSTANCE_ID --force

echo "=== E2E Workflow Complete ==="
```

## Quick Test Script

For a minimal test that actually runs training:

```bash
#!/bin/bash
# Quick E2E test - creates instance, trains, verifies, cleans up

INSTANCE_ID=$(runctl aws create t3.micro --spot --wait --output instance-id)
echo "Instance: $INSTANCE_ID"

# Train with minimal script (fast, no dependencies) - waits for completion
runctl aws train $INSTANCE_ID training/train_mnist_e2e.py --sync-code --wait -- --epochs 3

# Cleanup
runctl aws terminate $INSTANCE_ID --force
echo "✅ Test complete"
```

## Using the E2E Training Script

The `training/train_mnist_e2e.py` script is designed for E2E testing:

- **Fast**: 3 epochs by default, completes in ~5 seconds
- **No dependencies**: Uses only Python stdlib
- **Verifiable**: Creates checkpoints and completion markers
- **Configurable**: Supports `--epochs`, `--checkpoint-dir`, `--data-dir`

```bash
# Run locally to test
python3 training/train_mnist_e2e.py --epochs 5

# Use in runctl
runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    -- --epochs 5 --checkpoint-dir ./checkpoints
```

## Full Training Script

For actual ML training, use `training/train_mnist.py`:

```bash
# This requires dependencies (torch, etc.) to be installed
runctl aws train $INSTANCE_ID training/train_mnist.py \
    --sync-code \
    -- --epochs 10 --batch-size 64
```

## Runnable Example Scripts

We provide ready-to-use example scripts in the `examples/` directory:

### `examples/complete_workflow.sh`
Complete workflow with error handling, cleanup, and colored output:
```bash
./examples/complete_workflow.sh

# Customize with environment variables
INSTANCE_TYPE=g4dn.xlarge EPOCHS=10 ./examples/complete_workflow.sh
```

### `examples/quick_test.sh`
Minimal example for quick testing:
```bash
./examples/quick_test.sh
```

### `examples/workflow_train_example.sh`
Demonstrates the high-level workflow command:
```bash
./examples/workflow_train_example.sh
```

## Testing Checklist

When running E2E examples, verify:

- [ ] Instance created successfully (check output for instance ID)
- [ ] Instance is ready (--wait flag handles this automatically)
- [ ] Code sync completed (training starts without errors)
- [ ] Training completed (--wait flag ensures completion)
- [ ] Checkpoints created (check `checkpoints/` directory on instance)
- [ ] Instance terminated (cleanup happens automatically with trap)

## Troubleshooting

**Training script not found:**
```bash
# Create the E2E test script if missing
cp examples/test_training_script.py training/train_mnist_e2e.py
chmod +x training/train_mnist_e2e.py
```

**Instance not ready:**
```bash
# Check instance status
runctl aws status $INSTANCE_ID

# Wait for instance to be ready
runctl aws wait $INSTANCE_ID

# Note: --wait flag on create should handle this automatically
```

**Training doesn't start:**
```bash
# Check SSM connectivity
aws ssm describe-instance-information --instance-ids $INSTANCE_ID

# Check processes on instance
runctl aws processes $INSTANCE_ID
```

