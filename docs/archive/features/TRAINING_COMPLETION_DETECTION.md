# Training Completion Detection

## Overview

`runctl` uses multiple heuristics to detect when training has completed. This document explains how completion detection works and how to ensure your training scripts work correctly with it.

## Detection Methods

`runctl` checks for training completion using the following methods (in order):

### 1. Completion Marker File (Primary)

**File**: `training_complete.txt` in the project directory

**How it works**: `runctl` checks if this file exists. If it does, training is considered complete.

**How to use**: Create this file when your training script finishes successfully:

```python
# At the end of your training script
with open("training_complete.txt", "w") as f:
    f.write(f"Training completed successfully at {time.time()}\n")
    f.write(f"Final checkpoint: {final_checkpoint_path}\n")
```

**Example**: See `training/train_mnist_e2e.py` for a complete example.

### 2. Process Status (PID File)

**File**: `training.pid` in the project directory

**How it works**: `runctl` checks if the process with the PID stored in this file is still running. If the process has exited, training is considered complete.

**How to use**: The training command automatically creates this file. Your script doesn't need to manage it.

**Note**: This method is less reliable than the marker file because:
- The process might crash without creating a completion marker
- The PID file might exist even if training failed

### 3. Log File Patterns

**File**: `training.log` in the project directory

**How it works**: `runctl` searches for completion keywords in the log file:
- "Training complete"
- "Training finished"
- "COMPLETE"
- "DONE"

**How to use**: Print one of these keywords when training completes:

```python
print("Training complete")
# or
print("Training finished")
# or
print("COMPLETE")
```

**Note**: This method is less reliable because:
- Log files might contain these keywords in error messages
- Log files might be truncated or missing

### 4. Exit Code (Validation)

**File**: `training_exit_code.txt` in the project directory

**How it works**: After detecting completion via other methods, `runctl` checks the exit code. If the exit code is non-zero, a warning is logged (but completion is still considered successful).

**How to use**: The training command automatically captures the exit code. Your script doesn't need to create this file manually.

**Note**: This is used for validation only. A non-zero exit code will trigger a warning but won't prevent completion detection.

## Best Practices

### 1. Always Create a Completion Marker

The most reliable way to ensure completion detection works is to create `training_complete.txt`:

```python
import os
import sys
from pathlib import Path

def main():
    try:
        # Your training code here
        train_model()
        
        # Create completion marker
        Path("training_complete.txt").write_text(
            f"Training completed successfully\n"
        )
        sys.exit(0)
    except Exception as e:
        print(f"Training failed: {e}", file=sys.stderr)
        sys.exit(1)
```

### 2. Handle Errors Gracefully

If your training script encounters an error, exit with a non-zero code:

```python
try:
    train_model()
except Exception as e:
    print(f"Error: {e}", file=sys.stderr)
    sys.exit(1)  # Non-zero exit code indicates failure
```

### 3. Save Checkpoints Regularly

Even though completion detection doesn't require checkpoints, saving them regularly ensures you don't lose progress:

```python
for epoch in range(num_epochs):
    train_epoch()
    save_checkpoint(f"checkpoint_epoch_{epoch}.pt")
```

### 4. Use the Example Script as a Template

The `training/train_mnist_e2e.py` script demonstrates all best practices:
- Creates completion marker
- Saves checkpoints
- Handles errors
- Exits with appropriate codes

## Troubleshooting

### Training Never Completes

**Problem**: `--wait` times out even though training finished.

**Solutions**:
1. Ensure your script creates `training_complete.txt`
2. Check that the script is writing to the correct directory (project directory)
3. Verify the script is actually running (check `training.log`)
4. Check for errors in the log file

### False Positives

**Problem**: `--wait` reports completion but training actually failed.

**Solutions**:
1. Check the exit code in `training_exit_code.txt`
2. Verify checkpoints were actually created
3. Review `training.log` for error messages
4. Consider adding `--verify-checkpoints` flag (if implemented)

### Completion Detection Too Slow

**Problem**: Completion detection takes too long after training finishes.

**Solutions**:
1. Create `training_complete.txt` immediately when training finishes
2. Don't rely on log file patterns (they're checked last)
3. Ensure the script exits cleanly (so PID check works)

## Advanced: Custom Completion Detection

If you need custom completion detection, you can:

1. **Create a custom marker file**: Instead of `training_complete.txt`, create your own marker and modify the completion detection logic (requires code changes).

2. **Use exit codes**: Rely on the automatic exit code capture, but note that this only validates completion (doesn't detect it).

3. **Monitor logs manually**: Use `runctl aws monitor <instance-id>` to watch logs in real-time and detect completion manually.

## See Also

- `training/train_mnist_e2e.py` - Example training script with completion markers
- `docs/EXAMPLES.md` - More examples of training workflows
- `src/aws/training.rs` - Implementation of completion detection

