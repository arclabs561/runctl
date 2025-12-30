# runctl

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://github.com/arclabs561/runctl/actions/workflows/ci.yml/badge.svg)](https://github.com/arclabs561/runctl/actions/workflows/ci.yml)

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
runctl init

# Create spot instance
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# Train with automatic code sync
runctl aws train $INSTANCE_ID training/train_mnist.py --sync-code

# Monitor training
runctl aws monitor $INSTANCE_ID --follow

# Check resource usage
runctl aws processes $INSTANCE_ID --watch

# Stop when done (preserves data)
runctl aws stop $INSTANCE_ID

# Restart a stopped instance
runctl aws start $INSTANCE_ID --wait
```

### Example Training Script

We include a working MNIST training example in `training/train_mnist.py`:

```bash
# Test locally (requires PyTorch: pip install torch torchvision)
python training/train_mnist.py --epochs 5

# Or use runctl local
runctl local training/train_mnist.py --epochs 5

# Train on AWS
runctl aws train $INSTANCE_ID training/train_mnist.py --sync-code --epochs 10
```

See [training/README.md](training/README.md) for full documentation.

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
./scripts/test-runctl-integration.sh     # runctl integration
```

See [docs/AWS_TESTING_SETUP.md](docs/AWS_TESTING_SETUP.md) for detailed setup instructions.

## Configuration

Create `.runctl.toml` or use `runctl init`:

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
runctl local <script> [args...]
```

Runs training script locally. Automatically uses `uv` for Python scripts if available.

### RunPod

```bash
# Create pod
runctl runpod create [--name NAME] [--gpu GPU_TYPE] [--disk GB]

# Train on pod
runctl runpod train <pod-id> <script> [--background]

# Monitor pod
runctl runpod monitor <pod-id> [--follow]

# Download results
runctl runpod download <pod-id> <remote> <local>
```

### AWS EC2

```bash
# Create instance
runctl aws create [--instance-type TYPE] [--spot] [--spot-max-price PRICE]

# Train on instance
runctl aws train <instance-id> <script> [--data-s3 S3_PATH] [--output-s3 S3_PATH]

# Monitor instance
runctl aws monitor <instance-id> [--follow]

# Terminate instance
runctl aws terminate <instance-id>
```

### Monitoring

```bash
# Monitor log file
runctl monitor --log training.log [--follow]

# Monitor checkpoints
runctl monitor --checkpoint checkpoints/ [--follow]
```

### Checkpoints

```bash
# List checkpoints
runctl checkpoint list <dir>

# Show checkpoint info
runctl checkpoint info <path>

# Resume from checkpoint
runctl checkpoint resume <path> <script>
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
runctl local training/train.py
```

## Examples

### Complete AWS Training Workflow

```bash
# 1. Create spot instance with data volume
INSTANCE_ID=$(runctl aws create \
    --spot \
    --instance-type g4dn.xlarge \
    --data-volume-size 100 \
    | grep -o 'i-[a-z0-9]*')

# 2. Train with code sync and S3 data
runctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://bucket/datasets/ \
    --output-s3 s3://bucket/checkpoints/

# 3. Monitor in real-time
runctl aws monitor $INSTANCE_ID --follow

# 4. Check resource usage
runctl aws processes $INSTANCE_ID --watch

# 5. Stop (preserves data) or terminate
runctl aws stop $INSTANCE_ID
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

## Troubleshooting

**Instance creation fails:**
- Check AWS credentials: `aws sts get-caller-identity`
- Verify IAM permissions for EC2, SSM, S3
- Check region availability for instance type

**SSM connection fails:**
- Ensure SSM agent is running on instance
- Verify IAM instance profile has `AmazonSSMManagedInstanceCore`
- Check security group allows outbound HTTPS

**Training script not found:**
- Use absolute paths or paths relative to project root
- Check `--sync-code` is working: `runctl aws processes <instance-id>`

**Cost concerns:**
- Use `--spot` for instances (50-90% savings)
- Monitor with `runctl resources summary`
- Set up cleanup: `runctl resources cleanup --dry-run`

See [docs/AWS_TESTING_SETUP.md](docs/AWS_TESTING_SETUP.md) for detailed troubleshooting.

## License

MIT OR Apache-2.0

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.

