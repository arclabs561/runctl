# EBS Volume Concurrent Access: Risks and Safeguards

## ⚠️ CRITICAL: EBS Volume Safety

**EBS volumes can only be attached to ONE EC2 instance at a time.** This is a fundamental AWS constraint that prevents filesystem corruption from concurrent write access.

**If two instances try to write to the same filesystem simultaneously, you will experience:**
- ❌ Filesystem corruption
- ❌ Data loss
- ❌ Corrupted checkpoints
- ❌ Training failures

## The Problem: Two Training Pods Using the Same EBS Volume

### AWS Limitation

**EBS volumes can only be attached to ONE EC2 instance at a time.** This is a fundamental AWS constraint, not a runctl limitation. This constraint exists to prevent filesystem corruption.

### Current Safeguards in runctl

The `attach_volume` function in `src/ebs.rs` checks if a volume is already attached:

```rust
// Check if volume is already attached
if !volume.attachments().is_empty() {
    let attached_to = volume
        .attachments()
        .first()
        .and_then(|a| a.instance_id())
        .unwrap_or("unknown");
    return Err(TrainctlError::CloudProvider {
        provider: "aws".to_string(),
        message: format!(
            "Volume {} is already attached to instance {}.\n\
             Detach it first or use a different volume.",
            volume_id, attached_to
        ),
        source: None,
    });
}
```

**What this means:**
- ✅ If instance A has the volume attached, instance B's attachment will **fail immediately**
- ✅ The error message clearly indicates which instance has the volume
- ✅ No data corruption can occur from this path

### Race Condition Risk

**However, there's a potential race condition:**

If two instances try to attach the same volume **simultaneously**:

1. **Instance A** checks: `volume.attachments().is_empty()` → `true` (volume not attached)
2. **Instance B** checks: `volume.attachments().is_empty()` → `true` (volume not attached)
3. **Instance A** calls `attach_volume()` → AWS processes request
4. **Instance B** calls `attach_volume()` → AWS processes request

**What AWS does:**
- AWS will **reject the second attachment** with an error
- Only **one attachment will succeed** (typically the first one)
- The second instance will get an AWS API error

**Current code behavior:**
- The second instance will receive an error from AWS
- The error will be caught and returned as a `TrainctlError::Aws`
- No data corruption occurs

### What Would Happen If Both Got Access? (Hypothetical)

If somehow both instances could mount and write to the same filesystem simultaneously:

1. **Filesystem Corruption**
   - Both instances would have independent filesystem caches
   - Writes from one instance wouldn't be visible to the other
   - Metadata conflicts (inode tables, directory entries)
   - Filesystem would become inconsistent

2. **Data Loss**
   - Overwritten files
   - Lost writes (one instance's writes overwrite the other's)
   - Corrupted checkpoints
   - Broken training runs

3. **Filesystem Crash**
   - Most filesystems (XFS, ext4) are not designed for concurrent write access
   - Filesystem would likely crash or require fsck
   - Data recovery would be difficult or impossible

**This scenario is prevented by AWS's single-attachment constraint.**

## Solutions for Sharing Data Between Multiple Instances

### Option 1: EBS Multi-Attach (io2/io2 Block Express)

AWS supports **multi-attach** for `io2` and `io2 Block Express` volumes:

```bash
# Create multi-attach volume
runctl aws ebs create --type io2 \
    --multi-attach \
    --size 500 \
    --name shared-datasets
```

**Limitations:**
- ⚠️ **Read-only** for most use cases (concurrent writes still cause corruption)
- ⚠️ Only `io2` and `io2 Block Express` volume types
- ⚠️ More expensive than `gp3`
- ✅ Good for **read-only datasets** shared across instances

**Use case:** Pre-warmed dataset volume that multiple training instances read from

### Option 2: EBS Snapshots

Create volume snapshots and restore to new volumes for each instance:

```bash
# Create snapshot of source volume
runctl aws ebs snapshot vol-xxxxx --description "Dataset snapshot"

# Create new volume from snapshot for each instance
runctl aws ebs restore snap-xxxxx --size 500 --name instance-1-dataset
runctl aws ebs restore snap-xxxxx --size 500 --name instance-2-dataset
```

**Benefits:**
- ✅ Each instance gets its own volume (no conflicts)
- ✅ Fast creation from snapshot (minutes vs hours)
- ✅ Independent write access
- ✅ Can share read-only snapshots

**Use case:** Each training instance needs its own checkpoint volume

### Option 3: S3 as Shared Storage

Use S3 for shared datasets, EBS for instance-specific data:

```bash
# Shared dataset in S3 (read-only)
runctl aws train $INSTANCE_ID train.py \
    --data-s3 s3://bucket/shared-datasets/

# Instance-specific checkpoints on EBS
runctl aws create --ebs-volume vol-checkpoints-$INSTANCE_ID
```

**Benefits:**
- ✅ True concurrent read access
- ✅ No attachment conflicts
- ✅ Cost-effective for large datasets
- ⚠️ Slower than EBS for frequent access

### Option 4: Detach/Attach Workflow

Sequential access pattern:

```bash
# Instance 1 uses volume
runctl aws create --ebs-volume vol-xxxxx
runctl aws train $INSTANCE_1 train.py
runctl aws ebs detach vol-xxxxx

# Instance 2 uses volume
runctl aws ebs attach vol-xxxxx --instance $INSTANCE_2
runctl aws train $INSTANCE_2 train.py
```

**Use case:** Sequential training jobs that don't need concurrent access

## Best Practices

### For Read-Only Datasets

1. **Use EBS Multi-Attach (io2)**
   ```bash
   runctl aws ebs create --type io2 --multi-attach --size 1000 --name datasets
   # Multiple instances can attach and read
   ```

2. **Or use S3**
   ```bash
   # Faster for very large datasets, supports true concurrent access
   runctl aws train $INSTANCE_ID train.py --data-s3 s3://bucket/datasets/
   ```

### For Checkpoints (Read-Write)

1. **One volume per instance**
   ```bash
   # Each instance gets its own checkpoint volume
   runctl aws create --ebs-volume vol-checkpoints-$INSTANCE_ID
   ```

2. **Or use snapshots for backup**
   ```bash
   # Backup checkpoints to snapshot
   runctl aws ebs snapshot vol-checkpoints-$INSTANCE_ID
   # Restore to new instance when needed
   ```

### For Concurrent Training

1. **Separate volumes per instance**
   - Each training pod gets its own EBS volume
   - No conflicts, no race conditions
   - Independent checkpoint management

2. **Shared read-only data via S3 or Multi-Attach**
   - Datasets in S3 or multi-attach io2 volume
   - Checkpoints on per-instance EBS volumes

## Current runctl Behavior

When you try to attach a volume that's already attached:

```bash
$ runctl aws ebs attach vol-xxxxx --instance i-12345
Error: Volume vol-xxxxx is already attached to instance i-67890.
       Detach it first or use a different volume.
```

**This prevents:**
- ✅ Data corruption
- ✅ Filesystem conflicts
- ✅ Lost writes
- ✅ Training failures

**What you should do:**
1. Detach the volume from the first instance first
2. Or use a different volume for the second instance
3. Or use snapshots to create a copy for the second instance

## Summary

| Scenario | What Happens | Risk Level |
|----------|--------------|------------|
| Two instances try to attach same volume | Second attachment fails immediately | ✅ **Safe** - No data corruption |
| Race condition (simultaneous requests) | AWS rejects second attachment | ✅ **Safe** - AWS enforces single attachment |
| Hypothetical: Both get access | Filesystem corruption, data loss | ❌ **Critical** - But prevented by AWS |
| Multi-attach io2 (read-only) | Multiple instances can read | ✅ **Safe** - Read-only access |
| Multi-attach io2 (write) | Filesystem corruption | ❌ **Critical** - Don't do this |

**Bottom line:** runctl's safeguards + AWS's single-attachment constraint prevent data corruption. Use separate volumes, snapshots, or multi-attach (read-only) for sharing data between instances.

## Logging and Warnings in runctl

runctl now includes comprehensive logging and warnings about EBS volume safety:

### During Volume Attachment

When you attach a volume, you'll see:
- ✅ **Info logs**: Volume attachment status and exclusivity confirmation
- ⚠️ **Warnings**: Multi-attach volume warnings (if applicable)
- 📝 **Instructions**: Mount commands and safety reminders

### During Instance Creation

When auto-creating data volumes:
- ✅ **Info logs**: Volume creation and attachment status
- ⚠️ **Warnings**: Exclusive access warnings
- 📝 **Reminders**: Volume exclusivity and safety information

### During Docker Training

When EBS volumes are detected and mounted in containers:
- ✅ **Info logs**: Number of volumes detected and mount points
- ⚠️ **Warnings**: Reminders about concurrent access risks
- 📝 **Safety checks**: Verification that volumes aren't shared

### Error Messages

Enhanced error messages include:
- Clear explanation of why attachment failed
- Which instance currently has the volume attached
- Step-by-step resolution instructions
- Links to documentation

## Quick Reference: What to Do

### ✅ Safe Patterns

1. **One volume per instance** (recommended)
   ```bash
   # Each instance gets its own volume
   runctl aws create --ebs-volume vol-instance1
   runctl aws create --ebs-volume vol-instance2
   ```

2. **Snapshots for sharing**
   ```bash
   # Create snapshot, restore for each instance
   runctl aws ebs snapshot vol-source
   runctl aws ebs restore snap-xxx --name instance1-data
   runctl aws ebs restore snap-xxx --name instance2-data
   ```

3. **Multi-attach io2 (read-only)**
   ```bash
   # Create io2 volume, attach to multiple instances (read-only)
   runctl aws ebs create --type io2 --multi-attach --size 1000
   # Mount read-only on each instance
   ```

### ❌ Dangerous Patterns

1. **Trying to attach same volume to multiple instances**
   - Will fail (by design) - prevents corruption

2. **Multi-attach io2 with writes**
   - Will cause filesystem corruption
   - Only use for read-only datasets

3. **Detaching volume while in use**
   - May cause data loss if not properly unmounted
   - Always unmount before detaching

## runctl Safety Features

1. **Pre-attachment checks**: Verifies volume is not already attached
2. **Enhanced error messages**: Clear explanations and resolution steps
3. **Logging**: Comprehensive info/warn logs for all EBS operations
4. **Multi-attach detection**: Warns about io2 volumes and write risks
5. **Docker integration**: Warns when EBS volumes are mounted in containers
6. **Documentation**: Comprehensive guides and examples

## Need Help?

- See `docs/EBS_OPTIMIZATION.md` for optimization strategies
- See `docs/EBS_CONCURRENT_ACCESS.md` (this file) for safety details
- Check logs with `RUST_LOG=info runctl ...` for detailed information
- All warnings are logged to help you understand what's happening

