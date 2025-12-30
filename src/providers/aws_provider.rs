//! AWS EC2 provider implementation

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::provider::*;
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use chrono::{DateTime, Utc};
use std::path::Path;

/// AWS EC2 provider implementation
///
/// Currently unused - CLI uses direct AWS implementations in aws.rs.
/// Kept for potential future refactoring to use provider trait system.
#[allow(dead_code)]
pub struct AwsProvider {
    ec2_client: Ec2Client,
    #[allow(dead_code)]
    ssm_client: SsmClient,
    #[allow(dead_code)]
    config: Config,
}

impl AwsProvider {
    #[allow(dead_code)] // Reserved for future provider initialization
    pub async fn new(config: Config) -> Result<Self> {
        let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let ec2_client = Ec2Client::new(&aws_config);
        let ssm_client = SsmClient::new(&aws_config);

        Ok(Self {
            ec2_client,
            ssm_client,
            config,
        })
    }
}

#[async_trait]
impl TrainingProvider for AwsProvider {
    fn name(&self) -> &'static str {
        "aws"
    }

    async fn create_resource(
        &self,
        _instance_type: &str,
        _options: CreateResourceOptions,
    ) -> Result<ResourceId> {
        let _aws_cfg = self.config.aws.as_ref().ok_or_else(|| {
            TrainctlError::Config(crate::error::ConfigError::MissingField("aws".to_string()))
        })?;

        // Implementation would call create_instance logic
        // For now, return a placeholder
        Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "AWS instance creation not yet fully implemented in provider trait"
                .to_string(),
            source: None,
        })
    }

    async fn get_resource_status(&self, resource_id: &ResourceId) -> Result<ResourceStatus> {
        use crate::retry::{ExponentialBackoffPolicy, RetryPolicy};

        // Use retry logic for cloud API calls
        let response = ExponentialBackoffPolicy::for_cloud_api()
            .execute_with_retry(|| async {
                self.ec2_client
                    .describe_instances()
                    .instance_ids(resource_id)
                    .send()
                    .await
                    .map_err(|e| TrainctlError::Aws(format!("Failed to describe instance: {}", e)))
            })
            .await?;

        // Find the instance in reservations
        let instance = response
            .reservations()
            .iter()
            .flat_map(|reservation| reservation.instances())
            .find(|inst| {
                inst.instance_id()
                    .map(|id| id == resource_id)
                    .unwrap_or(false)
            })
            .ok_or_else(|| TrainctlError::ResourceNotFound {
                resource_type: "instance".to_string(),
                resource_id: resource_id.clone(),
            })?;

        // Reuse existing helper function to avoid code duplication
        // Note: This requires making the helper public or creating a provider-specific version
        // For now, we duplicate the logic but document that it should be refactored
        let state = normalize_state(
            instance
                .state()
                .and_then(|s| s.name())
                .map(|s| s.as_str())
                .unwrap_or("unknown"),
        );

        let tags: Vec<(String, String)> = instance
            .tags()
            .iter()
            .filter_map(|tag| {
                tag.key()
                    .zip(tag.value())
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();

        let instance_type = instance.instance_type().map(|t| t.as_str().to_string());
        let launch_time = instance
            .launch_time()
            .map(|lt| DateTime::<Utc>::from_timestamp(lt.secs(), 0).unwrap_or_else(chrono::Utc::now));

        let public_ip = instance.public_ip_address().map(|ip| ip.to_string());

        let cost_per_hour =
            crate::resources::estimate_instance_cost(instance_type.as_deref().unwrap_or("unknown"));

        Ok(ResourceStatus {
            id: resource_id.clone(),
            name: tags
                .iter()
                .find(|(k, _)| k == "Name")
                .map(|(_, v)| v.clone()),
            state,
            instance_type,
            launch_time,
            cost_per_hour,
            public_ip,
            tags,
        })
    }

    async fn list_resources(&self) -> Result<Vec<ResourceStatus>> {
        // Delegate to existing list_aws_instances logic
        // This would be refactored to use the provider trait
        Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "List resources not yet fully implemented in provider trait".to_string(),
            source: None,
        })
    }

    async fn train(&self, _resource_id: &ResourceId, _job: TrainingJob) -> Result<TrainingStatus> {
        // Implementation would use SSM to execute training
        Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "Training not yet fully implemented in provider trait".to_string(),
            source: None,
        })
    }

    async fn monitor(&self, _resource_id: &ResourceId, _follow: bool) -> Result<()> {
        // Implementation would use SSM to tail logs
        Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "Monitoring not yet fully implemented in provider trait".to_string(),
            source: None,
        })
    }

    async fn download(
        &self,
        _resource_id: &ResourceId,
        _remote_path: &Path,
        _local_path: &Path,
    ) -> Result<()> {
        // Implementation would use SSM to download files
        Err(TrainctlError::CloudProvider {
            provider: "aws".to_string(),
            message: "Download not yet fully implemented in provider trait".to_string(),
            source: None,
        })
    }

    async fn terminate(&self, resource_id: &ResourceId) -> Result<()> {
        use crate::retry::{ExponentialBackoffPolicy, RetryPolicy};

        // Use retry logic for cloud API calls
        ExponentialBackoffPolicy::for_cloud_api()
            .execute_with_retry(|| async {
                self.ec2_client
                    .terminate_instances()
                    .instance_ids(resource_id)
                    .send()
                    .await
                    .map_err(|e| TrainctlError::Aws(format!("Failed to terminate instance: {}", e)))
            })
            .await?;

        Ok(())
    }

    fn estimate_cost(&self, instance_type: &str, hours: f64) -> f64 {
        crate::resources::estimate_instance_cost(instance_type) * hours
    }
}
