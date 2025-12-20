# Testing runctl

## Quick Test Commands

```bash
# Build
cargo build

# Check compilation
cargo check

# Run tests
cargo test

# Test CLI help
./target/debug/runctl --help

# Test init
./target/debug/runctl init
cat .runctl.toml

# Test checkpoint listing
mkdir -p checkpoints
touch checkpoints/test.pt
./target/debug/runctl checkpoint list checkpoints/

# Test checkpoint info
./target/debug/runctl checkpoint info checkpoints/test.pt
```

## Integration Testing

The project includes basic integration tests in `tests/integration_test.rs`.

To run:
```bash
cargo test --test integration_test
```

## Manual Testing Checklist

- [x] CLI compiles successfully
- [x] `--help` works for all commands
- [x] `init` creates config file
- [x] `checkpoint list` works
- [x] `checkpoint info` works
- [ ] `local` training (requires actual script)
- [ ] `runpod` commands (requires runpodctl and API key)
- [ ] `aws` commands (requires AWS credentials)
- [ ] `monitor` with real log files

## Known Limitations

1. AWS EC2 instance creation is stubbed (needs full implementation)
2. RunPod API key reading from MCP config needs testing
3. Checkpoint metadata extraction from PyTorch files needs torch-sys or similar
4. Some error handling could be more robust

## Next Steps for Full Testing

1. Add mock AWS SDK responses for testing
2. Add integration tests with actual training scripts
3. Test graceful shutdown handling
4. Test checkpoint resume functionality
5. Test multi-platform workflows

