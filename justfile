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

