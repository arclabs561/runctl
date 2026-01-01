# EBS Volume Optimization for Spot Instances

## Problem Statement

Spot instances can be interrupted, causing:
- Loss of ephemeral storage data
- Need to re-download datasets on restart
- Slow training startup times
- Checkpoint loss if not backed up

## Solution: EBS Volumes for Persistent Storage

### Benefits

1. **Persistent Data Storage**
   - Data survives spot interruptions
   - No need to re-download datasets
   - Faster instance startup

2. **Pre-warmed Volumes**
   - Pre-populate EBS volumes with datasets
   - Attach to spot instances for instant access
   - 10-100x faster than S3 downloads

3. **Checkpoint Persistence**
   - Save checkpoints to EBS volumes
   - Survive instance terminations
   - Fast checkpoint I/O

4. **Cost Efficiency**
   - EBS gp3 volumes: ~$0.08/GB/month
   - Faster training = lower compute costs
   - Reusable across multiple spot instances

## Implementation Patterns

### Pattern 1: Pre-warmed EBS Volume

```python
# Create EBS volume with pre-loaded data
volume = ec2.create_volume(
    Size=500,  # GB
    VolumeType='gp3',
    AvailabilityZone=az,
    TagSpecifications=[{
        'ResourceType': 'volume',
        'Tags': [
            {'Key': 'Name', 'Value': 'training-datasets'},
            {'Key': 'Project', 'Value': 'runctl'},
        ]
    }]
)

# Pre-populate with data (one-time setup)
# aws s3 sync s3://bucket/datasets/ /mnt/data/
```

**runctl translation:**
```bash
# Create and pre-warm EBS volume
runctl aws ebs create --size 500 --type gp3 \
    --name training-datasets \
    --pre-warm s3://bucket/datasets/

# Launch spot instance with pre-warmed volume
runctl aws create --spot \
    --ebs-volume vol-xxxxx \
    --mount-point /mnt/data
```

### Pattern 2: EBS Snapshot for Checkpoints

```python
# Create snapshot of checkpoint volume
snapshot = ec2.create_snapshot(
    VolumeId=checkpoint_volume_id,
    Description='Training checkpoint backup'
)

# Restore from snapshot on new instance
volume = ec2.create_volume(
    SnapshotId=snapshot_id,
    VolumeType='gp3',
    AvailabilityZone=az
)
```

**runctl translation:**
```bash
# Backup checkpoints to snapshot
runctl aws ebs snapshot vol-xxxxx \
    --description "Checkpoint backup"

# Restore on new instance
runctl aws create --spot \
    --ebs-snapshot snap-xxxxx \
    --mount-point /mnt/checkpoints
```

### Pattern 3: EBS-Optimized Instances

```python
# Use EBS-optimized instances for better I/O
instance = ec2.run_instances(
    InstanceType='c5.2xlarge',  # EBS-optimized by default
    EbsOptimized=True,  # Explicit for older instance types
    BlockDeviceMappings=[{
        'DeviceName': '/dev/sdf',
        'Ebs': {
            'VolumeId': volume_id,
            'DeleteOnTermination': False  # Keep volume after termination
        }
    }]
)
```

**runctl translation:**
```bash
# Launch EBS-optimized instance with persistent volume
runctl aws create --spot \
    --instance-type c5.2xlarge \
    --ebs-optimized \
    --ebs-volume vol-xxxxx \
    --ebs-persist  # Don't delete on termination
```

## Performance Comparison

| Approach | Initial Setup | Restart Time | Cost/Month |
|----------|--------------|--------------|------------|
| S3 Download | 0 min | 10-30 min | $0.023/GB |
| EBS Pre-warmed | 10-30 min | <1 min | $0.08/GB + $0.005/GB IOPS |
| EBS Snapshot | 5-10 min | 2-5 min | $0.05/GB snapshot + $0.08/GB volume |

**Break-even:** If you restart spot instances >3 times/month, EBS is cheaper.

## Advanced Optimizations

### 1. Multi-Attach EBS Volumes (io2)

```bash
# io2 volumes support multi-attach (read-only)
# Share dataset volume across multiple spot instances
runctl aws ebs create --type io2 \
    --multi-attach \
    --size 1000 \
    --name shared-datasets
```

### 2. EBS Throughput Optimization

```bash
# gp3 volumes: tune IOPS and throughput
runctl aws ebs create --type gp3 \
    --size 500 \
    --iops 3000 \
    --throughput 125  # MB/s
```

### 3. Placement Groups for Network Performance

```bash
# Cluster placement group for low latency
runctl aws create --spot \
    --placement-group cluster \
    --ebs-volume vol-xxxxx
```

### 4. Pre-configured AMIs

```bash
# Create AMI with pre-installed dependencies
# Faster instance startup
runctl aws ami create \
    --instance-id i-xxxxx \
    --name training-base \
    --description "Pre-configured training environment"

# Launch from AMI
runctl aws create --spot \
    --ami ami-xxxxx \
    --ebs-volume vol-xxxxx
```

## Implementation Plan

### Phase 1: Basic EBS Support

1. **Create EBS volumes**
   ```rust
   runctl aws ebs create --size 500 --type gp3
   ```

2. **Attach to instances**
   ```rust
   runctl aws create --ebs-volume vol-xxxxx
   ```

3. **Mount volumes**
   ```rust
   // Auto-mount in user-data script
   ```

### Phase 2: Pre-warming

4. **Pre-populate volumes**
   ```rust
   runctl aws ebs pre-warm vol-xxxxx s3://bucket/data/
   ```

5. **Snapshot management**
   ```rust
   runctl aws ebs snapshot vol-xxxxx
   runctl aws ebs restore snap-xxxxx
   ```

### Phase 3: Advanced Features

6. **Multi-attach support**
7. **Performance tuning**
8. **AMI management**

## Cost Optimization

### When to Use EBS

✅ **Use EBS when:**
- Training runs >1 hour
- Spot instance restarts >3 times/month
- Dataset >10 GB
- Checkpoint I/O is bottleneck

❌ **Don't use EBS when:**
- One-time short training (<30 min)
- Dataset <1 GB (S3 is fine)
- Very infrequent training

### Cost Example

**Scenario:** 500 GB dataset, 10 spot restarts/month

- **S3 Download:** 10 × 30 min × $0.10/hr = $0.50 compute + $0.01 storage = **$0.51/month**
- **EBS Volume:** $0.08/GB × 500 GB = **$40/month** + $0 compute = **$40/month**

**Break-even:** ~80 restarts/month (unlikely for spot)

**Better approach:** Use EBS for checkpoints (smaller), S3 for datasets (larger)

## Best Practices

1. **Separate volumes for data vs checkpoints**
   - Data volume: Large, read-only, shared
   - Checkpoint volume: Smaller, read-write, per-training

2. **Use snapshots for backup**
   - Daily snapshots of checkpoint volumes
   - Lifecycle policy to delete old snapshots

3. **Optimize volume types**
   - gp3 for general use (cost-effective)
   - io2 for high IOPS (multi-attach)
   - st1 for large sequential reads (datasets)

4. **Monitor volume performance**
   - Track IOPS utilization
   - Adjust volume size/type based on usage

5. **Cleanup unused volumes**
   - Tag volumes for easy identification
   - Auto-cleanup after training completes

## runctl Commands (Proposed)

```bash
# EBS Volume Management
runctl aws ebs create --size 500 --type gp3 --name datasets
runctl aws ebs list
runctl aws ebs attach vol-xxxxx --instance i-xxxxx --device /dev/sdf
runctl aws ebs detach vol-xxxxx
runctl aws ebs delete vol-xxxxx

# Pre-warming
runctl aws ebs pre-warm vol-xxxxx s3://bucket/data/ --mount /mnt/data

# Snapshots
runctl aws ebs snapshot vol-xxxxx --description "Checkpoint backup"
runctl aws ebs snapshot list
runctl aws ebs restore snap-xxxxx --size 100

# Integration with instance creation
runctl aws create --spot \
    --ebs-volume vol-xxxxx \
    --ebs-mount /mnt/data \
    --ebs-persist
```

## Other Optimizations

### 1. Instance Store (Ephemeral) for Temporary Data

```bash
# Use instance store for temporary files (faster, free)
# But backup to EBS/S3 before termination
runctl aws create --spot \
    --instance-store /dev/sdb  # Use for temp files
    --ebs-volume vol-xxxxx     # Use for persistent data
```

### 2. S3 Transfer Acceleration

```bash
# For cross-region transfers
runctl s3 upload --use-acceleration \
    ./data/ s3://bucket/data/
```

### 3. Parallel S3 Downloads

```bash
# Use s5cmd for parallel downloads
runctl s3 download --parallel 10 \
    s3://bucket/data/ ./data/
```

### 4. Data Locality

```bash
# Launch instances in same AZ as S3 bucket
runctl aws create --spot \
    --availability-zone us-east-1a \
    --s3-bucket-region us-east-1
```

### 5. Compression

```bash
# Compress datasets before upload
runctl s3 upload --compress \
    ./data/ s3://bucket/data/
```

## Summary

**EBS volumes are valuable for:**
- Persistent checkpoints (survive spot interruptions)
- Pre-warmed datasets (faster startup)
- High IOPS requirements (training I/O)

**Best approach:**
- EBS for checkpoints (small, frequent I/O)
- S3 for datasets (large, infrequent access)
- Pre-warm EBS with frequently-used datasets
- Use snapshots for checkpoint backup

