# runctl Features

## Core Features

### Multi-Platform Training
- **Local**: Execute training scripts locally with automatic `uv` detection
- **RunPod**: Full integration with RunPod GPU pods
- **AWS EC2**: Spot and on-demand instance management

### Checkpoint Management
- List checkpoints with metadata (size, modified time)
- Inspect checkpoint details
- Resume training from checkpoints
- Automatic checkpoint organization

### Monitoring
- Real-time log following (`tail -f` style)
- Checkpoint directory watching
- Training session tracking
- Progress visualization

### Configuration
- TOML-based configuration
- Sensible defaults
- Platform-specific settings
- Environment variable support

## Modern Scripting Helpers

### Justfile Integration
```bash
just build          # Build release binary
just train-local    # Train locally
just runpod-create  # Create RunPod pod
just monitor        # Monitor training
```

### uv Integration
- Automatically detects and uses `uv` for Python scripts
- Falls back to `python3` if `uv` not available
- PEP 723 script support

## Architecture

- **Rust CLI**: Fast, reliable, single binary
- **Async runtime**: Tokio for concurrent operations
- **Modular design**: Easy to extend with new platforms
- **Error handling**: User-friendly error messages with `anyhow`
- **Logging**: Structured logging with `tracing`

## Based on Real Training Scripts

### Patterns Adopted

1. **Checkpoint Strategy** (from idf-est, matryoshka-box)
   - Frequent checkpointing (every N epochs)
   - Auto-resume on restart
   - Best model tracking
   - Checkpoint cleanup

2. **Ephemeral Training** (from idf-est)
   - Graceful shutdown handling
   - Persistent checkpoint directories
   - Lock files to prevent concurrent training

3. **Cloud Optimization** (from matryoshka-box)
   - Larger batch sizes for GPU
   - Multi-GPU DDP support structure
   - Cloud-specific configs

4. **Cost Optimization** (from decksage)
   - Spot instance support
   - Automatic fallback to on-demand
   - S3 integration for data/output

## Usage Patterns

### Development Workflow
```bash
# Quick local test
runctl local training/train.py --epochs 1

# Full training
runctl local training/train.py --epochs 50 --batch-size 128
```

### Cloud Training Workflow
```bash
# RunPod
runctl runpod create
runctl runpod train <pod-id> training/train.py --background
runctl runpod monitor <pod-id> --follow

# AWS
runctl aws create --spot
runctl aws train <instance-id> training/train.py
runctl aws monitor <instance-id> --follow
```

### Monitoring Workflow
```bash
# Watch training progress
runctl monitor --log training.log --follow

# Check checkpoint status
runctl monitor --checkpoint checkpoints/ --follow

# One-time status
runctl checkpoint list checkpoints/
```

## Extensibility

Easy to add:
- New platforms (GCP, Azure, etc.)
- New checkpoint formats (TensorFlow, JAX, etc.)
- New monitoring backends
- Custom training hooks

## Performance

- **Startup**: < 100ms
- **Checkpoint listing**: O(n) where n = checkpoints
- **Log monitoring**: File watching (efficient)
- **Binary size**: ~5MB (release), ~54MB (debug)

