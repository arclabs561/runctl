# Detailed Command-by-Command Review

**Date**: 2025-01-XX  
**Scope**: Comprehensive review of every command and subcommand for design, integration, and nuances

## Review Methodology

This review systematically examines:
1. **Design Consistency**: Argument patterns, naming, help text quality
2. **Integration**: How commands work together, shared state, dependencies
3. **Nuances**: Edge cases, implicit behaviors, non-obvious interactions
4. **Best Practices**: Alignment with clap conventions and CLI design principles

---

## 1. AWS Commands (`runctl aws`)

### 1.1 `aws create`

**Design Analysis:**
- ✅ Good: Comprehensive help text with examples
- ✅ Good: Uses `value_name` for all arguments
- ✅ Good: Clear flag descriptions
- ⚠️ Issue: Default `project_name` is hardcoded to `"matryoshka-box"` instead of deriving from current directory
- ⚠️ Issue: `spot`, `spot_max_price`, `no_fallback` are flags (good), but help text could be clearer about interaction
- ✅ Good: JSON output support implemented

**Integration:**
- ✅ Creates instances with tags (`runctl:project`, `runctl:created`, `runctl:user`)
- ✅ Integrates with EBS for data volumes (`--data-volume-size`)
- ✅ Integrates with SSM via `--iam-instance-profile`
- ⚠️ Issue: Doesn't check if EBS volumes exist before creating data volume
- ⚠️ Issue: Doesn't validate security group exists
- ⚠️ Issue: Doesn't validate key pair exists (if provided)

**Nuances:**
- **Spot fallback**: Falls back to on-demand by default unless `--no-fallback` is set. This is good but could be more explicit in help text.
- **AMI auto-detection**: Automatically detects Deep Learning AMI for GPU instances. Good, but silent failure if not found (falls back to default).
- **Root volume sizing**: Auto-increases for GPU instances (50GB vs 30GB). Good, but not documented in help.
- **Safety check**: Blocks creation if 50+ instances running. Good safety feature, but hardcoded limit.
- **EBS optimization**: Automatically enabled. Good, but not mentioned in help text.

**Recommendations:**
1. Derive `project_name` from current directory name by default
2. Add validation for `--key-name` and `--security-group` if provided
3. Document root volume auto-sizing in help text
4. Make instance limit configurable
5. Add `--no-ebs-optimized` flag if user wants to disable

### 1.2 `aws train`

**Design Analysis:**
- ✅ Good: Clear help text with examples
- ✅ Good: Uses `#[arg(last = true)]` for script args (correct pattern)
- ⚠️ Issue: `_output_s3` is unused (marked with `#[allow(dead_code)]` but should be implemented or removed)
- ✅ Good: JSON output support

**Integration:**
- ✅ Integrates with code syncing (`--sync-code`)
- ✅ Integrates with S3 for data pre-loading (`--data-s3`)
- ⚠️ Issue: `--output-s3` is defined but not implemented
- ✅ Integrates with SSM/SSH for command execution
- ⚠️ Issue: Doesn't verify instance is in `running` state before training

**Nuances:**
- **Code syncing**: Uses native Rust SSH (`ssh2-rs`) by default, falls back to SSM if available. Good, but behavior not documented.
- **Data pre-loading**: Downloads from S3 before training starts. Good feature, but path resolution could be clearer.
- **Script execution**: Runs in background with `nohup`. Good, but no way to see immediate output.
- **Project directory**: Uses `--project-name` to determine remote directory. Good, but should match `aws create` default.

**Recommendations:**
1. Implement `--output-s3` or remove it
2. Add instance state validation (must be `running`)
3. Document code syncing behavior (SSH vs SSM)
4. Add `--foreground` flag for immediate output
5. Verify project directory exists on instance before syncing

### 1.3 `aws monitor`

**Design Analysis:**
- ✅ Good: Simple, clear interface
- ✅ Good: `--follow` flag for continuous monitoring
- ❌ Issue: No JSON output support (streaming commands are harder, but could support JSONL)
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Uses SSM or SSH to tail logs
- ⚠️ Issue: Doesn't verify instance exists before monitoring
- ⚠️ Issue: Doesn't check if log file exists

**Nuances:**
- **Log location**: Assumes logs are in standard location. Could be more flexible.
- **SSM vs SSH**: Uses SSM if available, falls back to SSH. Good, but not documented.

**Recommendations:**
1. Add `--log-path` option to specify custom log location
2. Add instance validation
3. Add JSONL output support for programmatic use
4. Add examples to help text

### 1.4 `aws stop`

**Design Analysis:**
- ✅ Good: Clear purpose (pause vs terminate)
- ✅ Good: `--force` flag for safety bypass
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Checks for running training jobs (safety feature)
- ⚠️ Issue: Safety check might be too strict (what if training is actually done?)

**Nuances:**
- **Safety check**: Blocks stop if training processes detected. Good, but might be too conservative.
- **Volume preservation**: Volumes are preserved (good, but not mentioned in help).

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Document volume preservation behavior
4. Make safety check more intelligent (check if training is actually running vs just processes)

### 1.5 `aws terminate`

**Design Analysis:**
- ✅ Good: Clear aliases (`destroy`, `rm`, `delete`)
- ✅ Good: `--force` flag for safety bypass
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Checks for running training jobs (safety feature)
- ⚠️ Issue: Doesn't handle attached EBS volumes (are they deleted or preserved?)
- ⚠️ Issue: Doesn't clean up tags or other metadata

**Nuances:**
- **Volume handling**: Behavior depends on volume `DeleteOnTermination` setting. Not documented.
- **Safety check**: Same as `stop` - might be too conservative.

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Document volume deletion behavior
4. Add `--delete-volumes` flag for explicit control
5. Show what will be deleted before termination

### 1.6 `aws processes`

**Design Analysis:**
- ✅ Good: `--watch` and `--detailed` flags
- ✅ Good: Configurable refresh interval
- ❌ Issue: No JSON output support
- ✅ Good: Help text with examples

**Integration:**
- ✅ Uses SSM to execute `ps` commands
- ⚠️ Issue: Doesn't verify instance exists or is running

**Nuances:**
- **Process detection**: Uses `ps aux` via SSM. Good, but platform-specific.
- **Resource usage**: Shows CPU, memory, disk, GPU. Good feature.

**Recommendations:**
1. Add JSON output support (especially useful for `--watch` mode)
2. Add instance state validation
3. Add process filtering options (e.g., `--filter python`)

### 1.7 `aws ebs`

**Design Analysis:**
- ✅ Good: Nested subcommand structure
- ⚠️ Issue: EBS commands don't receive `output_format` parameter (inconsistent with other commands)
- ⚠️ Issue: EBS is conceptually separate from EC2 but nested under `aws`

**Integration:**
- ✅ EBS commands can work with instances created by `aws create`
- ⚠️ Issue: EBS commands don't share the same AWS client initialization pattern

**Nuances:**
- **Nesting**: EBS volumes are independent resources but nested under `aws`. This is a design choice that could be reconsidered.

**Recommendations:**
1. Add `output_format` parameter to `ebs::handle_command`
2. Consider making `ebs` a top-level command
3. Standardize AWS client initialization

---

## 2. EBS Commands (`runctl aws ebs`)

### 2.1 `ebs create`

**Design Analysis:**
- ✅ Excellent: Comprehensive help text with volume type descriptions
- ✅ Good: `--use-case` for auto-optimization
- ✅ Good: All arguments have `value_name`
- ❌ Issue: No JSON output support
- ✅ Good: Examples in help text

**Integration:**
- ✅ Integrates with `ebs_optimization` module for IOPS/throughput calculation
- ⚠️ Issue: Doesn't validate availability zone exists
- ⚠️ Issue: Doesn't check if volume name already exists

**Nuances:**
- **Use case optimization**: Auto-calculates IOPS/throughput based on use case. Good feature, but could be more prominent in help.
- **Volume type recommendations**: Help text includes recommendations, but not interactive.

**Recommendations:**
1. Add JSON output support
2. Add availability zone validation
3. Add volume name uniqueness check
4. Make use case optimization more prominent

### 2.2 `ebs list`

**Design Analysis:**
- ✅ Good: Simple interface
- ✅ Good: `--detailed` flag
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Lists volumes that can be used with instances
- ⚠️ Issue: Doesn't show which volumes are attached to which instances

**Nuances:**
- **Filtering**: Only supports `--name` filter. Could be more flexible.

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add more filtering options (state, type, size, etc.)
4. Show attachment information

### 2.3 `ebs attach`

**Design Analysis:**
- ✅ Good: Clear help text with examples
- ✅ Good: Input validation for volume and instance IDs
- ❌ Issue: No JSON output support
- ✅ Good: Default device name

**Integration:**
- ✅ Works with instances created by `aws create`
- ⚠️ Issue: Doesn't verify instance is in correct state (can attach to stopped instances, but not documented)
- ⚠️ Issue: Doesn't verify volume is in `available` state

**Nuances:**
- **Device naming**: Uses `/dev/sdf` by default. Good, but could detect next available device.
- **Availability zone**: Must match instance AZ. Validated, but error message could be clearer.

**Recommendations:**
1. Add JSON output support
2. Add instance and volume state validation
3. Auto-detect next available device name
4. Improve error messages for AZ mismatches

### 2.4 `ebs detach`

**Design Analysis:**
- ✅ Good: Clear help text
- ✅ Good: `--force` flag for stopped instances
- ❌ Issue: No JSON output support
- ✅ Good: Examples

**Integration:**
- ✅ Works with attached volumes
- ⚠️ Issue: Doesn't check if volume is in use (could cause data loss)

**Nuances:**
- **Force detach**: Required for stopped instances. Good, but behavior not fully documented.

**Recommendations:**
1. Add JSON output support
2. Add safety check for volumes in use
3. Document force behavior more clearly

### 2.5 `ebs delete`

**Design Analysis:**
- ✅ Good: Clear warning in help text
- ✅ Good: `--force` flag
- ❌ Issue: No JSON output support
- ✅ Good: Examples

**Integration:**
- ✅ Works with detached volumes
- ⚠️ Issue: Doesn't check for snapshots (should warn if snapshots exist)

**Nuances:**
- **Permanent deletion**: Clearly documented. Good.

**Recommendations:**
1. Add JSON output support
2. Add snapshot check and warning
3. Add confirmation prompt (unless `--force`)

### 2.6 `ebs pre-warm`

**Design Analysis:**
- ✅ Good: Clear purpose
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Integrates with S3 for data source
- ✅ Creates temporary instance for pre-warming
- ⚠️ Issue: Doesn't verify S3 path exists

**Nuances:**
- **Temporary instance**: Creates and destroys instance automatically. Good, but could be more explicit.

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add S3 path validation
4. Document temporary instance behavior

### 2.7 `ebs snapshot`

**Design Analysis:**
- ✅ Good: Simple interface
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with volumes
- ⚠️ Issue: Doesn't verify volume exists or is in correct state

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add volume validation

### 2.8 `ebs restore`

**Design Analysis:**
- ✅ Good: `--use-case` for optimization
- ❌ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with snapshots
- ⚠️ Issue: Doesn't verify snapshot exists

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add snapshot validation

---

## 3. S3 Commands (`runctl s3`)

### 3.1 `s3 upload`

**Design Analysis:**
- ✅ Good: Help text with examples
- ✅ Good: `--use-s5cmd` flag (though default is native Rust now)
- ✅ Good: JSON output support
- ⚠️ Issue: `--use-s5cmd` default is `false` but help text might not make this clear

**Integration:**
- ✅ Uses native Rust S3 operations by default
- ✅ Falls back to s5cmd if requested
- ⚠️ Issue: Doesn't verify S3 bucket exists or is accessible

**Nuances:**
- **Native vs s5cmd**: Native Rust is default (good for performance), but s5cmd might be faster for very large transfers. Not documented.

**Recommendations:**
1. Clarify in help text that native Rust is default
2. Add S3 bucket validation
3. Add progress indication for large uploads

### 3.2 `s3 download`

**Design Analysis:**
- ✅ Good: Similar to upload (consistent)
- ✅ Good: JSON output support
- ⚠️ Issue: Same as upload - help text clarity

**Integration:**
- ✅ Same as upload

**Recommendations:**
- Same as upload

### 3.3 `s3 sync`

**Design Analysis:**
- ✅ Good: Clear direction options
- ✅ Good: JSON output support
- ⚠️ Issue: `direction` default is `"up"` but not clearly documented

**Integration:**
- ✅ Works with local directories and S3 paths
- ⚠️ Issue: Doesn't verify paths exist

**Nuances:**
- **Direction**: `up` = local→S3, `down` = S3→local, `both` = bidirectional. Good, but could be clearer.

**Recommendations:**
1. Clarify direction options in help text
2. Add path validation
3. Add dry-run mode

### 3.4 `s3 list`

**Design Analysis:**
- ✅ Good: Simple interface
- ✅ Good: JSON output support
- ✅ Good: `--recursive` and `--human-readable` flags

**Integration:**
- ✅ Works with any S3 path
- ⚠️ Issue: Doesn't verify S3 path exists

**Recommendations:**
1. Add path validation
2. Add filtering options (by prefix, date, size, etc.)

### 3.5 `s3 cleanup`

**Design Analysis:**
- ✅ Good: `--dry-run` flag
- ✅ Good: JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with checkpoint directories
- ⚠️ Issue: Doesn't verify S3 path exists

**Recommendations:**
1. Add examples to help text
2. Add path validation
3. Add more cleanup strategies (by age, by size, etc.)

### 3.6 `s3 watch`

**Design Analysis:**
- ✅ Good: Simple interface
- ❌ Issue: No JSON output support (streaming, but could support JSONL)
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Monitors S3 paths
- ⚠️ Issue: Doesn't verify S3 path exists

**Recommendations:**
1. Add JSONL output support
2. Add examples to help text
3. Add path validation

### 3.7 `s3 review`

**Design Analysis:**
- ✅ Good: `--detailed` flag
- ✅ Good: JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Analyzes S3 paths
- ⚠️ Issue: Doesn't verify S3 path exists

**Recommendations:**
1. Add examples to help text
2. Add path validation

---

## 4. Checkpoint Commands (`runctl checkpoint`)

### 4.1 `checkpoint list`

**Design Analysis:**
- ✅ Good: Simple interface
- ✅ Good: JSON output support
- ⚠️ Issue: No help text examples (but has them in code)

**Integration:**
- ✅ Works with local directories
- ⚠️ Issue: Doesn't integrate with S3 checkpoints (only local)
- ⚠️ Issue: Doesn't verify directory exists (returns empty list, which is fine)

**Nuances:**
- **File extension**: Only looks for `.pt` files. Could be configurable.

**Recommendations:**
1. Add examples to help text (they exist in code but not in clap attributes)
2. Add `--extension` option for custom file types
3. Add S3 integration for remote checkpoints

### 4.2 `checkpoint info`

**Design Analysis:**
- ✅ Good: Simple interface
- ✅ Good: JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with checkpoint files
- ⚠️ Issue: Doesn't verify file exists (fails with generic error)

**Recommendations:**
1. Add examples to help text
2. Add file existence validation with helpful error

### 4.3 `checkpoint resume`

**Design Analysis:**
- ✅ Good: Clear purpose
- ⚠️ Issue: No JSON output support (though less critical for this command)
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Executes training script with checkpoint path
- ⚠️ Issue: Doesn't verify checkpoint or script exists

**Recommendations:**
1. Add examples to help text
2. Add file existence validation
3. Add `--output json` support for status

### 4.4 `checkpoint cleanup`

**Design Analysis:**
- ✅ Good: `--dry-run` flag
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with checkpoint directories
- ⚠️ Issue: Doesn't verify directory exists

**Recommendations:**
1. Add examples to help text
2. Add JSON output support
3. Add directory validation

---

## 5. Resources Commands (`runctl resources`)

### 5.1 `resources list`

**Design Analysis:**
- ✅ Excellent: Comprehensive filtering and sorting options
- ✅ Good: `--watch` mode
- ✅ Good: JSON output support
- ✅ Good: Export options
- ⚠️ Issue: Many options, could be overwhelming

**Integration:**
- ✅ Integrates with AWS, RunPod, and local processes
- ✅ Uses tags from `aws create` for filtering
- ⚠️ Issue: Doesn't show EBS volumes (only instances)

**Nuances:**
- **Platform filtering**: Supports `aws`, `runpod`, `local`, `all`. Good.
- **State filtering**: Supports `running`, `stopped`, `terminated`, `all`. Good.
- **Sorting**: Supports `cost`, `age`, `type`, `state`. Good.

**Recommendations:**
1. Add EBS volume listing option
2. Add more sorting options (name, project, user)
3. Consider grouping options into categories

### 5.2 `resources summary`

**Design Analysis:**
- ✅ Good: Quick overview
- ✅ Good: JSON output support
- ⚠️ Issue: No help text (it's a simple command, but could have examples)

**Integration:**
- ✅ Aggregates data from all platforms
- ✅ Shows cost estimates

**Recommendations:**
1. Add help text with examples
2. Add more summary metrics (total instances, total cost per hour, etc.)

### 5.3 `resources cleanup`

**Design Analysis:**
- ✅ Good: `--dry-run` and `--force` flags
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Finds orphaned resources
- ⚠️ Issue: Doesn't clean up EBS volumes
- ⚠️ Issue: Doesn't clean up S3 objects

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add EBS volume cleanup
4. Add S3 object cleanup option

### 5.4 `resources stop-all`

**Design Analysis:**
- ✅ Good: `--dry-run` and `--force` flags
- ✅ Good: Platform filtering
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Works with all platforms
- ⚠️ Issue: Doesn't check for running training jobs

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add training job check (like `aws stop`)

### 5.5 `resources insights`

**Design Analysis:**
- ✅ Good: Provides recommendations
- ✅ Good: JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Analyzes all resources
- ✅ Provides cost optimization suggestions

**Recommendations:**
1. Add examples to help text
2. Add more insight types (performance, security, etc.)

---

## 6. Config Commands (`runctl config`)

### 6.1 `config show`

**Design Analysis:**
- ✅ Good: Simple interface
- ✅ Good: JSON output support (via `--output` flag, not global)
- ⚠️ Issue: Uses its own `output` field instead of global `--output`

**Integration:**
- ✅ Loads config from file or uses defaults
- ⚠️ Issue: Inconsistent with other commands (uses local `output` instead of global)

**Nuances:**
- **Output format**: Has its own `--output` flag instead of using global. This is inconsistent.

**Recommendations:**
1. Use global `--output` flag instead of local
2. Add examples to help text

### 6.2 `config set`

**Design Analysis:**
- ✅ Good: Dot notation for nested keys
- ⚠️ Issue: Limited key support (only a few keys are supported)
- ⚠️ Issue: No help text examples showing dot notation

**Integration:**
- ✅ Writes to config file
- ⚠️ Issue: Doesn't validate values before writing

**Nuances:**
- **Key support**: Only supports a few keys. Should support all config fields.

**Recommendations:**
1. Add support for all config fields
2. Add value validation
3. Add examples to help text showing dot notation

### 6.3 `config validate`

**Design Analysis:**
- ✅ Good: Simple interface
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Validates config file
- ⚠️ Issue: Doesn't validate against schema (only TOML parsing)

**Recommendations:**
1. Add JSON output support
2. Add examples to help text
3. Add schema validation

---

## 7. Other Commands

### 7.1 `local train`

**Design Analysis:**
- ✅ Good: Simple interface
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text

**Integration:**
- ✅ Uses config for checkpoint directory
- ✅ Detects Python scripts and uses `uv` if available

**Recommendations:**
1. Add help text
2. Add JSON output support for status
3. Add examples

### 7.2 `monitor`

**Design Analysis:**
- ✅ Good: Simple interface
- ⚠️ Issue: No JSON output support
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Monitors local logs and checkpoints
- ⚠️ Issue: Doesn't integrate with AWS monitoring

**Recommendations:**
1. Add help text examples
2. Add JSONL output support
3. Add integration with AWS monitoring

### 7.3 `top` (dashboard)

**Design Analysis:**
- ✅ Good: Interactive TUI
- ✅ Good: Configurable refresh interval
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Shows resources from all platforms
- ✅ Shows processes and costs

**Recommendations:**
1. Add help text examples
2. Add keyboard shortcuts documentation

### 7.4 `init`

**Design Analysis:**
- ✅ Good: Simple interface
- ⚠️ Issue: No help text examples

**Integration:**
- ✅ Creates default config file

**Recommendations:**
1. Add help text examples

### 7.5 `transfer`

**Design Analysis:**
- ⚠️ Issue: Overlaps with `s3` commands
- ⚠️ Issue: No help text
- ⚠️ Issue: No JSON output support

**Integration:**
- ✅ Generic transfer interface
- ⚠️ Issue: Functionality overlaps with `s3 upload/download`

**Recommendations:**
1. Document when to use `transfer` vs `s3`
2. Add help text
3. Add JSON output support
4. Consider deprecating in favor of `s3` commands

### 7.6 `exec`

**Design Analysis:**
- ⚠️ Issue: No help text
- ⚠️ Issue: Purpose unclear (seems to duplicate `local train`)
- ⚠️ Issue: No JSON output support

**Integration:**
- ✅ Uses local training infrastructure

**Recommendations:**
1. Clarify purpose vs `local train`
2. Add help text
3. Add JSON output support
4. Consider removing if redundant

---

## 8. Cross-Cutting Issues

### 8.1 JSON Output Consistency

**Status**: ⚠️ Inconsistent

**Commands with JSON:**
- ✅ `aws create`
- ✅ `aws train`
- ✅ `s3 *` (all commands)
- ✅ `checkpoint list`
- ✅ `checkpoint info`
- ✅ `resources *` (all commands)
- ✅ `config show`

**Commands without JSON:**
- ❌ `aws monitor`
- ❌ `aws stop`
- ❌ `aws terminate`
- ❌ `aws processes`
- ❌ `ebs *` (all commands)
- ❌ `checkpoint resume`
- ❌ `checkpoint cleanup`
- ❌ `local train`
- ❌ `monitor`
- ❌ `transfer`
- ❌ `exec`
- ❌ `config set`
- ❌ `config validate`

**Recommendations:**
1. Add JSON output to all commands
2. Standardize JSON structure (always object with `success`, `data`, `message` fields)
3. Support JSONL for streaming commands

### 8.2 Error Handling Consistency

**Status**: ⚠️ Inconsistent

**Current State:**
- `aws.rs`: Uses `anyhow::Result`
- `s3.rs`: Uses `crate::error::Result`
- `checkpoint.rs`: Uses `crate::error::Result`
- `resources.rs`: Uses `anyhow::Result`
- `ebs.rs`: Uses `anyhow::Result`
- `config.rs`: Uses `crate::error::Result`

**Recommendations:**
1. Library code: Use `crate::error::Result`
2. CLI boundary: Use `anyhow::Result` with conversion
3. Standardize error conversion at boundaries

### 8.3 Input Validation Consistency

**Status**: ⚠️ Inconsistent

**Commands with validation:**
- ✅ `aws train` (instance ID)
- ✅ `aws terminate` (instance ID)
- ✅ `ebs attach` (volume/instance IDs)
- ✅ `ebs detach` (volume ID)
- ✅ `ebs delete` (volume ID)

**Commands without validation:**
- ❌ `s3 upload` (paths)
- ❌ `s3 download` (paths)
- ❌ `checkpoint list` (directory)
- ❌ `checkpoint info` (file path)
- ❌ `aws create` (key name, security group)

**Recommendations:**
1. Validate all inputs at command entry
2. Use `crate::validation` module consistently
3. Provide helpful error messages

### 8.4 Help Text Quality

**Status**: ⚠️ Inconsistent

**Commands with good help:**
- ✅ `aws create` (comprehensive)
- ✅ `aws train` (good)
- ✅ `ebs create` (excellent)
- ✅ `s3 *` (good)

**Commands with poor help:**
- ❌ `local train` (none)
- ❌ `monitor` (minimal)
- ❌ `transfer` (none)
- ❌ `exec` (none)
- ❌ Many subcommands missing examples

**Recommendations:**
1. Add help text to all commands
2. Add examples to all commands
3. Standardize help text format
4. Keep help text up to date

### 8.5 Configuration Loading

**Status**: ⚠️ Inconsistent

**Current State:**
- Some commands take `config: &Config` parameter
- Some commands load config internally
- Some commands don't use config at all

**Recommendations:**
1. Always pass `config: &Config` to handlers
2. Load config once in `main.rs`
3. Make config optional where appropriate

---

## 9. Integration Patterns

### 9.1 AWS + EBS Integration

**Current State:**
- ✅ `aws create` can create data volumes
- ✅ `ebs attach` works with instances
- ⚠️ Issue: EBS commands don't share AWS client initialization

**Recommendations:**
1. Centralize AWS client initialization
2. Add EBS volume listing to `resources list`
3. Add instance-aware EBS operations

### 9.2 AWS + S3 Integration

**Current State:**
- ✅ `aws train` can pre-load data from S3
- ⚠️ Issue: No automatic checkpoint upload to S3
- ⚠️ Issue: No coordination between S3 operations and instance lifecycle

**Recommendations:**
1. Add automatic checkpoint upload
2. Add S3 bucket validation before training
3. Add coordination for data pre-loading

### 9.3 Checkpoint + AWS Integration

**Current State:**
- ⚠️ Issue: Checkpoint commands only work with local files
- ⚠️ Issue: No way to list checkpoints on instances
- ⚠️ Issue: No way to download checkpoints from instances

**Recommendations:**
1. Add S3 checkpoint support
2. Add instance checkpoint listing
3. Add checkpoint download from instances

### 9.4 Resource Tracking Integration

**Current State:**
- ✅ Instances are tagged
- ⚠️ Issue: EBS volumes are not tagged
- ⚠️ Issue: S3 objects are not tagged
- ⚠️ Issue: Snapshots are not tagged

**Recommendations:**
1. Tag all resources consistently
2. Use tags for cost attribution
3. Use tags for cleanup operations

---

## 10. Priority Recommendations

### High Priority (Immediate)

1. **Add JSON output to all commands**
   - Especially `aws stop`, `aws terminate`, `aws processes`, `ebs *`
   - Standardize JSON structure

2. **Standardize error handling**
   - Use `crate::error::Result` in library code
   - Convert at CLI boundary

3. **Add input validation**
   - Validate all inputs at command entry
   - Use `crate::validation` module

4. **Fix `output_format` parameter inconsistency**
   - Add `output_format` to `ebs::handle_command`
   - Use global `--output` in `config show`

5. **Add help text to all commands**
   - Especially `local train`, `monitor`, `transfer`, `exec`
   - Add examples to all commands

### Medium Priority (Short-term)

6. **Centralize AWS client initialization**
   - Create shared `AwsClients` struct
   - Reduce duplication

7. **Improve integration between commands**
   - Add EBS volumes to `resources list`
   - Add S3 checkpoint support
   - Add instance checkpoint operations

8. **Standardize configuration loading**
   - Always pass `config: &Config` to handlers
   - Load once in `main.rs`

9. **Add resource tagging**
   - Tag EBS volumes
   - Tag S3 objects
   - Tag snapshots

10. **Improve error messages**
    - Always include context
    - Provide actionable suggestions
    - Use structured errors

### Low Priority (Long-term)

11. **Consider EBS command nesting**
    - Evaluate making `ebs` top-level
    - Document design decision

12. **Split large modules**
    - Split `aws.rs` into sub-modules
    - Split `ebs.rs` if needed

13. **Add more tests**
    - Unit tests for all modules
    - Integration tests for command combinations
    - E2E tests for workflows

14. **Improve documentation**
    - Document design decisions
    - Add architecture diagrams
    - Add workflow examples

15. **Add progress indicators**
    - For all long-running operations
    - Consistent UI

---

## 11. Summary

### Strengths
- ✅ Comprehensive command set
- ✅ Good help text for many commands
- ✅ JSON output for most important commands
- ✅ Safety features (instance limits, training checks)
- ✅ Good integration patterns (tags, project names)

### Weaknesses
- ❌ Inconsistent JSON output support
- ❌ Inconsistent error handling
- ❌ Inconsistent input validation
- ❌ Missing help text for some commands
- ❌ Inconsistent configuration loading
- ❌ Missing integration between some commands

### Key Findings
1. **Design**: Generally good, but inconsistent in details
2. **Integration**: Good for core workflows, but gaps exist
3. **Nuances**: Many implicit behaviors not documented
4. **Best Practices**: Mostly aligned, but room for improvement

### Next Steps
1. Address high-priority items first
2. Create issues for each recommendation
3. Track progress in this document
4. Re-review after fixes

