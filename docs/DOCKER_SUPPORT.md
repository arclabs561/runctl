# Docker Support for runctl

## Overview

`runctl` supports running training in Docker containers for reproducible environments. This is especially useful for:

- **Reproducible environments**: Same dependencies across local and cloud
- **Isolation**: Avoid conflicts between different projects
- **GPU support**: Use NVIDIA Docker runtime for GPU access
- **Multi-stage builds**: Optimize image size

## Current Status

Docker support is **planned** but not yet fully implemented. This document outlines the design and future implementation.

## Design

### Auto-Detection

`runctl` will automatically detect if a project uses Docker:

1. **Check for Dockerfile**: Look for `Dockerfile` in project root
2. **Build image**: Build Docker image if Dockerfile found
3. **Push to registry**: Push to ECR (AWS) or Docker Hub
4. **Run in container**: Execute training in container on EC2 instance

### Dockerfile Detection

`runctl` automatically detects Dockerfiles in common locations (checked in priority order):

1. `Dockerfile` in project root (most common)
2. `Dockerfile.train` in project root
3. `docker/Dockerfile` in project root
4. `deployment/Dockerfile` in project root
5. `training/Dockerfile` in project root
6. `scripts/Dockerfile` in project root
7. `src/Dockerfile` in project root

The first match found is used. If no Dockerfile is found in these locations, training runs without Docker.

### Container Registry

**AWS ECR** (recommended for AWS):
- Automatic authentication via IAM
- Fast pulls on EC2 instances
- Private by default

**Docker Hub** (fallback):
- Public or private repositories
- Requires Docker Hub credentials

## Usage (Planned)

### Basic Usage

```bash
# Auto-detect Dockerfile and build
runctl aws train $INSTANCE_ID training/train.py --use-docker

# Explicit Docker image
runctl aws train $INSTANCE_ID training/train.py \
    --docker-image my-registry/train:latest
```

### Dockerfile Example

```dockerfile
FROM pytorch/pytorch:2.1.0-cuda11.8-cudnn8-runtime

WORKDIR /workspace

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy project code
COPY . .

# Default command (can be overridden)
CMD ["python", "training/train_mnist.py"]
```

### GPU Support

For GPU instances, use NVIDIA Docker runtime:

```dockerfile
FROM nvidia/cuda:11.8.0-cudnn8-runtime-ubuntu22.04

# Install PyTorch with CUDA
RUN pip install torch torchvision --index-url https://download.pytorch.org/whl/cu118

# ... rest of Dockerfile ...
```

## Configuration

### .runctl.toml

```toml
[aws]
region = "us-east-1"
s3_bucket = "my-bucket"

[docker]
# ECR registry (auto-detected from AWS region)
ecr_registry = "123456789012.dkr.ecr.us-east-1.amazonaws.com"

# Docker Hub (optional)
dockerhub_username = "myuser"
dockerhub_password = "mypassword"  # Or use environment variable

# Build options
build_args = ["CUDA_VERSION=11.8"]
cache_from = ["my-registry/train:latest"]
```

## Implementation Plan

### Phase 1: Basic Docker Support
- [ ] Detect Dockerfile in project
- [ ] Build Docker image locally
- [ ] Push to ECR
- [ ] Run training in container on EC2

### Phase 2: Advanced Features
- [ ] Multi-stage builds
- [ ] Docker Hub support
- [ ] Image caching
- [ ] Build arguments

### Phase 3: Optimization
- [ ] Layer caching
- [ ] Parallel builds
- [ ] Image size optimization

## Current Workaround

Until Docker support is implemented, you can:

1. **Build image manually**:
```bash
docker build -t my-training:latest .
docker tag my-training:latest 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-training:latest
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin 123456789012.dkr.ecr.us-east-1.amazonaws.com
docker push 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-training:latest
```

2. **Run on instance manually**:
```bash
# Via SSM
aws ssm send-command \
    --instance-ids $INSTANCE_ID \
    --document-name "AWS-RunShellScript" \
    --parameters 'commands=["docker pull my-registry/train:latest", "docker run --gpus all my-registry/train:latest"]'
```

## Best Practices

1. **Use multi-stage builds** to reduce image size
2. **Cache dependencies** in separate layers
3. **Use .dockerignore** to exclude unnecessary files
4. **Tag images** with version numbers or commit hashes
5. **Test locally** before pushing to registry

## Example Dockerfile

```dockerfile
# Build stage
FROM python:3.10-slim as builder

WORKDIR /build
COPY requirements.txt .
RUN pip install --user -r requirements.txt

# Runtime stage
FROM python:3.10-slim

WORKDIR /workspace

# Copy dependencies from builder
COPY --from=builder /root/.local /root/.local

# Copy project code
COPY . .

# Add local bin to PATH
ENV PATH=/root/.local/bin:$PATH

# Default command
CMD ["python", "training/train_mnist.py"]
```

## Future Enhancements

- [ ] Docker Compose support
- [ ] Kubernetes integration
- [ ] Container orchestration
- [ ] Health checks
- [ ] Log aggregation

