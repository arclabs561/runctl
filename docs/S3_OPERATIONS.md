# S3 Operations

S3 upload, download, sync, and cleanup operations. Uses s5cmd when available, otherwise falls back to AWS SDK.

## Commands

```bash
runctl s3 upload <local> <s3://bucket/key> [--recursive]
runctl s3 download <s3://bucket/key> <local> [--recursive]
runctl s3 sync <source> <dest> [--direction up|down]
runctl s3 list <s3://bucket/prefix> [--recursive] [--human-readable]
runctl s3 cleanup <s3://bucket/prefix> --keep-last-n <N> [--dry-run]
runctl s3 watch <s3://bucket/prefix> [--interval <secs>]
runctl s3 review <s3://bucket/prefix> [--detailed]
```

## Local Cleanup

```bash
runctl checkpoint cleanup checkpoints/ --keep-last-n 10 [--dry-run]
```

## Installation

```bash
# macOS
brew install s5cmd

# Linux: Download from https://github.com/peak/s5cmd/releases
```

## Examples

```bash
# Upload checkpoints after training
runctl s3 upload ./checkpoints/ s3://bucket/checkpoints/ --recursive

# Watch for new checkpoints
runctl s3 watch s3://bucket/checkpoints/ --interval 30

# Cleanup old checkpoints
runctl s3 cleanup s3://bucket/checkpoints/ --keep-last-n 10 --dry-run
runctl checkpoint cleanup checkpoints/ --keep-last-n 5
```

