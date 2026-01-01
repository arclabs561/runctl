# Auto-Stop Lifecycle Gap Analysis

## Current Implementation

The `auto_stop` and `auto_terminate` flags provide automatic instance lifecycle management based on training completion:

1. **Monitoring**: `check_training_completion()` checks:
   - S3 output files (completion indicators, expected outputs)
   - Process status (PID file, process running)
   - Log file patterns (completion keywords)

2. **Auto-cleanup**: `monitor_and_cleanup()` automatically:
   - Stops instance when training completes (`--auto-stop`)
   - Terminates instance when training completes (`--auto-terminate`)

## The Gap

**Problem**: When `auto_stop` or `auto_terminate` triggers, it calls `stop_instance()` or `terminate_instance()` directly. These functions:

- ✅ Send SIGTERM for graceful shutdown (allows training script to save checkpoint)
- ❌ **Do NOT explicitly save checkpoints before stopping**
- ❌ **Do NOT upload checkpoints to S3 before stopping**
- ❌ **Do NOT verify checkpoints were saved**

**Risk**: If the training script doesn't handle SIGTERM properly, or if checkpoint saving fails, checkpoints may be lost when the instance stops/terminates.

## Comparison with Spot Interruption Handling

Spot interruption handling (`src/aws/spot_monitor.rs`) does checkpoint saving:

```rust
// Step 1: Save checkpoint (if training is running)
// - Sends SIGTERM
// - Waits for graceful shutdown
// - Finds latest checkpoint
// - Uploads to S3
```

But `auto_stop`/`auto_terminate` don't do this - they rely on:
1. Training script handling SIGTERM (which `stop_instance` sends)
2. Training script saving checkpoints (not guaranteed)
3. Checkpoints being in the right place (not verified)

## Recommended Fix

Add checkpoint saving to `monitor_and_cleanup()` before calling `stop_instance()` or `terminate_instance()`:

```rust
// Before stopping/terminating:
// 1. Ensure training has saved final checkpoint
// 2. Upload checkpoint to S3 (if configured)
// 3. Verify checkpoint exists
// 4. Then stop/terminate
```

This would make `auto_stop`/`auto_terminate` as robust as spot interruption handling.

## Intersection with Lifecycle Management

The `auto_stop`/`auto_terminate` features are lifecycle management features that:

- ✅ Manage instance lifecycle based on training state
- ✅ Provide automatic cleanup after completion
- ❌ Missing: Checkpoint saving before cleanup
- ❌ Missing: Resume capability after `auto_stop` (if instance is restarted)
- ❌ Missing: State persistence (what was training, where are checkpoints)

These gaps align with the lifecycle management analysis in `LIFECYCLE_MANAGEMENT_ANALYSIS.md`.

