//! Provider implementations for different cloud platforms
//!
//! This module contains provider trait implementations following industry patterns
//! similar to Terraform's plugin system and Pulumi's component model.
//!
//! **Current Status**: Provider trait system is defined but CLI uses direct implementations.
//! This follows the pragmatic pattern seen in mature tools (Terraform, Pulumi) where
//! abstraction layers are prepared but not forced until multi-cloud support is needed.
//!
//! **Architecture Decision**: See `docs/PROVIDER_TRAIT_DECISION.md` for rationale.
//!
//! ## Provider Registry Pattern
//!
//! When multi-cloud support becomes a priority, use `ProviderRegistry` to:
//! - Register available providers
//! - Select providers at runtime based on configuration or flags
//! - Enable gradual migration from direct implementations to trait-based code
//!
//! See `src/provider.rs` for the `TrainingProvider` trait definition.

mod aws_provider;
mod lyceum_provider;
mod runpod_provider;

// Re-export providers for external use (e.g., in tests)
// These are reserved for future multi-cloud support - see PROVIDER_TRAIT_DECISION.md
#[allow(unused_imports)]
pub use aws_provider::AwsProvider;
#[allow(unused_imports)]
pub use lyceum_provider::LyceumProvider;
#[allow(unused_imports)]
pub use runpod_provider::RunpodProvider;

use crate::error::{Result, TrainctlError};
use crate::provider::TrainingProvider;
use std::collections::HashMap;
use std::sync::Arc;

/// Provider registry for managing multiple cloud providers
///
/// Similar to Terraform's plugin registry, this enables dynamic provider discovery
/// and selection. Currently reserved for future multi-cloud support.
///
/// ## Usage Pattern (Future)
///
/// ```rust,no_run
/// use runctl::providers::{ProviderRegistry, AwsProvider, RunpodProvider};
/// use runctl::provider::CreateResourceOptions;
/// use runctl::config::Config;
/// use std::sync::Arc;
///
/// # async fn example() -> runctl::error::Result<()> {
/// let config = Config::default();
/// let mut registry = ProviderRegistry::new();
/// registry.register("aws", Arc::new(AwsProvider::new(config.clone()).await?))?;
/// registry.register("runpod", Arc::new(RunpodProvider::new(config)))?;
///
/// let provider = registry.get("aws")?;
/// let options = CreateResourceOptions::default();
/// let resource_id = provider.create_resource("g4dn.xlarge", options).await?;
/// # Ok(())
/// # }
/// ```
#[allow(dead_code)] // Reserved for future multi-cloud support
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn TrainingProvider>>,
}

#[allow(dead_code)] // Reserved for future multi-cloud support
impl ProviderRegistry {
    /// Create a new empty provider registry
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider with the registry
    ///
    /// # Arguments
    /// * `name` - Provider name (e.g., "aws", "runpod", "lyceum")
    /// * `provider` - Provider implementation
    ///
    /// # Errors
    /// Returns `ResourceExists` if a provider with the same name is already registered
    pub fn register(
        &mut self,
        name: impl Into<String>,
        provider: Arc<dyn TrainingProvider>,
    ) -> Result<()> {
        let name = name.into();
        if self.providers.contains_key(&name) {
            return Err(TrainctlError::ResourceExists {
                resource_type: "provider".to_string(),
                resource_id: name,
            });
        }
        self.providers.insert(name, provider);
        Ok(())
    }

    /// Get a provider by name
    ///
    /// # Arguments
    /// * `name` - Provider name to look up
    ///
    /// # Errors
    /// Returns `ResourceNotFound` if the provider is not registered
    pub fn get(&self, name: &str) -> Result<Arc<dyn TrainingProvider>> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "provider".to_string(),
                resource_id: name.to_string(),
            })
    }

    /// List all registered provider names
    pub fn list(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// Check if a provider is registered
    pub fn has(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
