#!/bin/bash
# Quick E2E Test - Minimal Example
#
# Fastest way to test runctl end-to-end:
# - Creates instance
# - Trains with minimal script
# - Cleans up
#
# Usage: ./examples/quick_test.sh

set -euo pipefail

echo "=== Quick E2E Test ==="

# Create instance and get ID
INSTANCE_ID=$(runctl aws create t3.micro --spot --wait --output instance-id)
echo "Instance: $INSTANCE_ID"

# Train (waits for completion)
runctl aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 3

# Cleanup
runctl aws terminate "$INSTANCE_ID" --force
echo "✅ Test complete"

