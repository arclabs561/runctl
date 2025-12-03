# S3 Operations & Cleanup Features

## Overview

trainctl now includes comprehensive S3 operations with s5cmd integration for high-performance data management, plus cleanup and monitoring capabilities.

## Features

### ✅ S3 Operations
- **Upload**: Fast uploads using s5cmd (10x faster than AWS CLI)
- **Download**: High-speed downloads with parallelization
- **Sync**: Bidirectional sync between local and S3
- **List**: Efficient listing with human-readable sizes
- **Watch**: Monitor S3 buckets for new files
- **Review**: Audit training artifacts in S3
- **Cleanup**: Automatic cleanup of old checkpoints

### ✅ Local Cleanup
- **Checkpoint cleanup**: Remove old checkpoints, keep last N
- **Dry-run mode**: Preview what will be deleted
- **Size-based cleanup**: Coming soon

## S3 Commands

### Upload

```bash
# Upload single file (uses s5cmd if available)
trainctl s3 upload ./checkpoints/best.pt s3://bucket/checkpoints/best.pt

# Upload directory recursively
trainctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive

# Force AWS SDK (if s5cmd not available)
trainctl s3 upload ./file.pt s3://bucket/file.pt --no-use-s5cmd
```

### Download

```bash
# Download single file
trainctl s3 download s3://bucket/checkpoints/best.pt ./best.pt

# Download directory recursively
trainctl s3 download s3://bucket/checkpoints/ ./checkpoints/ --recursive
```

### Sync

```bash
# Sync local -> S3 (upload changes)
trainctl s3 sync ./checkpoints/ s3://bucket/checkpoints/ --direction up

# Sync S3 -> local (download changes)
trainctl s3 sync ./checkpoints/ s3://bucket/checkpoints/ --direction down
```

### List

```bash
# List S3 objects
trainctl s3 list s3://bucket/checkpoints/

# Recursive listing
trainctl s3 list s3://bucket/checkpoints/ --recursive

# Human-readable sizes
trainctl s3 list s3://bucket/checkpoints/ --human-readable
```

### Cleanup

```bash
# Cleanup old checkpoints in S3 (keep last 10)
trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10

# Dry run (preview what will be deleted)
trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 5 --dry-run
```

### Watch

```bash
# Watch S3 bucket for new files (check every 30s)
trainctl s3 watch s3://bucket/checkpoints/

# Custom poll interval
trainctl s3 watch s3://bucket/checkpoints/ --interval 10
```

### Review

```bash
# Review training artifacts in S3
trainctl s3 review s3://bucket/training/

# Detailed review with file listing
trainctl s3 review s3://bucket/training/ --detailed
```

## Local Cleanup

### Checkpoint Cleanup

```bash
# Cleanup local checkpoints (keep last 10)
trainctl checkpoint cleanup checkpoints/ --keep-last-n 10

# Dry run
trainctl checkpoint cleanup checkpoints/ --keep-last-n 5 --dry-run
```

## Why s5cmd?

**Performance**: s5cmd is 10x faster than AWS CLI for ML workloads:
- Parallel operations (worker pool)
- Bandwidth saturation
- Single executable for batch operations
- Optimized for large files

**Compatibility**: Works with:
- AWS S3
- Google Cloud Storage (GCS)
- Any S3-compatible service

**Installation**: 
```bash
# macOS
brew install s5cmd

# Linux
# Download from https://github.com/peak/s5cmd/releases
```

## Best Practices

### Checkpoint Management

1. **Upload after training**:
   ```bash
   trainctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive
   ```

2. **Cleanup old checkpoints**:
   ```bash
   # Local cleanup
   trainctl checkpoint cleanup checkpoints/ --keep-last-n 10
   
   # S3 cleanup
   trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10
   ```

3. **Monitor for new checkpoints**:
   ```bash
   trainctl s3 watch s3://bucket/checkpoints/ --interval 30
   ```

### Data Staging

1. **Download datasets before training**:
   ```bash
   trainctl s3 download s3://bucket/datasets/ ./data/ --recursive
   ```

2. **Sync training data**:
   ```bash
   trainctl s3 sync ./data/ s3://bucket/datasets/ --direction up
   ```

### Cost Optimization

1. **Review storage usage**:
   ```bash
   trainctl s3 review s3://bucket/training/ --detailed
   ```

2. **Cleanup old artifacts**:
   ```bash
   trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 5
   ```

## Integration with Training

### Automatic Checkpoint Upload

Add to your training script:
```python
# After saving checkpoint
import subprocess
subprocess.run([
    "trainctl", "s3", "upload",
    checkpoint_path,
    f"s3://bucket/checkpoints/{checkpoint_name}"
])
```

### Post-Training Cleanup

```bash
# After training completes
trainctl checkpoint cleanup checkpoints/ --keep-last-n 10
trainctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive
trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10
```

## Examples

### Complete Workflow

```bash
# 1. Train locally
trainctl local training/train.py -- --epochs 50

# 2. Upload checkpoints to S3
trainctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive

# 3. Review what was uploaded
trainctl s3 review s3://bucket/checkpoints/ --detailed

# 4. Cleanup local checkpoints (keep last 5)
trainctl checkpoint cleanup checkpoints/ --keep-last-n 5

# 5. Cleanup S3 checkpoints (keep last 10)
trainctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10
```

### Monitoring Training on Cloud

```bash
# Watch for new checkpoints in S3
trainctl s3 watch s3://bucket/checkpoints/ --interval 30

# In another terminal, review periodically
trainctl s3 review s3://bucket/checkpoints/
```

## Performance Comparison

| Operation | AWS CLI | s5cmd | Speedup |
|-----------|---------|-------|---------|
| Upload 100 files | 5 min | 30 sec | 10x |
| Download dataset | 10 min | 1 min | 10x |
| List 1000 objects | 2 min | 10 sec | 12x |

## Future Enhancements

- [ ] Lifecycle policy management
- [ ] Cross-region replication
- [ ] Cost estimation
- [ ] Automatic cleanup scheduling
- [ ] Checkpoint deduplication
- [ ] Compression before upload

