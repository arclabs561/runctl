# Realistic E2E Workflows for runctl

This document provides step-by-step guides for common real-world ML training scenarios using `runctl`.

## Prerequisites

1. **SSM Setup** (one-time):
   ```bash
   ./scripts/setup-ssm-role.sh
   ```

2. **S3 Bucket** (configured in `.runctl.toml`):
   ```toml
   [aws]
   s3_bucket = "your-bucket-name"
   ```

3. **Build runctl**:
   ```bash
   cargo build --release
   ```

## Workflow 1: Basic Training with Code Sync

**Use Case**: Train a model with automatic code synchronization.

```bash
# 1. Create instance with SSM
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Train with code sync
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 10

# 3. Monitor (optional, if not using --wait)
./target/release/runctl aws monitor $INSTANCE_ID

# 4. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: ~2-3 minutes  
**Cost**: ~$0.01-0.02 (t3.medium for 2-3 minutes)

## Workflow 2: Training with EBS Volume

**Use Case**: Train with large datasets stored on persistent EBS volume.

```bash
# 1. Create EBS volume
VOLUME_ID=$(./target/release/runctl aws ebs create \
    --size 100 \
    --persistent \
    --output volume-id)

# 2. Create instance with volume attached
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --data-volume $VOLUME_ID \
    --wait \
    --output instance-id)

# 3. Pre-warm volume with data (optional)
./target/release/runctl aws ebs pre-warm $VOLUME_ID \
    --s3-source s3://your-bucket/datasets/

# 4. Train using data on EBS
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 10 \
    --data-dir /data/datasets \
    --checkpoint-dir /data/checkpoints

# 5. Cleanup (volume persists)
./target/release/runctl aws terminate $INSTANCE_ID
# Volume remains for next training run
```

**Time**: ~3-5 minutes (plus data transfer time)  
**Cost**: ~$0.01-0.02 (instance) + $0.10/month (100GB EBS)

## Workflow 3: Checkpoint Resume

**Use Case**: Resume training after interruption (spot termination, manual stop, etc.).

```bash
# 1. Create instance
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Start training (will be interrupted)
./target/release/runctl aws train $INSTANCE_ID training/train_with_checkpoints.py \
    --sync-code \
    -- --epochs 20 \
    --checkpoint-interval 2

# 3. Stop instance (simulates interruption)
./target/release/runctl aws stop $INSTANCE_ID
# Wait for instance to stop (check with: runctl resources list)

# 4. Restart instance
./target/release/runctl aws start $INSTANCE_ID --wait

# 5. Resume from checkpoint
./target/release/runctl aws train $INSTANCE_ID training/train_with_checkpoints.py \
    --sync-code \
    --wait \
    -- --epochs 20 \
    --resume-from checkpoints

# 6. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: Varies (depends on training duration)  
**Cost**: Only pay for running time

## Workflow 4: Hyperparameter Tuning

**Use Case**: Train with different hyperparameter configurations.

```bash
# 1. Create instance
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Train with hyperparameters (passed as script args)
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --lr 0.001 --batch-size 32 --epochs 10

# 3. Try different hyperparameters
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --lr 0.01 --batch-size 64 --epochs 10

# 4. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Note**: Hyperparameters are passed as script arguments after `--`. Your script needs to parse them using `argparse` or similar.

## Workflow 5: Spot Instance Training

**Use Case**: Cost-effective training with automatic checkpoint saving on interruption.

```bash
# 1. Create spot instance
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --spot \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Train (automatic checkpoint saving on interruption)
./target/release/runctl aws train $INSTANCE_ID training/train_with_checkpoints.py \
    --sync-code \
    --wait \
    -- --epochs 50 \
    --checkpoint-interval 5

# 3. If interrupted, resume on new instance
# (Spot monitoring automatically saves checkpoints)

# 4. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: Varies (spot instances can be interrupted)  
**Cost**: ~70-90% cheaper than on-demand

## Workflow 6: S3 Data Transfer

**Use Case**: Download training data from S3, train, upload results.

```bash
# 1. Create instance
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Train with S3 data
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --data-s3 s3://your-bucket/datasets/mnist/ \
    --output-s3 s3://your-bucket/outputs/ \
    --wait \
    -- --epochs 10

# 3. Check S3 for outputs
aws s3 ls s3://your-bucket/outputs/

# 4. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: ~3-5 minutes (plus data transfer time)  
**Cost**: ~$0.01-0.02 (instance) + S3 transfer costs

## Workflow 7: Docker Container Training

**Use Case**: Train in isolated Docker container with dependencies.

```bash
# 1. Setup ECR (one-time)
aws ecr create-repository --repository-name runctl-training

# 2. Build and push Docker image
./target/release/runctl docker build --push

# 3. Create instance
INSTANCE_ID=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 4. Train in Docker container
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --docker \
    --wait \
    -- --epochs 10

# 5. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: ~5-7 minutes (includes Docker build/push)  
**Cost**: ~$0.01-0.02 (instance) + ECR storage

## Workflow 8: Multi-Instance Parallel Training

**Use Case**: Train multiple models in parallel on different instances.

```bash
# 1. Create multiple instances
INSTANCE_1=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

INSTANCE_2=$(./target/release/runctl aws create t3.medium \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Train in parallel
./target/release/runctl aws train $INSTANCE_1 training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 10 &
    
./target/release/runctl aws train $INSTANCE_2 training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 10 &

wait

# 3. Cleanup
./target/release/runctl aws terminate $INSTANCE_1
./target/release/runctl aws terminate $INSTANCE_2
```

**Time**: ~2-3 minutes (parallel execution)  
**Cost**: ~$0.02-0.04 (2 instances)

## Workflow 9: Long-Running Training with Monitoring

**Use Case**: Monitor long-running training jobs.

```bash
# 1. Create instance
INSTANCE_ID=$(./target/release/runctl aws create g4dn.xlarge \
    --iam-instance-profile runctl-ssm-profile \
    --wait \
    --output instance-id)

# 2. Start training (background)
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    -- --epochs 100

# 3. Monitor in separate terminal
./target/release/runctl aws monitor $INSTANCE_ID --follow

# 4. Check resource usage
./target/release/runctl aws processes $INSTANCE_ID --watch

# 5. Wait for completion
./target/release/runctl aws train $INSTANCE_ID training/train_mnist_e2e.py \
    --sync-code \
    --wait \
    -- --epochs 100

# 6. Cleanup
./target/release/runctl aws terminate $INSTANCE_ID
```

**Time**: Varies (depends on training duration)  
**Cost**: Varies (g4dn.xlarge is ~$0.50/hr)

## Best Practices

1. **Always use `--wait`** for instance creation to ensure SSM is ready
2. **Use `--iam-instance-profile`** for secure SSM access (no SSH keys)
3. **Save checkpoints regularly** to handle interruptions
4. **Monitor costs** with `runctl resources list`
5. **Clean up resources** when done to avoid unnecessary costs
6. **Use spot instances** for fault-tolerant workloads
7. **Use EBS volumes** for large datasets (persistent storage)
8. **Use S3** for data transfer and checkpoint storage

## Troubleshooting

### SSM Not Ready
- Wait 60-90 seconds after instance creation
- Verify IAM instance profile is attached
- Check SSM agent status: `aws ssm describe-instance-information`

### Training Not Starting
- Check code sync completed successfully
- Verify script path is correct
- Check training.log on instance

### Completion Detection Not Working
- Ensure script creates `training_complete.txt`
- Check exit code in `training_exit_code.txt`
- Verify process completed (check PID file)

## See Also

- `docs/TRAINING_COMPLETION_DETECTION.md` - Completion detection guide
- `docs/E2E_USE_CASES_EXPERIENCE.md` - Real-world testing results
- `training/train_mnist_e2e.py` - Example training script
- `training/train_with_checkpoints.py` - Checkpoint resume example

