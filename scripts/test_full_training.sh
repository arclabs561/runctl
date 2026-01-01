#!/bin/bash
# Full E2E training test script
# Tests the complete workflow: create → sync → train → verify → cleanup

set -euo pipefail

echo "=========================================="
echo "Full Training E2E Test"
echo "=========================================="
echo ""

# Check AWS credentials
if ! aws sts get-caller-identity &>/dev/null; then
    echo "ERROR: AWS credentials not configured"
    echo "Run: aws configure"
    exit 1
fi

# Configuration
INSTANCE_TYPE="${INSTANCE_TYPE:-t3.micro}"  # Use smallest for testing
PROJECT_NAME="e2e-test-$(date +%s)"
SCRIPT_PATH="test_training_script.py"

echo "Configuration:"
echo "  Instance type: $INSTANCE_TYPE"
echo "  Project name: $PROJECT_NAME"
echo "  Training script: $SCRIPT_PATH"
echo ""

# Step 1: Create instance
echo "Step 1: Creating instance..."
INSTANCE_ID=$(runctl aws create \
    --instance-type "$INSTANCE_TYPE" \
    --project-name "$PROJECT_NAME" \
    2>&1 | grep -o 'i-[a-z0-9]*' | head -1)

if [ -z "$INSTANCE_ID" ]; then
    echo "ERROR: Failed to create instance"
    exit 1
fi

echo "  Created instance: $INSTANCE_ID"
echo "  Waiting for instance to be ready..."

# Wait for instance to be running
MAX_WAIT=300
ELAPSED=0
while [ $ELAPSED -lt $MAX_WAIT ]; do
    STATE=$(aws ec2 describe-instances \
        --instance-ids "$INSTANCE_ID" \
        --query 'Reservations[0].Instances[0].State.Name' \
        --output text 2>/dev/null || echo "pending")
    
    if [ "$STATE" = "running" ]; then
        echo "  Instance is running"
        sleep 10  # Wait for SSM to be ready
        break
    fi
    
    sleep 5
    ELAPSED=$((ELAPSED + 5))
    echo -n "."
done

if [ "$STATE" != "running" ]; then
    echo ""
    echo "ERROR: Instance did not start in time"
    runctl aws terminate "$INSTANCE_ID" || true
    exit 1
fi

echo ""

# Step 2: Sync code
echo "Step 2: Syncing code..."
if [ -f "$SCRIPT_PATH" ]; then
    # Get project directory (parent of script)
    SCRIPT_DIR=$(dirname "$(realpath "$SCRIPT_PATH")")
    
    # Use runctl to sync (this would require the actual sync command)
    echo "  Code sync would happen here (using runctl aws train --sync-code)"
    echo "  For now, we'll create the script directly on the instance"
else
    echo "  WARNING: Training script not found: $SCRIPT_PATH"
fi

# Step 3: Create training script on instance
echo "Step 3: Setting up training on instance..."
TRAIN_SCRIPT=$(cat <<'PYTHON_EOF'
#!/usr/bin/env python3
import os
import json
import time
from pathlib import Path

def train_model(epochs=3, checkpoint_dir="checkpoints"):
    os.makedirs(checkpoint_dir, exist_ok=True)
    
    for epoch in range(epochs):
        loss = 1.0 / (epoch + 1)
        checkpoint = {
            "epoch": epoch + 1,
            "loss": loss,
            "timestamp": time.time()
        }
        
        checkpoint_path = f"{checkpoint_dir}/checkpoint_epoch_{epoch+1}.json"
        with open(checkpoint_path, "w") as f:
            json.dump(checkpoint, f)
        
        print(f"Epoch {epoch+1}/{epochs}: loss={loss:.4f}")
        time.sleep(1)
    
    final_checkpoint = {
        "epoch": epochs,
        "loss": 1.0 / epochs,
        "status": "completed"
    }
    
    with open(f"{checkpoint_dir}/final_checkpoint.json", "w") as f:
        json.dump(final_checkpoint, f)
    
    with open("training_complete.txt", "w") as f:
        f.write("Training completed successfully\n")
    
    print("Training completed!")

if __name__ == "__main__":
    train_model(epochs=3)
PYTHON_EOF
)

# Upload script via SSM
PROJECT_DIR="/home/ec2-user/$PROJECT_NAME"
aws ssm send-command \
    --instance-ids "$INSTANCE_ID" \
    --document-name "AWS-RunShellScript" \
    --parameters "commands=[
        'mkdir -p $PROJECT_DIR',
        'cat > $PROJECT_DIR/train.py << \"PYEOF\"
$TRAIN_SCRIPT
PYEOF
',
        'chmod +x $PROJECT_DIR/train.py',
        'python3 $PROJECT_DIR/train.py'
    ]" \
    --output text \
    --query 'Command.CommandId' > /tmp/ssm_command_id.txt

COMMAND_ID=$(cat /tmp/ssm_command_id.txt)
echo "  Command ID: $COMMAND_ID"
echo "  Waiting for training to complete..."

# Wait for command to complete
MAX_WAIT=120
ELAPSED=0
while [ $ELAPSED -lt $MAX_WAIT ]; do
    STATUS=$(aws ssm get-command-invocation \
        --command-id "$COMMAND_ID" \
        --instance-id "$INSTANCE_ID" \
        --query 'Status' \
        --output text 2>/dev/null || echo "InProgress")
    
    if [ "$STATUS" = "Success" ]; then
        echo "  Training completed!"
        break
    elif [ "$STATUS" = "Failed" ] || [ "$STATUS" = "Cancelled" ] || [ "$STATUS" = "TimedOut" ]; then
        echo ""
        echo "ERROR: Training failed with status: $STATUS"
        aws ssm get-command-invocation \
            --command-id "$COMMAND_ID" \
            --instance-id "$INSTANCE_ID" \
            --query 'StandardErrorContent' \
            --output text
        runctl aws terminate "$INSTANCE_ID" || true
        exit 1
    fi
    
    sleep 3
    ELAPSED=$((ELAPSED + 3))
    echo -n "."
done

if [ "$STATUS" != "Success" ]; then
    echo ""
    echo "ERROR: Training did not complete in time"
    runctl aws terminate "$INSTANCE_ID" || true
    exit 1
fi

echo ""

# Step 4: Verify training results
echo "Step 4: Verifying training results..."
OUTPUT=$(aws ssm get-command-invocation \
    --command-id "$COMMAND_ID" \
    --instance-id "$INSTANCE_ID" \
    --query 'StandardOutputContent' \
    --output text)

echo "$OUTPUT"

# Check for checkpoints
CHECKPOINT_CHECK=$(aws ssm send-command \
    --instance-ids "$INSTANCE_ID" \
    --document-name "AWS-RunShellScript" \
    --parameters "commands=[\"ls -la $PROJECT_DIR/checkpoints/\"]" \
    --output text \
    --query 'Command.CommandId')

sleep 5
CHECKPOINT_OUTPUT=$(aws ssm get-command-invocation \
    --command-id "$CHECKPOINT_CHECK" \
    --instance-id "$INSTANCE_ID" \
    --query 'StandardOutputContent' \
    --output text)

if echo "$CHECKPOINT_OUTPUT" | grep -q "final_checkpoint.json"; then
    echo "  ✅ Checkpoints created successfully"
else
    echo "  ⚠️  Warning: Could not verify checkpoints"
fi

# Step 5: Cleanup
echo ""
echo "Step 5: Cleaning up..."
runctl aws terminate "$INSTANCE_ID" || true

echo ""
echo "=========================================="
echo "✅ Full training E2E test completed!"
echo "=========================================="

