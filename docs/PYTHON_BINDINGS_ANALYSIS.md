# Python Bindings Analysis

**Question**: Should we expose Python bindings for trainctl?

## Current State

### What Exists
- ✅ Rust CLI tool (`trainctl` binary)
- ✅ Rust library (`src/lib.rs` with public API)
- ✅ Python script detection (uses `uv` if available)
- ✅ Separate Python package (`pyproject.toml` for metrics utilities)

### Current Python Integration
- `src/local.rs` detects Python scripts and uses `uv` if available
- Python scripts can call `trainctl` via subprocess
- No programmatic Python API currently

## Use Cases for Python Bindings

### Potential Use Cases
1. **Orchestration from Training Scripts**
   ```python
   import trainctl
   
   # Create instance and train programmatically
   instance = trainctl.aws.create_instance(instance_type="g4dn.xlarge")
   trainctl.aws.train(instance, script="train.py", sync_code=True)
   ```

2. **Integration with ML Frameworks**
   ```python
   # In PyTorch Lightning callback
   from trainctl import checkpoint
   
   def on_train_epoch_end(self, trainer, pl_module):
       checkpoint.upload(trainer.checkpoint_callback.best_model_path)
   ```

3. **Workflow Automation**
   ```python
   # Automated training pipeline
   for config in configs:
       instance = trainctl.create_instance(config)
       trainctl.train(instance, config.script)
       trainctl.monitor(instance)
   ```

### Current Alternative (Subprocess)
```python
import subprocess

# Works today, no bindings needed
result = subprocess.run(
    ["trainctl", "aws", "create", "--instance-type", "g4dn.xlarge"],
    capture_output=True,
    text=True
)
```

## Analysis: Should We Add Python Bindings?

### ✅ Arguments FOR

1. **Better Developer Experience**
   - Type hints and autocomplete
   - Native Python error handling
   - No string parsing of CLI output

2. **Integration with ML Ecosystem**
   - Python is the primary ML language
   - Could integrate with PyTorch, Lightning, etc.
   - Enables programmatic workflows

3. **Architecture Already Supports It**
   - `src/lib.rs` already exposes library API
   - Core logic is separated from CLI
   - Would just need PyO3 wrapper

4. **Modern Practice**
   - Many Rust tools expose Python bindings (e.g., `ruff`, `polars`)
   - PyO3 is mature and well-supported
   - `maturin` makes packaging straightforward

### ❌ Arguments AGAINST

1. **Complexity Cost**
   - Adds PyO3 dependency and build complexity
   - Need to maintain Python package separately
   - Cross-platform build challenges
   - Version coordination (Rust crate vs Python package)

2. **Subprocess Works Fine**
   - CLI is already callable from Python
   - JSON output format available (`--output json`)
   - No compilation needed for Python users
   - Simpler deployment (just install binary)

3. **Limited Value**
   - Most trainctl operations are "fire and forget"
   - Users typically don't need fine-grained control
   - CLI interface is the primary use case

4. **Maintenance Burden**
   - Two APIs to maintain (CLI + Python)
   - Need to keep them in sync
   - More test surface area
   - Documentation for both

5. **Current Architecture**
   - trainctl is CLI-first, not library-first
   - Core value is in orchestration, not programmatic API
   - Library API exists but isn't the primary interface

## Recommendation

### **Recommendation: NOT YET, but keep the door open**

**Rationale:**

1. **YAGNI Principle**: We don't have evidence of users needing Python bindings
2. **Subprocess is sufficient**: JSON output + subprocess covers most use cases
3. **Focus on core value**: trainctl's value is in CLI orchestration, not programmatic API
4. **Complexity vs benefit**: The maintenance cost doesn't justify the benefit yet

### When to Revisit

Consider Python bindings if:

1. **Clear user demand**: Multiple users request it
2. **Specific integration needs**: Need to integrate with specific ML frameworks
3. **Library-first use cases**: Users want to build on trainctl as a library
4. **Performance requirements**: Subprocess overhead becomes a problem

### Alternative: Improve CLI for Python Integration

Instead of Python bindings, we could:

1. **Better JSON output**
   ```bash
   trainctl aws create --output json | jq -r '.instance_id'
   ```

2. **Python helper scripts**
   ```python
   # scripts/trainctl_wrapper.py
   import subprocess
   import json
   
   def create_instance(instance_type):
       result = subprocess.run(
           ["trainctl", "aws", "create", "--instance-type", instance_type, "--output", "json"],
           capture_output=True, text=True
       )
       return json.loads(result.stdout)
   ```

3. **uvx scripts**
   ```python
   #!/usr/bin/env -S uvx python
   # /// script
   # requires-python = ">=3.8"
   # dependencies = ["trainctl"]
   
   import trainctl  # Would be a thin wrapper around CLI
   ```

## Implementation Path (If We Do It)

If we decide to add Python bindings:

### Architecture
```
trainctl/
├── trainctl-core/     # Pure Rust library (current src/)
├── trainctl-cli/      # CLI binary (current main.rs)
└── trainctl-py/       # PyO3 bindings (new)
```

### Steps
1. Refactor to workspace structure
2. Add `trainctl-py/` crate with PyO3
3. Expose key functions: `create_instance`, `train`, `monitor`, `list_resources`
4. Use `maturin` for packaging
5. Publish to PyPI as `trainctl` package

### What to Expose
- Core operations: instance creation, training, monitoring
- Resource management: list, stop, terminate
- Checkpoint operations: list, upload, download
- Configuration: load, validate

### What NOT to Expose
- CLI-specific: argument parsing, output formatting
- Internal: retry logic, AWS SDK details
- Low-level: SSM commands, S3 operations (unless needed)

## Conclusion

**Current recommendation**: Don't add Python bindings yet.

**Reasoning**:
- No clear user demand
- Subprocess + JSON output is sufficient
- Focus should be on core CLI functionality
- Can always add later if needed

**If users request it**: Start with a thin Python wrapper around CLI, then consider PyO3 bindings if there's clear value.

