# EBS Volume Implementation

## Overview

EBS (Elastic Block Store) volume support has been added to `trainctl` to enable persistent storage for training workloads, especially valuable for spot instances.

## Commands

EBS commands are available under `trainctl aws ebs`:

```bash
# Create EBS volume
trainctl aws ebs create --size 500 --type gp3 --name datasets

# List volumes
trainctl aws ebs list
trainctl aws ebs list --detailed

# Attach volume to instance
trainctl aws ebs attach vol-xxxxx --instance-id i-xxxxx --device /dev/sdf

# Detach volume
trainctl aws ebs detach vol-xxxxx

# Delete volume
trainctl aws ebs delete vol-xxxxx --force

# Create snapshot
trainctl aws ebs snapshot vol-xxxxx --description "Checkpoint backup" --name backup-2025-01-15

# List snapshots
trainctl aws ebs snapshot-list
trainctl aws ebs snapshot-list --volume-id vol-xxxxx --detailed

# Restore volume from snapshot
trainctl aws ebs restore snap-xxxxx --size 100 --type gp3 --name restored-volume

# Pre-warm volume (populate from S3)
trainctl aws ebs pre-warm vol-xxxxx s3://bucket/data/ --mount-point /mnt/data
```

## Features

### ✅ Implemented
- Create volumes with configurable size, type, IOPS, throughput
- List volumes with filtering
- Attach/detach volumes to instances
- Delete volumes (with safety checks)
- Create snapshots
- List snapshots
- Restore volumes from snapshots
- Volume tagging (Name, CreatedBy)
- Encryption support

### 🚧 Partially Implemented
- **Pre-warming**: Structure exists, needs full implementation (requires temporary instance)

## Volume Types Supported

- **gp3**: General purpose SSD (default, cost-effective)
- **gp2**: General purpose SSD (legacy)
- **io2**: Provisioned IOPS SSD (high performance, multi-attach)
- **st1**: Throughput optimized HDD (large datasets)
- **sc1**: Cold HDD (archival)

## Use Cases

### 1. Pre-warmed Datasets
```bash
# Create volume and pre-warm with dataset
trainctl aws ebs create --size 500 --type gp3 --name datasets \
    --pre-warm s3://bucket/datasets/

# Launch spot instance with pre-warmed volume
trainctl aws create --spot --ebs-volume vol-xxxxx
```

### 2. Checkpoint Persistence
```bash
# Create checkpoint volume
trainctl aws ebs create --size 100 --type gp3 --name checkpoints

# Attach to training instance
trainctl aws ebs attach vol-xxxxx --instance-id i-xxxxx --device /dev/sdf

# Create snapshot backup
trainctl aws ebs snapshot vol-xxxxx --description "Daily checkpoint backup"
```

### 3. Spot Instance Resilience
```bash
# Pre-warm dataset volume
trainctl aws ebs create --size 500 --type gp3 --name datasets \
    --pre-warm s3://bucket/data/

# Launch spot instance with persistent volume
trainctl aws create --spot \
    --ebs-volume vol-xxxxx \
    --ebs-mount /mnt/data

# If spot interrupted, launch new instance with same volume
# Data persists, no re-download needed
```

## Integration with Instance Creation

EBS volumes can be integrated into instance creation (future enhancement):

```bash
trainctl aws create --spot \
    --ebs-volume vol-xxxxx \
    --ebs-mount /mnt/data \
    --ebs-persist  # Don't delete on termination
```

This would:
1. Create instance with EBS volume attached
2. Auto-mount volume in user-data script
3. Configure volume to persist after termination

## Cost Considerations

- **gp3**: ~$0.08/GB/month + $0.005/GB IOPS
- **Snapshot**: ~$0.05/GB/month
- **Break-even**: If restarting spot instances >3 times/month, EBS is cost-effective

**Best Practice**: Use EBS for checkpoints (small, frequent I/O), S3 for datasets (large, infrequent)

## Implementation Details

### Module Structure
- `src/ebs.rs`: EBS command handlers
- Integrated into `src/aws.rs` as subcommand
- Uses AWS SDK for EC2 operations

### Error Handling
- Volume attachment checks (already attached, wrong state)
- Deletion safety (checks if attached, requires `--force`)
- Snapshot validation (volume exists, accessible)

### Tagging
- Automatic `CreatedBy: trainctl` tag
- Optional `Name` tag for easy identification
- Snapshot tags for organization

## Next Steps

1. **Complete pre-warming**: Implement temporary instance creation, S3 sync, cleanup
2. **Instance integration**: Add `--ebs-volume` flag to `trainctl aws create`
3. **Auto-mount**: Generate user-data script to auto-mount volumes
4. **Volume monitoring**: Add volume status to `trainctl resources list`
5. **Cost tracking**: Include EBS costs in resource summaries

## Testing

EBS operations require AWS credentials. Test with:

```bash
# Create test volume
trainctl aws ebs create --size 10 --type gp3 --name test-volume

# Verify creation
trainctl aws ebs list --name test-volume

# Cleanup
trainctl aws ebs delete vol-xxxxx --force
```

## Documentation

- [EBS_OPTIMIZATION.md](EBS_OPTIMIZATION.md) - Comprehensive EBS strategies
- [EBS_OPTIMIZATION_SUMMARY.md](EBS_OPTIMIZATION_SUMMARY.md) - Quick reference

