# Improved Examples: Best Practices

This document shows improved examples using the latest runctl features for better developer experience.

## Key Improvements

1. **`--wait` flags**: No more manual `sleep` commands or guessing when operations complete
2. **`--output instance-id`**: Structured output instead of fragile `grep` parsing
3. **Better error handling**: Proper validation and cleanup
4. **Workflow commands**: High-level commands that orchestrate multiple operations

## Quick Start (Improved)

### Before (Fragile)
```bash
# ❌ Old way - fragile parsing, manual waiting
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')
sleep 60  # Hope it's ready
runctl aws train $INSTANCE_ID training/train.py --sync-code
```

### After (Robust)
```bash
# ✅ New way - structured output, automatic waiting
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge --wait --output instance-id)
runctl aws train $INSTANCE_ID training/train.py --sync-code --wait
```

## Complete Workflow Examples

### Example 1: Basic Training Workflow

```bash
#!/bin/bash
set -euo pipefail

# Create instance (waits until ready)
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)
echo "Created: $INSTANCE_ID"

# Train (waits until complete)
runctl aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
    --sync-code \
    --script-args "--epochs" "3" \
    --wait

# Cleanup
runctl aws terminate "$INSTANCE_ID" --force
```

### Example 2: Using Workflow Command

```bash
#!/bin/bash
# Even simpler - workflow command handles everything
runctl workflow train training/train_mnist_e2e.py \
    --instance-type t3.micro \
    --spot \
    --script-args "--epochs" "3"
```

### Example 3: With Error Handling

```bash
#!/bin/bash
set -euo pipefail

# Cleanup function
cleanup() {
    if [ -n "${INSTANCE_ID:-}" ]; then
        echo "Cleaning up instance $INSTANCE_ID..."
        runctl aws terminate "$INSTANCE_ID" --force || true
    fi
}
trap cleanup EXIT

# Create instance
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)

# Train with error handling
if ! runctl aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
    --sync-code \
    --script-args "--epochs" "3" \
    --wait; then
    echo "Training failed. Checking status..."
    runctl aws status "$INSTANCE_ID"
    exit 1
fi

echo "Training completed successfully!"
```

## Runnable Example Scripts

We provide ready-to-use example scripts in the `examples/` directory:

### `examples/complete_workflow.sh`
Complete workflow with:
- Prerequisites checking
- Error handling
- Cleanup on exit
- Colored output
- Configurable via environment variables

```bash
# Run with defaults
./examples/complete_workflow.sh

# Customize
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

## Common Patterns

### Pattern 1: Create and Train

```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)
runctl aws train "$INSTANCE_ID" training/train.py --sync-code --wait
runctl aws terminate "$INSTANCE_ID" --force
```

### Pattern 2: Monitor While Training

```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)

# Start training in background (without --wait)
runctl aws train "$INSTANCE_ID" training/train.py --sync-code &

# Monitor in foreground
runctl aws monitor "$INSTANCE_ID" --follow

# Wait for training
wait

# Cleanup
runctl aws terminate "$INSTANCE_ID" --force
```

### Pattern 3: Check Status Before Operations

```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)

# Check status
runctl aws status "$INSTANCE_ID"

# Train
runctl aws train "$INSTANCE_ID" training/train.py --sync-code --wait

# Check final status
runctl aws status "$INSTANCE_ID"

# Cleanup
runctl aws terminate "$INSTANCE_ID" --force
```

## Script Arguments Best Practices

### ✅ Good: Separate Arguments
```bash
runctl aws train "$INSTANCE_ID" training/train.py \
    --script-args "--epochs" "10" "--batch-size" "64" "--lr" "0.001"
```

### ❌ Avoid: Single String (may break with spaces)
```bash
runctl aws train "$INSTANCE_ID" training/train.py \
    --script-args "--epochs 10 --batch-size 64"
```

## Error Handling

### Always Use `set -euo pipefail`
```bash
#!/bin/bash
set -euo pipefail  # Exit on error, undefined vars, pipe failures
```

### Cleanup on Exit
```bash
cleanup() {
    if [ -n "${INSTANCE_ID:-}" ]; then
        runctl aws terminate "$INSTANCE_ID" --force || true
    fi
}
trap cleanup EXIT
```

### Validate Output
```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)

if [[ ! "$INSTANCE_ID" =~ ^i-[a-z0-9]+$ ]]; then
    echo "ERROR: Invalid instance ID: $INSTANCE_ID"
    exit 1
fi
```

## Migration Guide

### Migrating Old Scripts

**Before:**
```bash
INSTANCE_ID=$(runctl aws create --spot | grep -o 'i-[a-z0-9]*')
sleep 60
runctl aws train $INSTANCE_ID training/train.py
```

**After:**
```bash
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)
runctl aws train "$INSTANCE_ID" training/train.py --wait
```

### Key Changes

1. Add `--wait` to `create` command
2. Add `--output instance-id` to `create` command
3. Remove `sleep` commands
4. Remove `grep` parsing
5. Add `--wait` to `train` command (if you want to wait for completion)
6. Quote variables: `"$INSTANCE_ID"` instead of `$INSTANCE_ID`

## Testing Your Examples

1. **Validate prerequisites**:
   ```bash
   command -v runctl || echo "ERROR: runctl not found"
   aws sts get-caller-identity || echo "ERROR: AWS credentials not configured"
   ```

2. **Test with minimal instance**:
   ```bash
   INSTANCE_TYPE=t3.micro ./examples/complete_workflow.sh
   ```

3. **Check exit codes**:
   ```bash
   ./examples/complete_workflow.sh
   echo "Exit code: $?"
   ```

## Troubleshooting

### Instance Creation Fails
- Check AWS credentials: `aws sts get-caller-identity`
- Check instance limits: `aws ec2 describe-account-attributes`
- Try different instance type or region

### Training Fails
- Check instance status: `runctl aws status $INSTANCE_ID`
- Check SSM connectivity: `aws ssm describe-instance-information --instance-ids $INSTANCE_ID`
- Verify training script exists and is executable

### Script Arguments Not Working
- Use separate arguments: `--script-args "--epochs" "10"` not `--script-args "--epochs 10"`
- Check script accepts arguments correctly
- Test script locally first

