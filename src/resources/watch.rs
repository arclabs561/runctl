//! Watch mode for resource listing

use crate::config::Config;
use crate::error::Result;
use crate::resources::aws;
use crate::resources::types::ListResourcesOptions;
use chrono::Utc;
use std::io::{self, Write};

/// List resources in watch mode (continuous updates)
pub async fn list_resources_watch(
    config: &Config,
    platform: &str,
    filter: &str,
    sort: Option<&str>,
    interval: u64,
    project_filter: Option<&str>,
    user_filter: Option<&str>,
) -> Result<()> {
    loop {
        // Clear screen (ANSI escape code)
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush()?;

        println!("WATCH: refreshing every {}s | [Ctrl+C] to stop", interval);
        println!(
            "Last update: {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        let list_options = ListResourcesOptions {
            detailed: false,
            platform: platform.to_string(),
            output_format: "text".to_string(),
            format: "table".to_string(),
            filter: filter.to_string(),
            sort: sort.map(|s| s.to_string()),
            limit: None,
            show_terminated: false,
            export: None,
            export_file: None,
            project_filter: project_filter.map(|s| s.to_string()),
            user_filter: user_filter.map(|s| s.to_string()),
        };
        aws::list_resources(list_options, config).await?;

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}
