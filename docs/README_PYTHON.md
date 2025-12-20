# Using runctl from Python

runctl can be used from Python in two ways:

## Quick Start

### 1. Python Wrapper (Recommended)

```python
from runctl_wrapper import Trainctl

tc = Trainctl()
instance = tc.aws.create_instance("g4dn.xlarge", spot=True)
tc.aws.train(instance["instance_id"], "train.py", sync_code=True)
```

### 2. Direct Subprocess

```python
import subprocess
import json

result = subprocess.run(
    ["runctl", "aws", "create", "--instance-type", "g4dn.xlarge", "--output", "json"],
    capture_output=True, text=True
)
instance = json.loads(result.stdout)
```

## Installation

No installation needed - just use the wrapper script:

```bash
# Use the wrapper
python scripts/runctl_wrapper.py

# Or import in your code
from runctl_wrapper import Trainctl
```

## Documentation

- **Full Guide**: [docs/PYTHON_USAGE.md](docs/PYTHON_USAGE.md)
- **API Reference**: See `scripts/runctl_wrapper.py`
- **Examples**: `examples/python_usage.py`

## Why No PyO3 Bindings?

We evaluated Python bindings but decided against them for now:

- ✅ Subprocess + JSON output is sufficient
- ✅ No compilation needed
- ✅ Simpler deployment
- ✅ Easier maintenance

See [docs/PYTHON_BINDINGS_ANALYSIS.md](docs/PYTHON_BINDINGS_ANALYSIS.md) for full analysis.

If you need PyO3 bindings, please open an issue with your use case.

