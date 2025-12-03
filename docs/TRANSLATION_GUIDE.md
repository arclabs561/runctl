# Translation Guide: Reference Repos → trainctl

## Quick Reference: Common Patterns

### Pattern 1: Complete AWS Training Workflow

**decksage style:**
```python
# Create instance
instance_id = create_ec2_instance(use_spot=True, fallback=True)

# Download data
ssm.send_command(instance_id, "aws s3 cp s3://data/ ./data/ --recursive")

# Train
ssm.send_command(instance_id, "python train.py --epochs 50")

# Upload results
ssm.send_command(instance_id, "aws s3 cp ./checkpoints/ s3://output/ --recursive")

# Monitor
while True:
    status = ssm.get_command_invocation(command_id)
    if status['Status'] == 'Success':
        break
    time.sleep(30)
```

**trainctl equivalent:**
```bash
# Single command handles everything
trainctl aws train <instance-id> train.py \
    --data-s3 s3://bucket/data/ \
    --output-s3 s3://bucket/output/ \
    --epochs 50

# Monitor in another terminal
trainctl aws monitor <instance-id> --follow
```

### Pattern 2: Ephemeral Training with Auto-Resume

**idf-est style:**
```python
# Check for existing checkpoint
checkpoint_dir = Path("/workspace/checkpoints")
latest = find_latest_checkpoint(checkpoint_dir)

if latest:
    resume_from_checkpoint(latest)
else:
    start_training()

# Save checkpoint every epoch
for epoch in range(epochs):
    train_epoch()
    if epoch % checkpoint_interval == 0:
        save_checkpoint(epoch, model, optimizer)
```

**trainctl equivalent:**
```bash
# Auto-detects and resumes from latest checkpoint
trainctl local train_ephemeral.py --epochs 50

# Or explicitly
trainctl local train_ephemeral.py --resume checkpoints/latest.pt
```

### Pattern 3: Multi-GPU DDP Training

**matryoshka-box style:**
```python
# Setup DDP
torch.distributed.init_process_group(...)
rank = torch.distributed.get_rank()

# Only rank 0 saves checkpoints
if rank == 0:
    save_checkpoint(model, optimizer, epoch)
    upload_to_s3(checkpoint_path, s3_path)
```

**trainctl equivalent:**
```bash
# trainctl detects DDP and handles rank 0 automatically
torchrun --nproc_per_node=4 train_multi_gpu.py

# Or with trainctl wrapper (when implemented)
trainctl local train_multi_gpu.py --ddp --gpus 4
```

### Pattern 4: RunPod Workflow

**idf-est style:**
```python
# Create pod
pod_id = runpodctl.create_pod(gpu="RTX 4080")

# Upload code
runpodctl.upload(pod_id, "./training/", "/workspace/")

# Run training
runpodctl.exec(pod_id, "cd /workspace && python train.py")

# Monitor
runpodctl.tail_logs(pod_id, "/workspace/train.log")

# Download results
runpodctl.download(pod_id, "/workspace/checkpoints/", "./local/")
```

**trainctl equivalent:**
```bash
# Create pod
POD_ID=$(trainctl runpod create --gpu "RTX 4080")

# Train (handles upload automatically)
trainctl runpod train $POD_ID train.py

# Monitor
trainctl runpod monitor $POD_ID --follow

# Download
trainctl runpod download $POD_ID /workspace/checkpoints/ ./local/
```

### Pattern 5: Checkpoint Management

**All repos style:**
```python
# Save checkpoint
checkpoint = {
    'epoch': epoch,
    'model': model.state_dict(),
    'optimizer': optimizer.state_dict(),
    'loss': loss,
}
torch.save(checkpoint, f"checkpoint_epoch_{epoch}.pt")

# Upload to S3
s3.upload_file(f"checkpoint_epoch_{epoch}.pt", bucket, s3_key)

# List checkpoints
checkpoints = list_checkpoints("./checkpoints/")

# Resume
checkpoint = torch.load("checkpoint_epoch_10.pt")
model.load_state_dict(checkpoint['model'])
optimizer.load_state_dict(checkpoint['optimizer'])
```

**trainctl equivalent:**
```bash
# List checkpoints
trainctl checkpoint list checkpoints/

# Show checkpoint info
trainctl checkpoint info checkpoints/checkpoint_epoch_10.pt

# Resume training
trainctl checkpoint resume checkpoints/checkpoint_epoch_10.pt train.py

# Upload to S3
trainctl s3 upload checkpoints/ s3://bucket/checkpoints/ --recursive

# Download from S3
trainctl s3 download s3://bucket/checkpoints/ checkpoints/ --recursive
```

### Pattern 6: Resource Management

**decksage style:**
```python
# List instances
instances = ec2.describe_instances()
for instance in instances:
    print(f"{instance.id} - {instance.state} - {instance.instance_type}")

# Find zombies (running > 24h)
zombies = [i for i in instances 
           if i.state == 'running' 
           and (now - i.launch_time).hours > 24
           and 'trainctl' not in i.tags]

# Cleanup
for zombie in zombies:
    ec2.terminate_instances(InstanceIds=[zombie.id])
```

**trainctl equivalent:**
```bash
# List all resources
trainctl resources list

# Get summary
trainctl resources summary

# Find zombies
trainctl resources insights

# Cleanup zombies
trainctl resources cleanup --dry-run  # Preview
trainctl resources cleanup --force    # Actually cleanup
```

## Feature Comparison Matrix

| Feature | decksage | idf-est | matryoshka-box | trainctl |
|---------|----------|---------|----------------|-----------|
| AWS EC2 Creation | ✅ | ❌ | ❌ | 🚧 |
| Spot Instances | ✅ | ❌ | ❌ | 🚧 |
| SSM Execution | ✅ | ❌ | ❌ | ✅ |
| S3 Data Staging | ✅ | ❌ | ❌ | ❌ |
| RunPod Integration | ❌ | ✅ | ❌ | ✅ |
| Checkpoint Management | ✅ | ✅ | ✅ | ✅ |
| Auto-Resume | ✅ | ✅ | ✅ | ❌ |
| DDP Support | ❌ | ❌ | ✅ | ❌ |
| Graceful Shutdown | ✅ | ✅ | ❌ | ❌ |
| Resource Tracking | ✅ | ❌ | ❌ | ✅ |
| Cost Monitoring | ✅ | ❌ | ❌ | 🚧 |

Legend: ✅ = Full support, 🚧 = Partial, ❌ = Not implemented

## Migration Examples

### Migrating from decksage Script

**Before (decksage):**
```python
python train_on_aws_instance.py \
    --instance-type t3.medium \
    --use-spot \
    --data-s3 s3://bucket/data/ \
    --output-s3 s3://bucket/output/
```

**After (trainctl):**
```bash
# Create instance
INSTANCE_ID=$(trainctl aws create --spot --instance-type t3.medium)

# Train with automatic S3 staging
trainctl aws train $INSTANCE_ID train.py \
    --data-s3 s3://bucket/data/ \
    --output-s3 s3://bucket/output/
```

### Migrating from idf-est Script

**Before (idf-est):**
```python
python train_runpod.py \
    --gpu "RTX 4080" \
    --script train.py \
    --background
```

**After (trainctl):**
```bash
# Create pod
POD_ID=$(trainctl runpod create --gpu "RTX 4080")

# Train
trainctl runpod train $POD_ID train.py --background

# Monitor
trainctl runpod monitor $POD_ID --follow
```

### Migrating from matryoshka-box Script

**Before (matryoshka-box):**
```bash
torchrun --nproc_per_node=4 train_cloud_multi_gpu.py \
    --epochs 50 \
    --batch-size 1024
```

**After (trainctl):**
```bash
# trainctl handles DDP automatically
trainctl local train_cloud_multi_gpu.py -- --epochs 50 --batch-size 1024

# Or with explicit DDP
torchrun --nproc_per_node=4 train_cloud_multi_gpu.py --epochs 50 --batch-size 1024
```

## Best Practices Translation

### 1. Checkpoint Strategy

**Reference Pattern:**
- Save every N epochs
- Upload to S3 after each save
- Keep last N locally
- Cleanup old checkpoints

**trainctl:**
```bash
# Configure in .trainctl.toml
[checkpoint]
save_interval = 5
keep_last_n = 10
auto_upload_s3 = "s3://bucket/checkpoints/"

# Use in training
trainctl local train.py  # Auto-handles checkpointing
```

### 2. Error Handling

**Reference Pattern:**
- Try/except around training
- Save checkpoint on error
- Upload to S3 before exit

**trainctl (when implemented):**
```bash
# Automatic error handling
trainctl local train.py  # Auto-saves on error
```

### 3. Cost Optimization

**Reference Pattern:**
- Use spot instances
- Monitor costs
- Cleanup promptly

**trainctl:**
```bash
# Use spot instances
trainctl aws create --spot

# Monitor costs
trainctl resources summary

# Cleanup
trainctl resources cleanup --force
```

## Implementation Roadmap

Based on reference patterns, here's what to implement:

### Immediate (This Week)
1. ✅ Instance tagging
2. ✅ S3 data staging in AWS training
3. ✅ Auto-resume from latest checkpoint

### Short-term (Next Week)
4. ✅ DDP-aware checkpointing
5. ✅ Graceful shutdown handlers
6. ✅ Automatic S3 checkpoint upload

### Medium-term (Next Month)
7. ✅ Enhanced cost tracking
8. ✅ Metrics extraction from logs
9. ✅ Progress visualization

