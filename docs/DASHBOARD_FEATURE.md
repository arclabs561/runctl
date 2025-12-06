# Interactive Dashboard Feature

**Status**: ✅ **Implemented**

## Overview

The dashboard provides a ratatui-based interactive interface for monitoring:
- Running resources (instances, pods, processes)
- Current costs and billing
- Process resource usage (top-like view)
- Real-time updates

## Usage

```bash
# Launch dashboard with default 5-second update interval
trainctl dashboard

# Custom update interval (in seconds)
trainctl dashboard --interval 10
```

## Features

### 1. Overview Tab
- Summary of running instances
- Total accumulated cost
- Daily cost estimate (gauge)
- Quick instance list with costs

### 2. Instances Tab
- Detailed instance information
- CPU, memory, GPU usage (if available)
- Cost per hour and accumulated costs
- Runtime for each instance

### 3. Processes Tab
- Top processes by CPU usage (like `top`)
- Shows PID, user, CPU%, memory%, command
- Select an instance to view its processes
- Real-time process monitoring

### 4. Costs Tab
- Total accumulated cost
- Estimated daily cost
- Current hourly rate
- Cost breakdown by instance

## Controls

- `q` or `Esc` - Quit dashboard
- `h` / `←` - Previous tab
- `l` / `→` - Next tab
- `r` - Force refresh
- `↑` / `↓` - Navigate instances/processes (future)

## Implementation Details

### Architecture
- Uses `ratatui` for TUI rendering
- `crossterm` for terminal control
- Async updates via tokio
- SSM commands for process/resource data

### Data Collection
- Instance list: EC2 `describe_instances`
- Resource usage: `diagnostics::get_instance_resource_usage`
- Process list: SSM `ps aux` command
- Costs: Calculated from instance types and runtime

### Performance
- Update interval configurable (default: 5s)
- Non-blocking resource usage collection
- Graceful error handling (shows 0.0% on failure)

## Future Enhancements

1. **Instance Selection**: Navigate and select instances to view details
2. **Process Filtering**: Filter processes by name, user, CPU threshold
3. **Historical Data**: Show cost trends over time
4. **Alerts**: Visual warnings for high costs or resource usage
5. **Multi-region**: Support for multiple AWS regions
6. **Export**: Export dashboard data to JSON/CSV

## Technical Notes

- Dashboard requires terminal with color support
- Uses alternate screen mode (full-screen)
- Restores terminal state on exit
- Handles terminal resize gracefully

