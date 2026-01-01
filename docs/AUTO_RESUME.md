# Auto-Resume After Spot Interruption

## Overview

`runctl` can automatically resume training on a new spot instance after an interruption. This feature:

1. **Detects interruption** (via spot monitoring)
2. **Saves checkpoint** before termination
3. **Uploads to S3** (if configured)
4. **Creates new spot instance**
5. **Resumes training** from latest checkpoint

## Usage

### Enable Auto-Resume

Set the `TRAINCTL_AUTO_RESUME` environment variable:

```bash
export TRAINCTL_AUTO_RESUME=1
runctl aws train $INSTANCE_ID training/train.py --sync-code
```

Or enable it per-command:

```bash
TRAINCTL_AUTO_RESUME=1 runctl aws train $INSTANCE_ID training/train.py --sync-code
```

### Requirements

1. **S3 bucket configured** in `.runctl.toml`:
   ```toml
   [aws]
   s3_bucket = "my-checkpoint-bucket"
   ```

2. **IAM permissions** for:
   - EC2: Create instances
   - S3: Read/write checkpoints
   - SSM: Execute commands

3. **Training script** must support `--resume` argument:
   ```python
   parser.add_argument("--resume", help="Resume from checkpoint")
   ```

## How It Works

### Interruption Detection

When a spot instance receives a 2-minute termination notice:

1. **Monitoring detects** interruption via EC2 metadata service
2. **Graceful shutdown** sends SIGTERM to training process
3. **Checkpoint saved** (if training script handles SIGTERM)
4. **Checkpoint uploaded** to S3 at `s3://bucket/checkpoints/spot-interruptions/{instance_id}/`

### Auto-Resume Sequence

After checkpoint is saved:

1. **Find latest checkpoint** in S3
2. **Create new spot instance** (same type as original)
3. **Sync code** to new instance
4. **Resume training** with `--resume {checkpoint_s3_path}`

### Checkpoint Location

Checkpoints are stored in S3 at:
```
s3://{bucket}/checkpoints/spot-interruptions/{instance_id}/{checkpoint_file}
```

The auto-resume function finds the latest checkpoint by modification time.

## Example

```bash
# Enable auto-resume
export TRAINCTL_AUTO_RESUME=1

# Start training on spot instance
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')
runctl aws train $INSTANCE_ID training/train.py --sync-code

# If instance is interrupted:
# 1. Checkpoint saved automatically
# 2. New instance created automatically
# 3. Training resumes from checkpoint
```

## Manual Resume

If auto-resume fails or is disabled, you can manually resume:

```bash
# Find latest checkpoint in S3
aws s3 ls s3://my-bucket/checkpoints/spot-interruptions/i-123/ --recursive | sort

# Create new instance
NEW_INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# Resume training
runctl aws train $NEW_INSTANCE_ID training/train.py \
    --sync-code \
    -- --resume s3://my-bucket/checkpoints/spot-interruptions/i-123/checkpoints/epoch_10.pt
```

## Limitations

- **Single instance**: Auto-resume creates one new instance (not multiple)
- **Same instance type**: Uses default instance type from config
- **Checkpoint required**: If checkpoint save fails, auto-resume won't work
- **S3 required**: Auto-resume requires S3 bucket configuration

## Troubleshooting

### Auto-Resume Not Triggering

**Problem**: Interruption detected but auto-resume doesn't start

**Solutions**:
1. Verify `TRAINCTL_AUTO_RESUME=1` is set
2. Check S3 bucket is configured in `.runctl.toml`
3. Verify checkpoint was uploaded to S3
4. Check logs for auto-resume errors

### Checkpoint Not Found

**Problem**: Auto-resume can't find checkpoint in S3

**Solutions**:
1. Verify checkpoint was uploaded before instance terminated
2. Check S3 bucket permissions
3. Verify checkpoint path format matches expected pattern
4. Manually check S3: `aws s3 ls s3://bucket/checkpoints/spot-interruptions/`

### New Instance Creation Fails

**Problem**: Auto-resume fails to create new instance

**Solutions**:
1. Check EC2 instance limits
2. Verify IAM permissions for EC2
3. Check spot instance availability
4. Review error logs for specific failure reason

## Future Enhancements

- [ ] Configurable instance type for resume
- [ ] Multiple instance support (distributed training)
- [ ] Checkpoint validation before resume
- [ ] Resume from EBS volumes (not just S3)
- [ ] Retry logic for failed auto-resume attempts

