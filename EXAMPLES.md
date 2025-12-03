# trainctl Examples

## Local Training

```bash
# Basic local training
trainctl local training/train.py --epochs 50 --batch-size 128

# With custom checkpoint directory
trainctl local training/train.py --checkpoint-dir ./my_checkpoints
```

## RunPod Workflow

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

## AWS EC2 Workflow

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

