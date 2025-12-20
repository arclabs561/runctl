# trainctl Examples

> **Note**: AWS EC2 is the primary and most tested platform. RunPod and local training are available but less actively maintained.

## AWS EC2 Workflow (Primary Platform)

### Quick Start

```bash
# 1. Create spot instance (cost-effective)
INSTANCE_ID=$(trainctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# 2. Train with automatic code sync
trainctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://my-bucket/data/ \
    --output-s3 s3://my-bucket/outputs/

# 3. Monitor training logs
trainctl aws monitor $INSTANCE_ID --follow

# 4. Check processes and resource usage
trainctl aws processes $INSTANCE_ID --detailed

# 5. Stop instance (preserves data)
trainctl aws stop $INSTANCE_ID

# Or terminate when done
trainctl aws terminate $INSTANCE_ID
```

### Complete Training Workflow

```bash
# Create instance with data volume
INSTANCE_ID=$(trainctl aws create \
    --spot \
    --instance-type g4dn.xlarge \
    --data-volume-size 100 \
    | grep -o 'i-[a-z0-9]*')

# Wait for instance to be ready
echo "Instance: $INSTANCE_ID"
echo "Waiting for instance to be ready..."
sleep 60

# Train with code sync and S3 data
trainctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://my-bucket/datasets/imagenet/ \
    --output-s3 s3://my-bucket/checkpoints/ \
    --script-args "--epochs 100 --batch-size 64"

# Monitor in real-time
trainctl aws monitor $INSTANCE_ID --follow

# Check resource usage
trainctl aws processes $INSTANCE_ID --watch

# When done, stop (preserves data) or terminate
trainctl aws stop $INSTANCE_ID
# trainctl aws terminate $INSTANCE_ID
```

### EBS Volume Management

```bash
# Create persistent volume
VOLUME_ID=$(trainctl aws ebs create --size 500 --persistent | grep -o 'vol-[a-z0-9]*')

# Pre-warm volume with data from S3
trainctl aws ebs pre-warm $VOLUME_ID --s3-source s3://my-bucket/datasets/

# Attach to instance
trainctl aws ebs attach $VOLUME_ID $INSTANCE_ID

# Create snapshot
SNAPSHOT_ID=$(trainctl aws ebs snapshot $VOLUME_ID | grep -o 'snap-[a-z0-9]*')

# Restore from snapshot
trainctl aws ebs restore $SNAPSHOT_ID --attach $INSTANCE_ID
```

### Resource Management

```bash
# List all resources
trainctl resources list

# List only AWS instances
trainctl resources list --platform aws

# Filter by project
trainctl resources list --project my-project

# Watch mode (auto-refresh)
trainctl resources list --watch

# Stop all running instances
trainctl resources stop-all

# Cleanup orphaned resources
trainctl resources cleanup
```

## Local Training

```bash
# Basic local training
trainctl local training/train.py --epochs 50 --batch-size 128

# With custom checkpoint directory
trainctl local training/train.py --checkpoint-dir ./my_checkpoints
```

## RunPod Workflow (Experimental)

> **Note**: RunPod support is experimental and less tested than AWS.

```bash
# 1. Create a pod
POD_ID=$(trainctl runpod create --gpu "RTX 4080 SUPER" | grep -o 'pod-[a-z0-9]*')

# 2. Train on the pod
trainctl runpod train $POD_ID training/train_cloud.py --background

# 3. Monitor training
trainctl runpod monitor $POD_ID --follow

# 4. Download results
trainctl runpod download $POD_ID /workspace/checkpoints/best.pt ./best.pt
```

```bash
# 1. Create spot instance (cost-effective)
INSTANCE_ID=$(trainctl aws create --spot --instance-type t3.medium | grep -o 'i-[a-z0-9]*')

# 2. Train with S3 data
trainctl aws train $INSTANCE_ID training/train.py \
    --data-s3 s3://bucket/data.csv \
    --output-s3 s3://bucket/output/

# 3. Monitor
trainctl aws monitor $INSTANCE_ID --follow

# 4. Terminate when done
trainctl aws terminate $INSTANCE_ID
```

## Checkpoint Management

```bash
# List all checkpoints
trainctl checkpoint list checkpoints/

# Show checkpoint details
trainctl checkpoint info checkpoints/best_model.pt

# Resume from checkpoint
trainctl checkpoint resume checkpoints/checkpoint_epoch_10.pt training/train.py
```

## Monitoring

```bash
# Monitor log file (follow mode)
trainctl monitor --log training.log --follow

# Monitor checkpoints directory
trainctl monitor --checkpoint checkpoints/ --follow

# One-time status check
trainctl monitor --log training.log
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
trainctl init

# Use custom config
trainctl --config custom.toml local training/train.py
```

