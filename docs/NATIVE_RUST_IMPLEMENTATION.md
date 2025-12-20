# Native Rust Implementation

## Overview

runctl now uses **native Rust AWS SDK** for all local S3 operations by default, with parallel transfers matching `s5cmd` performance. External tools like `s5cmd` are now optional.

## Why Native Rust?

1. **No External Dependencies**: Works out-of-the-box without requiring `s5cmd` or AWS CLI installation
2. **Better Error Handling**: Structured errors instead of parsing shell command output
3. **Progress Indicators**: Built-in progress bars for long operations
4. **JSON Output**: Native support for structured output
5. **Performance**: Parallel transfers using `tokio` tasks (10 concurrent by default, matching `s5cmd`)

## Implementation Details

### Local S3 Operations (Native Rust)

All local S3 operations (`upload`, `download`, `sync`) now use native Rust by default:

- **Parallel Uploads**: Uses `tokio::spawn` with concurrency limit (10 concurrent transfers)
- **Parallel Downloads**: Lists objects first, then downloads in parallel
- **Progress Bars**: Real-time progress indicators using `indicatif`
- **Error Handling**: Structured `TrainctlError` types

### Code Syncing (Native Rust)

Code syncing now uses native Rust implementations:

- **SSH Connection**: `ssh2-rs` crate for SSH connections and authentication
- **File Transfer**: SFTP for incremental sync (file-by-file comparison)
- **Archive Transfer**: `tar` crate + `flate2` for full sync (tar.gz archives)
- **Progress Indicators**: Real-time progress bars during sync operations
- **Error Handling**: Structured errors with actionable suggestions

**Benefits**:
- No dependency on external `rsync`, `tar`, or `ssh` commands
- Better error messages and progress feedback
- Works on systems without these tools installed
- More control over sync behavior and exclusions

### Instance-Side Operations (Shell Commands)

Instance-side operations (data loading) still use shell commands because:

1. **Rust Installation Complexity**: Installing Rust toolchain on instances is heavy (100+ MB)
2. **AMI Compatibility**: Most AMIs already have `aws s3 cp` and can install `s5cmd` easily
3. **SSM Execution**: Shell commands are simpler to execute via SSM than binary distribution

**Future Improvement**: Could distribute a statically-linked Rust binary via S3 for instance-side operations, but current approach is pragmatic.

## Performance Comparison

Native Rust implementation matches `s5cmd` performance:

- **Concurrency**: 10 parallel transfers (same as `s5cmd --concurrency 10`)
- **Throughput**: Similar to `s5cmd` for most workloads
- **Overhead**: Minimal - direct AWS SDK calls, no subprocess overhead

## Usage

### Default (Native Rust)
```bash
runctl s3 upload ./data s3://bucket/data --recursive
runctl s3 download s3://bucket/data ./local --recursive
runctl s3 sync ./local s3://bucket/data --direction up
```

### Optional: Use s5cmd
```bash
runctl s3 upload ./data s3://bucket/data --recursive --use-s5cmd
```

## Benefits

1. **Self-Contained**: No external tool dependencies
2. **Consistent**: Same error handling and output format across all operations
3. **Maintainable**: Pure Rust code, easier to debug and extend
4. **Fast**: Parallel transfers match external tool performance

## Technical Details

### Parallel Upload Implementation

```rust
// Collect all files
let files: Vec<_> = WalkDir::new(&source_path)
    .filter(|e| e.file_type().is_file())
    .collect();

// Upload with concurrency limit
const PARALLEL_CONCURRENCY: usize = 10;
for file in files {
    let handle = tokio::spawn(async move {
        upload_file_to_s3(&client, &bucket, &key, &path).await
    });
    handles.push(handle);
    
    // Limit concurrency
    if handles.len() >= PARALLEL_CONCURRENCY {
        futures::future::select_all(handles).await;
    }
}
```

### Parallel Download Implementation

```rust
// List all objects
let response = client.list_objects_v2()
    .bucket(bucket)
    .prefix(prefix)
    .send()
    .await?;

// Download in parallel
for obj in response.contents() {
    let handle = tokio::spawn(async move {
        download_object(&client, &bucket, &key, &local_path).await
    });
    // ... concurrency limiting
}
```

## Migration Notes

- `--use-s5cmd` flag now defaults to `false` (was `true`)
- Native Rust is now the default for all local operations
- Instance-side operations unchanged (still use shell commands)

