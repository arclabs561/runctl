#!/bin/bash
# Complete E2E Training Workflow Example
#
# This script demonstrates the complete workflow using improved runctl features:
# - --wait flags for async operations
# - --output instance-id for structured output
# - Proper error handling and cleanup
#
# Usage: ./examples/complete_workflow.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
INSTANCE_TYPE="${INSTANCE_TYPE:-t3.micro}"
USE_SPOT="${USE_SPOT:-true}"
TRAINING_SCRIPT="${TRAINING_SCRIPT:-training/train_mnist_e2e.py}"
EPOCHS="${EPOCHS:-3}"

# Cleanup function
cleanup() {
    if [ -n "${INSTANCE_ID:-}" ]; then
        echo -e "${YELLOW}Cleaning up instance ${INSTANCE_ID}...${NC}"
        runctl aws terminate "$INSTANCE_ID" --force || true
    fi
}

# Set trap for cleanup on exit
trap cleanup EXIT

echo -e "${GREEN}=== Complete E2E Training Workflow ===${NC}"
echo ""

# Check prerequisites
echo "Checking prerequisites..."

# Check if runctl is available
if ! command -v runctl &> /dev/null; then
    echo -e "${RED}ERROR: runctl not found. Please build it first: cargo build --release${NC}"
    exit 1
fi

# Check if training script exists
if [ ! -f "$TRAINING_SCRIPT" ]; then
    echo -e "${YELLOW}WARNING: Training script not found: $TRAINING_SCRIPT${NC}"
    echo "Creating minimal test script..."
    mkdir -p training
    cat > "$TRAINING_SCRIPT" << 'EOF'
#!/usr/bin/env python3
import time
import json
from pathlib import Path

print("Starting training...")
Path("checkpoints").mkdir(exist_ok=True)

for epoch in range(3):
    print(f"Epoch {epoch+1}/3")
    checkpoint = {"epoch": epoch+1, "loss": 1.0/(epoch+1)}
    Path(f"checkpoints/epoch_{epoch+1}.json").write_text(json.dumps(checkpoint))
    time.sleep(1)

Path("training_complete.txt").write_text("Training completed")
print("Training completed!")
EOF
    chmod +x "$TRAINING_SCRIPT"
    echo -e "${GREEN}Created test script: $TRAINING_SCRIPT${NC}"
fi

# Check AWS credentials
if ! aws sts get-caller-identity &> /dev/null; then
    echo -e "${RED}ERROR: AWS credentials not configured. Run 'aws configure' first.${NC}"
    exit 1
fi

# Check S3 bucket configuration (required for SSM code sync)
if [ -f .runctl.toml ]; then
    if ! grep -q "s3_bucket" .runctl.toml; then
        echo -e "${YELLOW}WARNING: S3 bucket not configured in .runctl.toml${NC}"
        echo "  SSM code sync requires S3 bucket. Add to .runctl.toml:"
        echo "  [aws]"
        echo "  s3_bucket = \"your-bucket-name\""
        echo ""
        echo "  Or use SSH: Create instance with --key-name instead of --iam-instance-profile"
        echo ""
        read -p "Continue anyway? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
fi

echo -e "${GREEN}Prerequisites check passed${NC}"
echo ""

# Step 1: Create instance
echo -e "${GREEN}Step 1: Creating instance...${NC}"
echo "  Instance type: $INSTANCE_TYPE"
echo "  Spot instance: $USE_SPOT"
echo ""

SPOT_FLAG=""
if [ "$USE_SPOT" = "true" ]; then
    SPOT_FLAG="--spot"
fi

INSTANCE_ID=$(runctl aws create "$INSTANCE_TYPE" $SPOT_FLAG --wait --output instance-id 2>&1)

# Validate instance ID
if [[ ! "$INSTANCE_ID" =~ ^i-[a-z0-9]+$ ]]; then
    echo -e "${RED}ERROR: Failed to create instance${NC}"
    echo "Output: $INSTANCE_ID"
    exit 1
fi

echo -e "${GREEN}✅ Instance created: $INSTANCE_ID${NC}"
echo ""

# Step 2: Check instance status
echo -e "${GREEN}Step 2: Checking instance status...${NC}"
runctl aws status "$INSTANCE_ID"
echo ""

# Step 3: Train with code sync
echo -e "${GREEN}Step 3: Starting training...${NC}"
echo "  Script: $TRAINING_SCRIPT"
echo "  Epochs: $EPOCHS"
echo ""

if runctl aws train "$INSTANCE_ID" "$TRAINING_SCRIPT" \
    --sync-code \
    --wait \
    -- --epochs "$EPOCHS"; then
    echo -e "${GREEN}✅ Training completed successfully!${NC}"
else
    echo -e "${RED}ERROR: Training failed${NC}"
    echo "Checking instance status for debugging..."
    runctl aws status "$INSTANCE_ID"
    exit 1
fi

echo ""

# Step 4: Verify training results
echo -e "${GREEN}Step 4: Verifying training results...${NC}"
runctl aws status "$INSTANCE_ID"
echo ""

# Step 5: Cleanup (handled by trap, but show message)
echo -e "${GREEN}Step 5: Cleanup${NC}"
echo "Instance will be terminated on exit..."
echo ""

echo -e "${GREEN}=== E2E Workflow Complete ===${NC}"

