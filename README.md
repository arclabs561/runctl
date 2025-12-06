# trainctl

Modern training orchestration CLI for ML workloads. Supports local, RunPod, and AWS EC2 training with unified checkpoint management and monitoring.

## Features

- **AWS EC2** (Primary): Full-featured, well-tested, production-ready
- **Local training**: Quick local development
- **RunPod** (Experimental): GPU pods (less tested)
- **Checkpoint management**: List, inspect, resume from checkpoints
- **Real-time monitoring**: Interactive `top` command with ratatui dashboard
- **Native S3 operations**: Parallel uploads/downloads without external tools
- **EBS optimization**: Auto-configured IOPS/throughput for data loading
- **SSM integration**: Secure command execution without SSH keys
- **Cost optimization**: Spot instances, efficient resource usage
- **Modern tooling**: Rust CLI with `uv`, `just` integration

## Installation

```bash
# Using cargo
cargo install --path .

# Or build from source
cargo build --release
```

## Quick Start (AWS EC2)

```bash
# Initialize config
trainctl init

# Create spot instance
INSTANCE_ID=$(trainctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# Train with automatic code sync
trainctl aws train $INSTANCE_ID training/train.py --sync-code

# Monitor training
trainctl aws monitor $INSTANCE_ID --follow

# Check resource usage
trainctl aws processes $INSTANCE_ID --watch

# Stop when done (preserves data)
trainctl aws stop $INSTANCE_ID
```

## Testing with Temporary Credentials

For secure testing, use IAM roles with temporary credentials instead of long-term access keys:

```bash
# Setup test environment (one-time)
./scripts/setup-test-role.sh

# Verify setup is correct
./scripts/verify-setup.sh

# Run comprehensive test suite
./scripts/run-all-tests.sh

# Or test individually
source scripts/assume-test-role.sh
./scripts/test-auth.sh                    # Basic authentication
./scripts/test-security-boundaries.sh      # Security verification
./scripts/test-trainctl-integration.sh     # trainctl integration
```

See [docs/AWS_TESTING_SETUP.md](docs/AWS_TESTING_SETUP.md) for detailed setup instructions and [docs/AWS_TESTING_REVIEW.md](docs/AWS_TESTING_REVIEW.md) for review of security and robustness.

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

### Complete AWS Training Workflow

```bash
# 1. Create spot instance with data volume
INSTANCE_ID=$(trainctl aws create \
    --spot \
    --instance-type g4dn.xlarge \
    --data-volume-size 100 \
    | grep -o 'i-[a-z0-9]*')

# 2. Train with code sync and S3 data
trainctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://bucket/datasets/ \
    --output-s3 s3://bucket/checkpoints/

# 3. Monitor in real-time
trainctl aws monitor $INSTANCE_ID --follow

# 4. Check resource usage
trainctl aws processes $INSTANCE_ID --watch

# 5. Stop (preserves data) or terminate
trainctl aws stop $INSTANCE_ID
```

## Architecture

- **Rust CLI**: Fast, reliable, cross-platform
- **Async runtime**: Tokio for concurrent operations
- **AWS SDK**: Native AWS integration
- **Modular design**: Separate modules for each platform
- **Error handling**: Custom `TrainctlError` types in library, `anyhow` at CLI boundary

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

