# Training Examples

This directory contains example training scripts that work with `runctl`.

## MNIST Example

A simple CNN for MNIST digit classification that demonstrates:
- Real PyTorch training with actual data
- Checkpoint saving and resuming
- Integration with runctl workflows

### Local Training

```bash
# Train locally
python training/train_mnist.py --epochs 5

# Or use runctl local
runctl local training/train_mnist.py --epochs 5
```

### AWS EC2 Training

```bash
# 1. Create instance
INSTANCE_ID=$(runctl aws create --spot --instance-type g4dn.xlarge | grep -o 'i-[a-z0-9]*')

# 2. Train with code sync
runctl aws train $INSTANCE_ID training/train_mnist.py \
    --sync-code \
    --script-args "--epochs 10 --batch-size 128"

# 3. Monitor training
runctl aws monitor $INSTANCE_ID --follow

# 4. When done, stop instance
runctl aws stop $INSTANCE_ID
```

### Resume from Checkpoint

```bash
# Resume training from a saved checkpoint
python training/train_mnist.py \
    --epochs 10 \
    --resume checkpoints/checkpoint_epoch_5.pt
```

### Checkpoint Management

```bash
# List checkpoints
runctl checkpoint list checkpoints/

# Show checkpoint info
runctl checkpoint info checkpoints/final_checkpoint.pt

# Resume using runctl
runctl checkpoint resume checkpoints/checkpoint_epoch_5.pt training/train_mnist.py
```

## Requirements

The training scripts require:
- Python 3.8+
- PyTorch
- torchvision

Install with:
```bash
pip install torch torchvision
```

Or if using `uv`:
```bash
uv pip install torch torchvision
```

## Expected Output

The training script will:
1. Download MNIST dataset (first run only)
2. Train for specified epochs
3. Save checkpoints after each epoch
4. Save final checkpoint when complete
5. Print training and validation metrics

Example output:
```
Epoch 1/5
  Batch 0/938: loss=2.3012, acc=9.38%
  ...
Epoch 1 Summary: train_loss=0.2341, train_acc=92.45%, val_loss=0.1234, val_acc=96.12%
  Saved checkpoint: checkpoints/checkpoint_epoch_1.pt
  New best validation accuracy: 96.12%
```
