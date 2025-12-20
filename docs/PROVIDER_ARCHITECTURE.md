# Provider Architecture

## Overview

`runctl` uses a provider-agnostic architecture that allows it to work with any cloud training platform (AWS, RunPod, Lyceum AI, etc.) through a unified interface.

## Design Principles

1. **Provider Trait**: All providers implement the `TrainingProvider` trait
2. **Unified Interface**: Common operations (create, train, monitor, terminate) work the same across providers
3. **Easy Extension**: Adding a new provider only requires implementing the trait
4. **No Lock-in**: Core logic doesn't depend on provider-specific code

## Architecture

```
src/
├── provider.rs          # Trait definitions and common types
└── providers/
    ├── mod.rs          # Provider registry
    ├── aws_provider.rs # AWS EC2 implementation
    ├── runpod_provider.rs # RunPod implementation
    └── lyceum_provider.rs # Lyceum AI implementation
```

## Core Traits

### `TrainingProvider`

The main trait that all providers must implement:

```rust
#[async_trait]
pub trait TrainingProvider: Send + Sync {
    fn name(&self) -> &'static str;
    
    async fn create_resource(
        &self,
        instance_type: &str,
        options: CreateResourceOptions,
    ) -> Result<ResourceId>;
    
    async fn get_resource_status(&self, resource_id: &ResourceId) -> Result<ResourceStatus>;
    async fn list_resources(&self) -> Result<Vec<ResourceStatus>>;
    async fn train(&self, resource_id: &ResourceId, job: TrainingJob) -> Result<TrainingStatus>;
    async fn monitor(&self, resource_id: &ResourceId, follow: bool) -> Result<()>;
    async fn download(&self, resource_id: &ResourceId, remote_path: &PathBuf, local_path: &PathBuf) -> Result<()>;
    async fn terminate(&self, resource_id: &ResourceId) -> Result<()>;
    fn estimate_cost(&self, instance_type: &str, hours: f64) -> f64;
}
```

## Common Types

### `ResourceStatus`

Unified status information across all providers:

```rust
pub struct ResourceStatus {
    pub id: ResourceId,
    pub name: Option<String>,
    pub state: ResourceState,
    pub instance_type: Option<String>,
    pub launch_time: Option<DateTime<Utc>>,
    pub cost_per_hour: f64,
    pub public_ip: Option<String>,
    pub tags: Vec<(String, String)>,
}
```

### `ResourceState`

Normalized states that work across providers:

- `Running` - Resource is active
- `Starting` - Resource is provisioning
- `Stopped` - Resource is stopped but can be restarted
- `Terminating` - Resource is shutting down
- `Terminated` - Resource is gone
- `Error(String)` - Resource is in error state
- `Unknown` - State couldn't be determined

### `TrainingJob`

Unified training job configuration:

```rust
pub struct TrainingJob {
    pub script: PathBuf,
    pub args: Vec<String>,
    pub data_source: Option<String>,
    pub output_dest: Option<String>,
    pub checkpoint_dir: Option<PathBuf>,
    pub environment: Vec<(String, String)>,
}
```

## Adding a New Provider

1. Create a new file in `src/providers/` (e.g., `new_provider.rs`)
2. Implement the `TrainingProvider` trait
3. Add the module to `src/providers/mod.rs`
4. Register the provider in the registry

Example:

```rust
use crate::provider::*;
use async_trait::async_trait;

pub struct NewProvider {
    // Provider-specific config
}

#[async_trait]
impl TrainingProvider for NewProvider {
    fn name(&self) -> &'static str {
        "newprovider"
    }
    
    // Implement all trait methods...
}
```

## State Normalization

Provider-specific states are normalized using `normalize_state()`:

```rust
pub fn normalize_state(state_str: &str) -> ResourceState {
    // Maps provider-specific states to ResourceState enum
}
```

This ensures that `"running"`, `"active"`, and `"ready"` all map to `ResourceState::Running`.

## Provider Registry

The `ProviderRegistry` allows dynamic provider discovery:

```rust
let mut registry = ProviderRegistry::new();
registry.register(Box::new(AwsProvider::new(config).await?));
registry.register(Box::new(RunpodProvider::new(config)));

let provider = registry.get("aws")?;
let status = provider.get_resource_status(&instance_id).await?;
```

## Benefits

1. **Unified CLI**: Same commands work across all providers
2. **Easy Testing**: Mock providers for unit tests
3. **Cost Comparison**: Compare costs across providers
4. **Migration**: Easy to switch providers
5. **Extensibility**: Add new providers without changing core code

## Current Status

- ✅ Trait definitions complete
- ✅ AWS provider skeleton (needs full implementation)
- ✅ RunPod provider skeleton (needs full implementation)
- 🚧 Lyceum AI provider skeleton (needs implementation)
- 🚧 Provider registry integration
- 🚧 CLI commands using providers

## Next Steps

1. Complete AWS provider implementation
2. Complete RunPod provider implementation
3. Implement Lyceum AI provider
4. Refactor CLI commands to use providers
5. Add provider selection/auto-detection
6. Add cost comparison across providers

