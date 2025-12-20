# Comprehensive Design Review

## Overview
This document provides a systematic review of all commands, subcommands, and their design/integration/nuances.

## Command Structure

### Top-Level Commands
1. `aws` - AWS EC2 instance management
2. `ebs` - EBS volume operations (nested under `aws`)
3. `s3` - S3 operations
4. `checkpoint` - Checkpoint management
5. `resources` - Resource listing and management
6. `config` - Configuration management
7. `monitor` - Monitoring operations
8. `local` - Local training
9. `top` - Interactive dashboard (ratatui)
10. `init` - Initialize configuration

## Issues Found

### 1. Error Handling Inconsistency

**Problem**: Mixed use of `anyhow::Result` and `crate::error::Result`

**Current State**:
- `main.rs`: Uses `anyhow::Result` (appropriate for CLI boundary)
- `aws.rs`: Mix of `anyhow::Result` and `crate::error::Result`
- `ebs.rs`: Uses `anyhow::Result` in some places
- `s3.rs`: Uses `crate::error::Result` (good)
- `checkpoint.rs`: Uses `crate::error::Result` (good)
- `resources.rs`: Uses `anyhow::Result`
- Library modules: Should use `crate::error::Result`

**Impact**: 
- Inconsistent error types make error handling harder
- Some errors lose structured information
- Error messages vary in quality

**Recommendation**:
- Library code: Use `crate::error::Result`
- CLI boundary (`main.rs`): Use `anyhow::Result` with `.map_err(|e| anyhow::anyhow!("{}", e))`
- Standardize error conversion at boundaries

### 2. AWS Client Initialization Duplication

**Problem**: Every command creates its own AWS clients

**Current State**:
```rust
// In aws.rs
let ec2_client = Ec2Client::new(aws_config);
let ssm_client = SsmClient::new(aws_config);

// In ebs.rs
let ec2_client = Ec2Client::new(aws_config);

// In s3.rs
let s3_client = S3Client::new(aws_config);
```

**Impact**:
- Code duplication
- Inconsistent error handling for AWS config loading
- Harder to add cross-cutting concerns (retry, logging, etc.)

**Recommendation**:
- Create a shared `AwsClients` struct
- Initialize once and pass to handlers
- Centralize AWS config loading

### 3. JSON Output Inconsistency

**Problem**: Not all commands support JSON output

**Current State**:
- `aws create`: ✅ JSON support
- `aws train`: ✅ JSON support
- `s3 list`: ✅ JSON support
- `checkpoint list`: ✅ JSON support
- `resources list`: ✅ JSON support
- `ebs list`: ❌ No JSON support
- `aws processes`: ❌ No JSON support
- `aws monitor`: ❌ No JSON support

**Impact**:
- Inconsistent user experience
- Harder to script/automate
- Some commands can't be used programmatically

**Recommendation**:
- Add JSON output to all commands
- Standardize JSON structure
- Document JSON schemas

### 4. Configuration Loading Inconsistency

**Problem**: Different commands load config differently

**Current State**:
- Some commands take `config: &Config` parameter
- Some commands load config internally
- Some commands don't use config at all

**Impact**:
- Harder to test (can't inject config)
- Inconsistent behavior
- Some commands ignore user config

**Recommendation**:
- Always pass `config: &Config` to handlers
- Load config once in `main.rs`
- Make config optional where appropriate

### 5. Project Name Handling

**Problem**: Inconsistent project name derivation and usage

**Current State**:
- Default: `"matryoshka-box"` (hardcoded)
- Some commands: `--project-name` flag
- Some commands: Derive from current directory
- Some commands: Use config value

**Impact**:
- Confusing defaults
- Inconsistent tagging
- Hard to track resources across projects

**Recommendation**:
- Standardize: derive from current directory name
- Make default explicit and documented
- Use consistent tagging across all resources

### 6. Input Validation Inconsistency

**Problem**: Some commands validate input, others don't

**Current State**:
- `aws train`: ✅ Validates instance ID
- `aws terminate`: ✅ Validates instance ID
- `ebs attach`: ✅ Validates volume/instance IDs
- `s3 upload`: ❌ No path validation
- `checkpoint list`: ❌ No path validation

**Impact**:
- Some commands fail with cryptic errors
- Inconsistent user experience
- Security concerns (path traversal, etc.)

**Recommendation**:
- Validate all inputs at command entry
- Use `crate::validation` module consistently
- Provide helpful error messages

### 7. Command Naming Inconsistency

**Problem**: Mixed naming conventions

**Current State**:
- `aws create` (verb)
- `aws train` (verb)
- `aws terminate` (verb)
- `ebs list` (verb)
- `resources list` (verb)
- `checkpoint list` (verb)
- `config show` (verb)
- `config set` (verb)
- `top` (noun, alias: `dashboard`)

**Impact**:
- Inconsistent mental model
- Harder to discover commands
- Some commands feel out of place

**Recommendation**:
- Standardize on verb-noun pattern where possible
- Group related commands logically
- Consider aliases for common operations

### 8. EBS Command Nesting

**Problem**: EBS is nested under `aws` but conceptually separate

**Current State**:
- `runctl aws ebs create`
- `runctl aws ebs list`
- etc.

**Impact**:
- Longer command paths
- EBS operations feel AWS-specific (but volumes are independent)
- Harder to discover

**Recommendation**:
- Consider making `ebs` top-level: `runctl ebs create`
- Or keep nested but improve discoverability
- Document the design decision

### 9. Error Message Quality

**Problem**: Error messages vary in helpfulness

**Current State**:
- Some errors: Clear, actionable messages
- Some errors: Generic AWS errors
- Some errors: Missing context

**Examples**:
- Good: "Instance ID must start with 'i-', got: invalid-id"
- Bad: "service error"
- Bad: "Failed to describe instance" (no context)

**Recommendation**:
- Standardize error message format
- Always include context (what was being done, what failed)
- Provide actionable suggestions
- Use structured errors for programmatic access

### 10. Progress Indicators

**Problem**: Inconsistent progress feedback

**Current State**:
- `aws create`: ✅ Progress indicators
- `s3 upload`: ✅ Progress bars
- `ebs create`: ❌ No progress
- `checkpoint list`: ❌ No progress
- `resources list`: ❌ No progress (but fast)

**Impact**:
- Users don't know if long operations are working
- Inconsistent experience
- Hard to debug slow operations

**Recommendation**:
- Add progress indicators for operations > 1 second
- Use consistent progress UI (spinners, bars, etc.)
- Show estimated time for long operations

### 11. Retry Logic Inconsistency

**Problem**: Not all AWS operations use retry logic

**Current State**:
- Some operations: Use `ExponentialBackoffPolicy`
- Some operations: No retry
- Some operations: Manual retry loops

**Impact**:
- Transient failures cause unnecessary errors
- Inconsistent reliability
- Some operations more fragile than others

**Recommendation**:
- Use retry policy for all AWS API calls
- Centralize retry logic
- Make retry configurable

### 12. Resource Tagging Inconsistency

**Problem**: Not all resources are tagged consistently

**Current State**:
- EC2 instances: ✅ Tagged with `runctl:*` tags
- EBS volumes: ❌ Not tagged
- S3 objects: ❌ Not tagged
- Snapshots: ❌ Not tagged

**Impact**:
- Hard to track resources
- Hard to clean up
- Cost attribution unclear

**Recommendation**:
- Tag all resources with `runctl:*` tags
- Use consistent tag keys
- Document tagging strategy

### 13. Documentation Inconsistency

**Problem**: Help text quality varies

**Current State**:
- Some commands: Comprehensive help with examples
- Some commands: Minimal help
- Some commands: Missing examples
- Some commands: Outdated help

**Impact**:
- Users can't discover features
- Hard to use commands correctly
- Inconsistent experience

**Recommendation**:
- Standardize help text format
- Always include examples
- Keep help text up to date
- Add longer docs for complex commands

### 14. Testing Coverage

**Problem**: Inconsistent test coverage

**Current State**:
- Some modules: Good unit tests
- Some modules: No tests
- E2E tests: Exist but not comprehensive

**Impact**:
- Hard to refactor safely
- Bugs slip through
- Inconsistent quality

**Recommendation**:
- Add unit tests for all modules
- Improve E2E test coverage
- Test error paths
- Test edge cases

### 15. Code Organization

**Problem**: Some modules are too large

**Current State**:
- `aws.rs`: ~1800 lines (very large)
- `ebs.rs`: ~1200 lines (large)
- `s3.rs`: ~800 lines (moderate)
- Other modules: Reasonable size

**Impact**:
- Hard to navigate
- Hard to test
- Hard to maintain

**Recommendation**:
- Split large modules into logical sub-modules
- Extract common patterns
- Use composition over large files

## Integration Issues

### 1. AWS/SSM Integration
- **Issue**: SSM requires IAM profile, but not all instances have it
- **Impact**: Some commands fail silently or with unclear errors
- **Fix**: Better error messages, auto-detect SSM availability

### 2. S3/EC2 Integration
- **Issue**: S3 operations don't coordinate with EC2 instance lifecycle
- **Impact**: Data might be uploaded but instance not ready
- **Fix**: Add coordination, better state management

### 3. Checkpoint/EC2 Integration
- **Issue**: Checkpoint operations don't know about running instances
- **Impact**: Can't automatically sync checkpoints from instances
- **Fix**: Add instance-aware checkpoint operations

### 4. Resource Tracking Integration
- **Issue**: Not all resources are tracked
- **Impact**: Cost tracking incomplete
- **Fix**: Track all resources, improve cost attribution

## Nuances and Edge Cases

### 1. Spot Instance Fallback
- **Nuance**: Falls back to on-demand by default
- **Issue**: User might not expect this
- **Fix**: Make behavior explicit, add flag to disable fallback

### 2. EBS Volume Attachment
- **Nuance**: Volumes can be attached to stopped instances
- **Issue**: User might expect instance to be running
- **Fix**: Document behavior, add validation if needed

### 3. Code Syncing
- **Nuance**: Uses SSH or SSM depending on availability
- **Issue**: Behavior changes based on instance config
- **Fix**: Make fallback explicit, document behavior

### 4. Project Name Default
- **Nuance**: Defaults to "matryoshka-box" not current directory
- **Issue**: Confusing for users
- **Fix**: Derive from current directory, make explicit

### 5. JSON Output Format
- **Nuance**: Some commands return arrays, others objects
- **Issue**: Inconsistent for scripting
- **Fix**: Standardize format (always object with `data` field)

### 6. Error Output Format
- **Nuance**: Errors sometimes JSON, sometimes text
- **Issue**: Hard to script error handling
- **Fix**: Always respect `--output json` for errors

### 7. Configuration Precedence
- **Nuance**: CLI flags override config, but not always
- **Issue**: Unclear precedence
- **Fix**: Document precedence clearly, make consistent

### 8. Instance State Handling
- **Nuance**: Some operations work on stopped instances
- **Issue**: User might not expect this
- **Fix**: Document state requirements, validate when needed

## Recommendations Priority

### High Priority
1. ✅ Standardize error handling (anyhow vs crate::error)
2. ✅ Add JSON output to all commands
3. ✅ Standardize input validation
4. ✅ Improve error messages
5. ✅ Fix project name handling

### Medium Priority
6. Centralize AWS client initialization
7. Standardize configuration loading
8. Add progress indicators consistently
9. Improve resource tagging
10. Split large modules

### Low Priority
11. Standardize command naming
12. Improve documentation consistency
13. Add more tests
14. Consider EBS command nesting
15. Add retry logic everywhere

## Next Steps

1. Create issues for high-priority items
2. Start with error handling standardization
3. Add JSON output to missing commands
4. Improve error messages systematically
5. Document design decisions

