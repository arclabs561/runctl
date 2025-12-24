# Cloud Provider Comparison for ML Training

## Overview

This document compares cloud providers and platforms that would be valuable to support in `runctl` for ML training workloads.

## GPU Cloud Providers

### 1. **Lambda Labs**
- **Best For**: Researchers, startups, enterprise training
- **GPUs**: A100, H100, RTX 6000
- **Pricing**: Mid-range, transparent hourly
- **Features**: 
  - Bare-metal and virtual GPU servers
  - PyTorch/TensorFlow integration
  - Multi-GPU machines
  - Hands-on support
- **API/CLI**: Python SDK, REST API, SSH access
- **Integration Complexity**: Medium (REST API)

### 2. **Vast.ai**
- **Best For**: Cost-conscious experiments, fine-tuning
- **GPUs**: Mixed ecosystem (various GPUs from providers)
- **Pricing**: Budget-friendly, best value
- **Features**:
  - Self-serve infrastructure
  - Hourly billing
  - Real-time GPU availability
- **API/CLI**: Python API client, CLI tool
- **Integration Complexity**: Low (simple API)

### 3. **CoreWeave**
- **Best For**: Large-scale distributed training
- **GPUs**: A100, H100, MI300X
- **Pricing**: Value tier ($2.20-3.00/hr for H100)
- **Features**:
  - InfiniBand networking
  - Kubernetes integration
  - Bare-metal and containerized
  - White-glove enterprise support
- **API/CLI**: Kubernetes (`kubectl`), container-based
- **Integration Complexity**: High (Kubernetes expertise needed)

### 4. **Paperspace**
- **Best For**: Startups, quick starts
- **GPUs**: A100, H100, T4
- **Pricing**: Budget-friendly
- **Features**:
  - Quick deployment
  - Git-based workflows
  - Gradient ML platform integration
- **API/CLI**: Python SDK, web dashboard
- **Integration Complexity**: Low (REST API)

### 5. **Modal**
- **Best For**: Serverless/event-driven ML, batch jobs
- **GPUs**: A100, H100, L40S
- **Pricing**: Per-request billing (serverless)
- **Features**:
  - Serverless compute
  - Python decorator-based API
  - No infrastructure management
  - Automatic orchestration
- **API/CLI**: Python decorators (`@modal.gpu.cuda_fn()`)
- **Integration Complexity**: Low (unique decorator pattern)

### 6. **RunPod** (Already Supported)
- **Best For**: General ML training, fine-tuning
- **GPUs**: RTX 4080, RTX 4090, A100, H100
- **Pricing**: Competitive
- **Features**:
  - `runpodctl` CLI
  - Pod-based model
  - Pre-configured images
- **API/CLI**: `runpodctl` CLI, REST API
- **Integration Complexity**: Low (CLI-based)

### 7. **Lyceum AI** (Already Planned)
- **Best For**: AI/ML workloads
- **Status**: Provider skeleton exists, needs implementation
- **Integration Complexity**: TBD

## Other Platforms

### 8. **Google Cloud Platform (GCP)**
- **Best For**: Enterprise, multi-cloud strategies
- **Services**: Compute Engine, Vertex AI, TPUs
- **Features**:
  - TPU support (unique advantage)
  - Vertex AI for managed ML
  - Integration with GCP ecosystem
- **API/CLI**: `gcloud` CLI, REST APIs
- **Integration Complexity**: Medium (GCP SDK)

### 9. **Azure**
- **Best For**: Enterprise, Microsoft ecosystem
- **Services**: Virtual Machines, Azure ML
- **Features**:
  - Azure ML integration
  - Enterprise support
  - Hybrid cloud options
- **API/CLI**: Azure CLI, REST APIs
- **Integration Complexity**: Medium (Azure SDK)

### 10. **Kubernetes (Generic)**
- **Best For**: Organizations with existing K8s infrastructure
- **Features**:
  - Kubeflow for ML workflows
  - Ray for distributed training
  - GPU operators (NVIDIA, AMD)
  - Multi-cloud portability
- **API/CLI**: `kubectl`, Kubernetes API
- **Integration Complexity**: High (Kubernetes expertise)

### 11. **Slurm (HPC)**
- **Best For**: Academic/research institutions, HPC clusters
- **Features**:
  - Job scheduling
  - Multi-node training
  - Resource management
- **API/CLI**: `sbatch`, `squeue`, REST API (Slurm REST API)
- **Integration Complexity**: Medium (job submission)

## Implementation Priority

### High Priority (Easy Integration, High Value)
1. **Vast.ai** - Simple API, best value, popular
2. **Lambda Labs** - Good API, popular with researchers
3. **Paperspace** - Simple API, startup-friendly

### Medium Priority (Moderate Complexity, Good Value)
4. **Modal** - Unique serverless model, different paradigm
5. **GCP** - Enterprise demand, TPU support
6. **Azure** - Enterprise demand

### Lower Priority (High Complexity, Specialized)
7. **CoreWeave** - Kubernetes-based, requires K8s expertise
8. **Kubernetes (Generic)** - Very flexible but complex
9. **Slurm** - Niche HPC market

## Integration Patterns

### Pattern 1: REST API Providers (Vast.ai, Lambda Labs, Paperspace)
```rust
// Similar to RunPod - REST API calls
pub struct VastProvider {
    api_key: String,
    client: reqwest::Client,
}

impl TrainingProvider for VastProvider {
    async fn create_resource(&self, ...) -> Result<ResourceId> {
        // POST to API endpoint
    }
}
```

### Pattern 2: CLI-Based Providers (RunPod-style)
```rust
// Use existing CLI tools
pub struct PaperspaceProvider {
    // Use paperspace CLI or API
}
```

### Pattern 3: Kubernetes Providers (CoreWeave, Generic K8s)
```rust
// Use k8s-rs or similar
pub struct KubernetesProvider {
    k8s_client: k8s::Client,
}

impl TrainingProvider for KubernetesProvider {
    async fn create_resource(&self, ...) -> Result<ResourceId> {
        // Create Job/Pod via Kubernetes API
    }
}
```

### Pattern 4: Serverless Providers (Modal)
```rust
// Different paradigm - functions instead of instances
pub struct ModalProvider {
    // Modal has Python SDK, might need Rust bridge
    // Or use Modal's REST API
}
```

## Cost Comparison (H100 GPU, approximate)

| Provider | Price/Hour | Notes |
|----------|-----------|-------|
| Vast.ai | $1.49-2.99 | Best value |
| CoreWeave | $2.20-3.00 | Value tier |
| Lambda Labs | ~$3.00 | Mid-range |
| RunPod | ~$2.99 | Competitive |
| AWS | $3-4 | On-demand |
| GCP | $3-4 | On-demand |
| Paperspace | ~$2.50 | Budget-friendly |
| Modal | Per-request | Serverless pricing |

## Recommendations

### Immediate Next Steps
1. **Complete existing providers** (AWS, RunPod, Lyceum AI)
2. **Add Vast.ai** - Simple API, best value, high demand
3. **Add Lambda Labs** - Popular with researchers, good API

### Medium-term
4. **Add Paperspace** - Startup-friendly, simple integration
5. **Add Modal** - Unique serverless paradigm, growing popularity

### Long-term
6. **Kubernetes support** - Generic K8s provider for flexibility
7. **GCP/Azure** - Enterprise demand

## Architecture Considerations

### Provider Trait Compatibility
All these providers can implement the `TrainingProvider` trait:
- `create_resource()` → Create instance/pod/job
- `train()` → Execute training
- `monitor()` → Watch logs/progress
- `terminate()` → Cleanup

### Special Cases
- **Modal**: Serverless model - `create_resource()` might return a function ID instead of instance ID
- **Kubernetes**: Uses Jobs/Pods instead of instances
- **Slurm**: Uses job IDs instead of instance IDs

### Unified Interface Benefits
- Same CLI commands work across all providers
- Cost comparison across providers
- Easy migration between providers
- Consistent checkpoint management

## Implementation Notes

### Vast.ai Integration
- Simple REST API
- Python client available (can reference for Rust implementation)
- Instance management straightforward
- Good documentation

### Lambda Labs Integration
- REST API with authentication
- Instance lifecycle management
- SSH key management
- Job submission API

### Paperspace Integration
- REST API
- Gradient platform integration
- Notebook support
- Git-based workflows

### Modal Integration
- Python SDK (primary interface)
- REST API available
- Function-based model (different from instance-based)
- May need adapter pattern for `TrainingProvider` trait

## Conclusion

The provider-agnostic architecture we've built makes it straightforward to add new providers. The highest-value additions would be:

1. **Vast.ai** - Best value, simple API
2. **Lambda Labs** - Popular with researchers
3. **Paperspace** - Startup-friendly

These three would give `runctl` coverage of the major GPU cloud providers beyond AWS and RunPod, with minimal implementation complexity.

