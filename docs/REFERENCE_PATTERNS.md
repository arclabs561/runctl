# Reference Repository Patterns Analysis

## Overview

Analysis of how matryoshka-box, idf-est, and decksage use AWS/cloud training, and how runctl translates these patterns.

## decksage: AWS EC2 Training Patterns

### Key Patterns Found

#### 1. **Spot Instance with On-Demand Fallback**
```python
# Pattern: Try spot first, fallback to on-demand
if use_spot:
    try:
        # Create spot instance
        spot_response = ec2.request_spot_instances(...)
    except Exception as e:
        if fallback_to_ondemand:
            # Fallback to on-demand
            ondemand_response = ec2.run_instances(...)
```

**runctl translation:**
```rust
// Already implemented in src/aws.rs
if use_spot {
    match create_spot_instance(...).await {
        Ok(id) => return Ok(id),
        Err(e) if !no_fallback => {
            // Fallback to on-demand
            create_ondemand_instance(...).await
        }
    }
}
```

#### 2. **SSM Command Execution**
```python
# Pattern: Use SSM to execute commands without SSH
ssm = boto3.client("ssm")
response = ssm.send_command(
    InstanceIds=[instance_id],
    DocumentName="AWS-RunShellScript",
    Parameters={
        "commands": [command_string]
    }
)

# Poll for completion
while True:
    status = ssm.get_command_invocation(
        CommandId=command_id,
        InstanceId=instance_id
    )
    if status['Status'] in ['Success', 'Failed']:
        break
    time.sleep(30)
```

**runctl translation:**
```rust
// Implemented in src/aws.rs::train_on_instance
let response = client
    .send_command()
    .instance_ids(&instance_id)
    .document_name("AWS-RunShellScript")
    .parameters("commands", vec![command])
    .send()
    .await?;

// Poll for completion (already implemented)
loop {
    let status = client.get_command_invocation(...).await?;
    match status.status() {
        "Success" => break,
        "Failed" => bail!("Training failed"),
        _ => sleep(Duration::from_secs(30)).await,
    }
}
```

#### 3. **S3 Data Staging**
```python
# Pattern: Download data from S3 before training
command = f"""
aws s3 cp {s3_data_path} ./data/
python3.11 training_script.py
aws s3 cp ./output/ {s3_output_path} --recursive
"""
```

**runctl translation:**
```rust
// Should be in src/aws.rs::train_on_instance
// Add S3 download before training:
let download_cmd = format!(
    "aws s3 cp {} ./data/ --recursive",
    data_s3_path
);

// Add S3 upload after training:
let upload_cmd = format!(
    "aws s3 cp ./checkpoints/ {} --recursive",
    output_s3_path
);
```

#### 4. **Instance Tagging for Tracking**
```python
# Pattern: Tag instances for easy identification
tags = [
    {"Key": "Name", "Value": "decksage-training"},
    {"Key": "Project", "Value": "decksage"},
]
```

**runctl translation:**
```rust
// Should add to src/aws.rs::create_instance
let tags = vec![
    ("Name", "runctl-training"),
    ("Project", "runctl"),
    ("CreatedBy", "runctl"),
];
```

## idf-est: RunPod Training Patterns

### Key Patterns Found

#### 1. **RunPod Pod Management**
```python
# Pattern: Create pod, upload code, run training
pod_id = runpodctl.create_pod(...)
runpodctl.upload(pod_id, local_path, remote_path)
runpodctl.exec(pod_id, "python train.py")
```

**runctl translation:**
```rust
// Already in src/runpod.rs
// create_pod() - ✅ Implemented
// train_on_pod() - ✅ Implemented (uses runpodctl)
// download() - ✅ Implemented
```

#### 2. **Ephemeral Training with Checkpointing**
```python
# Pattern: Frequent checkpointing for ephemeral pods
if epoch % checkpoint_interval == 0:
    save_checkpoint(epoch, model, optimizer)
    # Upload to persistent storage
    upload_to_s3(checkpoint_path, s3_path)
```

**runctl translation:**
```rust
// Should add automatic S3 upload after checkpoint
// In training scripts, add:
// runctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive
```

#### 3. **Background Training with Monitoring**
```python
# Pattern: Run training in background, monitor logs
runpodctl.exec(pod_id, "nohup python train.py > train.log 2>&1 &")
# Monitor logs
runpodctl.tail_logs(pod_id, "train.log")
```

**runctl translation:**
```rust
// Already in src/runpod.rs::train_on_pod
// Has --background flag support
// Monitor via: runctl runpod monitor <pod-id> --follow
```

## matryoshka-box: Multi-GPU Training Patterns

### Key Patterns Found

#### 1. **DDP Setup and Checkpointing**
```python
# Pattern: Only rank 0 saves checkpoints
if rank == 0:
    save_checkpoint(model, optimizer, epoch, checkpoint_dir)
    # Also save to S3
    upload_to_s3(checkpoint_path, s3_backup_path)
```

**runctl translation:**
```rust
// Should add DDP-aware checkpointing
// Check for RANK env var:
let rank = env::var("RANK").ok().and_then(|s| s.parse().ok());
if rank == Some(0) || rank.is_none() {
    // Save checkpoint
    // Upload to S3
}
```

#### 2. **Cloud-Optimized Configs**
```python
# Pattern: Different configs for cloud vs local
if is_cloud:
    batch_size = 1024  # Larger for cloud GPUs
    learning_rate = 0.001 * 4  # Scaled for larger batch
    epochs = 50  # More epochs for cloud
```

**runctl translation:**
```rust
// Already in src/config.rs
// Can add platform-specific configs:
[cloud]
batch_size_multiplier = 4
learning_rate_multiplier = 4.0
```

#### 3. **Ephemeral Pod Handling**
```python
# Pattern: Auto-resume from latest checkpoint
checkpoint_dir = Path("/workspace/checkpoints")
latest_checkpoint = find_latest_checkpoint(checkpoint_dir)
if latest_checkpoint:
    resume_from_checkpoint(latest_checkpoint)
```

**runctl translation:**
```rust
// Should add to src/local.rs and src/runpod.rs
// Auto-detect and resume from latest checkpoint:
let latest = find_latest_checkpoint(&checkpoint_dir)?;
if let Some(ckpt) = latest {
    // Add --resume flag to training command
}
```

## Common Patterns Across All Repos

### 1. **Checkpoint Management Workflow**

**Pattern:**
```python
# Save checkpoint
checkpoint = {
    'epoch': epoch,
    'model_state_dict': model.state_dict(),
    'optimizer_state_dict': optimizer.state_dict(),
    'loss': loss,
    'config': config.__dict__,
    'timestamp': datetime.now().isoformat(),
}
torch.save(checkpoint, checkpoint_path)

# Upload to S3
s3.upload_file(checkpoint_path, bucket, s3_key)
```

**runctl translation:**
```rust
// runctl checkpoint save <path>
// runctl s3 upload <path> s3://bucket/checkpoints/
// Or automatic: runctl local script.py --auto-upload-s3
```

### 2. **Data Pipeline**

**Pattern:**
```python
# Download data
aws s3 sync s3://bucket/datasets/ ./data/

# Preprocess
python preprocess.py

# Train
python train.py

# Upload results
aws s3 sync ./output/ s3://bucket/output/
```

**runctl translation:**
```rust
// runctl s3 download s3://bucket/datasets/ ./data/ --recursive
// runctl local preprocess.py
// runctl local train.py
// runctl s3 upload ./output/ s3://bucket/output/ --recursive
```

### 3. **Monitoring and Logging**

**Pattern:**
```python
# Stream logs
for line in tail_logs(log_file):
    print(line)
    if "error" in line.lower():
        alert()
```

**runctl translation:**
```rust
// runctl monitor --log training.log --follow
// Already implemented in src/monitor.rs
```

### 4. **Error Recovery**

**Pattern:**
```python
try:
    train()
except Exception as e:
    # Save checkpoint before exit
    save_checkpoint(epoch, model, optimizer)
    # Upload to S3
    upload_to_s3(checkpoint_path, s3_path)
    raise
```

**runctl translation:**
```rust
// Should add signal handlers for SIGTERM/SIGINT
// Auto-save checkpoint on interruption
// Auto-upload to S3
```

## Missing Features in runctl

### High Priority

1. **Automatic S3 Upload After Checkpoint**
   - Currently: Manual `runctl s3 upload`
   - Needed: Auto-upload after each checkpoint save

2. **S3 Data Staging in AWS Training**
   - Currently: Manual S3 commands in script
   - Needed: Automatic download before training, upload after

3. **DDP-Aware Checkpointing**
   - Currently: All ranks might save
   - Needed: Only rank 0 saves

4. **Auto-Resume from Latest Checkpoint**
   - Currently: Manual `--resume` flag
   - Needed: Auto-detect and resume

5. **Instance Tagging**
   - Currently: No tags
   - Needed: Tag instances for tracking

### Medium Priority

6. **Background Training with Log Monitoring**
   - Currently: Basic background support
   - Needed: Better log streaming

7. **Error Recovery with Checkpoint Save**
   - Currently: No signal handlers
   - Needed: Graceful shutdown with checkpoint save

8. **Cost Tracking**
   - Currently: Basic cost estimation
   - Needed: Real-time cost tracking

## Recommended Implementation Order

1. **Instance Tagging** (Easy, high value)
2. **S3 Data Staging** (Medium, high value)
3. **Auto-Resume** (Medium, high value)
4. **DDP-Aware Checkpointing** (Medium, medium value)
5. **Error Recovery** (Hard, high value)
6. **Cost Tracking** (Medium, medium value)

## Translation Examples

### Example 1: Complete Training Workflow

**Original (decksage pattern):**
```python
# Create instance
instance_id = create_ec2_instance(use_spot=True)

# Download data
ssm.send_command(instance_id, "aws s3 cp s3://data/ ./data/")

# Train
ssm.send_command(instance_id, "python train.py")

# Upload results
ssm.send_command(instance_id, "aws s3 cp ./output/ s3://output/")
```

**runctl translation:**
```bash
# Create instance
INSTANCE_ID=$(runctl aws create --spot)

# Train with automatic data staging
runctl aws train $INSTANCE_ID train.py \
    --data-s3 s3://bucket/data/ \
    --output-s3 s3://bucket/output/

# (runctl handles S3 download/upload automatically)
```

### Example 2: Ephemeral Training

**Original (idf-est pattern):**
```python
# Create pod
pod_id = runpodctl.create_pod()

# Train with auto-checkpointing
runpodctl.exec(pod_id, "python train_ephemeral.py")

# Monitor
runpodctl.tail_logs(pod_id)
```

**runctl translation:**
```bash
# Create pod
POD_ID=$(runctl runpod create)

# Train (auto-checkpoints every epoch)
runctl runpod train $POD_ID train_ephemeral.py

# Monitor
runctl runpod monitor $POD_ID --follow
```

### Example 3: Multi-GPU Training

**Original (matryoshka-box pattern):**
```python
# Setup DDP
torch.distributed.init_process_group(...)

# Only rank 0 saves
if rank == 0:
    save_checkpoint(...)
    upload_to_s3(...)
```

**runctl translation:**
```bash
# runctl detects DDP and only rank 0 saves
runctl local train_multi_gpu.py --ddp

# Auto-uploads checkpoints from rank 0
```

## Key Insights

1. **S3 is central** - All repos use S3 for data, checkpoints, outputs
2. **Checkpointing is critical** - Frequent saves, auto-upload, auto-resume
3. **Error handling matters** - Save on error, graceful shutdown
4. **Monitoring is essential** - Log streaming, progress tracking
5. **Cost awareness** - Spot instances, cleanup, resource tracking

## Next Steps for runctl

1. Add automatic S3 integration to training commands
2. Implement DDP-aware checkpointing
3. Add signal handlers for graceful shutdown
4. Enhance monitoring with metrics extraction
5. Add cost tracking and alerts

