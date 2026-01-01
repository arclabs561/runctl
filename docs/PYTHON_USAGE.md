# Using runctl from Python

Use runctl programmatically from Python without PyO3 bindings.

## Quick Start

### Option 1: Python Wrapper

Use the provided wrapper script:

```python
import sys
from pathlib import Path

# Add scripts directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))

from runctl_wrapper import Trainctl

tc = Trainctl()

# Create instance
instance = tc.aws.create_instance(
    instance_type="g4dn.xlarge",
    spot=True,
    project_name="my-project"
)

# Start training
tc.aws.train(
    instance["instance_id"],
    "training/train.py",
    sync_code=True,
    args=["--epochs", "10"]
)

# Monitor
tc.aws.monitor(instance["instance_id"], follow=True)
```

### Option 2: Direct Subprocess

Call runctl directly:

```python
import subprocess
import json

# Create instance
result = subprocess.run(
    ["runctl", "aws", "create", "--instance-type", "g4dn.xlarge", "--output", "json"],
    capture_output=True,
    text=True
)
instance = json.loads(result.stdout)
instance_id = instance["instance_id"]

# Train
subprocess.run([
    "runctl", "aws", "train", instance_id, "training/train.py",
    "--sync-code", "--output", "json"
])
```

## Installation

The wrapper script requires no additional dependencies beyond Python 3.8+:

```bash
# No installation needed - just use the script
python scripts/runctl_wrapper.py
```

## API Reference

### Trainctl Class

Main wrapper class:

```python
tc = Trainctl(binary="runctl", output_format="json")
```

**Methods:**
- `version()` - Get runctl version

**Attributes:**
- `tc.aws` - AWS commands
- `tc.resources` - Resource management
- `tc.checkpoint` - Checkpoint operations

### AWS Commands

```python
# Create instance
instance = tc.aws.create_instance(
    instance_type="g4dn.xlarge",
    spot=True,
    spot_max_price=0.5,
    ami_id="ami-12345678",
    project_name="my-project"
)

# Train
tc.aws.train(
    instance_id="i-1234567890abcdef0",
    script="training/train.py",
    data_s3="s3://bucket/data/",
    sync_code=True,
    project_name="my-project",
    args=["--epochs", "10", "--batch-size", "32"]
)

# Stop instance
tc.aws.stop(instance_id="i-1234567890abcdef0", force=False)

# Terminate instance
tc.aws.terminate(instance_id="i-1234567890abcdef0", force=False)

# Monitor
tc.aws.monitor(instance_id="i-1234567890abcdef0", follow=True)

# List processes
tc.aws.processes(instance_id="i-1234567890abcdef0", watch=False)
```

### Resource Commands

```python
# List resources
resources = tc.resources.list(
    platform="aws",  # or "runpod", "local", "all"
    detailed=True,
    limit=10
)

# Get summary
summary = tc.resources.summary()

# Stop all
tc.resources.stop_all(force=False)
```

### Checkpoint Commands

```python
# List checkpoints
checkpoints = tc.checkpoint.list("checkpoints/")

# Get checkpoint info
info = tc.checkpoint.info("checkpoints/checkpoint_epoch_5.pt")
```

## Error Handling

The wrapper raises `TrainctlError` on failures:

```python
from runctl_wrapper import Trainctl, TrainctlError

tc = Trainctl()

try:
    instance = tc.aws.create_instance("g4dn.xlarge")
except TrainctlError as e:
    print(f"Failed to create instance: {e}")
    sys.exit(1)
```

## Examples

### Complete Training Workflow

```python
from runctl_wrapper import Trainctl, TrainctlError

tc = Trainctl()

try:
    # Create instance
    print("Creating instance...")
    instance = tc.aws.create_instance(
        instance_type="g4dn.xlarge",
        spot=True,
        project_name="training-run-1"
    )
    instance_id = instance["instance_id"]
    print(f"Created: {instance_id}")
    
    # Wait for instance to be ready (you'd add a wait loop here)
    # ...
    
    # Start training
    print("Starting training...")
    tc.aws.train(
        instance_id,
        "training/train.py",
        sync_code=True,
        data_s3="s3://my-bucket/data/",
        args=["--epochs", "50"]
    )
    
    # Monitor
    print("Monitoring training...")
    tc.aws.monitor(instance_id, follow=True)
    
    # Stop when done
    print("Stopping instance...")
    tc.aws.stop(instance_id)
    
except TrainctlError as e:
    print(f"Error: {e}")
    sys.exit(1)
```

### Integration with PyTorch Lightning

```python
from pytorch_lightning import Callback
from runctl_wrapper import Trainctl

class TrainctlCheckpointCallback(Callback):
    """Upload checkpoints to S3 via runctl."""
    
    def __init__(self, instance_id: str):
        self.tc = Trainctl()
        self.instance_id = instance_id
    
    def on_train_epoch_end(self, trainer, pl_module):
        checkpoint_path = trainer.checkpoint_callback.best_model_path
        if checkpoint_path:
            # Upload checkpoint
            subprocess.run([
                "runctl", "s3", "upload",
                checkpoint_path,
                f"s3://my-bucket/checkpoints/{checkpoint_path.name}"
            ])
```

## Using with uvx Scripts

You can create standalone Python scripts that use runctl:

```python
#!/usr/bin/env -S uvx python
# /// script
# requires-python = ">=3.8"
# dependencies = []

import subprocess
import json

def main():
    result = subprocess.run(
        ["runctl", "resources", "list", "--output", "json"],
        capture_output=True,
        text=True
    )
    resources = json.loads(result.stdout)
    print(f"Found {len(resources)} resources")

if __name__ == "__main__":
    main()
```

## Limitations

1. **No async support**: The wrapper uses synchronous subprocess calls
2. **No streaming**: Can't stream output in real-time (use `follow=True` for monitoring)
3. **Error handling**: Errors are returned as JSON, not Python exceptions (except `TrainctlError`)
4. **Performance**: Subprocess overhead (minimal for most use cases)

## When to Use Python Bindings Instead

Consider PyO3 bindings if you need:
- Async/await support
- Real-time streaming
- Lower latency (no subprocess overhead)
- Direct access to Rust types

For most use cases, the wrapper is sufficient and simpler.

## See Also

- `scripts/runctl_wrapper.py` - Full wrapper implementation
- `examples/python_usage.py` - Example usage
- `docs/PYTHON_BINDINGS_ANALYSIS.md` - Analysis of Python bindings

