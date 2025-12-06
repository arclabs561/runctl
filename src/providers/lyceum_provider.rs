//! Lyceum AI provider implementation

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::provider::*;
use async_trait::async_trait;
use std::path::Path;

/// Lyceum AI provider implementation
///
/// Currently a stub - not yet implemented.
/// Kept for future Lyceum AI integration.
#[allow(dead_code)]
pub struct LyceumProvider {
    #[allow(dead_code)]
    config: Config,
    #[allow(dead_code)]
    api_key: Option<String>,
}

impl LyceumProvider {
    pub fn new(config: Config, api_key: Option<String>) -> Self {
        Self { config, api_key }
    }
}

#[async_trait]
impl TrainingProvider for LyceumProvider {
    fn name(&self) -> &'static str {
        "lyceum"
    }

    async fn create_resource(
        &self,
        _instance_type: &str,
        _options: CreateResourceOptions,
    ) -> Result<ResourceId> {
        // TODO: Implement Lyceum AI pod creation
        // Would use Lyceum AI API or CLI
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn get_resource_status(&self, _resource_id: &ResourceId) -> Result<ResourceStatus> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn list_resources(&self) -> Result<Vec<ResourceStatus>> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn train(&self, _resource_id: &ResourceId, _job: TrainingJob) -> Result<TrainingStatus> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn monitor(&self, _resource_id: &ResourceId, _follow: bool) -> Result<()> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn download(
        &self,
        _resource_id: &ResourceId,
        _remote_path: &Path,
        _local_path: &Path,
    ) -> Result<()> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    async fn terminate(&self, _resource_id: &ResourceId) -> Result<()> {
        Err(TrainctlError::CloudProvider {
            provider: "lyceum".to_string(),
            message: "Lyceum AI provider not yet implemented".to_string(),
            source: None,
        })
    }

    fn estimate_cost(&self, instance_type: &str, hours: f64) -> f64 {
        // Lyceum AI pricing (placeholder - needs actual pricing)
        let cost_per_hour = match instance_type {
            "A100" => 2.50,
            "H100" => 4.00,
            _ => 1.00,
        };
        cost_per_hour * hours
    }
}
