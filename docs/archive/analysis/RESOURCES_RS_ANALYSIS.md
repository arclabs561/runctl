# resources.rs Structure Analysis

**Status**: ✅ **COMPLETED** - This file has been split into `src/resources/` module structure.

See `docs/RESOURCES_SPLIT_COMPLETE.md` for the current structure.

## Historical Overview (Pre-Split)
- **Total Lines**: 2287
- **Purpose**: Resource listing, management, and reporting across AWS, RunPod, and local platforms

## Logical Module Boundaries

### 1. **Command Handling & Entry Points** (~200 lines)
- `ResourceCommands` enum (CLI command definitions)
- `handle_command` function (main dispatcher)
- `show_quick_status` (quick status overview)

### 2. **Data Structures & Types** (~150 lines)
- `ResourceSummary`, `AwsInstance`, `RunPodPod`, `LocalProcess`
- `ListResourcesOptions`, `ListAwsInstancesOptions`
- `InstanceInfo` struct

### 3. **JSON/Serialization Functions** (~200 lines)
- `get_resource_summary_json`
- `list_aws_instances_json`
- `list_runpod_pods_json`
- `list_local_processes_json`

### 4. **AWS Resource Listing** (~450 lines)
- `list_aws_instances` (main listing function)
- `display_table_format` (table rendering)
- `sync_resource_tracker_with_aws` (tracker synchronization)
- AWS-specific filtering, sorting, formatting

### 5. **RunPod Resource Listing** (~100 lines)
- `list_runpod_pods` (RunPod pod listing)

### 6. **Local Process Listing** (~70 lines)
- `list_local_processes` (local process detection)

### 7. **Summary & Reporting** (~250 lines)
- `show_summary` (resource summary with costs)
- `show_insights` (recommendations and insights)

### 8. **Export Functions** (~200 lines)
- `export_resources` (export to CSV/HTML/JSON)
- `generate_csv` (CSV generation)
- `generate_html` (HTML generation)

### 9. **Watch Mode** (~50 lines)
- `list_resources_watch` (continuous monitoring)

### 10. **Cleanup Operations** (~200 lines)
- `cleanup_zombies` (orphaned resource cleanup)
- `stop_all_instances` (bulk stop operation)

### 11. **Utility Functions** (~50 lines)
- `estimate_instance_cost` (cost estimation)

## Proposed Module Structure

```
src/resources/
├── mod.rs              # Command enum, handle_command, re-exports
├── types.rs            # Data structures (ResourceSummary, AwsInstance, etc.)
├── json.rs             # JSON serialization functions
├── aws.rs              # AWS-specific listing and operations
├── runpod.rs           # RunPod-specific listing
├── local.rs            # Local process listing
├── summary.rs          # Summary and insights
├── export.rs           # Export functions (CSV, HTML)
├── watch.rs            # Watch mode
├── cleanup.rs          # Cleanup operations
└── utils.rs            # Utility functions
```

## Dependencies
- AWS SDK (EC2 client)
- ResourceTracker
- Config
- Checkpoint module
- Utils (cost calculation, formatting)

## Notes
- Heavy use of AWS SDK for EC2 operations
- Integration with ResourceTracker for cost tracking
- Multiple output formats (text, JSON, CSV, HTML)
- Watch mode for continuous monitoring
- Cross-platform resource aggregation (AWS, RunPod, local)

