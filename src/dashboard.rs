//! Interactive dashboard for monitoring resources and processes
//!
//! Provides a ratatui-based dashboard showing:
//! - Running resources (instances, pods, processes)
//! - Current costs and billing
//! - Process resource usage (top-like view)
//! - Real-time updates

use crate::aws_utils;
use crate::config::Config;
use crate::diagnostics;
use crate::error::{Result, TrainctlError};
use crate::resource_tracking::ResourceUsage as TrackedResourceUsage;
use aws_config::BehaviorVersion;
use aws_sdk_ec2::Client as Ec2Client;
use aws_sdk_ssm::Client as SsmClient;
use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table, Tabs},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};

struct DashboardState {
    selected_tab: usize,
    selected_instance: Option<String>,
    last_update: Instant,
    update_interval: Duration,
    instances: Vec<InstanceInfo>,
    processes: Vec<ProcessInfo>,
    total_cost: f64,
    running_count: usize,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            selected_tab: 0,
            selected_instance: None,
            last_update: Instant::now(),
            update_interval: Duration::from_secs(5),
            instances: Vec::new(),
            processes: Vec::new(),
            total_cost: 0.0,
            running_count: 0,
        }
    }
}

struct InstanceInfo {
    id: String,
    instance_type: String,
    state: String,
    cost_per_hour: f64,
    runtime: String,
    accumulated_cost: f64,
    cpu_usage: f64,
    memory_usage: f64,
    gpu_usage: Option<f64>,
}

struct ProcessInfo {
    pid: String,
    user: String,
    cpu: f64,
    memory: f64,
    command: String,
    #[allow(dead_code)]
    instance_id: Option<String>,
}

pub async fn run_dashboard(config: &Config, update_interval_secs: u64) -> Result<()> {
    let mut terminal = init_terminal()?;
    let mut state = DashboardState {
        update_interval: Duration::from_secs(update_interval_secs),
        ..Default::default()
    };

    loop {
        // Update data
        update_state(&mut state, config).await?;

        // Render
        terminal.draw(|f| render_dashboard(f, &state))?;

        // Handle input
        if crossterm::event::poll(state.update_interval)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('h') | KeyCode::Left => {
                            state.selected_tab = state.selected_tab.saturating_sub(1);
                        }
                        KeyCode::Char('l') | KeyCode::Right => {
                            state.selected_tab = (state.selected_tab + 1).min(3);
                        }
                        KeyCode::Up => {
                            // Navigate instances/processes
                        }
                        KeyCode::Down => {
                            // Navigate instances/processes
                        }
                        KeyCode::Char('r') => {
                            // Force refresh
                            state.last_update = Instant::now() - state.update_interval;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    restore_terminal(&mut terminal)?;
    Ok(())
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

async fn update_state(state: &mut DashboardState, config: &Config) -> Result<()> {
    let now = Instant::now();
    if now.duration_since(state.last_update) < state.update_interval {
        return Ok(());
    }

    // Load AWS instances
    let region_str = config
        .aws
        .as_ref()
        .map(|a| a.region.clone())
        .unwrap_or_else(|| "us-east-1".to_string());

    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_sdk_ec2::config::Region::new(region_str))
        .load()
        .await;
    let ec2_client = Ec2Client::new(&sdk_config);

    // Get running instances with retry logic
    use crate::retry::{ExponentialBackoffPolicy, RetryPolicy};
    let response = ExponentialBackoffPolicy::for_cloud_api()
        .execute_with_retry(|| async {
            ec2_client
                .describe_instances()
                .set_filters(Some(vec![aws_sdk_ec2::types::Filter::builder()
                    .name("instance-state-name")
                    .values("running")
                    .build()]))
                .send()
                .await
                .map_err(|e| TrainctlError::Aws(format!("Failed to describe instances: {}", e)))
        })
        .await?;

    let mut instances = Vec::new();
    let mut total_cost = 0.0;
    let mut running_count = 0;

    for reservation in response.reservations() {
        for instance in reservation.instances() {
            if let Some(instance_id) = instance.instance_id() {
                running_count += 1;
                let instance_type = instance
                    .instance_type()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                let state = instance
                    .state()
                    .and_then(|s| s.name())
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // Calculate runtime duration (needed for both cost and display)
                let launch_time = instance
                    .launch_time()
                    .and_then(|t| chrono::DateTime::from_timestamp(t.secs(), 0));
                let runtime_duration = if let Some(lt) = launch_time {
                    chrono::Utc::now() - lt
                } else {
                    chrono::TimeDelta::zero()
                };

                // Get costs from ResourceTracker if available, otherwise calculate
                let instance_id_string = instance_id.to_string();
                let (cost_per_hour, accumulated_cost) =
                    crate::utils::get_instance_cost_with_tracker(
                        config.resource_tracker.as_deref(),
                        &instance_id_string,
                        &instance_type,
                        launch_time,
                        state == "running",
                    )
                    .await;
                total_cost += accumulated_cost;

                // Get resource usage (async, but don't block on errors)
                let (cpu_usage, memory_usage, gpu_usage) = if state == "running" {
                    let usage_result = get_instance_usage(&sdk_config, instance_id).await;

                    // Update ResourceTracker with usage data if available
                    if let (Ok((cpu, mem, gpu)), Some(tracker)) =
                        (&usage_result, &config.resource_tracker)
                    {
                        let usage = TrackedResourceUsage {
                            cpu_percent: *cpu,
                            memory_mb: *mem * 1024.0, // Convert GB to MB (assuming mem is in GB)
                            gpu_utilization: *gpu,
                            network_in_mb: 0.0,  // Not tracked yet
                            network_out_mb: 0.0, // Not tracked yet
                            timestamp: Utc::now(),
                        };
                        if let Err(e) = tracker.update_usage(&instance_id_string, usage).await {
                            tracing::debug!("Failed to update usage in tracker: {}", e);
                        }
                    }

                    usage_result.unwrap_or((0.0, 0.0, None))
                } else {
                    (0.0, 0.0, None)
                };

                instances.push(InstanceInfo {
                    id: instance_id.to_string(),
                    instance_type,
                    state,
                    cost_per_hour,
                    runtime: format_runtime(runtime_duration),
                    accumulated_cost,
                    cpu_usage,
                    memory_usage,
                    gpu_usage,
                });
            }
        }
    }

    // Use ResourceTracker total cost if available
    let final_total_cost = if let Some(tracker) = &config.resource_tracker {
        tracker.get_total_cost().await
    } else {
        total_cost
    };

    state.instances = instances;
    state.total_cost = final_total_cost;
    state.running_count = running_count;
    state.last_update = now;

    // Load processes for selected instance
    if let Some(instance_id) = &state.selected_instance {
        state.processes = get_instance_processes(&sdk_config, instance_id)
            .await
            .unwrap_or_default();
    }

    Ok(())
}

async fn get_instance_usage(
    sdk_config: &aws_config::SdkConfig,
    instance_id: &str,
) -> Result<(f64, f64, Option<f64>)> {
    // Use diagnostics module to get resource usage
    let ssm_client = SsmClient::new(sdk_config);
    let usage = diagnostics::get_instance_resource_usage(&ssm_client, instance_id).await?;

    let gpu_usage = usage
        .gpu_info
        .as_ref()
        .and_then(|gpu| gpu.gpus.first())
        .map(|gpu| gpu.utilization_percent);

    Ok((usage.cpu_percent, usage.memory_percent, gpu_usage))
}

async fn get_instance_processes(
    sdk_config: &aws_config::SdkConfig,
    instance_id: &str,
) -> Result<Vec<ProcessInfo>> {
    // Get top processes via SSM
    let ssm_client = SsmClient::new(sdk_config);

    let command = r"ps aux --sort=-%cpu | head -20 | awk '{print $2,$1,$3,$4,$11}'";
    let output = aws_utils::execute_ssm_command(&ssm_client, instance_id, command).await?;

    let mut processes = Vec::new();
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            processes.push(ProcessInfo {
                pid: parts[0].to_string(),
                user: parts[1].to_string(),
                cpu: parts[2].parse().unwrap_or(0.0),
                memory: parts[3].parse().unwrap_or(0.0),
                command: parts[4..].join(" "),
                instance_id: Some(instance_id.to_string()),
            });
        }
    }

    Ok(processes)
}

fn format_runtime(duration: chrono::Duration) -> String {
    let hours = duration.num_hours();
    let minutes = duration.num_minutes() % 60;
    let seconds = duration.num_seconds() % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn render_dashboard(f: &mut Frame, state: &DashboardState) {
    let size = f.size();

    // Tabs
    let tabs = Tabs::new(vec!["Overview", "Instances", "Processes", "Costs"])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("runctl Dashboard"),
        )
        .select(state.selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(size);

    f.render_widget(tabs, chunks[0]);

    match state.selected_tab {
        0 => render_overview(f, chunks[1], state),
        1 => render_instances(f, chunks[1], state),
        2 => render_processes(f, chunks[1], state),
        3 => render_costs(f, chunks[1], state),
        _ => {}
    }
}

fn render_overview(f: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

    // Summary
    let summary = Paragraph::new(vec![Line::from(vec![
        Span::styled("Running: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{} instances", state.running_count),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" | "),
        Span::styled("Total Cost: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("${:.2}", state.total_cost),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" | "),
        Span::styled("Last Update: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{}s ago", state.last_update.elapsed().as_secs()),
            Style::default().fg(Color::White),
        ),
    ])])
    .block(Block::default().borders(Borders::ALL).title("Summary"));

    f.render_widget(summary, chunks[0]);

    // Cost gauge (show as percentage of $100/day budget)
    let daily_estimate = state.total_cost * 24.0;
    let cost_percent = ((daily_estimate.min(100.0) / 100.0) * 100.0) as u16;
    let cost_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Daily Cost Estimate"),
        )
        .gauge_style(Style::default().fg(Color::Yellow))
        .percent(cost_percent)
        .label(format!("${:.2}/day est.", daily_estimate));

    f.render_widget(cost_gauge, chunks[1]);

    // Quick instance list
    let rows: Vec<Row> = state
        .instances
        .iter()
        .take(10)
        .map(|inst| {
            Row::new(vec![
                Cell::from(inst.id.clone()),
                Cell::from(inst.instance_type.clone()),
                Cell::from(inst.state.clone()),
                Cell::from(format!("${:.2}/h", inst.cost_per_hour)),
                Cell::from(format!("${:.2}", inst.accumulated_cost)),
                Cell::from(inst.runtime.clone()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(20),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(15),
    ];
    let table = Table::new(rows, widths)
        .block(Block::default().borders(Borders::ALL).title("Instances"))
        .header(
            Row::new(vec!["ID", "Type", "State", "Cost/h", "Total", "Runtime"]).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        );

    f.render_widget(table, chunks[2]);
}

fn render_instances(f: &mut Frame, area: Rect, state: &DashboardState) {
    let rows: Vec<Row> = state
        .instances
        .iter()
        .map(|inst| {
            Row::new(vec![
                Cell::from(inst.id.clone()),
                Cell::from(inst.instance_type.clone()),
                Cell::from(inst.state.clone()),
                Cell::from(format!("{:.1}%", inst.cpu_usage)),
                Cell::from(format!("{:.1}%", inst.memory_usage)),
                Cell::from(
                    inst.gpu_usage
                        .map(|g| format!("{:.1}%", g))
                        .unwrap_or_else(|| "N/A".to_string()),
                ),
                Cell::from(format!("${:.2}/h", inst.cost_per_hour)),
                Cell::from(format!("${:.2}", inst.accumulated_cost)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(20),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Instances (Press 'r' to refresh)"),
        )
        .header(
            Row::new(vec![
                "ID", "Type", "State", "CPU", "Mem", "GPU", "Cost/h", "Total",
            ])
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        );

    f.render_widget(table, area);
}

fn render_processes(f: &mut Frame, area: Rect, state: &DashboardState) {
    if state.processes.is_empty() {
        let msg = if state.selected_instance.is_some() {
            "No processes found. Select an instance first."
        } else {
            "Select an instance to view processes (navigate with arrow keys)"
        };

        let paragraph =
            Paragraph::new(msg).block(Block::default().borders(Borders::ALL).title("Processes"));
        f.render_widget(paragraph, area);
        return;
    }

    let rows: Vec<Row> = state
        .processes
        .iter()
        .map(|proc| {
            Row::new(vec![
                Cell::from(proc.pid.clone()),
                Cell::from(proc.user.clone()),
                Cell::from(format!("{:.1}%", proc.cpu)),
                Cell::from(format!("{:.1}%", proc.memory)),
                Cell::from(proc.command.clone()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Min(30),
    ];
    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Top Processes (like top)"),
        )
        .header(
            Row::new(vec!["PID", "User", "CPU%", "Mem%", "Command"]).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        );

    f.render_widget(table, area);
}

fn render_costs(f: &mut Frame, area: Rect, state: &DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    // Total cost
    let total = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Total Accumulated Cost: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("${:.2}", state.total_cost),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Estimated Daily Cost: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("${:.2}", state.total_cost * 24.0),
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("Cost Summary"));

    f.render_widget(total, chunks[0]);

    // Hourly cost
    let hourly: f64 = state.instances.iter().map(|i| i.cost_per_hour).sum();
    let hourly_para = Paragraph::new(vec![Line::from(vec![
        Span::styled("Current Hourly Rate: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("${:.2}/hour", hourly),
            Style::default().fg(Color::Green),
        ),
    ])])
    .block(Block::default().borders(Borders::ALL).title("Current Rate"));

    f.render_widget(hourly_para, chunks[1]);

    // Cost breakdown by instance
    let rows: Vec<Row> = state
        .instances
        .iter()
        .map(|inst| {
            Row::new(vec![
                Cell::from(inst.id.clone()),
                Cell::from(inst.instance_type.clone()),
                Cell::from(format!("${:.2}/h", inst.cost_per_hour)),
                Cell::from(format!("${:.2}", inst.accumulated_cost)),
                Cell::from(inst.runtime.clone()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(20),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(15),
    ];
    let table = Table::new(rows, widths)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Cost Breakdown"),
        )
        .header(
            Row::new(vec!["Instance", "Type", "Rate", "Accumulated", "Runtime"]).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        );

    f.render_widget(table, chunks[2]);
}
