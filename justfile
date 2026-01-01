# Modern training orchestration helpers (train-ops)

# Default recipe
default:
    @just --list

# Build
build:
    cargo build --release

# Install
install: build
    cargo install --path .

# Run tests
test:
    cargo test -- --nocapture

# Lint
lint:
    cargo clippy -- -D warnings
    cargo fmt --check
    @echo "Checking shell scripts..."
    @shellcheck examples/*.sh scripts/*.sh 2>/dev/null || echo "Note: Install shellcheck with 'brew install shellcheck' for shell script linting"
    @echo "Checking Dockerfiles..."
    @find . -name "Dockerfile*" -o -name "*.dockerfile" 2>/dev/null | xargs -r hadolint 2>/dev/null || echo "Note: Install hadolint with 'brew install hadolint' for Dockerfile linting"

# Format
fmt:
    cargo fmt

# Quick dev build
dev:
    cargo build

# Run CLI
run *args:
    cargo run -- {{args}}

# Initialize new project
init:
    cargo run -- init

# Example: Train locally
train-local script="training/train.py":
    cargo run -- local {{script}} --verbose

# Example: Create RunPod pod
runpod-create:
    cargo run -- runpod create --gpu "NVIDIA GeForce RTX 4080 SUPER"

# Example: Train on RunPod
runpod-train pod_id script="training/train.py":
    cargo run -- runpod train {{pod_id}} {{script}}

# Example: Monitor training
monitor log="training.log":
    cargo run -- monitor --log {{log}} --follow

# Example: List checkpoints
checkpoints:
    cargo run -- checkpoint list checkpoints/

# Clean build artifacts
clean:
    cargo clean
    @rm -rf target/

# Check for updates
update:
    cargo update

# Generate docs
docs:
    cargo doc --open --no-deps

# Run with tracing
trace *args:
    RUST_LOG=debug cargo run -- {{args}}

# Lint shell scripts
shellcheck:
    @echo "Checking shell scripts with shellcheck..."
    @shellcheck examples/*.sh scripts/*.sh || echo "Some issues found. Run 'shellcheck examples/*.sh scripts/*.sh' for details."

# Format shell scripts (using shfmt if available)
shfmt:
    @if command -v shfmt >/dev/null 2>&1; then \
        shfmt -w examples/*.sh scripts/*.sh; \
        echo "Formatted shell scripts"; \
    else \
        echo "shfmt not installed. Install with: brew install shfmt"; \
    fi

# Lint Dockerfiles
hadolint:
    @echo "Checking Dockerfiles with hadolint..."
    @if find . -name "Dockerfile*" -o -name "*.dockerfile" 2>/dev/null | grep -q .; then \
        find . -name "Dockerfile*" -o -name "*.dockerfile" 2>/dev/null | xargs hadolint || echo "Some issues found. Run 'hadolint Dockerfile' for details."; \
    else \
        echo "No Dockerfiles found to lint"; \
    fi

