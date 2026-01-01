# Developer Experience Critique: E2E Examples Analysis

## Executive Summary

The tool's examples and E2E tests reveal significant gaps between the intended developer experience and the actual workflow. Developers are forced to write substantial orchestration code, parse unstructured output, and manually handle timing/waiting that the tool should abstract away.

## Critical Issues

### 1. Fragile Output Parsing

**Problem**: Examples use `grep -o` to extract instance IDs from command output.

```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro | grep -o 'i-[a-z0-9]*')
```

**Why this is bad**:
- Breaks if output format changes
- Fails silently if instance creation fails but still outputs text
- No structured output option for scripting
- Forces developers to write brittle parsing logic

**What should exist**:
- `--output json` flag that returns structured data
- `--instance-id` flag to output only the ID
- Exit codes that clearly indicate success/failure

### 2. Manual Waiting and Polling

**Problem**: Examples require manual `sleep 60` waits and manual status checks.

```bash
# IMPORTANT: Wait for instance to be ready (usually 30-60 seconds)
sleep 60
# Or check status: runctl aws instances list
```

**Why this is bad**:
- Developers must guess wait times (30s? 60s? 90s?)
- No built-in retry/wait logic
- Forces polling patterns in every script
- Race conditions when training starts before instance is ready

**What should exist**:
- `runctl aws create --wait` flag that blocks until ready
- `runctl aws train --wait` flag that blocks until training completes
- Automatic retry with exponential backoff
- Clear status indicators during waiting

### 3. Verification Logic Not Integrated

**Problem**: Examples require manual SSM commands to verify training completion.

```bash
aws ssm send-command \
    --instance-ids $INSTANCE_ID \
    --document-name "AWS-RunShellScript" \
    --parameters "commands=['test -f training_complete.txt && echo COMPLETE || echo NOT_COMPLETE']" \
    --output text \
    --query 'Command.CommandId' > /tmp/check_cmd.txt

sleep 5
STATUS=$(aws ssm get-command-invocation \
    --command-id $(cat /tmp/check_cmd.txt) \
    --instance-id $INSTANCE_ID \
    --query 'StandardOutputContent' \
    --output text)
```

**Why this is bad**:
- Forces developers to use AWS CLI directly
- Complex multi-step verification
- No abstraction over SSM command lifecycle
- Error-prone (file I/O, command ID tracking)

**What should exist**:
- `runctl aws train --wait` that handles verification internally
- `runctl aws status $INSTANCE_ID` that shows training state
- Built-in completion detection (checkpoints, logs, markers)
- Clear success/failure indicators

### 4. E2E Tests Don't Test the CLI

**Problem**: E2E tests bypass the CLI entirely and use AWS SDK directly.

```rust
let response = ec2_client
    .run_instances()
    .image_id("ami-0c55b159cbfafe1f0")
    .instance_type(InstanceType::T3Micro)
    // ... direct AWS SDK calls
```

**Why this is bad**:
- Tests don't verify the actual developer workflow
- Tests don't catch CLI bugs or UX issues
- Tests require AWS SDK knowledge to understand
- Tests don't validate output formats or error messages

**What should exist**:
- Tests that use `cargo run -- aws create ...` (actual CLI)
- Tests that verify output formats (JSON, text)
- Tests that verify error messages are helpful
- Tests that verify waiting/retry logic works

### 5. Examples Don't Match Real Usage

**Problem**: Examples reference scripts that may not exist (`training/train.py`).

```bash
runctl aws train $INSTANCE_ID training/train.py \
    --sync-code \
    --data-s3 s3://my-bucket/data/
```

**Why this is bad**:
- Developers copy examples that fail immediately
- No validation that referenced files exist
- Examples show ideal scenarios, not edge cases
- No troubleshooting guidance when examples fail

**What should exist**:
- Examples that use actual files in the repo
- Validation that referenced files exist before running
- Examples for common failure scenarios
- Clear error messages when prerequisites are missing

### 6. No Built-in Orchestration

**Problem**: Every workflow requires custom bash scripts.

```bash
#!/bin/bash
set -e

echo "=== Complete E2E Training Workflow ==="

# 1. Create instance
INSTANCE_ID=$(runctl aws create ... | grep -o 'i-[a-z0-9]*')

# 2. Wait
sleep 60

# 3. Train
runctl aws train $INSTANCE_ID ...

# 4. Monitor (background)
runctl aws monitor $INSTANCE_ID --follow &

# 5. Verify (manual SSM)
aws ssm send-command ...

# 6. Cleanup
runctl aws terminate $INSTANCE_ID
```

**Why this is bad**:
- Forces every developer to write the same orchestration
- No reusable patterns
- Error handling is manual and error-prone
- No atomic workflows (partial failures leave resources)

**What should exist**:
- `runctl workflow train` command that does create → train → wait → verify
- Built-in cleanup on failure
- Atomic operations with rollback
- Workflow templates for common patterns

## Specific Pain Points

### Instance Creation

**Current**:
```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro | grep -o 'i-[a-z0-9]*')
sleep 60  # Hope it's ready
```

**Should be**:
```bash
INSTANCE_ID=$(runctl aws create --spot --instance-type t3.micro --wait --output instance-id)
# Instance is ready, SSM is connected, no guessing
```

### Training Verification

**Current**:
```bash
# Manual SSM commands, file I/O, polling
aws ssm send-command ...
sleep 5
STATUS=$(aws ssm get-command-invocation ...)
```

**Should be**:
```bash
runctl aws train $INSTANCE_ID training/train.py --wait
# Training completes, exit code indicates success/failure
```

### Error Handling

**Current**: Silent failures, unclear error messages, manual cleanup

**Should be**:
- Clear error messages with actionable next steps
- Automatic cleanup on failure (optional flag)
- Structured error output for scripting

## Recommendations

### High Priority

1. **Add `--wait` flags** to all long-running operations
   - `runctl aws create --wait`
   - `runctl aws train --wait`
   - Blocks until operation completes, shows progress

2. **Add structured output options**
   - `--output json` for all commands
   - `--output instance-id` for single-value extraction
   - Consistent JSON schema across commands

3. **Add workflow commands**
   - `runctl workflow train` - complete training workflow
   - `runctl workflow cleanup` - cleanup all resources
   - Reduces boilerplate for common patterns

4. **Fix E2E tests to use CLI**
   - Test actual developer workflow
   - Verify output formats and error messages
   - Catch UX issues early

### Medium Priority

5. **Add status commands**
   - `runctl aws status $INSTANCE_ID` - show training state
   - `runctl aws wait $INSTANCE_ID` - wait for ready state
   - Clear indicators of what's happening

6. **Improve error messages**
   - Actionable next steps
   - Validation before operations
   - Clear prerequisites

7. **Add example validation**
   - Check that referenced files exist
   - Validate prerequisites before running
   - Provide working examples in repo

### Low Priority

8. **Add workflow templates**
   - Common patterns as reusable templates
   - Customizable workflows
   - Documentation for extending workflows

## Conclusion

The tool currently requires developers to write significant orchestration code that should be built into the tool itself. The examples reveal that the CLI doesn't provide the abstractions needed for a smooth developer experience. Fixing these issues would make the tool dramatically easier to use and reduce the cognitive load on developers.

