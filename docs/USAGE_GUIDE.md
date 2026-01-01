# runctl Usage Guide

**Clear guidance to avoid common mistakes and use runctl effectively.**

## Table of Contents

1. [Getting Started](#getting-started)
2. [Common Mistakes to Avoid](#common-mistakes-to-avoid)
3. [Cost Management](#cost-management)
4. [Data Management](#data-management)
5. [Best Practices](#best-practices)
6. [Troubleshooting](#troubleshooting)

## Getting Started

### First-Time Setup

```bash
# 1. Initialize configuration
runctl init

# 2. Verify AWS credentials
aws sts get-caller-identity

# 3. Check your setup
runctl resources list
```

**⚠️ Security Note**: Never use AWS root credentials. Use IAM users or roles instead.
See [AWS Security Best Practices](AWS_SECURITY_BEST_PRACTICES.md) for details.

### Basic Workflow

```bash
# 1. Create instance
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# 2. Wait for instance to be ready (IMPORTANT!)
# Check status: runctl aws instances list
# Or wait: sleep 60

# 3. Train
runctl aws train $INSTANCE_ID training/train.py --sync-code

# 4. Monitor
runctl aws monitor $INSTANCE_ID --follow

# 5. Clean up
runctl aws stop $INSTANCE_ID  # or terminate
```

## Common Mistakes to Avoid

### 1. ❌ Forgetting to Wait for Instance

**Mistake:**
```bash
INSTANCE_ID=$(runctl aws create ...)
runctl aws train $INSTANCE_ID train.py  # FAILS: instance not ready
```

**✅ Correct:**
```bash
INSTANCE_ID=$(runctl aws create ...)
# Wait 30-60 seconds, or check status
runctl aws instances list
runctl aws train $INSTANCE_ID train.py
```

### 2. ❌ Not Monitoring Costs

**Mistake:**
```bash
# Create 10 instances, forget about them
for i in {1..10}; do
    runctl aws create --instance-type p3.2xlarge &
done
# $1000+ bill later...
```

**✅ Correct:**
```bash
# Monitor costs regularly
runctl resources list --watch

# Set reminders or use cost alerts
# Check before leaving: runctl resources list
```

### 3. ❌ Using Terminate Instead of Stop

**Mistake:**
```bash
runctl aws terminate $INSTANCE_ID  # Can't restart, lost instance
```

**✅ Correct:**
```bash
runctl aws stop $INSTANCE_ID       # Can restart later
runctl aws start $INSTANCE_ID      # Resume where you left off
```

**When to use each:**
- **`stop`**: You might need the instance again, want to preserve state
- **`terminate`**: You're done forever, want to save money immediately

### 4. ❌ Forgetting EBS Volume Costs

**Mistake:**
```bash
runctl aws create --data-volume-size 500
runctl aws terminate $INSTANCE_ID
# Volume still exists, still costing $50/month
```

**✅ Correct:**
```bash
runctl aws create --data-volume-size 500
VOLUME_ID=$(runctl aws ebs list | grep ...)
runctl aws terminate $INSTANCE_ID
runctl aws ebs delete $VOLUME_ID  # Delete if not needed
```

### 5. ❌ Not Using Spot Instances for Fault-Tolerant Work

**Mistake:**
```bash
# Always using on-demand (expensive)
runctl aws create --instance-type g4dn.xlarge  # $0.50/hr
```

**✅ Correct:**
```bash
# Use spot for fault-tolerant training (90% cheaper)
runctl aws create --spot --instance-type g4dn.xlarge  # $0.05/hr
# Ensure training checkpoints frequently!
```

### 6. ❌ Running Training Without Checkpoints

**Mistake:**
```bash
# Training script doesn't save checkpoints
runctl aws train $INSTANCE_ID train.py
# Spot instance terminates, all progress lost
```

**✅ Correct:**
```bash
# Ensure training script saves checkpoints
runctl aws train $INSTANCE_ID train.py \
    --hyperparams checkpoint_interval=5
# Progress saved, can resume
```

### 7. ❌ Not Syncing Code

**Mistake:**
```bash
runctl aws train $INSTANCE_ID train.py --sync-code false
# Running old code, confusing results
```

**✅ Correct:**
```bash
# Always sync code (default)
runctl aws train $INSTANCE_ID train.py --sync-code
# Or explicitly: --sync-code true
```

### 8. ❌ Wrong Availability Zone for EBS

**Mistake:**
```bash
# Create volume in us-east-1a
runctl aws ebs create --size 500 --availability-zone us-east-1a
# Create instance in us-east-1b
runctl aws create --instance-type g4dn.xlarge  # Might be in 1b
runctl aws ebs attach $VOLUME_ID $INSTANCE_ID  # FAILS: wrong AZ
```

**✅ Correct:**
```bash
# Create instance first, note its AZ
INSTANCE_ID=$(runctl aws create ...)
AZ=$(runctl aws instances list | grep $INSTANCE_ID | awk '{print $5}')
# Create volume in same AZ
runctl aws ebs create --size 500 --availability-zone $AZ
runctl aws ebs attach $VOLUME_ID $INSTANCE_ID  # Works!
```

## Cost Management

### Monitor Costs Regularly

```bash
# Watch costs in real-time
runctl resources list --watch

# Check costs before leaving
runctl resources list

# Set up AWS billing alerts (outside runctl)
```

### Use Spot Instances

```bash
# 90% cost savings for fault-tolerant workloads
runctl aws create --spot --instance-type g4dn.xlarge
```

### Clean Up Unused Resources

```bash
# List all resources
runctl resources list

# Stop unused instances
runctl resources stop-all

# Delete unused EBS volumes
runctl aws ebs list
runctl aws ebs delete <volume-id>

# Clean up orphaned resources
runctl resources cleanup
```

### Cost Estimation

```bash
# Check instance costs
runctl resources list  # Shows hourly costs

# Estimate total cost
# Hourly cost × hours running = total cost
# Example: $0.50/hr × 24 hours = $12/day
```

## Data Management

### Using EBS Volumes

```bash
# 1. Create volume
VOLUME_ID=$(runctl aws ebs create --size 500 | grep -o 'vol-[a-z0-9]*')

# 2. Pre-warm with data (optional, faster)
runctl aws ebs pre-warm $VOLUME_ID --s3-source s3://bucket/data/

# 3. Attach to instance (must be in same AZ!)
runctl aws ebs attach $VOLUME_ID $INSTANCE_ID

# 4. Mount on instance (one-time setup)
# SSH or SSM into instance:
sudo mkfs -t xfs /dev/nvme1n1  # First time only
sudo mkdir -p /mnt/data
sudo mount /dev/nvme1n1 /mnt/data

# 5. Use in training
runctl aws train $INSTANCE_ID train.py  # EBS automatically mounted in Docker
```

### Using S3 for Data

```bash
# Auto-download before training
runctl aws train $INSTANCE_ID train.py \
    --data-s3 s3://bucket/datasets/

# Data is downloaded to {project_dir}/data on instance
```

### Checkpoint Management

```bash
# List checkpoints
runctl checkpoint list checkpoints/

# Resume from checkpoint
runctl checkpoint resume checkpoints/epoch_10.pt train.py
```

## Best Practices

### 1. Use Spot Instances for Development

```bash
# Development/testing: use spot (cheap, can be interrupted)
runctl aws create --spot --instance-type t3.medium

# Production: use on-demand (reliable, more expensive)
runctl aws create --instance-type g4dn.xlarge
```

### 2. Checkpoint Frequently

```bash
# In your training script, save checkpoints every N epochs
# Example: --checkpoint-interval 5
runctl aws train $INSTANCE_ID train.py \
    --hyperparams checkpoint_interval=5
```

### 3. Use Hyperparameters Flag

```bash
# Cleaner than --script-args
runctl aws train $INSTANCE_ID train.py \
    --hyperparams epochs=50,lr=0.001,batch_size=32

# Instead of:
runctl aws train $INSTANCE_ID train.py \
    -- --epochs 50 --lr 0.001 --batch-size 32
```

### 4. Monitor Training Progress

```bash
# Watch logs in real-time
runctl aws monitor $INSTANCE_ID --follow

# Check resource usage
runctl aws processes $INSTANCE_ID --watch
```

### 5. Use Project Names for Organization

```bash
# Group related instances
runctl aws create --project-name my-experiment-1 ...
runctl aws create --project-name my-experiment-2 ...

# List by project
runctl resources list --project my-experiment-1
```

### 6. Clean Up Regularly

```bash
# Daily cleanup routine
runctl resources list                    # Check what's running
runctl resources stop-all                # Stop unused instances
runctl aws ebs list                     # Check volumes
runctl resources cleanup                # Clean up orphaned resources
```

## Troubleshooting

### Instance Not Ready

**Problem:** Training fails immediately after creating instance.

**Solution:**
```bash
# Wait for instance to be ready
runctl aws instances list  # Check status
# Wait 30-60 seconds, then try again
```

### SSM Not Working

**Problem:** "SSM command failed" errors.

**Solution:**
```bash
# Check SSM connectivity
aws ssm describe-instance-information --instance-ids $INSTANCE_ID

# Verify IAM role has SSM permissions
# Or use SSH instead: provide --key-name when creating instance
```

### EBS Volume Can't Attach

**Problem:** "Availability zone mismatch" error.

**Solution:**
```bash
# Check instance AZ
runctl aws instances list | grep $INSTANCE_ID

# Create volume in same AZ
runctl aws ebs create --size 500 --availability-zone <same-az>
```

### Training Not Starting

**Problem:** Training command runs but nothing happens.

**Solution:**
```bash
# Check logs
runctl aws monitor $INSTANCE_ID

# Check if process is running
runctl aws processes $INSTANCE_ID

# Verify script path is correct
# Verify dependencies are installed
```

### High Costs

**Problem:** Unexpectedly high AWS bill.

**Solution:**
```bash
# Check what's running
runctl resources list

# Stop unused instances
runctl resources stop-all

# Delete unused EBS volumes
runctl aws ebs list
runctl aws ebs delete <volume-id>

# Set up AWS billing alerts
```

## Quick Reference

### Cost-Saving Tips

1. ✅ Use spot instances for development
2. ✅ Stop instances when not in use (don't terminate if you'll restart)
3. ✅ Delete unused EBS volumes
4. ✅ Monitor costs regularly: `runctl resources list --watch`
5. ✅ Use smaller instance types when possible

### Safety Tips

1. ✅ Always checkpoint frequently (especially with spot instances)
2. ✅ Use `stop` instead of `terminate` if you might restart
3. ✅ Verify instance is ready before training
4. ✅ Monitor training progress regularly
5. ✅ Clean up resources when done

### Performance Tips

1. ✅ Pre-warm EBS volumes with data from S3
2. ✅ Use EBS volumes for large datasets (faster than S3)
3. ✅ Use `--data-s3` for automatic data download
4. ✅ Use `--hyperparams` for cleaner argument passing
5. ✅ Monitor resource usage to optimize instance types

## Getting Help

- **Command help**: `runctl <command> --help` (most commands have detailed help)
- **Examples**: `docs/EXAMPLES.md` (complete workflow examples)
- **Usage guide**: `docs/USAGE_GUIDE.md` (this document)
- **Architecture**: `docs/ARCHITECTURE.md` (system design)
- **Security**: `docs/AWS_SECURITY_BEST_PRACTICES.md` (security best practices)
- **Integration docs**: `docs/INTEGRATION_IMPLEMENTATIONS.md` (feature details)

## Quick Command Reference

### Most Common Commands

```bash
# Create and train
runctl aws create --spot --instance-type g4dn.xlarge
sleep 60  # Wait for instance ready
runctl aws train <instance-id> train.py --sync-code
runctl aws monitor <instance-id> --follow

# Cost management
runctl resources list --watch          # Monitor costs in real-time
runctl resources summary               # Quick cost summary
runctl resources stop-all              # Stop all instances (save money)
runctl aws ebs list                    # See EBS volumes and costs

# Cleanup
runctl aws stop <instance-id>          # Stop (can restart, saves compute costs)
runctl aws terminate <instance-id>     # Delete permanently (cannot recover)
runctl aws ebs delete <volume-id>      # Delete unused volume (saves storage costs)
```

### When to Use What

- **`create`**: Start new instance (wait 30-60s before using)
- **`train`**: Run training job (instance must be ready)
- **`monitor`**: Watch training progress (use --follow for real-time)
- **`stop`**: Pause instance (can restart, saves compute costs)
- **`terminate`**: Delete instance permanently (cannot recover)
- **`resources list`**: Check costs and running resources (use --watch)
- **`ebs list`**: See all EBS volumes and costs (delete unused ones)
- **`resources cleanup`**: Remove orphaned resources (use --dry-run first)

