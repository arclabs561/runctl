# runctl

ML training orchestration CLI for local, RunPod, and AWS EC2 with unified checkpoint management.

## Prerequisites

- Rust 1.70+
- AWS credentials configured (`aws configure` or IAM role)
- SSM agent enabled on EC2 (default on Amazon Linux 2)

## Installation

```bash
cargo install --path .
# or
cargo build --release
```

## Quick Start

```bash
# Initialize config
runctl init

# Create instance (use t3.micro for testing, ~$0.01/hr)
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)

# Train (waits until complete)
runctl aws train $INSTANCE_ID training/train_mnist.py --sync-code --wait

# Or use workflow command
runctl workflow train training/train_mnist.py --instance-type t3.micro --spot
```

Examples: `./examples/complete_workflow.sh` ([examples/README.md](examples/README.md))

## Commands

### AWS EC2

```bash
runctl aws create [--instance-type TYPE] [--spot] [--wait] [--output FORMAT]
runctl aws train <instance-id> <script> [--sync-code] [--wait] [--data-s3 PATH] [--output-s3 PATH]
runctl aws monitor <instance-id> [--follow]
runctl aws processes <instance-id> [--watch]
runctl aws start|stop|terminate <instance-id>
runctl aws status|wait <instance-id>
```

### Local

```bash
runctl local <script> [args...]
```

Uses `uv` for Python scripts if available, otherwise falls back to `python3`.

### RunPod

```bash
runctl runpod create [--gpu TYPE] [--disk GB]
runctl runpod train <pod-id> <script>
runctl runpod monitor <pod-id> [--follow]
runctl runpod download <pod-id> <remote> <local>
```

### Resources

```bash
runctl resources list [--platform aws|runpod|local] [--detailed]
runctl resources summary
runctl resources insights
runctl resources cleanup [--dry-run] [--force]
```

### S3

```bash
runctl s3 upload <local> <s3://bucket/key> [--recursive]
runctl s3 download <s3://bucket/key> <local> [--recursive]
runctl s3 sync <source> <dest> [--direction up|down]
runctl s3 list <s3://bucket/prefix> [--recursive]
runctl s3 cleanup <s3://bucket/prefix> --keep-last-n <N> [--dry-run]
```

### Monitoring & Checkpoints

```bash
runctl monitor --log <file> [--follow]
runctl monitor --checkpoint <dir> [--follow]
runctl checkpoint list <dir>
runctl checkpoint info <path>
runctl checkpoint resume <path> <script>
runctl top
```

### Workflow

```bash
runctl workflow train <script> [--instance-type TYPE] [--spot]
```

### Docker

```bash
runctl docker build [--push] [--repository NAME]
runctl docker train <image> <script>
```

## Configuration

Create `.runctl.toml` or use `runctl init`:

```toml
[aws]
region = "us-east-1"
default_instance_type = "t3.medium"
use_spot = true
s3_bucket = "your-bucket"

[runpod]
api_key = "your-key"  # or from ~/.cursor/mcp.json
default_gpu = "NVIDIA GeForce RTX 4080 SUPER"

[checkpoint]
dir = "checkpoints"
save_interval = 5
```

## Development

```bash
just test    # Run tests
just lint    # Lint
just fmt     # Format
just dev     # Dev build
```

## License

MIT OR Apache-2.0

