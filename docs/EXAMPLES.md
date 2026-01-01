# Examples

## AWS EC2

```bash
# Create instance
INSTANCE_ID=$(runctl aws create --instance-type t3.micro --spot --wait --output instance-id)

# Train
runctl aws train $INSTANCE_ID training/train.py --sync-code --wait

# Monitor (if not using --wait)
runctl aws monitor $INSTANCE_ID --follow

# Check processes
runctl aws processes $INSTANCE_ID --watch

# Stop or terminate
runctl aws stop $INSTANCE_ID
```

### With S3 Data

```bash
INSTANCE_ID=$(runctl aws create --spot --wait --output instance-id)

runctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://bucket/data/ \
    --output-s3 s3://bucket/checkpoints/ \
    --wait
```

## EBS Volumes

```bash
VOLUME_ID=$(runctl aws ebs create --size 500 --output volume-id)
runctl aws ebs pre-warm $VOLUME_ID --s3-source s3://bucket/datasets/
runctl aws ebs attach $VOLUME_ID $INSTANCE_ID
```

## Resources

```bash
runctl resources list [--platform aws] [--watch]
runctl resources summary
runctl resources cleanup [--dry-run]
```

## Local Training

```bash
runctl local training/train.py --epochs 50
```

## RunPod

```bash
POD_ID=$(runctl runpod create --gpu "RTX 4080 SUPER" --output pod-id)
runctl runpod train $POD_ID training/train.py
runctl runpod monitor $POD_ID --follow
```

## Checkpoints

```bash
runctl checkpoint list checkpoints/
runctl checkpoint info checkpoints/best.pt
runctl checkpoint resume checkpoints/epoch_10.pt training/train.py
```

## Monitoring

```bash
runctl monitor --log training.log [--follow]
runctl monitor --checkpoint checkpoints/ [--follow]
runctl top  # Interactive dashboard
```

