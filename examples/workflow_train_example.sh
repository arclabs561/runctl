#!/bin/bash
# Workflow Train Command Example
#
# Demonstrates the high-level workflow command that orchestrates everything:
# - Creates instance
# - Trains
# - Waits for completion
# - Provides cleanup instructions
#
# Usage: ./examples/workflow_train_example.sh

set -euo pipefail

echo "=== Workflow Train Example ==="

# Use the high-level workflow command
runctl workflow train training/train_mnist_e2e.py \
    --instance-type t3.micro \
    --spot \
    -- --epochs 3

echo "✅ Workflow complete!"
echo ""
echo "Note: The workflow command handles instance creation, training, and provides"
echo "cleanup instructions. Check the output above for the instance ID to terminate."

