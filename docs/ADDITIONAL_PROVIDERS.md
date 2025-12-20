# Additional Provider Recommendations

Based on research and analysis, here are the most valuable additional providers to add to `runctl`:

## High Priority Additions

### 1. **Vast.ai** ⭐ Top Recommendation
- **Why**: Best value pricing, simple API, very popular
- **Integration**: REST API (similar to RunPod)
- **Complexity**: Low
- **Use Case**: Cost-conscious training, experiments, fine-tuning
- **Pricing**: $1.49-2.99/hr for H100 (best value)
- **API**: Python client available, REST API well-documented

### 2. **Lambda Labs**
- **Why**: Popular with researchers, good API, enterprise support
- **Integration**: REST API
- **Complexity**: Low-Medium
- **Use Case**: Research, startups, enterprise training
- **Pricing**: ~$3.00/hr for H100 (mid-range)
- **API**: Python SDK, REST API

### 3. **Paperspace**
- **Why**: Startup-friendly, simple integration, Gradient platform
- **Integration**: REST API
- **Complexity**: Low
- **Use Case**: Quick starts, startups, experiments
- **Pricing**: ~$2.50/hr (budget-friendly)
- **API**: Python SDK, REST API

## Medium Priority

### 4. **Modal**
- **Why**: Unique serverless model, growing popularity
- **Integration**: Python decorators (may need adapter)
- **Complexity**: Medium (different paradigm)
- **Use Case**: Serverless/event-driven ML, batch jobs
- **Pricing**: Per-request (serverless)
- **API**: Python SDK with decorators, REST API available

### 5. **Google Cloud Platform (GCP)**
- **Why**: Enterprise demand, TPU support (unique)
- **Integration**: `gcloud` CLI, REST APIs
- **Complexity**: Medium
- **Use Case**: Enterprise, multi-cloud, TPU workloads
- **Pricing**: $3-4/hr (on-demand)
- **API**: `gcloud` CLI, GCP SDK

### 6. **Azure**
- **Why**: Enterprise demand, Microsoft ecosystem
- **Integration**: Azure CLI, REST APIs
- **Complexity**: Medium
- **Use Case**: Enterprise, Microsoft shops
- **Pricing**: $3-4/hr (on-demand)
- **API**: Azure CLI, Azure SDK

## Lower Priority (Specialized)

### 7. **CoreWeave**
- **Why**: Premium GPU cloud, InfiniBand networking
- **Integration**: Kubernetes-based
- **Complexity**: High (requires K8s expertise)
- **Use Case**: Large-scale distributed training
- **Pricing**: $2.20-3.00/hr (value tier)
- **API**: Kubernetes (`kubectl`)

### 8. **Kubernetes (Generic)**
- **Why**: Maximum flexibility, multi-cloud portability
- **Integration**: Kubernetes API
- **Complexity**: High
- **Use Case**: Organizations with existing K8s infrastructure
- **Pricing**: Varies (uses underlying cloud)
- **API**: `kubectl`, Kubernetes API

### 9. **Slurm (HPC)**
- **Why**: Academic/research institutions
- **Integration**: Slurm REST API, `sbatch`/`squeue`
- **Complexity**: Medium
- **Use Case**: HPC clusters, academic research
- **Pricing**: Varies (institutional)
- **API**: Slurm REST API, CLI tools

## Implementation Strategy

### Phase 1: Quick Wins (1-2 weeks each)
1. **Vast.ai** - Simple REST API, high value
2. **Lambda Labs** - Good API, popular
3. **Paperspace** - Simple integration

### Phase 2: Enterprise (2-3 weeks each)
4. **GCP** - Enterprise demand, TPU support
5. **Azure** - Enterprise demand

### Phase 3: Advanced (3-4 weeks each)
6. **Modal** - Different paradigm, adapter needed
7. **Kubernetes** - High complexity, maximum flexibility

## Provider Trait Compatibility

All providers can implement `TrainingProvider` trait:

```rust
// Vast.ai example
pub struct VastProvider {
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl TrainingProvider for VastProvider {
    fn name(&self) -> &'static str { "vast" }
    
    async fn create_resource(&self, instance_type: &str, options: CreateResourceOptions) -> Result<ResourceId> {
        // POST to Vast.ai API
        let response = self.client
            .post("https://vast.ai/api/v0/asks/")
            .json(&json!({
                "client_id": "me",
                "type": "on-demand",
                "gpu_name": instance_type,
                // ...
            }))
            .send()
            .await?;
        // Extract instance ID
    }
    
    // ... implement other methods
}
```

## Cost Comparison Summary

| Provider | H100 Price/hr | Best For |
|----------|--------------|----------|
| Vast.ai | $1.49-2.99 | Best value |
| CoreWeave | $2.20-3.00 | Large-scale |
| Paperspace | ~$2.50 | Startups |
| Lambda Labs | ~$3.00 | Research |
| RunPod | ~$2.99 | General |
| AWS/GCP/Azure | $3-4 | Enterprise |

## Recommendation

**Start with Vast.ai** - it offers:
- Best value pricing
- Simple REST API
- High demand from users
- Low integration complexity
- Good documentation

This would give `runctl` coverage of the major budget-friendly GPU providers alongside AWS and RunPod.

