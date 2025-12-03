# Migration Guide: train-ops → trainctl

## Overview

The tool has been renamed from `train-ops` to `trainctl` for better CLI naming conventions. This guide helps you migrate your setup.

## What Changed

### Tool Name
- **Old**: `train-ops`
- **New**: `trainctl`

### Configuration Files
- **Old**: `.train-ops.toml`
- **New**: `.trainctl.toml`

### Configuration Directory
- **Old**: `~/.config/train-ops/`
- **New**: `~/.config/trainctl/`

### Environment Variables
- **Old**: `TRAIN_OPS_*`
- **New**: `TRAINCTL_*`

### AWS Tags
- **Old**: Tags containing `train-ops`
- **New**: Tags containing `trainctl`

## Migration Steps

### 1. Rebuild the Tool

```bash
cd /path/to/trainctl  # (formerly infra-utils)
cargo build --release
```

### 2. Rename Configuration File

```bash
# If you have a local config
mv .train-ops.toml .trainctl.toml

# If you have a global config
mv ~/.config/train-ops/config.toml ~/.config/trainctl/config.toml
```

### 3. Update Scripts and Aliases

Update any scripts, aliases, or automation that references `train-ops`:

```bash
# Old
train-ops local train.py

# New
trainctl local train.py
```

### 4. Update Environment Variables

If you use environment variables in your training scripts:

```bash
# Old
export TRAIN_OPS_CHECKPOINT_DIR=./checkpoints

# New
export TRAINCTL_CHECKPOINT_DIR=./checkpoints
```

### 5. Update AWS Tags (Optional)

If you want to update existing AWS instance tags:

```bash
# List instances with old tags
aws ec2 describe-instances --filters "Name=tag:train-ops,Values=*"

# Update tags (example)
aws ec2 create-tags \
  --resources i-1234567890abcdef0 \
  --tags Key=trainctl,Value=true
```

### 6. Rename Directory (Optional)

If you want to rename the project directory:

```bash
cd /Users/arc/Documents/dev
mv infra-utils trainctl
cd trainctl
```

**Note**: After renaming, you may need to:
- Update workspace paths in your IDE
- Update any hardcoded paths in scripts
- Rebuild the project

## Backward Compatibility

The old name is **not** supported. You must:
- Use `trainctl` for all commands
- Update configuration file names
- Update any automation/scripts

## Verification

After migration, verify everything works:

```bash
# Check version
trainctl --version

# Check config loads
trainctl init

# Test a command
trainctl resources list
```

## Common Issues

### Issue: "command not found: trainctl"

**Solution**: Rebuild and install:
```bash
cargo build --release
cargo install --path .
```

### Issue: Config file not found

**Solution**: Rename your config file:
```bash
mv .train-ops.toml .trainctl.toml
```

### Issue: Environment variables not working

**Solution**: Update variable names from `TRAIN_OPS_*` to `TRAINCTL_*`

## Questions?

If you encounter issues during migration, check:
1. All references updated in your scripts
2. Config file renamed correctly
3. Tool rebuilt and installed
4. Environment variables updated

