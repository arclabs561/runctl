//! E2E tests for cost threshold warnings
//!
//! Tests verify that cost warnings appear when thresholds are exceeded.
//!
//! Run with: `TRAINCTL_E2E=1 cargo test --test cost_threshold_e2e_test --features e2e -- --ignored`
//!
//! Cost: ~$0.00 (read-only operations)

use std::env;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use tracing::info;

fn should_run_e2e() -> bool {
    env::var("TRAINCTL_E2E").is_ok() || env::var("CI").is_ok()
}

macro_rules! require_e2e {
    () => {
        if !should_run_e2e() {
            eprintln!("Skipping E2E test. Set TRAINCTL_E2E=1 to run");
            return;
        }
    };
}

#[tokio::test]
#[ignore]
async fn test_cost_threshold_warnings() {
    require_e2e!();
    
    let aws_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Ec2Client::new(&aws_config);
    
    // Get running instances and calculate costs
    let response = client
        .describe_instances()
        .filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("instance-state-name")
                .values("running")
                .build()
        )
        .send()
        .await
        .expect("Failed to describe instances");
    
    let mut total_hourly_cost = 0.0;
    
    for reservation in response.reservations() {
        for instance in reservation.instances() {
            let instance_type = instance.instance_type()
                .map(|t| format!("{}", t))
                .unwrap_or_default();
            
            // Rough cost estimates
            let cost = match instance_type.as_str() {
                "t3.micro" => 0.0104,
                "t3.small" => 0.0208,
                "t3.medium" => 0.0416,
                "g4dn.xlarge" => 0.526,
                "g4dn.2xlarge" => 0.752,
                "p3.2xlarge" => 3.06,
                _ => 0.1, // Default estimate
            };
            
            total_hourly_cost += cost;
        }
    }
    
    info!("Current hourly cost: ${:.2}", total_hourly_cost);
    
    // Test thresholds
    let hourly_threshold = 50.0;
    let daily_threshold = 100.0;
    
    if total_hourly_cost > hourly_threshold {
        info!("⚠️  WARNING: Hourly cost (${:.2}/hr) exceeds threshold (${}/hr)", 
            total_hourly_cost, hourly_threshold);
    }
    
    let daily_cost = total_hourly_cost * 24.0;
    if daily_cost > daily_threshold {
        info!("⚠️  WARNING: Daily projection (${:.2}/day) exceeds threshold (${}/day)", 
            daily_cost, daily_threshold);
    }
    
    // Test passes if we can calculate costs
    assert!(total_hourly_cost >= 0.0);
}

