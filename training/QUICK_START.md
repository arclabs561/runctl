# Quick Start: Training Example

This guide shows how to use the MNIST training example with `runctl`.

## Prerequisites

1. **Install PyTorch** (required for the training script):
   ```bash
   pip install torch torchvision
   ```
   
   Or with `uv`:
   ```bash
   uv pip install torch torchvision
   ```

2. **Build runctl** (if not already built):
   ```bash
   cargo build --release
   ```

## Local Testing

First, test the training script locally:

```bash
# Quick test (1 epoch, small batch)
python training/train_mnist.py --epochs 1 --batch-size 32

# Full training (5 epochs)
python training/train_mnist.py --epochs 5
```

Expected output:
- Downloads MNIST dataset (first run)
- Trains for specified epochs
- Saves checkpoints to `checkpoints/` directory
- Prints training/validation metrics

## Using with runctl Local

```bash
# Train locally with runctl
runctl local training/train_mnist.py --epochs 5

# With custom arguments
runctl local training/train_mnist.py --epochs 10 --batch-size 128 --lr 0.0001
```

## Using with runctl AWS

```bash
# 1. Create a spot instance (cost-effective)
INSTANCE_ID=$(runctl aws create \
    --spot \
    --instance-type g4dn.xlarge \
    | grep -o 'i-[a-z0-9]*')

echo "Created instance: $INSTANCE_ID"

# 2. Wait for instance to be ready (SSH access)
echo "Waiting for instance to be ready..."
sleep 60

# 3. Train with automatic code sync
runctl aws train $INSTANCE_ID training/train_mnist.py \
    --sync-code \
    --script-args "--epochs 10 --batch-size 128"

# 4. Monitor training progress
runctl aws monitor $INSTANCE_ID --follow

# 5. Check processes and resource usage
runctl aws processes $INSTANCE_ID --detailed

# 6. When done, stop instance (preserves data)
runctl aws stop $INSTANCE_ID

# Or terminate if you don't need the instance
# runctl aws terminate $INSTANCE_ID
```

## Resume from Checkpoint

If training is interrupted, you can resume:

```bash
# Resume from a specific checkpoint
python training/train_mnist.py \
    --epochs 10 \
    --resume checkpoints/checkpoint_epoch_5.pt

# Or use runctl checkpoint command
runctl checkpoint resume checkpoints/checkpoint_epoch_5.pt training/train_mnist.py
```

## Verify Training Worked

After training completes, verify checkpoints were created:

```bash
# List checkpoints
runctl checkpoint list checkpoints/

# Show checkpoint info
runctl checkpoint info checkpoints/final_checkpoint.pt
```

You should see:
- `checkpoint_epoch_1.pt`, `checkpoint_epoch_2.pt`, etc.
- `final_checkpoint.pt` (final model)

## Troubleshooting

**PyTorch not found:**
```bash
pip install torch torchvision
```

**CUDA not available (CPU training):**
- The script automatically falls back to CPU if CUDA isn't available
- Training will be slower but will work

**Checkpoints not saving:**
- Ensure `checkpoints/` directory exists and is writable
- Check disk space

**Instance not ready:**
- Wait longer after creating instance (may need 2-3 minutes)
- Check instance status: `runctl aws list`
