# PEP 723 Support Plan

## Overview

PEP 723 allows inline script dependencies in Python scripts using special comment blocks. This enables scripts to declare dependencies without a separate `requirements.txt` file.

## PEP 723 Format

```python
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "torch>=2.0.0",
#     "torchvision>=0.15.0",
# ]
# ///
```

## Current State

runctl does **not** currently support PEP 723. It only checks for `requirements.txt`.

## Implementation Plan

### 1. Detect PEP 723 Scripts

Add function to detect inline dependencies:

```rust
fn detect_pep723_dependencies(script_path: &Path) -> Result<Option<Vec<String>>> {
    let content = std::fs::read_to_string(script_path)?;
    
    // Look for PEP 723 marker: # /// script
    if !content.contains("# /// script") {
        return Ok(None);
    }
    
    // Parse dependencies from comment block
    // Extract dependencies array
    // Return list of dependency strings
}
```

### 2. Support `uv run`

`uv run` automatically handles PEP 723 scripts:

```bash
uv run script.py  # Automatically installs inline dependencies
```

Update training command to:
1. Check for PEP 723 dependencies
2. Use `uv run` if available and PEP 723 detected
3. Fallback to `python3` + manual dependency install

### 3. Integration Points

- **Local training** (`src/local.rs`): Use `uv run` for PEP 723 scripts
- **AWS training** (`src/aws/training.rs`): Detect PEP 723, use `uv run` if available
- **Dependency installation**: Skip if using `uv run` (it handles it)

### 4. Priority Order

1. If `uv` available and PEP 723 detected → use `uv run`
2. If `requirements.txt` exists → install deps, then `python3`
3. Otherwise → just `python3`

## Benefits

- No separate `requirements.txt` needed for simple scripts
- Self-contained scripts
- Better developer experience
- Works with `uv` ecosystem

## Implementation Status

- [ ] Add PEP 723 detection
- [ ] Add `uv run` support
- [ ] Update local training
- [ ] Update AWS training
- [ ] Add tests
- [ ] Update documentation

