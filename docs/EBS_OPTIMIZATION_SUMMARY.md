# EBS Optimization Summary

## Key Insight: Pre-warmed EBS Volumes for Spot Instances

**Problem:** Spot instances can be interrupted, requiring re-download of datasets (10-30 min).

**Solution:** Pre-warm EBS volumes with datasets, attach to spot instances.

**Benefits:**
- 10-100x faster than S3 downloads
- Data survives spot interruptions
- Faster instance startup (<1 min vs 10-30 min)

## Quick Implementation

```bash
# 1. Create and pre-warm EBS volume (one-time)
trainctl aws ebs create --size 500 --type gp3 --name datasets
trainctl aws ebs pre-warm vol-xxxxx s3://bucket/datasets/

# 2. Launch spot instance with pre-warmed volume
trainctl aws create --spot \
    --ebs-volume vol-xxxxx \
    --mount-point /mnt/data

# 3. Training starts immediately (no download needed)
trainctl aws train $INSTANCE_ID train.py
```

## Cost Analysis

**Scenario:** 500 GB dataset, 10 spot restarts/month

- **S3 Download:** 10 × 30 min × $0.10/hr = $0.50 compute + $0.01 storage = **$0.51/month**
- **EBS Volume:** $0.08/GB × 500 GB = **$40/month** + $0 compute = **$40/month**

**Break-even:** ~80 restarts/month (unlikely)

**Better approach:** 
- EBS for checkpoints (small, frequent I/O) - **$4/month for 50 GB**
- S3 for datasets (large, infrequent) - **$0.01/month for 500 GB**

## Other Optimizations

See [OPTIMIZATION_OPPORTUNITIES.md](OPTIMIZATION_OPPORTUNITIES.md) for:
- Network optimizations (placement groups, enhanced networking)
- Data transfer optimizations (parallel downloads, compression)
- Instance selection (right-sizing, spot diversification)
- Checkpoint optimizations (compression, deduplication)
- Training loop optimizations (mixed precision, gradient accumulation)

## Implementation Priority

1. **EBS volume support** (create, attach, mount)
2. **Pre-warming** (populate from S3)
3. **Snapshot management** (backup checkpoints)
4. **Multi-attach support** (share datasets)

