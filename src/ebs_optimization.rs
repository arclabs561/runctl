//! EBS Volume Optimization Utilities
//!
//! Provides automatic optimization of EBS volume settings based on use case,
//! volume size, and performance requirements.

use crate::error::{Result, TrainctlError};

/// Use case for EBS volume optimization
#[derive(Debug, Clone, Copy)]
pub enum VolumeUseCase {
    /// Data loading - high throughput for reading datasets
    DataLoading,
    /// Checkpoint storage - high IOPS for frequent small writes
    Checkpoints,
    /// General purpose - balanced performance
    GeneralPurpose,
    /// Archive storage - low cost, infrequent access
    Archive,
}

/// Optimized EBS volume configuration
#[derive(Debug, Clone)]
pub struct OptimizedVolumeConfig {
    pub volume_type: String,
    pub iops: Option<i32>,
    pub throughput: Option<i32>,
    pub recommendation: String,
}

/// Calculate optimal IOPS for gp3 volume based on size and use case
///
/// gp3 baseline: 3,000 IOPS (included)
/// gp3 maximum: 80,000 IOPS
/// Formula: min(3000 + size_gb * iops_per_gb, 80000)
/// For data loading: 500 IOPS/GB (up to max)
/// For checkpoints: 300 IOPS/GB (up to max)
/// For general: 100 IOPS/GB (up to max)
pub fn calculate_optimal_iops(size_gb: i32, use_case: VolumeUseCase) -> i32 {
    let iops_per_gb = match use_case {
        VolumeUseCase::DataLoading => 500, // Maximum performance for data loading
        VolumeUseCase::Checkpoints => 300, // High IOPS for frequent writes
        VolumeUseCase::GeneralPurpose => 100, // Moderate performance
        VolumeUseCase::Archive => 0,       // Use baseline only
    };

    if iops_per_gb == 0 {
        return 3000; // Baseline only
    }

    // gp3: up to 500 IOPS per GB, max 80,000 IOPS
    // Minimum volume size for max IOPS: 160 GB
    let calculated = 3000 + (size_gb * iops_per_gb);
    calculated.min(80000)
}

/// Calculate optimal throughput for gp3 volume based on IOPS
///
/// gp3 baseline: 125 MiB/s (included)
/// gp3 maximum: 2,000 MiB/s
/// Formula: 0.25 MiB/s per provisioned IOPS
/// For max throughput (2,000 MiB/s): need 8,000 IOPS
pub fn calculate_optimal_throughput(iops: i32) -> i32 {
    // Throughput = 0.25 MiB/s per IOPS
    // Max throughput: 2,000 MiB/s (requires 8,000 IOPS minimum)
    let calculated = (iops as f64 * 0.25) as i32;
    calculated.clamp(125, 2000) // At least baseline
}

/// Get optimized volume configuration for a use case
pub fn optimize_volume_config(
    size_gb: i32,
    use_case: VolumeUseCase,
    volume_type: Option<&str>,
) -> Result<OptimizedVolumeConfig> {
    let vol_type = volume_type.unwrap_or("gp3").to_string();

    match vol_type.as_str() {
        "gp3" => {
            let iops = calculate_optimal_iops(size_gb, use_case);
            let throughput = calculate_optimal_throughput(iops);

            let recommendation = match use_case {
                VolumeUseCase::DataLoading => format!(
                    "Data loading optimized: {} IOPS, {} MiB/s throughput for {} GB volume. \
                    Provides high throughput for reading large datasets.",
                    iops, throughput, size_gb
                ),
                VolumeUseCase::Checkpoints => format!(
                    "Checkpoint optimized: {} IOPS, {} MiB/s throughput for {} GB volume. \
                    Provides high IOPS for frequent small writes.",
                    iops, throughput, size_gb
                ),
                VolumeUseCase::GeneralPurpose => format!(
                    "General purpose: {} IOPS, {} MiB/s throughput for {} GB volume. \
                    Balanced performance for mixed workloads.",
                    iops, throughput, size_gb
                ),
                VolumeUseCase::Archive => {
                    "Archive storage: Baseline performance (3,000 IOPS, 125 MiB/s). \
                    Cost-optimized for infrequent access."
                        .to_string()
                }
            };

            Ok(OptimizedVolumeConfig {
                volume_type: vol_type,
                iops: Some(iops),
                throughput: Some(throughput),
                recommendation,
            })
        }
        "gp2" => {
            // gp2: 3 IOPS per GB, max 16,000 IOPS, 250 MiB/s throughput
            // No optimization needed - performance tied to size
            Ok(OptimizedVolumeConfig {
                volume_type: vol_type,
                iops: None,       // gp2 doesn't support IOPS provisioning
                throughput: None, // gp2 doesn't support throughput provisioning
                recommendation: format!(
                    "gp2 volume: {} IOPS (3 IOPS/GB), 250 MiB/s throughput. \
                    Performance scales with size. Consider gp3 for better cost/performance.",
                    (size_gb * 3).min(16000)
                ),
            })
        }
        "io2" => {
            // io2: Provisioned IOPS SSD, up to 64,000 IOPS (256,000 with io2 Block Express)
            // Best for high IOPS requirements
            let iops = match use_case {
                VolumeUseCase::DataLoading | VolumeUseCase::Checkpoints => {
                    (size_gb * 500).min(64000) // High performance
                }
                _ => 3000, // Baseline
            };

            Ok(OptimizedVolumeConfig {
                volume_type: vol_type,
                iops: Some(iops),
                throughput: None, // io2 doesn't support throughput provisioning
                recommendation: format!(
                    "io2 volume: {} IOPS. Best for high IOPS workloads. \
                    Supports multi-attach. More expensive than gp3.",
                    iops
                ),
            })
        }
        "st1" => {
            // st1: Throughput optimized HDD, 500 MiB/s baseline, 500 MiB/s max
            // Good for large sequential reads (data loading)
            Ok(OptimizedVolumeConfig {
                volume_type: vol_type,
                iops: None,
                throughput: None,
                recommendation:
                    "st1 volume: 500 MiB/s throughput. Best for large sequential reads. \
                    Lower cost than SSD. Minimum size: 125 GB."
                        .to_string(),
            })
        }
        "sc1" => {
            // sc1: Cold HDD, 250 MiB/s baseline, 250 MiB/s max
            // Lowest cost, infrequent access
            Ok(OptimizedVolumeConfig {
                volume_type: vol_type,
                iops: None,
                throughput: None,
                recommendation: "sc1 volume: 250 MiB/s throughput. Lowest cost option. \
                    Best for archival storage. Minimum size: 125 GB."
                    .to_string(),
            })
        }
        _ => Err(TrainctlError::Validation {
            field: "volume_type".to_string(),
            reason: format!(
                "Unsupported volume type: {}. Use: gp3, gp2, io2, st1, sc1",
                vol_type
            ),
        }),
    }
}

/// Get volume type recommendation based on use case
pub fn recommend_volume_type(use_case: VolumeUseCase, size_gb: i32) -> &'static str {
    match use_case {
        VolumeUseCase::DataLoading => {
            if size_gb > 1000 {
                "st1" // Large datasets, sequential reads
            } else {
                "gp3" // Small-medium datasets, random access
            }
        }
        VolumeUseCase::Checkpoints => {
            if size_gb < 100 {
                "gp3" // Small checkpoints, high IOPS
            } else {
                "io2" // Large checkpoints, multi-attach support
            }
        }
        VolumeUseCase::GeneralPurpose => "gp3",
        VolumeUseCase::Archive => {
            if size_gb > 500 {
                "sc1" // Large archives
            } else {
                "gp3" // Small archives, might need random access
            }
        }
    }
}

/// Format volume type description for help text
pub fn volume_type_description() -> String {
    r#"Volume Types:
  gp3  - General Purpose SSD (default, recommended)
         • Baseline: 3,000 IOPS, 125 MiB/s
         • Max: 80,000 IOPS, 2,000 MiB/s
         • Best for: Most workloads, cost-effective
         • Cost: ~$0.08/GB-month

  gp2  - General Purpose SSD (legacy)
         • 3 IOPS/GB (max 16,000 IOPS)
         • 250 MiB/s throughput
         • Best for: Legacy compatibility
         • Cost: ~$0.10/GB-month (20% more than gp3)

  io2  - Provisioned IOPS SSD
         • Up to 64,000 IOPS (256,000 with Block Express)
         • Multi-attach support
         • Best for: High IOPS, shared datasets
         • Cost: ~$0.125/GB-month + IOPS charges

  st1  - Throughput Optimized HDD
         • 500 MiB/s throughput
         • Best for: Large sequential reads (data loading)
         • Cost: ~$0.045/GB-month (cheaper than SSD)
         • Min size: 125 GB

  sc1  - Cold HDD
         • 250 MiB/s throughput
         • Best for: Archival storage
         • Cost: ~$0.015/GB-month (lowest cost)
         • Min size: 125 GB

Recommendations:
  • Data loading (<1TB): gp3 with optimized IOPS/throughput
  • Data loading (>1TB): st1 for cost efficiency
  • Checkpoints: gp3 or io2 (if multi-attach needed)
  • General purpose: gp3 (default)
  • Archive: sc1 or gp3 (if random access needed)
"#
    .to_string()
}
