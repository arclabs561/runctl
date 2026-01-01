# runctl Examples

> **Note**: AWS EC2 is the primary and most tested platform. RunPod and local training are available but less actively maintained.

## AWS EC2 Workflow (Primary Platform)

### Quick Start

```bash
# 1. Create spot instance (cost-effective) - with --wait to ensure it's ready
INSTANCE_ID=$(runctl aws create g4dn.xlarge --spot --wait --output instance-id)

# 2. Train with automatic code sync - with --wait to ensure completion
runctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://my-bucket/data/ \
    --output-s3 s3://my-bucket/outputs/ \
    --wait

# 3. Monitor training logs (if not using --wait)
runctl aws monitor $INSTANCE_ID --follow

# 4. Check processes and resource usage
runctl aws processes $INSTANCE_ID --detailed

# 5. Stop instance (preserves data)
runctl aws stop $INSTANCE_ID

# Or terminate when done
runctl aws terminate $INSTANCE_ID
```

### Complete Training Workflow

```bash
# Create instance with data volume - with --wait to ensure it's ready
INSTANCE_ID=$(runctl aws create g4dn.xlarge \
    --spot \
    --data-volume-size 100 \
    --wait \
    --output instance-id)

echo "Instance: $INSTANCE_ID"

# Train with code sync and S3 data - with --wait to ensure completion
runctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://my-bucket/datasets/imagenet/ \
    --output-s3 s3://my-bucket/checkpoints/ \
    --script-args "--epochs" "100" "--batch-size" "64" \
    --wait

# Monitor in real-time (optional, if not using --wait)
runctl aws monitor $INSTANCE_ID --follow

# Check resource usage
runctl aws processes $INSTANCE_ID --watch

# When done, stop (preserves data) or terminate
runctl aws stop $INSTANCE_ID
# runctl aws terminate $INSTANCE_ID
```

### EBS Volume Management

```bash
# Create persistent volume - use --output volume-id for structured output
VOLUME_ID=$(runctl aws ebs create --size 500 --persistent --output volume-id)

# Pre-warm volume with data from S3
runctl aws ebs pre-warm $VOLUME_ID --s3-source s3://my-bucket/datasets/

# Attach to instance
runctl aws ebs attach $VOLUME_ID $INSTANCE_ID

# Create snapshot
SNAPSHOT_ID=$(runctl aws ebs snapshot $VOLUME_ID | grep -o 'snap-[a-z0-9]*')

# Restore from snapshot
runctl aws ebs restore $SNAPSHOT_ID --attach $INSTANCE_ID
```

### Resource Management

```bash
# List all resources
runctl resources list

# List only AWS instances
runctl resources list --platform aws

# Filter by project
runctl resources list --project my-project

# Watch mode (auto-refresh)
runctl resources list --watch

# Stop all running instances
runctl resources stop-all

# Cleanup orphaned resources
runctl resources cleanup
```

## Local Training

```bash
# Basic local training
runctl local training/train.py --epochs 50 --batch-size 128

# With custom checkpoint directory
runctl local training/train.py --checkpoint-dir ./my_checkpoints
```

## RunPod Workflow (Experimental)

> **Note**: RunPod support is experimental and less tested than AWS.

```bash
# 1. Create a pod - use --output pod-id for structured output
POD_ID=$(runctl runpod create --gpu "RTX 4080 SUPER" --output pod-id)

# 2. Train on the pod
runctl runpod train $POD_ID training/train_cloud.py --background

# 3. Monitor training
runctl runpod monitor $POD_ID --follow

# 4. Download results
runctl runpod download $POD_ID /workspace/checkpoints/best.pt ./best.pt
```

```bash
# 1. Create spot instance (cost-effective) - with --wait to ensure it's ready
INSTANCE_ID=$(runctl aws create t3.medium --spot --wait --output instance-id)

# 2. Train with S3 data - with --wait to ensure completion
runctl aws train $INSTANCE_ID training/train.py \
    --data-s3 s3://bucket/data.csv \
    --output-s3 s3://bucket/output/ \
    --wait

# 3. Monitor (optional, if not using --wait)
runctl aws monitor $INSTANCE_ID --follow

# 4. Terminate when done
runctl aws terminate $INSTANCE_ID
```

## Checkpoint Management

```bash
# List all checkpoints
runctl checkpoint list checkpoints/

# Show checkpoint details
runctl checkpoint info checkpoints/best_model.pt

# Resume from checkpoint
runctl checkpoint resume checkpoints/checkpoint_epoch_10.pt training/train.py
```

## Monitoring

```bash
# Monitor log file (follow mode)
runctl monitor --log training.log --follow

# Monitor checkpoints directory
runctl monitor --checkpoint checkpoints/ --follow

# One-time status check
runctl monitor --log training.log
```

## Using with `just`

```bash
# Build
just build

# Train locally
just train-local training/train.py

# RunPod workflow
just runpod-create
just runpod-train <pod-id> training/train.py

# Monitor
just monitor training.log
```

## Configuration

```bash
# Initialize config
runctl init

# Use custom config
runctl --config custom.toml local training/train.py
```

