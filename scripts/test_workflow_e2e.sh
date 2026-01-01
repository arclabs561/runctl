#!/bin/bash
# Test the improved workflow with --wait flags and structured output
# This script tests the actual developer experience

set -euo pipefail

echo "=== Testing Improved Workflow ==="
echo ""

# Check if training script exists
if [ ! -f "training/train_mnist_e2e.py" ]; then
    echo "ERROR: training/train_mnist_e2e.py not found"
    echo "Creating it from examples/test_training_script.py..."
    cp examples/test_training_script.py training/train_mnist_e2e.py
    chmod +x training/train_mnist_e2e.py
fi

# Test 1: Create instance with --wait and structured output
echo "Test 1: Creating instance with --wait and --output instance-id"
echo "Command: runctl aws create t3.micro --spot --iam-instance-profile runctl-ssm-profile --wait --output instance-id"
INSTANCE_ID=$(./target/release/runctl aws create t3.micro --spot --iam-instance-profile runctl-ssm-profile --wait --output instance-id 2>&1)
echo "Result: $INSTANCE_ID"
echo ""

# Verify we got an instance ID
if [[ ! "$INSTANCE_ID" =~ ^i-[a-z0-9]+$ ]]; then
    echo "ERROR: Did not get valid instance ID"
    echo "Output was: $INSTANCE_ID"
    exit 1
fi

echo "✅ Instance created: $INSTANCE_ID"
echo ""

# Test 2: Check instance status
echo "Test 2: Checking instance status"
echo "Command: runctl aws status $INSTANCE_ID"
./target/release/runctl aws status "$INSTANCE_ID"
echo ""

# Test 3: Train with --wait
echo "Test 3: Starting training with --wait"
echo "Command: runctl aws train $INSTANCE_ID training/train_mnist_e2e.py --sync-code -- --epochs 3 --wait"
./target/release/runctl aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 3
echo ""

# Test 4: Verify training completed
echo "Test 4: Verifying training completed"
echo "Command: runctl aws status $INSTANCE_ID"
./target/release/runctl aws status "$INSTANCE_ID"
echo ""

# Test 5: Cleanup
echo "Test 5: Cleaning up"
echo "Command: runctl aws terminate $INSTANCE_ID --force"
./target/release/runctl aws terminate "$INSTANCE_ID" --force
echo ""

echo "=== All Tests Complete ==="

