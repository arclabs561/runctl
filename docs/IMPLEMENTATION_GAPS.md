# Implementation Gaps Analysis

## What Reference Repos Do vs What runctl Has

### ✅ Already Implemented

1. **Basic AWS EC2 Creation** - ✅ Structure in place
2. **SSM Command Execution** - ✅ Implemented
3. **RunPod Integration** - ✅ Full implementation
4. **Checkpoint Listing** - ✅ Implemented
5. **Resource Management** - ✅ Implemented

### ❌ Missing Critical Features

#### 1. **Automatic S3 Integration in Training**

**Reference Pattern (decksage):**
```python
# Automatically downloads data before training
command = f"""
aws s3 cp {s3_data} ./data/
python train.py
aws s3 cp ./output/ {s3_output} --recursive
"""
```

**runctl Current:**
- S3 operations exist but not integrated into training flow
- User must manually run S3 commands

**Needed:**
```rust
// In src/aws.rs::train_on_instance
// Automatically add S3 download before training
if let Some(data_s3) = data_s3 {
    let download_cmd = format!("aws s3 cp {} ./data/ --recursive", data_s3);
    // Execute before training
}

// Automatically add S3 upload after training
if let Some(output_s3) = output_s3 {
    let upload_cmd = format!("aws s3 cp ./checkpoints/ {} --recursive", output_s3);
    // Execute after training
}
```

#### 2. **Instance Tagging for Tracking**

**Reference Pattern (decksage):**
```python
tags = [
    {"Key": "Name", "Value": "decksage-training"},
    {"Key": "Project", "Value": "decksage"},
    {"Key": "CreatedBy", "Value": "train-script"},
]
```

**runctl Current:**
- No tagging implemented

**Needed:**
```rust
// In src/aws.rs::create_instance
let tags = vec![
    ("Name", "runctl-training"),
    ("Project", "runctl"),
    ("CreatedBy", "runctl"),
    ("SessionId", &session_id),
];
```

#### 3. **DDP-Aware Checkpointing**

**Reference Pattern (matryoshka-box):**
```python
def save_checkpoint(..., rank):
    if rank != 0:
        return  # Only rank 0 saves
    torch.save(checkpoint, path)
    upload_to_s3(path, s3_path)
```

**runctl Current:**
- No DDP awareness
- All processes might save

**Needed:**
```rust
// Check for DDP rank
let rank = env::var("RANK")
    .ok()
    .and_then(|s| s.parse::<usize>().ok())
    .unwrap_or(0);

if rank == 0 {
    // Save checkpoint
    // Upload to S3
}
```

#### 4. **Auto-Resume from Latest Checkpoint**

**Reference Pattern (idf-est):**
```python
# Auto-detect and resume
checkpoint_dir = Path("/workspace/checkpoints")
latest = find_latest_checkpoint(checkpoint_dir)
if latest:
    resume_from_checkpoint(latest)
```

**runctl Current:**
- Manual `--resume` flag required
- No auto-detection

**Needed:**
```rust
// In src/local.rs and src/runpod.rs
// Auto-detect latest checkpoint
let latest = find_latest_checkpoint(&checkpoint_dir)?;
if let Some(ckpt) = latest {
    // Automatically add --resume flag
    args.push("--resume".to_string());
    args.push(ckpt.to_string_lossy().to_string());
}
```

#### 5. **Graceful Shutdown with Checkpoint Save**

**Reference Pattern (idf-est):**
```python
def signal_handler(signum, frame):
    print("Saving checkpoint before shutdown...")
    save_checkpoint(...)
    upload_to_s3(...)
    sys.exit(0)

signal.signal(signal.SIGTERM, signal_handler)
signal.signal(signal.SIGINT, signal_handler)
```

**runctl Current:**
- No signal handling
- No graceful shutdown

**Needed:**
```rust
// Add signal handlers
use tokio::signal;
use std::sync::Arc;

let checkpoint_dir = Arc::new(checkpoint_dir);
tokio::spawn(async move {
    signal::ctrl_c().await.ok();
    // Save checkpoint
    // Upload to S3
    std::process::exit(0);
});
```

#### 6. **Automatic Checkpoint Upload to S3**

**Reference Pattern (all repos):**
```python
# After each checkpoint save
save_checkpoint(...)
upload_to_s3(checkpoint_path, s3_path)
```

**runctl Current:**
- Manual S3 upload required

**Needed:**
```rust
// In checkpoint save logic
// If S3 output configured, auto-upload
if let Some(s3_path) = config.s3_checkpoint_path {
    s3::upload_to_s3(checkpoint_path, s3_path).await?;
}
```

#### 7. **Cost Tracking and Alerts**

**Reference Pattern (decksage):**
```python
# Track instance costs
instance_cost = get_instance_cost(instance_type, hours_running)
if instance_cost > budget:
    alert("Cost exceeded budget")
```

**runctl Current:**
- Basic cost estimation
- No real-time tracking
- No alerts

**Needed:**
```rust
// Track costs per session
struct CostTracker {
    instance_type: String,
    start_time: DateTime<Utc>,
    cost_per_hour: f64,
}

// Alert on high costs
if total_cost > config.max_cost_per_hour {
    eprintln!("⚠️  High cost alert: ${:.2}/hr", total_cost);
}
```

## Priority Implementation Plan

### Phase 1: Critical Missing Features (Week 1)

1. **Instance Tagging** - Easy, high value
   - Add tags to EC2 instances
   - Use for zombie detection
   - Track training sessions

2. **S3 Data Staging** - Medium, high value
   - Auto-download data before training
   - Auto-upload results after training
   - Integrate into `train` commands

3. **Auto-Resume** - Medium, high value
   - Detect latest checkpoint
   - Auto-add `--resume` flag
   - Works for local and cloud

### Phase 2: Important Features (Week 2)

4. **DDP-Aware Checkpointing** - Medium, medium value
   - Check RANK env var
   - Only rank 0 saves
   - Document for users

5. **Graceful Shutdown** - Hard, high value
   - Signal handlers
   - Checkpoint save on interrupt
   - S3 upload on exit

6. **Automatic S3 Upload** - Medium, high value
   - Config option for S3 checkpoint path
   - Auto-upload after each save
   - Background upload option

### Phase 3: Nice-to-Have (Week 3)

7. **Cost Tracking** - Medium, medium value
   - Real-time cost calculation
   - Budget alerts
   - Cost reports

8. **Enhanced Monitoring** - Medium, medium value
   - Metrics extraction from logs
   - Progress visualization
   - ETA estimation

## Code Examples for Implementation

### Example 1: Add S3 Staging to AWS Training

```rust
// In src/aws.rs::train_on_instance
async fn train_on_instance(
    instance_id: String,
    script: PathBuf,
    data_s3: Option<String>,
    output_s3: Option<String>,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let client = SsmClient::new(aws_config);
    
    // Build command with S3 staging
    let mut commands = Vec::new();
    
    // Download data if provided
    if let Some(data_path) = data_s3 {
        commands.push(format!("aws s3 cp {} ./data/ --recursive", data_path));
    }
    
    // Training script
    let script_content = fs::read_to_string(&script)?;
    commands.push(format!("python3 {}", script_content));
    
    // Upload results if provided
    if let Some(output_path) = output_s3 {
        commands.push(format!("aws s3 cp ./checkpoints/ {} --recursive", output_path));
        commands.push(format!("aws s3 cp ./output/ {} --recursive", output_path));
    }
    
    let full_command = commands.join(" && ");
    // Execute via SSM...
}
```

### Example 2: Add Instance Tagging

```rust
// In src/aws.rs::create_instance
let tags = vec![
    aws_sdk_ec2::types::Tag::builder()
        .key("Name")
        .value("runctl-training")
        .build()?,
    aws_sdk_ec2::types::Tag::builder()
        .key("Project")
        .value("runctl")
        .build()?,
    aws_sdk_ec2::types::Tag::builder()
        .key("CreatedBy")
        .value("runctl")
        .build()?,
    aws_sdk_ec2::types::Tag::builder()
        .key("SessionId")
        .value(&session_id)
        .build()?,
];

client
    .run_instances()
    .tag_specifications(
        aws_sdk_ec2::types::TagSpecification::builder()
            .resource_type(aws_sdk_ec2::types::ResourceType::Instance)
            .tags(tags)
            .build()?
    )
    // ... rest of config
```

### Example 3: Auto-Resume Logic

```rust
// In src/local.rs::train
fn find_latest_checkpoint(dir: &Path) -> Result<Option<PathBuf>> {
    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension() == Some("pt".as_ref()) {
            if let Ok(metadata) = fs::metadata(&path) {
                checkpoints.push((path, metadata.modified()?));
            }
        }
    }
    
    checkpoints.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(checkpoints.first().map(|(path, _)| path.clone()))
}

// In train function
let latest_checkpoint = find_latest_checkpoint(&checkpoint_dir)?;
if let Some(ckpt) = latest_checkpoint {
    args.push("--resume".to_string());
    args.push(ckpt.to_string_lossy().to_string());
    println!("📂 Auto-resuming from: {}", ckpt.display());
}
```

## Testing Strategy

### For Each Feature

1. **Unit Tests** - Test logic in isolation
2. **Integration Tests** - Test with mock AWS
3. **E2E Tests** - Test with real AWS (opt-in)

### Example E2E Test

```rust
#[tokio::test]
#[ignore]
async fn test_aws_training_with_s3_staging() {
    if !should_run_e2e() { return; }
    
    // Create instance
    let instance_id = create_test_instance().await?;
    
    // Train with S3 staging
    train_on_instance(
        instance_id,
        test_script,
        Some("s3://test-bucket/data/".to_string()),
        Some("s3://test-bucket/output/".to_string()),
    ).await?;
    
    // Verify S3 upload
    assert_s3_object_exists("s3://test-bucket/output/checkpoint.pt").await?;
    
    // Cleanup
    terminate_instance(instance_id).await?;
}
```

## Summary

**Key Gaps:**
1. S3 integration not automatic in training flow
2. No instance tagging for tracking
3. No DDP awareness
4. No auto-resume
5. No graceful shutdown
6. No automatic checkpoint upload
7. Limited cost tracking

**Priority Order:**
1. Instance tagging (easy, high value)
2. S3 staging (medium, high value)
3. Auto-resume (medium, high value)
4. Graceful shutdown (hard, high value)
5. DDP awareness (medium, medium value)
6. Auto S3 upload (medium, high value)
7. Cost tracking (medium, medium value)

