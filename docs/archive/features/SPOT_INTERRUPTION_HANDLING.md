# Spot Instance Interruption Handling

## Overview

`runctl` now automatically monitors spot instances for interruption warnings and handles graceful shutdown. When AWS sends a 2-minute termination notice, `runctl` will:

1. **Detect the interruption** via EC2 metadata service
2. **Save checkpoints** before termination
3. **Upload checkpoints to S3** (if configured)
4. **Gracefully stop training** to allow checkpoint save

## How It Works

### Automatic Monitoring

When you start training on a spot instance with SSM enabled, `runctl` automatically starts a background monitoring task that:

- Polls the EC2 metadata service every 30 seconds
- Checks for spot interruption warnings
- Triggers graceful shutdown when interruption is detected

### Graceful Shutdown Sequence

When an interruption is detected:

1. **Send SIGTERM** to training process (allows checkpoint save)
2. **Wait up to 90 seconds** for graceful shutdown
3. **Force kill** if still running after timeout
4. **Save latest checkpoint** (if available)
5. **Upload to S3** (if `s3_bucket` is configured in `.runctl.toml`)

## Configuration

### Required Setup

1. **IAM Instance Profile**: Instance must have an IAM role with SSM permissions
2. **S3 Bucket** (optional): Configure in `.runctl.toml` for checkpoint uploads

```toml
[aws]
region = "us-east-1"
s3_bucket = "my-checkpoint-bucket"  # For checkpoint uploads
```

### Training Script Requirements

Your training script should:

1. **Handle SIGTERM**: Save checkpoint on signal
2. **Save checkpoints regularly**: Use `checkpoint.save_interval` in config
3. **Store checkpoints in standard location**: `checkpoints/` directory

Example PyTorch training script:

```python
import signal
import torch

checkpoint_dir = "checkpoints"

def save_checkpoint(epoch, model, optimizer, loss):
    os.makedirs(checkpoint_dir, exist_ok=True)
    torch.save({
        'epoch': epoch,
        'model_state_dict': model.state_dict(),
        'optimizer_state_dict': optimizer.state_dict(),
        'loss': loss,
    }, f"{checkpoint_dir}/epoch_{epoch}.pt")

def signal_handler(sig, frame):
    print("Received SIGTERM, saving checkpoint...")
    # Save current state
    save_checkpoint(current_epoch, model, optimizer, current_loss)
    sys.exit(0)

signal.signal(signal.SIGTERM, signal_handler)

# Training loop
for epoch in range(num_epochs):
    # ... training code ...
    save_checkpoint(epoch, model, optimizer, loss)
```

## Usage

### Basic Usage

Spot interruption monitoring is **automatic** when:
- Instance is a spot instance
- SSM is available (IAM instance profile configured)
- Training is started with `runctl aws train`

```bash
# Create spot instance with IAM profile
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge \
    --iam-instance-profile MySSMProfile | grep -o 'i-[a-z0-9]*')

# Start training (monitoring starts automatically)
runctl aws train $INSTANCE_ID training/train.py --sync-code
```

### Manual Monitoring (Advanced)

You can also start monitoring manually (though this is usually not needed):

```bash
# This is done automatically, but you can verify it's running
runctl aws spot-monitor $INSTANCE_ID \
    --checkpoint-dir /home/ubuntu/project/checkpoints \
    --s3-bucket my-bucket \
    --s3-prefix checkpoints/spot-interruptions
```

## Checkpoint Recovery

After a spot interruption:

1. **Check S3** (if configured): Latest checkpoint should be in `s3://bucket/checkpoints/spot-interruptions/{instance_id}/`
2. **Resume training**: Use the saved checkpoint

```bash
# Download checkpoint from S3
aws s3 cp s3://my-bucket/checkpoints/spot-interruptions/i-123/checkpoints/epoch_10.pt ./checkpoints/

# Resume training
runctl aws train $NEW_INSTANCE_ID training/train.py \
    --sync-code \
    --script-args "--resume checkpoints/epoch_10.pt"
```

## Monitoring and Logs

### Check Monitoring Status

The monitoring task runs in the background. To verify it's working:

```bash
# Check instance logs
runctl aws monitor $INSTANCE_ID --follow

# Check for interruption events
runctl aws processes $INSTANCE_ID --detailed
```

### Interruption Events

When an interruption occurs, you'll see:

```
WARNING: Spot interruption detected for instance i-123!
Handling spot interruption for instance i-123
Interruption time: 2024-01-01T12:00:00Z
Training detected on instance i-123, attempting graceful shutdown...
Checkpoint saved: /home/ubuntu/project/checkpoints/epoch_10.pt
Checkpoint uploaded to S3: s3://bucket/checkpoints/spot-interruptions/i-123/...
```

## Best Practices

1. **Use EBS volumes** for checkpoint storage (persists after termination)
2. **Configure S3 bucket** for automatic checkpoint uploads
3. **Save checkpoints frequently** (every epoch or every N iterations)
4. **Handle SIGTERM** in your training script
5. **Test interruption handling** with a small training job first

## Limitations

- **SSM required**: Monitoring only works with SSM-enabled instances
- **2-minute warning**: AWS gives 2 minutes notice, but network delays may reduce this
- **Checkpoint size**: Large checkpoints may not upload in time (use EBS volumes)
- **Single instance**: Monitoring is per-instance (not coordinated across multiple instances)

## Troubleshooting

### Monitoring Not Starting

**Problem**: Monitoring doesn't start automatically

**Solutions**:
1. Verify instance is a spot instance: `runctl resources list --platform aws`
2. Check SSM connectivity: `aws ssm describe-instance-information --instance-ids $INSTANCE_ID`
3. Verify IAM role has SSM permissions

### Checkpoints Not Saved

**Problem**: Checkpoints not saved before termination

**Solutions**:
1. Ensure training script handles SIGTERM
2. Check checkpoint directory exists and is writable
3. Verify training process is writing checkpoints regularly
4. Increase `checkpoint.save_interval` in config

### S3 Upload Fails

**Problem**: Checkpoints not uploaded to S3

**Solutions**:
1. Verify `s3_bucket` is configured in `.runctl.toml`
2. Check IAM role has S3 write permissions
3. Verify AWS CLI is installed on instance (for upload command)
4. Check S3 bucket exists and is accessible

## Future Enhancements

- [ ] Auto-resume training on new instance after interruption
- [ ] Multi-instance coordination for distributed training
- [ ] Direct S3 upload (without AWS CLI dependency)
- [ ] Checkpoint validation before termination
- [ ] Metrics and alerting for interruption events

