# runctl Expansion Roadmap

## Overview

This document outlines the expansion of `runctl` to support:
1. **Docker containers** for reproducible training environments
2. **Spot instance interruption handling** for cost-effective training
3. **Additional use cases** beyond ML training (data processing, inference, etc.)
4. **Comprehensive E2E testing** for all scenarios

## 1. Spot Instance Interruption Handling

### Problem
Spot instances can be terminated with only a 2-minute warning. If training is in progress:
- Checkpoints may not be saved
- Training progress is lost
- Cost is wasted on incomplete training

### Solution

#### 1.1 Spot Interruption Monitoring
- Monitor EC2 instance metadata service for spot interruption warnings
- Poll `/latest/meta-data/spot/instance-action` endpoint every 30 seconds
- When interruption detected, trigger graceful shutdown sequence

#### 1.2 Graceful Shutdown Sequence
1. Detect spot interruption warning (2-minute notice)
2. Send SIGTERM to training process (allows checkpoint save)
3. Wait up to 90 seconds for graceful shutdown
4. If still running, send SIGKILL
5. Upload final checkpoint to S3 (if configured)
6. Log interruption event

#### 1.3 Auto-Resume (Optional)
- After interruption, automatically:
  - Create new spot instance
  - Sync code
  - Resume from latest checkpoint
  - Continue training

### Implementation Plan

**New Module**: `src/aws/spot_monitor.rs`
- `monitor_spot_interruption()`: Polls metadata service
- `handle_spot_interruption()`: Graceful shutdown sequence
- `save_checkpoint_before_termination()`: Emergency checkpoint save

**Integration Points**:
- `src/aws/training.rs`: Start monitoring when training begins
- `src/aws/instance.rs`: Tag spot instances for monitoring
- `src/checkpoint.rs`: Add `save_emergency_checkpoint()` function

**CLI Command**:
```bash
# Monitor spot instance for interruptions (runs in background)
runctl aws spot-monitor $INSTANCE_ID --auto-save-checkpoint --upload-to-s3 s3://bucket/checkpoints/
```

### Testing Strategy
1. **Unit Tests**: Mock metadata service responses
2. **Integration Tests**: Use EC2 instance with simulated interruption
3. **E2E Tests**: Create spot instance, start training, manually trigger interruption

## 2. Docker Container Support

### Problem
- Training environments need to be reproducible
- Different projects require different dependencies
- Containerization provides isolation and portability

### Solution

#### 2.1 Docker-Based Training
- Support training scripts that run in Docker containers
- Auto-build Docker images from `Dockerfile` in project root
- Push images to ECR (AWS) or Docker Hub
- Run training in containers on EC2 instances

#### 2.2 Dockerfile Detection
- Auto-detect `Dockerfile` in project root
- If found, build and use container for training
- If not found, use bare-metal execution (current behavior)

#### 2.3 Container Registry Integration
- **AWS ECR**: Push images to ECR, pull on instances
- **Docker Hub**: Support public/private repositories
- **Local**: Build locally, sync to instance

### Implementation Plan

**New Module**: `src/docker.rs`
- `detect_dockerfile()`: Check for Dockerfile
- `build_image()`: Build Docker image
- `push_to_registry()`: Push to ECR/Docker Hub
- `run_in_container()`: Execute training in container

**Integration Points**:
- `src/aws/training.rs`: Check for Dockerfile before training
- `src/aws/instance.rs`: Ensure Docker is installed on instance
- `src/config.rs`: Add Docker registry configuration

**CLI Command**:
```bash
# Train with Docker (auto-detects Dockerfile)
runctl aws train $INSTANCE_ID training/train.py --use-docker

# Explicit Docker image
runctl aws train $INSTANCE_ID training/train.py --docker-image my-registry/train:latest
```

### Example Dockerfile
```dockerfile
FROM pytorch/pytorch:2.1.0-cuda11.8-cudnn8-runtime

WORKDIR /workspace
COPY requirements.txt .
RUN pip install -r requirements.txt

COPY . .

CMD ["python", "training/train_mnist.py"]
```

### Testing Strategy
1. **Unit Tests**: Mock Docker API calls
2. **Integration Tests**: Build and run Docker containers locally
3. **E2E Tests**: Build Docker image, push to ECR, run on EC2

## 3. Additional Use Cases

### 3.1 Data Processing Pipelines

**Use Case**: Preprocess large datasets before training
- Download data from S3
- Transform/clean data
- Upload processed data back to S3

**Example**:
```bash
runctl aws train $INSTANCE_ID scripts/preprocess.py \
    --input-s3 s3://bucket/raw-data/ \
    --output-s3 s3://bucket/processed-data/ \
    --workers 8
```

### 3.2 Model Evaluation

**Use Case**: Evaluate trained models on test sets
- Load model from checkpoint
- Run evaluation on test data
- Generate metrics report

**Example**:
```bash
runctl aws train $INSTANCE_ID scripts/evaluate.py \
    --checkpoint s3://bucket/checkpoints/best.pt \
    --test-data s3://bucket/test-data/ \
    --output s3://bucket/evaluation-results/
```

### 3.3 Hyperparameter Tuning

**Use Case**: Run multiple training jobs with different hyperparameters
- Generate hyperparameter combinations
- Launch multiple spot instances
- Collect results and identify best configuration

**Example**:
```bash
runctl aws hyperparameter-tune \
    --script training/train.py \
    --hyperparameters learning_rate:0.001,0.01,0.1 batch_size:32,64,128 \
    --instances 5 \
    --spot
```

### 3.4 Inference Serving

**Use Case**: Deploy model for inference
- Load model from checkpoint
- Start inference server (FastAPI, TorchServe, etc.)
- Expose API endpoint

**Example**:
```bash
runctl aws serve $INSTANCE_ID \
    --checkpoint s3://bucket/checkpoints/best.pt \
    --port 8080 \
    --script scripts/inference_server.py
```

### 3.5 Distributed Training

**Use Case**: Multi-node training with PyTorch DDP
- Launch multiple instances
- Configure networking
- Coordinate training across nodes

**Example**:
```bash
runctl aws train-distributed \
    --script training/train_ddp.py \
    --nodes 4 \
    --instance-type g4dn.xlarge \
    --spot
```

## 4. E2E Testing Strategy

### 4.1 Test Categories

#### Spot Instance Interruption Tests
1. **Test**: Create spot instance, start training, simulate interruption
   - **Expected**: Checkpoint saved, training can resume
2. **Test**: Interruption during checkpoint save
   - **Expected**: Partial checkpoint saved, can resume from it
3. **Test**: Auto-resume after interruption
   - **Expected**: New instance created, training resumes automatically

#### Docker Tests
1. **Test**: Build Docker image, push to ECR, run on EC2
   - **Expected**: Training runs in container successfully
2. **Test**: Docker image with GPU support
   - **Expected**: GPU accessible in container
3. **Test**: Multi-stage Docker builds
   - **Expected**: Optimized image size, training works

#### Use Case Tests
1. **Data Processing**: Process dataset, verify output in S3
2. **Model Evaluation**: Evaluate model, verify metrics
3. **Hyperparameter Tuning**: Run multiple jobs, verify results collected
4. **Inference Serving**: Start server, verify API responses
5. **Distributed Training**: Launch multi-node, verify coordination

### 4.2 Test Infrastructure

**Test Utilities** (`tests/e2e/test_utils.rs`):
- `create_test_spot_instance()`: Create spot instance for testing
- `simulate_spot_interruption()`: Trigger interruption warning
- `wait_for_training_completion()`: Poll until training done
- `verify_checkpoint_exists()`: Check checkpoint in S3/local

**Test Fixtures**:
- `training/train_mnist.py`: Simple training script
- `training/train_with_checkpoints.py`: Training with frequent checkpoints
- `scripts/preprocess_data.py`: Data processing example
- `scripts/evaluate_model.py`: Model evaluation example

## 5. Implementation Priority

### Phase 1: Critical (Week 1)
1. ✅ Spot instance interruption monitoring
2. ✅ Graceful shutdown on interruption
3. ✅ Emergency checkpoint save
4. ✅ E2E test for spot interruption

### Phase 2: Important (Week 2)
5. ✅ Docker support (basic)
6. ✅ Dockerfile detection
7. ✅ ECR integration
8. ✅ E2E test for Docker

### Phase 3: Enhancement (Week 3)
9. ✅ Data processing example
10. ✅ Model evaluation example
11. ✅ Hyperparameter tuning support
12. ✅ Inference serving support

### Phase 4: Advanced (Week 4)
13. ✅ Distributed training support
14. ✅ Auto-resume after interruption
15. ✅ Comprehensive E2E test suite

## 6. Questions to Address

### Spot Instances
- [x] How do we detect spot interruptions? (EC2 metadata service)
- [x] How long should we wait for graceful shutdown? (90 seconds)
- [x] Should we auto-resume? (Optional, configurable)
- [x] How do we handle partial checkpoints? (Save what we can)

### Docker
- [x] Which container registry? (ECR for AWS, Docker Hub as fallback)
- [x] How do we handle GPU access in containers? (nvidia-docker runtime)
- [x] Should we cache Docker images? (Yes, on instance)
- [x] How do we handle multi-stage builds? (Support Dockerfile syntax)

### Use Cases
- [x] Should data processing be a separate command? (Yes, `runctl aws process`)
- [x] How do we handle long-running inference servers? (Background process, health checks)
- [x] How do we coordinate distributed training? (Use PyTorch DDP, SSM for coordination)
- [x] How do we collect hyperparameter tuning results? (S3 output, aggregate locally)

## 7. Documentation Updates

### New Documentation Files
1. `docs/SPOT_INTERRUPTION_HANDLING.md`: Guide for spot instance handling
2. `docs/DOCKER_SUPPORT.md`: Guide for Docker-based training
3. `docs/USE_CASES.md`: Examples for all use cases
4. `docs/E2E_TESTING.md`: Guide for running E2E tests

### Updated Documentation
1. `README.md`: Add sections for Docker, spot handling, use cases
2. `docs/EXAMPLES.md`: Add examples for all new features
3. `docs/ARCHITECTURE.md`: Document new modules and integrations

## 8. Success Criteria

### Spot Interruption Handling
- ✅ Can detect spot interruption within 30 seconds
- ✅ Can save checkpoint before termination
- ✅ Can resume training from checkpoint after interruption
- ✅ E2E test passes consistently

### Docker Support
- ✅ Can build Docker image from Dockerfile
- ✅ Can push image to ECR
- ✅ Can run training in container on EC2
- ✅ GPU accessible in container
- ✅ E2E test passes consistently

### Use Cases
- ✅ All use cases have working examples
- ✅ All use cases have E2E tests
- ✅ Documentation is complete and accurate

