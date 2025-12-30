//! Export functions for resource data

use crate::config::Config;
use crate::error::{Result, TrainctlError};
use crate::resources::json;
use chrono::Utc;

/// Export resources to file
pub async fn export_resources(
    config: &Config,
    _platform: &str,
    format: &str,
    file: Option<&str>,
) -> Result<()> {
    let summary = json::get_resource_summary_json(config).await?;

    match format {
        "csv" => {
            let csv = generate_csv(&summary)?;
            if let Some(path) = file {
                std::fs::write(path, csv)?;
                println!("Exported to {}", path);
            } else {
                print!("{}", csv);
            }
        }
        "html" => {
            let html = generate_html(&summary)?;
            if let Some(path) = file {
                std::fs::write(path, html)?;
                println!("Exported to {}", path);
            } else {
                print!("{}", html);
            }
        }
        _ => {
            return Err(TrainctlError::Validation {
                field: "format".to_string(),
                reason: format!("Unsupported export format: {}. Use 'csv' or 'html'", format),
            });
        }
    }

    Ok(())
}

fn generate_csv(summary: &serde_json::Value) -> Result<String> {
    let mut csv = String::from(
        "Instance ID,Type,State,Cost/hr,Accumulated,Public IP,Private IP,Is Spot,Runtime\n",
    );

    if let Some(aws) = summary.get("aws") {
        if let Some(instances) = aws.get("instances").and_then(|v| v.as_array()) {
            for inst in instances {
                let id = inst
                    .get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let inst_type = inst
                    .get("instance_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let state = inst.get("state").and_then(|v| v.as_str()).unwrap_or("");
                let cost = inst
                    .get("cost_per_hour")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let total = inst
                    .get("accumulated_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let public_ip = inst.get("public_ip").and_then(|v| v.as_str()).unwrap_or("");
                let private_ip = inst
                    .get("private_ip")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let is_spot = inst
                    .get("is_spot")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let runtime = inst.get("runtime").and_then(|v| v.as_str()).unwrap_or("");

                csv.push_str(&format!(
                    "{},{},{},{:.4},{:.2},{},{},{},{}\n",
                    id, inst_type, state, cost, total, public_ip, private_ip, is_spot, runtime
                ));
            }
        }
    }

    Ok(csv)
}

fn generate_html(summary: &serde_json::Value) -> Result<String> {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>runctl Resource Report</title>
    <style>
        body { font-family: monospace; margin: 20px; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
        th { background-color: #4CAF50; color: white; }
        tr:nth-child(even) { background-color: #f2f2f2; }
        .running { color: green; }
        .stopped { color: orange; }
        .terminated { color: red; }
    </style>
</head>
<body>
    <h1>Resource Report</h1>
    <p>Generated: "#,
    );

    html.push_str(&Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string());
    html.push_str(
        r#"</p>
    <table>
        <tr>
            <th>Instance ID</th>
            <th>Type</th>
            <th>State</th>
            <th>Cost/hr</th>
            <th>Total</th>
            <th>Public IP</th>
            <th>Runtime</th>
        </tr>"#,
    );

    if let Some(aws) = summary.get("aws") {
        if let Some(instances) = aws.get("instances").and_then(|v| v.as_array()) {
            for inst in instances {
                let id = inst
                    .get("instance_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let inst_type = inst
                    .get("instance_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let state = inst.get("state").and_then(|v| v.as_str()).unwrap_or("");
                let state_class = match state {
                    "running" => "running",
                    "stopped" => "stopped",
                    "terminated" => "terminated",
                    _ => "",
                };
                let cost = inst
                    .get("cost_per_hour")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let total = inst
                    .get("accumulated_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let public_ip = inst.get("public_ip").and_then(|v| v.as_str()).unwrap_or("");
                let runtime = inst.get("runtime").and_then(|v| v.as_str()).unwrap_or("");

                html.push_str(&format!(
                    r#"<tr>
            <td>{}</td>
            <td>{}</td>
            <td class="{}">{}</td>
            <td>${:.4}</td>
            <td>${:.2}</td>
            <td>{}</td>
            <td>{}</td>
        </tr>"#,
                    id, inst_type, state_class, state, cost, total, public_ip, runtime
                ));
            }
        }
    }

    html.push_str(
        r#"
    </table>
</body>
</html>"#,
    );

    Ok(html)
}
