# Lifecycle Management Analysis

## Current State

### Spot Instance Lifecycle ✅ (Partially Implemented)

**What Exists:**
- ✅ Spot interruption detection via EC2 metadata service (`src/aws/spot_monitor.rs`)
- ✅ Graceful shutdown sequence (SIGTERM → wait → SIGKILL)
- ✅ Checkpoint saving before termination
- ✅ S3 checkpoint upload (if configured)
- ✅ Auto-resume capability (`src/aws/auto_resume.rs`)

**What's Missing:**
- ❌ **Automatic monitoring start**: Spot monitoring is NOT automatically started when training begins
- ❌ **Unified lifecycle management**: No unified abstraction for spot vs on-demand lifecycle
- ❌ **Lifecycle state machine**: No explicit state tracking (pending → running → interrupted → terminated)
- ❌ **Recovery orchestration**: Auto-resume exists but isn't well-integrated

### On-Demand Instance Lifecycle ⚠️ (Basic)

**What Exists:**
- ✅ Create, start, stop, terminate commands
- ✅ Resource tracking (ResourceTracker)
- ✅ Safe cleanup checks

**What's Missing:**
- ❌ **Graceful shutdown**: No automatic checkpoint saving on stop/terminate
- ❌ **Lifecycle hooks**: No pre-stop/pre-terminate hooks for checkpoint saving
- ❌ **State persistence**: No tracking of training state across stop/start cycles
- ❌ **Resume capability**: No automatic resume from checkpoints after restart

## Design Issues

### 1. Spot Monitoring Not Automatic

**Problem**: Spot monitoring exists but must be manually started. Users expect it to "just work" when training on spot instances.

**Current State**:
```rust
// src/aws/training.rs - train_on_instance()
// Does NOT automatically start spot monitoring
```

**Expected Behavior**:
```rust
// When training starts on a spot instance:
if is_spot_instance {
    start_spot_monitoring(...).await?;
}
```

### 2. No Unified Lifecycle Manager

**Problem**: Spot and on-demand instances have different lifecycle handling, but there's no unified abstraction.

**Current State**:
- Spot: Manual monitoring, manual recovery
- On-demand: Basic start/stop, no checkpoint management

**Needed**: Unified lifecycle manager that:
- Tracks instance state (pending → running → stopping → stopped → terminated)
- Handles checkpoint saving for BOTH spot interruptions AND manual stops
- Provides resume capability for both spot and on-demand
- Manages resource cleanup consistently

### 3. Missing Lifecycle Hooks

**Problem**: No way to execute actions at lifecycle transitions (pre-stop, pre-terminate, post-start).

**Current State**:
- Stop command: Just stops the instance, no checkpoint saving
- Terminate command: Just terminates, no checkpoint saving
- Start command: Just starts, no resume from checkpoint

**Needed**: Lifecycle hooks that:
- Save checkpoint before stop/terminate (for on-demand)
- Resume from checkpoint after start (for on-demand)
- Handle spot interruptions (already exists but not automatic)

### 4. No State Persistence

**Problem**: Training state (checkpoint location, training script, hyperparameters) is lost when instance stops.

**Current State**:
- No metadata about what was running
- No way to resume training after stop/start cycle
- No tracking of checkpoint locations

**Needed**: State persistence that:
- Stores training metadata (script, args, checkpoint location)
- Enables automatic resume after restart
- Tracks checkpoint history

## Proposed Solution

### Unified Lifecycle Manager

```rust
pub struct InstanceLifecycleManager {
    instance_id: String,
    instance_type: InstanceType, // Spot or OnDemand
    state: LifecycleState,
    training_metadata: Option<TrainingMetadata>,
    checkpoint_manager: CheckpointManager,
}

pub enum LifecycleState {
    Pending,
    Running { training_active: bool },
    Stopping { reason: StopReason },
    Stopped,
    Interrupted { checkpoint_saved: bool },
    Terminated,
}

pub enum InstanceType {
    Spot { max_price: Option<f64> },
    OnDemand,
}

pub struct TrainingMetadata {
    script_path: PathBuf,
    script_args: Vec<String>,
    checkpoint_dir: PathBuf,
    last_checkpoint: Option<PathBuf>,
}
```

### Lifecycle Transitions

1. **Create** → `Pending`
2. **Start Training** → `Running { training_active: true }`
   - If spot: Start monitoring automatically
   - Register with ResourceTracker
3. **Spot Interruption** → `Interrupted { checkpoint_saved: true }`
   - Save checkpoint
   - Upload to S3
   - Optionally auto-resume
4. **Manual Stop** → `Stopping { reason: Manual }`
   - Save checkpoint (if training active)
   - Stop instance → `Stopped`
5. **Start (from Stopped)** → `Running { training_active: false }`
   - Resume from checkpoint (if available)
6. **Terminate** → `Terminated`
   - Save checkpoint (if training active)
   - Clean up resources

### Automatic Behaviors

1. **Spot Monitoring**: Automatically start when training begins on spot instance
2. **Checkpoint Saving**: Automatically save before stop/terminate (both spot and on-demand)
3. **Resume**: Automatically resume from latest checkpoint after start (if available)
4. **State Tracking**: Track lifecycle state in ResourceTracker or separate state store

## Implementation Plan

### Phase 1: Automatic Spot Monitoring
- [ ] Modify `train_on_instance()` to automatically start spot monitoring
- [ ] Detect spot instance type
- [ ] Start monitoring in background task
- [ ] Handle monitoring errors gracefully

### Phase 2: Unified Lifecycle Manager
- [ ] Create `InstanceLifecycleManager` struct
- [ ] Implement state machine
- [ ] Add lifecycle hooks (pre-stop, pre-terminate, post-start)
- [ ] Integrate with existing commands

### Phase 3: Checkpoint Management
- [ ] Save checkpoint before stop/terminate (on-demand)
- [ ] Resume from checkpoint after start (on-demand)
- [ ] Track checkpoint locations
- [ ] Provide resume command

### Phase 4: State Persistence
- [ ] Store training metadata
- [ ] Track checkpoint history
- [ ] Enable automatic resume

## Questions to Answer

1. **Should on-demand instances also save checkpoints on stop?**
   - Yes: Enables resume after restart
   - Trade-off: Adds latency to stop command

2. **Should resume be automatic or manual?**
   - Automatic: Better UX, but may surprise users
   - Manual: More control, but requires user action
   - **Recommendation**: Make it opt-in with `--resume` flag

3. **Where should lifecycle state be stored?**
   - ResourceTracker: Already tracks resources
   - Separate state file: More explicit
   - Instance tags: Survives instance termination
   - **Recommendation**: Use instance tags + ResourceTracker

4. **Should lifecycle management be provider-agnostic?**
   - Yes: Enables future providers (RunPod, etc.)
   - But: Spot interruptions are AWS-specific
   - **Recommendation**: Abstract lifecycle, provider-specific implementations

## Current Gaps Summary

| Feature | Spot | On-Demand | Status |
|---------|------|-----------|--------|
| Automatic monitoring | ❌ | N/A | Must be manual |
| Checkpoint on stop | ❌ | ❌ | Not implemented |
| Checkpoint on terminate | ✅ | ❌ | Only for spot interruptions |
| Resume after start | ✅ (auto-resume) | ❌ | Only for spot |
| State tracking | ⚠️ | ⚠️ | Basic (ResourceTracker) |
| Lifecycle hooks | ⚠️ | ❌ | Only for spot interruptions |

**Legend:**
- ✅ = Fully implemented
- ⚠️ = Partially implemented
- ❌ = Not implemented

