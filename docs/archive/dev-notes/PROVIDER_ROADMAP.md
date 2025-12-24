# Provider Roadmap

## Current Status

### ✅ Implemented (Skeletons)
- AWS EC2
- RunPod
- Lyceum AI

### 🚧 In Progress
- AWS EBS volumes (basic operations complete, pre-warming pending)

## Recommended Provider Additions

### Phase 1: High-Value, Low-Complexity (Next 2-4 weeks)

#### 1. Vast.ai ⭐ **Top Priority**
- **Why**: Best value pricing ($1.49-2.99/hr H100), simple REST API, very popular
- **Complexity**: Low
- **Effort**: 1-2 weeks
- **API**: REST API, Python client available for reference
- **Value**: High - covers budget-conscious users

#### 2. Lambda Labs
- **Why**: Popular with researchers, good API, enterprise support
- **Complexity**: Low-Medium
- **Effort**: 1-2 weeks
- **API**: REST API, Python SDK
- **Value**: High - researcher community

#### 3. Paperspace
- **Why**: Startup-friendly, simple integration, Gradient platform
- **Complexity**: Low
- **Effort**: 1 week
- **API**: REST API, Python SDK
- **Value**: Medium - startup market

### Phase 2: Enterprise (4-6 weeks)

#### 4. Google Cloud Platform (GCP)
- **Why**: Enterprise demand, TPU support (unique advantage)
- **Complexity**: Medium
- **Effort**: 2-3 weeks
- **API**: `gcloud` CLI, REST APIs, GCP SDK
- **Value**: High - enterprise, TPU workloads

#### 5. Azure
- **Why**: Enterprise demand, Microsoft ecosystem
- **Complexity**: Medium
- **Effort**: 2-3 weeks
- **API**: Azure CLI, REST APIs, Azure SDK
- **Value**: Medium - Microsoft shops

### Phase 3: Advanced/Specialized (6+ weeks)

#### 6. Modal
- **Why**: Unique serverless model, growing popularity
- **Complexity**: Medium (different paradigm)
- **Effort**: 2-3 weeks
- **API**: Python decorators (may need adapter pattern)
- **Value**: Medium - serverless use cases

#### 7. Kubernetes (Generic)
- **Why**: Maximum flexibility, multi-cloud portability
- **Complexity**: High
- **Effort**: 3-4 weeks
- **API**: Kubernetes API, `kubectl`
- **Value**: High - but requires K8s expertise

#### 8. CoreWeave
- **Why**: Premium GPU cloud, InfiniBand networking
- **Complexity**: High (Kubernetes-based)
- **Effort**: 3-4 weeks
- **API**: Kubernetes (`kubectl`)
- **Value**: Medium - specialized use case

## Implementation Strategy

### Quick Wins First
Start with **Vast.ai** - it provides:
- Best value for users
- Simple REST API
- High demand
- Low complexity
- Good documentation

### Provider Trait Pattern
All providers follow the same pattern:

```rust
pub struct NewProvider {
    // Provider-specific config
}

#[async_trait]
impl TrainingProvider for NewProvider {
    fn name(&self) -> &'static str { "newprovider" }
    
    async fn create_resource(&self, ...) -> Result<ResourceId> {
        // Provider-specific implementation
    }
    
    // ... other trait methods
}
```

### Testing Strategy
1. **Unit tests**: Mock API responses
2. **Integration tests**: Test with real APIs (opt-in)
3. **E2E tests**: Full workflow tests

## Cost Comparison

| Provider | H100/hr | Best For |
|----------|---------|----------|
| Vast.ai | $1.49-2.99 | Best value ⭐ |
| CoreWeave | $2.20-3.00 | Large-scale |
| Paperspace | ~$2.50 | Startups |
| Lambda Labs | ~$3.00 | Research |
| RunPod | ~$2.99 | General |
| AWS/GCP/Azure | $3-4 | Enterprise |

## Benefits of Multi-Provider Support

1. **Cost Optimization**: Compare and choose cheapest provider
2. **Availability**: Fallback if one provider is unavailable
3. **Feature Diversity**: Different providers excel at different things
4. **No Lock-in**: Easy migration between providers
5. **Unified Interface**: Same commands work everywhere

## Next Actions

1. ✅ Research providers - **DONE**
2. ✅ Document recommendations - **DONE**
3. 🚧 Implement Vast.ai provider - **NEXT**
4. 🚧 Implement Lambda Labs provider
5. 🚧 Implement Paperspace provider

## Success Metrics

- **Coverage**: Support 5+ major providers
- **Cost Savings**: Enable users to save 30-50% via provider comparison
- **Adoption**: Users actively switching providers based on cost/availability
- **Reliability**: Fallback providers when primary is unavailable

