# trainctl

Modern training orchestration CLI for ML workloads. Supports local, RunPod, and AWS EC2 training with unified checkpoint management and monitoring.

## Features

- **Multi-platform**: Local, RunPod, AWS EC2
- **Checkpoint management**: List, inspect, resume from checkpoints
- **Real-time monitoring**: Follow training logs and checkpoint progress
- **Cost optimization**: Spot instances, efficient resource usage
- **Modern tooling**: Rust CLI with `uv`, `just` integration

## Installation

```bash
# Using cargo
cargo install --path .

# Or build from source
cargo build --release
```

## Quick Start

```bash
# Initialize config
trainctl init

# Train locally
trainctl local training/train.py --epochs 50

# Create RunPod pod
trainctl runpod create --gpu "NVIDIA GeForce RTX 4080 SUPER"

# Train on RunPod
trainctl runpod train <pod-id> training/train.py

# Monitor training
trainctl monitor --log training.log --follow

# List checkpoints
trainctl checkpoint list checkpoints/
```

## Configuration

Create `.trainctl.toml` or use `trainctl init`:

```toml
[runpod]
api_key = "your-api-key"  # Or read from ~/.cursor/mcp.json
default_gpu = "NVIDIA GeForce RTX 4080 SUPER"
default_disk_gb = 30

[aws]
region = "us-east-1"
default_instance_type = "t3.medium"
use_spot = true
s3_bucket = "your-bucket"

[checkpoint]
dir = "checkpoints"
save_interval = 5
keep_last_n = 10

[monitoring]
log_dir = "logs"
update_interval_secs = 10
```

## Commands

### Local Training

```bash
trainctl local <script> [args...]
```

Runs training script locally. Automatically uses `uv` for Python scripts if available.

### RunPod

```bash
# Create pod
trainctl runpod create [--name NAME] [--gpu GPU_TYPE] [--disk GB]

# Train on pod
trainctl runpod train <pod-id> <script> [--background]

# Monitor pod
trainctl runpod monitor <pod-id> [--follow]

# Download results
trainctl runpod download <pod-id> <remote> <local>
```

### AWS EC2

```bash
# Create instance
trainctl aws create [--instance-type TYPE] [--spot] [--spot-max-price PRICE]

# Train on instance
trainctl aws train <instance-id> <script> [--data-s3 S3_PATH] [--output-s3 S3_PATH]

# Monitor instance
trainctl aws monitor <instance-id> [--follow]

# Terminate instance
trainctl aws terminate <instance-id>
```

### Monitoring

```bash
# Monitor log file
trainctl monitor --log training.log [--follow]

# Monitor checkpoints
trainctl monitor --checkpoint checkpoints/ [--follow]
```

### Checkpoints

```bash
# List checkpoints
trainctl checkpoint list <dir>

# Show checkpoint info
trainctl checkpoint info <path>

# Resume from checkpoint
trainctl checkpoint resume <path> <script>
```

## Modern Scripting Helpers

### Using `just`

```bash
# Build
just build

# Train locally
just train-local training/train.py

# RunPod workflow
just runpod-create
just runpod-train <pod-id> training/train.py

# Monitor
just monitor training.log
```

### Using `uv`

The CLI automatically uses `uv` for Python scripts:

```bash
# This automatically uses `uv run` if uv is available
trainctl local training/train.py
```

## Examples

### Complete RunPod Workflow

```bash
# 1. Create pod
POD_ID=$(trainctl runpod create --gpu "RTX 4080 SUPER" | grep -o 'pod-[a-z0-9]*')

# 2. Upload and train
trainctl runpod train $POD_ID training/train_cloud.py --background

# 3. Monitor
trainctl runpod monitor $POD_ID --follow

# 4. Download results
trainctl runpod download $POD_ID /workspace/checkpoints/best.pt ./best.pt
```

### AWS Spot Instance Training

```bash
# 1. Create spot instance
INSTANCE_ID=$(trainctl aws create --spot --instance-type t3.medium | grep -o 'i-[a-z0-9]*')

# 2. Train
trainctl aws train $INSTANCE_ID training/train.py \
    --data-s3 s3://bucket/data.csv \
    --output-s3 s3://bucket/output/

# 3. Monitor
trainctl aws monitor $INSTANCE_ID --follow

# 4. Terminate
trainctl aws terminate $INSTANCE_ID
```

## Architecture

- **Rust CLI**: Fast, reliable, cross-platform
- **Async runtime**: Tokio for concurrent operations
- **AWS SDK**: Native AWS integration
- **Modular design**: Separate modules for each platform
- **Error handling**: `anyhow` for user-friendly errors

## Development

```bash
# Run tests
just test

# Lint
just lint

# Format
just fmt

# Dev build
just dev
```

## License

MIT OR Apache-2.0

