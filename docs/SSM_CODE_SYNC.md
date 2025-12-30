# SSM Code Sync Implementation

## Overview

SSM-based code syncing enables secure code transfer to EC2 instances without SSH keys. This is implemented using S3 as intermediate storage.

## Architecture

```
Local Machine                    S3                    EC2 Instance
     |                           |                          |
     |--1. Create tar.gz-------->|                          |
     |                           |                          |
     |--2. Upload to S3--------->|                          |
     |                           |                          |
     |--3. SSM Command------------------------------------->|
     |                           |                          |
     |                           |<--4. Download from S3----|
     |                           |                          |
     |                           |                          |
     |<--5. Extract & Verify-----|                          |
     |                           |                          |
     |--6. Cleanup S3------------>|                          |
```

## Implementation Details

### File: `src/aws/ssm_sync.rs`

**Key Functions:**

1. **`collect_files_to_sync()`**: 
   - Walks project directory
   - Respects `.gitignore` patterns
   - Supports `include_patterns` to override gitignore
   - Returns list of files to sync

2. **`sync_code_via_ssm()`**:
   - Creates tar.gz archive of project code
   - Uploads to S3 temporary location (`s3://bucket/runctl-temp/{instance-id}/{uuid}.tar.gz`)
   - Uses SSM to download and extract on instance
   - Verifies code was extracted correctly
   - Cleans up S3 temporary file

### Error Handling

- **Empty file list**: Returns clear error if no files to sync
- **Missing files**: Skips non-existent files with warning
- **S3 upload failures**: Returns detailed error message
- **SSM command failures**: Propagates SSM errors with context
- **Verification failures**: Warns but doesn't fail (allows partial sync)
- **Cleanup failures**: Warns but doesn't fail (S3 cleanup is best-effort)

### Progress Feedback

- Progress spinner for non-JSON output
- Status messages for each step:
  - "Creating code archive..."
  - "Archiving N files..."
  - "Archive created: X.X MB"
  - "Uploading to S3..."
  - "Downloading and extracting on instance..."
  - "Cleaning up temporary files..."

### Verification

After code sync, the system verifies:
- Script file exists
- Training directory exists
- Lists synced Python files

## Usage

### Prerequisites

1. **IAM Role with SSM permissions**:
   ```bash
   ./scripts/setup-ssm-role.sh
   ```

2. **S3 bucket configured**:
   ```toml
   [aws]
   s3_bucket = "your-bucket"
   ```

3. **IAM role with S3 access**:
   ```bash
   aws iam put-role-policy --role-name runctl-ssm-role \
     --policy-name S3BucketAccess \
     --policy-document '{
       "Version": "2012-10-17",
       "Statement": [{
         "Effect": "Allow",
         "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
         "Resource": "arn:aws:s3:::your-bucket/*"
       }]
     }'
   ```

### Example

```bash
# Create instance with SSM
runctl aws create g4dn.xlarge \
  --iam-instance-profile runctl-ssm-profile

# Train with automatic SSM code sync
runctl aws train i-xxx training/train_mnist.py \
  --sync-code \
  -- --epochs 10
```

## Automatic Detection

The system automatically uses SSM code sync when:
- Instance has IAM instance profile (SSM enabled)
- `s3_bucket` is configured in `.runctl.toml`

Otherwise, falls back to SSH-based sync (requires SSH key).

## Performance

- **Archive creation**: ~1-2 seconds for typical projects
- **S3 upload**: Depends on archive size and network (typically 5-30 seconds)
- **SSM download/extract**: ~10-30 seconds depending on archive size
- **Total**: Typically 20-60 seconds for code sync

## Limitations

1. **Archive size**: Large projects (>100MB) may take longer
2. **S3 costs**: Temporary storage costs are minimal (files are cleaned up)
3. **Network dependency**: Requires internet connectivity for S3 access
4. **IAM permissions**: Instance role must have S3 read/write access

## Troubleshooting

### "No files to sync"
- Check project root detection (looks for `requirements.txt`, `setup.py`, `pyproject.toml`, `Cargo.toml`, `.git`)
- Verify files aren't all gitignored
- Use `--include-pattern` to force include specific directories

### "S3 upload failed"
- Check IAM role has S3 write permissions
- Verify bucket exists and is accessible
- Check network connectivity

### "SSM command failed"
- Verify SSM connectivity: `aws ssm describe-instance-information --instance-ids i-xxx`
- Check instance has IAM role with SSM permissions
- Verify instance is in "Online" state

### "Verification warning"
- Code may have synced but some expected files missing
- Check instance disk space
- Verify S3 download completed successfully

## Future Improvements

- [ ] Incremental sync (only changed files)
- [ ] Compression level tuning for large projects
- [ ] Parallel file upload for very large archives
- [ ] Resume capability for interrupted syncs
- [ ] Progress bar for large uploads

