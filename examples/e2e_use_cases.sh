#!/bin/bash
# Realistic E2E Use Cases for $RUNCTL
#
# This script demonstrates various real-world scenarios:
# 1. Basic training workflow
# 2. EBS volume workflow
# 3. S3 data transfer
# 4. Checkpoint resume
# 5. Spot instance handling
# 6. Docker container training
#
# Usage: ./examples/e2e_use_cases.sh [use-case-number]

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTANCE_TYPE="${INSTANCE_TYPE:-t3.medium}"
REGION="${REGION:-us-east-1}"

# Find runctl binary
if command -v runctl &> /dev/null; then
    RUNCTL="runctl"
elif [ -f "./target/release/runctl" ]; then
    RUNCTL="./target/release/runctl"
elif [ -f "./target/debug/runctl" ]; then
    RUNCTL="./target/debug/runctl"
else
    echo "Error: runctl not found. Please build it first: cargo build --release"
    exit 1
fi

# Cleanup function
cleanup() {
    if [ -n "${INSTANCE_ID:-}" ]; then
        echo -e "${YELLOW}Cleaning up instance ${INSTANCE_ID}...${NC}"
        $RUNCTL aws terminate "$INSTANCE_ID" --force 2>/dev/null || true
    fi
    if [ -n "${VOLUME_ID:-}" ]; then
        echo -e "${YELLOW}Cleaning up volume ${VOLUME_ID}...${NC}"
        $RUNCTL aws ebs delete "$VOLUME_ID" --force 2>/dev/null || true
    fi
}

trap cleanup EXIT

print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ️  $1${NC}"
}

# Use Case 1: Basic Training Workflow
use_case_1_basic_training() {
    print_header "Use Case 1: Basic Training Workflow"
    
    print_info "Creating instance with SSM..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --wait \
        --output instance-id)
    
    print_success "Created instance: $INSTANCE_ID"
    
    print_info "Starting training with code sync..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
        --sync-code \
        --wait \
        -- --epochs 3
    
    print_success "Training completed successfully!"
    
    print_info "Checking training logs..."
    $RUNCTL aws monitor "$INSTANCE_ID" | head -20
    
    print_success "Use case 1 completed!"
}

# Use Case 2: EBS Volume Workflow
use_case_2_ebs_volume() {
    print_header "Use Case 2: EBS Volume Workflow"
    
    print_info "Creating EBS volume..."
    VOLUME_ID=$($RUNCTL aws ebs create \
        --size 20 \
        --persistent \
        --output volume-id)
    
    print_success "Created volume: $VOLUME_ID"
    
    print_info "Creating instance..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --data-volume "$VOLUME_ID" \
        --wait \
        --output instance-id)
    
    print_success "Created instance with volume attached: $INSTANCE_ID"
    
    print_info "Starting training (data on EBS volume)..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
        --sync-code \
        --wait \
        -- --epochs 2 \
        --checkpoint-dir /data/checkpoints
    
    print_success "Training completed with EBS volume!"
    
    print_info "Listing volumes..."
    $RUNCTL aws ebs list
    
    print_success "Use case 2 completed!"
}

# Use Case 3: Checkpoint Resume
use_case_3_checkpoint_resume() {
    print_header "Use Case 3: Checkpoint Resume"
    
    print_info "Creating instance..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --wait \
        --output instance-id)
    
    print_success "Created instance: $INSTANCE_ID"
    
    print_info "Starting training (will be interrupted)..."
    # Start training in background, will stop after 2 epochs
    $RUNCTL aws train "$INSTANCE_ID" training/train_with_checkpoints.py \
        --sync-code \
        -- --epochs 5 \
        --checkpoint-interval 1 &
    
    TRAIN_PID=$!
    sleep 15  # Let it train for a bit
    
    print_info "Stopping instance to simulate interruption..."
    $RUNCTL aws stop "$INSTANCE_ID" --wait
    
    print_info "Restarting instance..."
    $RUNCTL aws start "$INSTANCE_ID" --wait
    
    print_info "Resuming training from checkpoint..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_with_checkpoints.py \
        --sync-code \
        --wait \
        -- --epochs 5 \
        --resume-from checkpoints
    
    print_success "Training resumed and completed!"
    
    print_success "Use case 3 completed!"
}

# Use Case 4: Hyperparameter Tuning
use_case_4_hyperparameters() {
    print_header "Use Case 4: Hyperparameter Tuning"
    
    print_info "Creating instance..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --wait \
        --output instance-id)
    
    print_success "Created instance: $INSTANCE_ID"
    
    print_info "Training with hyperparameters..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
        --sync-code \
        --hyperparams "lr=0.001,batch_size=32,epochs=3" \
        --wait
    
    print_success "Training with hyperparameters completed!"
    
    print_success "Use case 4 completed!"
}

# Use Case 5: Spot Instance (if available)
use_case_5_spot_instance() {
    print_header "Use Case 5: Spot Instance Training"
    
    print_info "Creating spot instance..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --spot \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --wait \
        --output instance-id 2>&1) || {
        print_error "Spot instance creation failed (may be capacity issues)"
        print_info "This is expected - spot instances may not be available"
        return 0
    }
    
    print_success "Created spot instance: $INSTANCE_ID"
    
    print_info "Starting training on spot instance..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
        --sync-code \
        --wait \
        -- --epochs 2
    
    print_success "Training on spot instance completed!"
    
    print_success "Use case 5 completed!"
}

# Use Case 6: Docker Container Training
use_case_6_docker() {
    print_header "Use Case 6: Docker Container Training"
    
    print_info "Checking for Dockerfile..."
    if [ ! -f "Dockerfile" ] && [ ! -f "training/Dockerfile" ]; then
        print_info "Creating sample Dockerfile..."
        cat > Dockerfile << 'EOF'
FROM python:3.9-slim
WORKDIR /app
COPY training/ /app/
RUN pip install --no-cache-dir -q numpy
CMD ["python3", "train_mnist_e2e.py", "--epochs", "2"]
EOF
    fi
    
    print_info "Creating instance..."
    INSTANCE_ID=$($RUNCTL aws create "$INSTANCE_TYPE" \
        --iam-instance-profile $RUNCTL-ssm-profile \
        --wait \
        --output instance-id)
    
    print_success "Created instance: $INSTANCE_ID"
    
    print_info "Building and pushing Docker image..."
    # Note: This requires ECR setup - may fail if not configured
    $RUNCTL docker build --push 2>&1 || {
        print_error "Docker build/push failed (ECR may not be configured)"
        print_info "Skipping Docker use case - requires ECR setup"
        return 0
    }
    
    print_info "Training in Docker container..."
    $RUNCTL aws train "$INSTANCE_ID" training/train_mnist_e2e.py \
        --sync-code \
        --docker \
        --wait \
        -- --epochs 2
    
    print_success "Docker training completed!"
    
    print_success "Use case 6 completed!"
}

# Main
main() {
    local use_case="${1:-all}"
    
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}$RUNCTL E2E Use Cases${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "Available use cases:"
    echo "  1. Basic Training Workflow"
    echo "  2. EBS Volume Workflow"
    echo "  3. Checkpoint Resume"
    echo "  4. Hyperparameter Tuning"
    echo "  5. Spot Instance Training"
    echo "  6. Docker Container Training"
    echo ""
    echo "Usage: $0 [1-6|all]"
    echo ""
    
    case "$use_case" in
        1)
            use_case_1_basic_training
            ;;
        2)
            use_case_2_ebs_volume
            ;;
        3)
            use_case_3_checkpoint_resume
            ;;
        4)
            use_case_4_hyperparameters
            ;;
        5)
            use_case_5_spot_instance
            ;;
        6)
            use_case_6_docker
            ;;
        all)
            use_case_1_basic_training
            use_case_2_ebs_volume
            use_case_3_checkpoint_resume
            use_case_4_hyperparameters
            use_case_5_spot_instance
            use_case_6_docker
            ;;
        *)
            print_error "Invalid use case: $use_case"
            echo "Use 1-6 or 'all'"
            exit 1
            ;;
    esac
    
    echo -e "\n${GREEN}========================================${NC}"
    echo -e "${GREEN}All use cases completed!${NC}"
    echo -e "${GREEN}========================================${NC}\n"
}

main "$@"

