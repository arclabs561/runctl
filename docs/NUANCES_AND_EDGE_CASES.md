# Nuances and Edge Cases to Respect

## Resource Lifecycle Nuances

### 1. Instance Termination with Attached Volumes
**Issue**: Terminating an instance with attached EBS volumes
- **Persistent volumes**: Should detach, not delete (data survives)
- **Ephemeral volumes**: Can delete on termination (if configured)
- **Current behavior**: No check - volumes may be orphaned or deleted incorrectly

**Solution**: Check volume tags before termination
```rust
// Before terminating instance:
// 1. List attached volumes
// 2. Check if persistent (trainctl:persistent=true)
// 3. If persistent: detach only
// 4. If ephemeral: warn or delete based on config
```

### 2. Spot Instance Interruptions
**Issue**: Spot instances can be terminated with 2-minute warning
- **Training in progress**: Should save checkpoint before termination
- **Data on instance storage**: Lost (use EBS for persistence)
- **Current behavior**: No interruption handling

**Solution**: 
- Monitor spot interruption warnings
- Auto-save checkpoints before termination
- Use EBS volumes for data persistence

### 3. Volume Attachment Across AZs
**Issue**: EBS volumes must be in same AZ as instance
- **Current behavior**: User must specify AZ manually
- **Risk**: Attachment fails if AZ mismatch

**Solution**: 
- Auto-detect instance AZ when attaching
- Validate AZ match before attachment
- Suggest correct AZ if mismatch

### 4. Snapshot Dependencies
**Issue**: Deleting volume with snapshots
- **Snapshots**: Independent of volume, but represent data
- **Current behavior**: No check for snapshots

**Solution**:
- Warn if volume has snapshots before deletion
- List snapshots in delete confirmation
- Option to delete snapshots first

### 5. Running Training Jobs
**Issue**: Terminating instance while training is active
- **Data loss**: Checkpoints not saved
- **Cost waste**: Training time lost
- **Current behavior**: No check for active training

**Solution**:
- Check for running training processes
- Warn before termination
- Option to wait for completion or force terminate

### 6. Cost Thresholds
**Issue**: Resources accumulating high costs
- **Current behavior**: Tracks costs but no alerts
- **Risk**: Unexpected bills

**Solution**:
- Warn when hourly cost exceeds threshold
- Alert on accumulated costs over time
- Suggest cleanup of old resources

### 7. Resource Dependencies
**Issue**: Deleting resources with dependencies
- **Volumes attached to instances**: Can't delete while attached
- **Snapshots from volumes**: Can delete volume, snapshots remain
- **Checkpoints on volumes**: Should warn if deleting checkpoint volume

**Solution**:
- Dependency graph checking
- Warn about dependent resources
- Cascade deletion option (with confirmation)

### 8. Time-Based Protections
**Issue**: Accidentally deleting recently created resources
- **Current behavior**: No time-based protection
- **Risk**: Delete resources just created

**Solution**:
- Protect resources created < 5 minutes ago
- Require --force for recent resources
- Show age in resource listings

### 9. Multi-Resource Operations
**Issue**: Operations affecting multiple resources
- **Terminate instance**: Should handle attached volumes
- **Delete volume**: Should check if attached
- **Cleanup**: Should respect dependencies

**Solution**:
- Transaction-like operations
- Rollback on partial failure
- Dry-run mode for complex operations

### 10. Checkpoint Safety
**Issue**: Deleting checkpoints in use
- **Active training**: May be writing checkpoints
- **Recent checkpoints**: May be needed for resume
- **Current behavior**: No safety checks

**Solution**:
- Check if checkpoint is recent (< 1 hour)
- Warn if checkpoint is in use
- Protect checkpoints from cleanup

## Implementation Priorities

### High Priority (Safety Critical)
1. ✅ Persistent volume protection (DONE)
2. ⚠️ Instance termination with attached volumes
3. ⚠️ Running training job detection
4. ⚠️ Cost threshold warnings

### Medium Priority (User Experience)
5. ⚠️ AZ validation for volume attachment
6. ⚠️ Snapshot dependency warnings
7. ⚠️ Time-based protections
8. ⚠️ Checkpoint safety

### Low Priority (Nice to Have)
9. Spot interruption handling
10. Dependency graph visualization
11. Multi-resource transaction support

