# EBS Volume Optimization Guide

## Overview

runctl now includes automatic EBS volume optimization based on use case, volume size, and performance requirements. This ensures optimal IOPS and throughput settings for ML training workloads.

## Use Cases

### Data Loading (`--use-case data-loading`)

Optimized for high-throughput sequential reads of large datasets.

**Configuration:**
- **IOPS**: 500 IOPS per GB (up to 80,000 max)
- **Throughput**: 0.25 MiB/s per IOPS (up to 2,000 MiB/s max)
- **Best for**: Loading datasets from EBS volumes to training instances
- **Example**: 500 GB volume → 253,000 IOPS (capped at 80,000) → 2,000 MiB/s throughput

```bash
runctl ebs create --size 500 --use-case data-loading --availability-zone us-east-1a
```

### Checkpoints (`--use-case checkpoints`)

Optimized for high IOPS for frequent small writes.

**Configuration:**
- **IOPS**: 300 IOPS per GB (up to 80,000 max)
- **Throughput**: 0.25 MiB/s per IOPS (up to 2,000 MiB/s max)
- **Best for**: Frequent checkpoint saves during training
- **Example**: 100 GB volume → 33,000 IOPS → 1,000 MiB/s throughput

```bash
runctl ebs create --size 100 --use-case checkpoints --availability-zone us-east-1a
```

### General Purpose (`--use-case general`)

Balanced performance for mixed workloads.

**Configuration:**
- **IOPS**: 100 IOPS per GB (up to 80,000 max)
- **Throughput**: 0.25 MiB/s per IOPS (up to 2,000 MiB/s max)
- **Best for**: General training workloads with mixed I/O patterns
- **Example**: 200 GB volume → 23,000 IOPS → 750 MiB/s throughput

```bash
runctl ebs create --size 200 --use-case general --availability-zone us-east-1a
```

### Archive (`--use-case archive`)

Cost-optimized for infrequent access.

**Configuration:**
- **IOPS**: Baseline only (3,000 IOPS)
- **Throughput**: Baseline only (125 MiB/s)
- **Best for**: Long-term storage, infrequent access
- **Cost**: Lowest cost option

```bash
runctl ebs create --size 1000 --use-case archive --availability-zone us-east-1a
```

## Volume Type Selection

### gp3 (Recommended, Default)

**Baseline Performance:**
- 3,000 IOPS (included)
- 125 MiB/s throughput (included)

**Maximum Performance:**
- 80,000 IOPS (requires 160+ GB volume)
- 2,000 MiB/s throughput (requires 8,000+ IOPS)

**Cost:** ~$0.08/GB-month

**Best for:** Most workloads, cost-effective, independent performance scaling

### gp2 (Legacy)

**Performance:**
- 3 IOPS per GB (max 16,000 IOPS)
- 250 MiB/s throughput (fixed)

**Cost:** ~$0.10/GB-month (20% more than gp3)

**Best for:** Legacy compatibility only

### io2 (High Performance)

**Performance:**
- Up to 64,000 IOPS (256,000 with Block Express)
- Multi-attach support

**Cost:** ~$0.125/GB-month + IOPS charges

**Best for:** High IOPS requirements, shared datasets (multi-attach)

### st1 (Throughput Optimized HDD)

**Performance:**
- 500 MiB/s throughput (fixed)
- Lower IOPS than SSD

**Cost:** ~$0.045/GB-month

**Best for:** Large sequential reads, cost-sensitive data loading
**Minimum size:** 125 GB

### sc1 (Cold HDD)

**Performance:**
- 250 MiB/s throughput (fixed)
- Lowest IOPS

**Cost:** ~$0.015/GB-month

**Best for:** Archival storage, infrequent access
**Minimum size:** 125 GB

## Auto-Optimization Examples

### Example 1: Data Loading Volume

```bash
# Create 500 GB volume optimized for data loading
runctl ebs create \
  --size 500 \
  --use-case data-loading \
  --availability-zone us-east-1a \
  --name training-datasets

# Output:
#    Data loading optimized: 80000 IOPS, 2000 MiB/s throughput for 500 GB volume.
#    Provides high throughput for reading large datasets.
```

### Example 2: Checkpoint Volume

```bash
# Create 100 GB volume optimized for checkpoints
runctl ebs create \
  --size 100 \
  --use-case checkpoints \
  --volume-type gp3 \
  --availability-zone us-east-1a \
  --name training-checkpoints

# Output:
#    Checkpoint optimized: 33000 IOPS, 1000 MiB/s throughput for 100 GB volume.
#    Provides high IOPS for frequent small writes.
```

### Example 3: Manual Configuration

```bash
# Override auto-optimization with manual settings
runctl ebs create \
  --size 500 \
  --volume-type gp3 \
  --iops 16000 \
  --throughput 1000 \
  --availability-zone us-east-1a
```

## EBS-Optimized Instances

All instances created by runctl are automatically configured with EBS optimization enabled. This provides:

- **Dedicated bandwidth** for EBS volumes
- **Consistent performance** without contention
- **Better I/O performance** for data loading and checkpoint operations

**Note:** Most modern instance types (c5, m5, g4dn, p3, etc.) are EBS-optimized by default, but enabling it explicitly ensures optimal performance.

## Performance Comparison

| Volume Type | Size | Use Case | IOPS | Throughput | Cost/Month |
|-------------|------|----------|------|------------|------------|
| gp3 | 500 GB | Data Loading | 80,000 | 2,000 MiB/s | $40 |
| gp3 | 100 GB | Checkpoints | 33,000 | 1,000 MiB/s | $8 |
| gp3 | 200 GB | General | 23,000 | 750 MiB/s | $16 |
| gp3 | 1000 GB | Archive | 3,000 | 125 MiB/s | $80 |
| st1 | 500 GB | Data Loading | ~500 | 500 MiB/s | $22.50 |
| io2 | 100 GB | Checkpoints | 50,000 | ~1,250 MiB/s | $12.50 + IOPS |

## Recommendations

### For Data Loading
- **< 1 TB**: Use `gp3` with `--use-case data-loading`
- **> 1 TB**: Consider `st1` for cost efficiency (sequential reads only)

### For Checkpoints
- **< 100 GB**: Use `gp3` with `--use-case checkpoints`
- **> 100 GB**: Consider `io2` if multi-attach is needed

### For General Purpose
- Always use `gp3` with `--use-case general` (default)

### For Archive
- Use `sc1` for large archives (> 500 GB)
- Use `gp3` with `--use-case archive` for smaller archives that may need random access

## Cost Optimization Tips

1. **Right-size volumes**: Don't over-provision storage
2. **Use appropriate volume types**: st1/sc1 for large sequential workloads
3. **Optimize IOPS/throughput**: Use `--use-case` to avoid over-provisioning
4. **Separate data and checkpoints**: Use different volumes with different optimizations
5. **Use snapshots**: Create snapshots of pre-warmed volumes for faster restoration

## Technical Details

### gp3 Performance Formula

- **IOPS**: `min(3000 + size_gb * iops_per_gb, 80000)`
- **Throughput**: `min(iops * 0.25, 2000)` MiB/s
- **Minimum for max IOPS**: 160 GB
- **Minimum for max throughput**: 8,000 IOPS (32 GB volume)

### Instance Bandwidth Limits

Even with optimized EBS volumes, instance bandwidth limits apply:
- **t3.medium**: ~347 Mbps
- **c5.xlarge**: ~4,750 Mbps
- **g4dn.xlarge**: ~4,750 Mbps

Match instance type to volume performance to avoid unused capacity.

## See Also

- [EBS_OPTIMIZATION.md](EBS_OPTIMIZATION.md) - General EBS optimization strategies
- [OPTIMIZATION_OPPORTUNITIES.md](OPTIMIZATION_OPPORTUNITIES.md) - Other optimization opportunities

