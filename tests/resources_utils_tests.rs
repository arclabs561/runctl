//! Unit tests for resources module utility functions
//!
//! Tests utility functions in the resources module without requiring AWS credentials.

use runctl::resources::estimate_instance_cost;

#[test]
fn test_estimate_instance_cost_t3_family() {
    // t3 family cost estimation
    assert_eq!(estimate_instance_cost("t3.micro"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.small"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.medium"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.large"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.xlarge"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.2xlarge"), 0.0416);
}

#[test]
fn test_estimate_instance_cost_t4g_family() {
    // t4g family cost estimation
    assert_eq!(estimate_instance_cost("t4g.micro"), 0.0336);
    assert_eq!(estimate_instance_cost("t4g.small"), 0.0336);
    assert_eq!(estimate_instance_cost("t4g.medium"), 0.0336);
}

#[test]
fn test_estimate_instance_cost_m5_family() {
    // m5 family cost estimation
    assert_eq!(estimate_instance_cost("m5.large"), 0.192);
    assert_eq!(estimate_instance_cost("m5.xlarge"), 0.192);
    assert_eq!(estimate_instance_cost("m5.2xlarge"), 0.192);
}

#[test]
fn test_estimate_instance_cost_c5_family() {
    // c5 family cost estimation
    assert_eq!(estimate_instance_cost("c5.large"), 0.17);
    assert_eq!(estimate_instance_cost("c5.xlarge"), 0.17);
    assert_eq!(estimate_instance_cost("c5.2xlarge"), 0.17);
}

#[test]
fn test_estimate_instance_cost_gpu_instances() {
    // GPU instance cost estimation
    assert_eq!(estimate_instance_cost("g4dn.xlarge"), 0.526);
    assert_eq!(estimate_instance_cost("g4dn.2xlarge"), 0.526);
    assert_eq!(estimate_instance_cost("g4dn.4xlarge"), 0.526);
    
    assert_eq!(estimate_instance_cost("p3.2xlarge"), 3.06);
    assert_eq!(estimate_instance_cost("p3.8xlarge"), 3.06);
    assert_eq!(estimate_instance_cost("p3.16xlarge"), 3.06);
}

#[test]
fn test_estimate_instance_cost_unknown_types() {
    // Unknown instance types return default
    assert_eq!(estimate_instance_cost("unknown.type"), 0.1);
    assert_eq!(estimate_instance_cost(""), 0.1);
    assert_eq!(estimate_instance_cost("custom-instance"), 0.1);
}

#[test]
fn test_estimate_instance_cost_all_positive() {
    // All cost estimates are positive
    let instance_types = [
        "t3.micro", "t3.medium", "t3.large",
        "t4g.micro", "t4g.medium",
        "m5.large", "m5.xlarge",
        "c5.large", "c5.xlarge",
        "g4dn.xlarge", "g4dn.2xlarge",
        "p3.2xlarge", "p3.8xlarge",
        "unknown.type", "custom",
    ];
    
    for instance_type in instance_types {
        let cost = estimate_instance_cost(instance_type);
        assert!(cost > 0.0, "Cost should be positive for {}", instance_type);
        assert!(cost < 100.0, "Cost should be reasonable for {}", instance_type);
    }
}

#[test]
fn test_estimate_instance_cost_gpu_more_expensive() {
    // GPU instances cost more than CPU
    let cpu_cost = estimate_instance_cost("t3.medium");
    let gpu_cost = estimate_instance_cost("g4dn.xlarge");
    
    assert!(gpu_cost > cpu_cost, "GPU instances should cost more than CPU");
    
    let high_end_gpu = estimate_instance_cost("p3.2xlarge");
    assert!(high_end_gpu > gpu_cost, "High-end GPU should cost more than mid-range");
}

#[test]
fn test_estimate_instance_cost_case_insensitive() {
    // Case handling (currently case-sensitive)
    assert_eq!(estimate_instance_cost("T3.MICRO"), 0.1); // Falls back to default
    assert_eq!(estimate_instance_cost("t3.micro"), 0.0416); // Correct case works
}

#[test]
fn test_estimate_instance_cost_with_prefix() {
    // Prefix matching
    assert_eq!(estimate_instance_cost("t3.micro"), 0.0416);
    assert_eq!(estimate_instance_cost("t3.anything"), 0.0416);
    assert_eq!(estimate_instance_cost("t3"), 0.1); // No dot, falls back
    
    assert_eq!(estimate_instance_cost("g4dn.xlarge"), 0.526);
    assert_eq!(estimate_instance_cost("g4dn.anything"), 0.526);
}

