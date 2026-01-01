# runctl Quick Reference

**One-page reference for common operations and best practices.**

## Essential Commands

### Instance Management

```bash
# Create instance
runctl aws create --spot --instance-type g4dn.xlarge

# Wait for ready (IMPORTANT!)
sleep 60  # or: runctl aws instances list

# Train
runctl aws train <instance-id> train.py --sync-code

# Monitor
runctl aws monitor <instance-id> --follow

# Stop (can restart) or Terminate (permanent)
runctl aws stop <instance-id>
runctl aws terminate <instance-id>
```

### Cost Management

```bash
# Monitor costs
runctl resources list --watch

# Stop all instances
runctl resources stop-all

# List EBS volumes (check for unused)
runctl aws ebs list

# Delete unused volume
runctl aws ebs delete <volume-id>
```

### EBS Volumes

```bash
# Create volume (note: must match instance AZ!)
runctl aws ebs create --size 500 --availability-zone us-east-1a

# Attach to instance
runctl aws ebs attach <volume-id> --instance-id <instance-id>

# Mount on instance (one-time)
sudo mkfs -t xfs /dev/nvme1n1
sudo mkdir -p /mnt/data
sudo mount /dev/nvme1n1 /mnt/data
```

## Common Patterns

### Spot Instance Training

```bash
# Create spot instance
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# Wait for ready
sleep 60

# Train with checkpoints
runctl aws train $INSTANCE_ID train.py \
    --sync-code \
    --hyperparams epochs=50,checkpoint_interval=5

# Monitor
runctl aws monitor $INSTANCE_ID --follow
```

### Using S3 Data

```bash
# Auto-download data before training
runctl aws train <instance-id> train.py \
    --data-s3 s3://bucket/datasets/
```

### Using EBS for Large Datasets

```bash
# Create and pre-warm volume
VOLUME_ID=$(runctl aws ebs create --size 500 | grep -o 'vol-[a-z0-9]*')
runctl aws ebs pre-warm $VOLUME_ID --s3-source s3://bucket/data/

# Attach and mount (see EBS section above)
runctl aws ebs attach $VOLUME_ID --instance-id <instance-id>
# Then mount on instance

# Train (EBS automatically mounted in Docker)
runctl aws train <instance-id> train.py
```

## Cost-Saving Tips

1. ✅ **Use spot instances** (90% cheaper)
2. ✅ **Stop instances** when not in use (don't terminate if restarting)
3. ✅ **Delete unused EBS volumes** (they cost money)
4. ✅ **Monitor costs regularly**: `runctl resources list --watch`
5. ✅ **Use smaller instance types** when possible

## Safety Tips

1. ✅ **Checkpoint frequently** (especially with spot instances)
2. ✅ **Use `stop` not `terminate`** if you might restart
3. ✅ **Wait for instance ready** (30-60 seconds after creation)
4. ✅ **Monitor training** regularly
5. ✅ **Clean up resources** when done

## Common Mistakes to Avoid

❌ **Don't**: Train immediately after creating instance  
✅ **Do**: Wait 30-60 seconds or check status first

❌ **Don't**: Forget to delete unused EBS volumes  
✅ **Do**: Regularly check `runctl aws ebs list` and delete unused

❌ **Don't**: Use `terminate` when you might restart  
✅ **Do**: Use `stop` to preserve instance for later

❌ **Don't**: Create EBS volume in wrong AZ  
✅ **Do**: Check instance AZ first, create volume in same AZ

❌ **Don't**: Forget to monitor costs  
✅ **Do**: Use `runctl resources list --watch` regularly

## Getting Help

```bash
# Command help
runctl <command> --help

# Examples
cat docs/EXAMPLES.md

# Full usage guide
cat docs/USAGE_GUIDE.md
```

## Emergency Commands

```bash
# Stop all instances immediately
runctl resources stop-all --force

# See what's running
runctl resources list

# See costs
runctl resources summary
```

