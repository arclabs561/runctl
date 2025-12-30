//! Utility functions for resource management

/// Estimate instance cost per hour
///
/// Simplified cost estimation (would use AWS Pricing API in production).
/// These are approximate on-demand prices per hour.
pub fn estimate_instance_cost(instance_type: &str) -> f64 {
    match instance_type {
        t if t.starts_with("t3.") => 0.0416, // t3.medium ~$0.0416/hr
        t if t.starts_with("t4g.") => 0.0336,
        t if t.starts_with("m5.") => 0.192,
        t if t.starts_with("c5.") => 0.17,
        t if t.starts_with("g4dn.") => 0.526, // GPU instance
        t if t.starts_with("p3.") => 3.06,    // GPU instance
        _ => 0.1,                             // Default estimate
    }
}
