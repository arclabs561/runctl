# Contributing to runctl

## Development Setup

```bash
# Clone and build
git clone <repo>
cd runctl
cargo build

# Run tests
cargo test

# Run with verbose output
cargo test -- --nocapture
```

## Code Style

- Follow Rust standard formatting: `cargo fmt`
- Run clippy: `cargo clippy -- -D warnings`
- Use meaningful variable names
- Add doc comments for public APIs

## Testing

### Unit Tests
```bash
cargo test --lib
```

### Integration Tests
```bash
cargo test --test integration_test
```

### E2E Tests
```bash
# Requires AWS credentials
TRAIN_OPS_E2E=1 cargo test --test aws_resources_test --features e2e
```

## Adding Features

1. **Create feature branch**: `git checkout -b feature/my-feature`
2. **Write tests first**: Add tests in appropriate test file
3. **Implement feature**: Add code in `src/`
4. **Update docs**: Update README.md and EXAMPLES.md
5. **Run tests**: Ensure all tests pass
6. **Submit PR**: Create pull request with description

## AWS Testing

When testing with AWS:
- Use **dry-run mode** when possible
- **Tag resources** with `runctl-test` for identification
- **Clean up** all resources after tests
- Use **test accounts** when available
- Respect **rate limits** and quotas

## Documentation

- Update README.md for user-facing changes
- Add examples to EXAMPLES.md
- Update inline docs for API changes
- Keep CHANGELOG.md updated

## Commit Messages

Use conventional commits:
- `feat: add S3 sync command`
- `fix: correct AWS instance listing`
- `docs: update examples`
- `test: add E2E tests for resources`

## Code Review

- All PRs require review
- Tests must pass
- Documentation must be updated
- Code must be formatted

## Questions?

Open an issue or reach out to maintainers.

