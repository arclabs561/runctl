# Workspace and Code Copying

## Overview

runctl uses a **bare VM approach** (not Docker/containers). When you run training, your project code is copied to the EC2 instance and runs directly on the instance.

## What Gets Copied

When you use `--sync-code` (or it's enabled by default), runctl:

1. **Detects your project root** by looking for:
   - `requirements.txt`
   - `setup.py`
   - `pyproject.toml`
   - `Cargo.toml`
   - `.git` directory

2. **Creates a tar archive** of the entire project root

3. **Respects `.gitignore` files** - All patterns in `.gitignore` are automatically excluded
   - This includes common patterns like `.git/`, `__pycache__/`, `*.pyc`, `node_modules/`, `.venv/`, etc.
   - You can customize exclusions by adding patterns to your `.gitignore` file
   - The tool uses the `ignore` crate to parse `.gitignore` according to the gitignore spec
   - **Override for data directories**: Use `--include-pattern` to sync gitignored files (e.g., `--include-pattern data/` to sync `data/` even if it's gitignored)

4. **Transfers via SSH** using tar pipe:
   ```bash
   tar -czf - [excludes] | ssh [instance] 'mkdir -p [project_dir] && tar -xzf -'
   ```

5. **Extracts to** `/home/{user}/{project_name}/` on the instance

## Workspace Structure on Instance

```
/home/ubuntu/{project_name}/     # Your project code
├── training/
│   └── train.py                 # Your training script
├── requirements.txt             # Dependencies
└── ...                          # All other project files

/home/ubuntu/data/               # Local data (if no EBS volume)
/mnt/data/                       # EBS volume mount (if attached)
```

## Auto-Created Services

When an instance starts, the user-data script automatically:

### ✅ Installed Services
- **Python 3** + pip
- **uv** (Python package manager) - installed automatically
- **git**, **curl**, build tools
- **AWS CLI** (on most AMIs)
- **s5cmd** (if available, for fast S3 transfers)

### ❌ NOT Installed
- **Docker** - Not used (bare VM)
- **Containers** - Not used
- **Kubernetes** - Not used
- **Systemd services** - Training runs as user process

### Setup Steps
1. System updates (apt/yum)
2. Install Python dependencies
3. Install `uv` for faster Python package management
4. Create project directory: `/home/{user}/{project_name}`
5. Setup data volume (if attached): `/mnt/data`
6. Configure `PYTHONPATH` to include project directory
7. Create helper script: `~/start_training.sh`

## Caching

### Incremental Code Sync (NEW)
- ✅ **Automatic detection**: Checks if code already exists on instance
- ✅ **Incremental sync**: Uses `rsync` for faster updates (only syncs changed files)
- ✅ **Fallback**: Falls back to full `tar` sync if `rsync` unavailable
- ✅ **Better exclusions**: Excludes `node_modules`, `.venv` in addition to standard patterns

### Dependency Caching
- ✅ **Pre-installed libraries**: Common ML libraries (numpy, pandas) pre-installed in user-data
- ✅ **Cache directory**: `/opt/runctl-cache` created for future dependency caching
- ⚠️ **Per-project deps**: Still installed from `requirements.txt` per project

### Optimization Strategies
1. **Incremental code sync** - Only syncs changed files (automatic)
2. **Use EBS volumes** for persistent data (pre-warmed volumes)
3. **Use S3** for data storage (fast with s5cmd)
4. **Reuse instances** instead of creating new ones
5. **Pre-installed libs** - Common ML libraries already available

## Training Execution

Training runs as:
- **User process** (not systemd service)
- **Background process** (with `&` and PID tracking)
- **Python module** execution: `python3 -m training.train_lightning`

The training script:
1. Changes to project directory
2. Sets up environment (PYTHONPATH, PATH)
3. Installs dependencies from `requirements.txt` (if needed)
4. Runs training script as Python module
5. Logs to `{project_dir}/training.log`
6. Saves PID to `{project_dir}/training.pid`

## Example Flow

```bash
# 1. Create instance
runctl aws create --instance-type g4dn.xlarge

# 2. Train (with code sync)
runctl aws train i-123 training/train.py --sync-code

# What happens:
# - Project root detected: /path/to/my-project
# - Code synced to: /home/ubuntu/my-project/
# - Training runs: python3 -m training.train
# - Logs: /home/ubuntu/my-project/training.log
```

## Differences from Docker

| Aspect | runctl (Bare VM) | Docker |
|--------|-------------------|--------|
| Isolation | None (shared OS) | Container isolation |
| Caching | None | Layer caching |
| Setup | User-data script | Dockerfile |
| Persistence | EBS volumes | Volumes/bind mounts |
| Speed | Fast (no container overhead) | Slower (container startup) |
| Flexibility | Full OS access | Limited to container |

