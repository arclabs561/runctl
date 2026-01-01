# Runnable Examples

This directory contains ready-to-use example scripts that demonstrate runctl workflows.

## Quick Start

### 1. Complete Workflow (Recommended)
```bash
./examples/complete_workflow.sh
```

This script demonstrates:
- Prerequisites checking
- Instance creation with `--wait`
- Training with `--wait`
- Error handling
- Automatic cleanup

### 2. Quick Test
```bash
./examples/quick_test.sh
```

Minimal example for quick testing.

### 3. Workflow Command
```bash
./examples/workflow_train_example.sh
```

Demonstrates the high-level `runctl workflow train` command.

## Prerequisites

Before running examples:

1. **Build runctl**:
   ```bash
   cargo build --release
   ```

2. **Configure AWS credentials**:
   ```bash
   aws configure
   ```

3. **Verify AWS access**:
   ```bash
   aws sts get-caller-identity
   ```

4. **Ensure training script exists**:
   ```bash
   ls training/train_mnist_e2e.py
   ```
   
   If missing, the scripts will create a minimal test script automatically.

## Customization

### Environment Variables

The `complete_workflow.sh` script supports customization via environment variables:

```bash
# Use different instance type
INSTANCE_TYPE=g4dn.xlarge ./examples/complete_workflow.sh

# Use on-demand instead of spot
USE_SPOT=false ./examples/complete_workflow.sh

# Use different training script
TRAINING_SCRIPT=training/train_mnist.py ./examples/complete_workflow.sh

# Customize epochs
EPOCHS=10 ./examples/complete_workflow.sh

# Combine options
INSTANCE_TYPE=t3.medium EPOCHS=5 USE_SPOT=true ./examples/complete_workflow.sh
```

## Script Details

### `complete_workflow.sh`

**Features**:
- ✅ Prerequisites validation
- ✅ Error handling with cleanup
- ✅ Colored output for better readability
- ✅ Configurable via environment variables
- ✅ Automatic cleanup on exit (trap)

**Usage**:
```bash
./examples/complete_workflow.sh
```

**Environment Variables**:
- `INSTANCE_TYPE`: EC2 instance type (default: `t3.micro`)
- `USE_SPOT`: Use spot instances (default: `true`)
- `TRAINING_SCRIPT`: Path to training script (default: `training/train_mnist_e2e.py`)
- `EPOCHS`: Number of training epochs (default: `3`)

### `quick_test.sh`

**Features**:
- ✅ Minimal example
- ✅ Fast execution
- ✅ No configuration needed

**Usage**:
```bash
./examples/quick_test.sh
```

### `workflow_train_example.sh`

**Features**:
- ✅ Demonstrates high-level workflow command
- ✅ Single command for complete workflow

**Usage**:
```bash
./examples/workflow_train_example.sh
```

## Best Practices

### 1. Always Use `--wait` Flags

```bash
# ✅ Good: Waits for instance to be ready
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)

# ❌ Bad: No waiting, may fail
INSTANCE_ID=$(runctl aws create --spot --output instance-id)
```

### 2. Use Structured Output

```bash
# ✅ Good: Structured output
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)

# ❌ Bad: Fragile parsing
INSTANCE_ID=$(runctl aws create --spot | grep -o 'i-[a-z0-9]*')
```

### 3. Always Cleanup

```bash
# ✅ Good: Cleanup on exit
cleanup() {
    if [ -n "${INSTANCE_ID:-}" ]; then
        runctl aws terminate "$INSTANCE_ID" --force || true
    fi
}
trap cleanup EXIT

# ❌ Bad: No cleanup, resources left running
```

### 4. Validate Output

```bash
# ✅ Good: Validate instance ID
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)
if [[ ! "$INSTANCE_ID" =~ ^i-[a-z0-9]+$ ]]; then
    echo "ERROR: Invalid instance ID"
    exit 1
fi

# ❌ Bad: Assume output is correct
```

### 5. Use Proper Error Handling

```bash
# ✅ Good: Exit on error, undefined vars, pipe failures
set -euo pipefail

# ❌ Bad: No error handling
```

## Troubleshooting

### Script Fails to Execute

**Problem**: Permission denied
```bash
chmod +x examples/*.sh
```

### Instance Creation Fails

**Problem**: AWS credentials not configured
```bash
aws configure
aws sts get-caller-identity
```

**Problem**: Instance limit reached
```bash
aws ec2 describe-account-attributes
```

### Training Fails

**Problem**: Training script not found
```bash
# Scripts will create a minimal test script automatically
# Or create manually:
cp examples/test_training_script.py training/train_mnist_e2e.py
chmod +x training/train_mnist_e2e.py
```

**Problem**: SSM not available
```bash
# Check SSM connectivity
aws ssm describe-instance-information --instance-ids $INSTANCE_ID

# Or use SSH fallback (requires SSH key)
runctl aws create --spot --key-name my-key --wait --output instance-id
```

## Cost Considerations

### Spot Instances (Recommended for Testing)

- **Cheaper**: 50-90% discount
- **Can be interrupted**: May terminate during training
- **Good for**: Testing, development, fault-tolerant workloads

### On-Demand Instances

- **More expensive**: Full price
- **Stable**: Won't be interrupted
- **Good for**: Production, critical workloads

### Instance Types

- **t3.micro**: Free tier eligible, good for testing (~$0.01/hour)
- **t3.medium**: Small workloads (~$0.04/hour)
- **g4dn.xlarge**: GPU instances for ML (~$0.50/hour)

## Next Steps

1. **Try the examples**: Run `./examples/complete_workflow.sh`
2. **Customize**: Use environment variables to customize
3. **Read documentation**: See `docs/EXAMPLES_IMPROVED.md` for best practices
4. **Create your own**: Use examples as templates for your workflows

## See Also

- `docs/EXAMPLES_RUNNABLE.md` - Runnable examples documentation
- `docs/EXAMPLES_IMPROVED.md` - Best practices and patterns
- `docs/EXAMPLES.md` - General examples (may use older patterns)

