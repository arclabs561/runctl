# Resource Recommendations for ML Training (2024-2025)

This document provides modern recommendations for CPU, GPU, storage, and memory configurations for ML training workloads.

## Instance Type Recommendations

### GPU Instances (Primary for ML Training)

**Latest Generation (2025)**
- **Trn2 instances**: AWS Trainium2 chips
  - 4x faster than Trn1, 4x more memory bandwidth, 3x more memory capacity
  - 30-40% better price-performance than P5e/P5en
  - Best for: Large-scale ML training workloads
  - Available in: US East (Ohio) region
  - Cost: Significantly lower than equivalent GPU instances

**Established GPU Options**
- **P5 instances**: NVIDIA H100 GPUs
  - Best for: Large model training, high-performance computing
  - Cost: ~$32-98/hour depending on size
  - Use when: Trn2 not available or framework compatibility required

- **G5 instances**: NVIDIA A10G GPUs
  - Best for: General ML training, inference, graphics workloads
  - Cost: ~$1-4/hour depending on size
  - Use when: Cost-effective GPU training needed

- **G4dn instances**: NVIDIA T4 GPUs
  - Best for: Entry-level GPU training, inference
  - Cost: ~$0.5-2/hour depending on size
  - Use when: Budget-conscious GPU workloads

- **P3 instances**: NVIDIA V100 GPUs (legacy)
  - Best for: Legacy workloads, proven performance
  - Cost: ~$3-25/hour depending on size
  - Consider: Upgrading to G5 or P5 for better price-performance

### CPU Instances (Supporting Tasks)

**General Purpose**
- **M7i/M6i instances**: Latest generation Intel processors
  - Best for: Data preprocessing, postprocessing, evaluation
  - Cost: ~$0.1-0.8/hour depending on size
  - Use when: GPU not needed for specific pipeline stages

- **M5 instances**: Previous generation
  - Best for: Cost-effective general compute
  - Cost: ~$0.1-0.8/hour depending on size
  - Use when: M7i/M6i not available in region

- **T3 instances**: Burstable performance
  - Best for: Development, testing, low-intensity tasks
  - Cost: ~$0.01-0.17/hour depending on size
  - Use when: Intermittent workloads, cost optimization

**Compute Optimized**
- **C5/C6i instances**: High CPU performance
  - Best for: Scientific modeling, batch processing, distributed analytics
  - Cost: ~$0.1-0.8/hour depending on size
  - Use when: CPU-intensive preprocessing or evaluation

### Memory Requirements

**For GPU Training**
- Minimum: 16 GB RAM per GPU (for small models)
- Recommended: 32-64 GB RAM per GPU (for medium models)
- Large models: 128+ GB RAM (may require multiple GPUs or larger instances)

**For CPU Training**
- Small models: 8-16 GB RAM
- Medium models: 32-64 GB RAM
- Large models: 128+ GB RAM (consider memory-optimized instances)

**Memory-Optimized Instances**
- **R7i/R6i instances**: Latest generation, high memory
- **R5 instances**: Previous generation
- Use when: Large datasets in memory, memory-intensive preprocessing

## EBS Volume Recommendations

### Volume Types

**gp3 (General Purpose SSD) - Recommended Default**
- Baseline: 3,000 IOPS, 125 MiB/s (included)
- Maximum: 80,000 IOPS, 2,000 MiB/s
- Best for: Most workloads, cost-effective
- Cost: ~$0.08/GB-month
- **Recommendation**: Use for general-purpose storage, checkpoints, code

**io2 (Provisioned IOPS SSD)**
- Maximum: 64,000 IOPS (256,000 with Block Express)
- Multi-attach support
- Best for: High IOPS requirements, shared datasets
- Cost: ~$0.125/GB-month + IOPS charges
- **Recommendation**: Use when gp3 IOPS limits are insufficient

**st1 (Throughput Optimized HDD)**
- Throughput: 500 MiB/s
- Best for: Large sequential reads (data loading)
- Cost: ~$0.045/GB-month (cheaper than SSD)
- Minimum size: 125 GB
- **Recommendation**: Use for large dataset storage (>1TB), sequential access patterns

**sc1 (Cold HDD)**
- Throughput: 250 MiB/s
- Best for: Archival storage, infrequent access
- Cost: ~$0.015/GB-month (lowest cost)
- Minimum size: 125 GB
- **Recommendation**: Use for long-term checkpoint archives

### Use Case Recommendations

**Data Loading**
- <1TB: gp3 with optimized IOPS/throughput (500 IOPS/GB, up to max)
- >1TB: st1 for cost efficiency (sequential reads)
- Random access needed: gp3

**Checkpoint Storage**
- Small checkpoints (<100GB): gp3 with high IOPS (300 IOPS/GB)
- Large checkpoints (>100GB): io2 if multi-attach needed, otherwise gp3
- Archive checkpoints: sc1 or gp3 (if random access needed)

**General Purpose**
- Default: gp3 (balanced performance, cost-effective)
- High IOPS needed: io2
- Cost optimization: st1 for large sequential data

## Cost Optimization Strategies

1. **Use Trn2 instances** when available for 30-40% better price-performance
2. **Reserve instances** for predictable workloads (up to 72% savings)
3. **Use spot instances** for fault-tolerant workloads (up to 90% savings)
4. **Right-size instances** - test with expected data volumes
5. **Optimize EBS volumes** - use appropriate type and size for workload
6. **Stage-specific instances** - use cheaper instances for preprocessing/evaluation
7. **Auto-scaling** - scale down when not training

## Implementation in runctl

The following functions provide these recommendations:

- `utils::get_instance_cost()`: Returns hourly costs for instance types
- `ebs_optimization::optimize_volume_config()`: Auto-optimizes EBS IOPS/throughput
- `ebs_optimization::recommend_volume_type()`: Recommends volume type by use case

## References

- AWS EC2 Instance Types: https://aws.amazon.com/ec2/instance-types/
- AWS EBS Volume Types: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ebs-volume-types.html
- Trn2 Instances: https://aws.amazon.com/blogs/aws/amazon-ec2-trn2-instances-and-trn2-ultraservers-for-aiml-training-and-inference-is-now-available/

