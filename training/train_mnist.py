#!/usr/bin/env python3
"""
Simple MNIST training example for runctl.

This script demonstrates a complete training workflow that works with runctl:
- Downloads MNIST dataset
- Trains a simple CNN
- Saves checkpoints
- Supports resuming from checkpoints

Usage:
    # Local training
    python training/train_mnist.py --epochs 5

    # With runctl (local)
    runctl local training/train_mnist.py --epochs 5

    # With runctl (AWS)
    runctl aws train <instance-id> training/train_mnist.py --sync-code --epochs 5
"""

import argparse
import time
from pathlib import Path

import torch
import torch.nn as nn
import torch.optim as optim
from torch.utils.data import DataLoader
from torchvision import datasets, transforms


class SimpleCNN(nn.Module):
    """Simple CNN for MNIST classification."""

    def __init__(self):
        super().__init__()
        self.conv1 = nn.Conv2d(1, 32, kernel_size=3, padding=1)
        self.conv2 = nn.Conv2d(32, 64, kernel_size=3, padding=1)
        self.pool = nn.MaxPool2d(2, 2)
        self.fc1 = nn.Linear(64 * 7 * 7, 128)
        self.fc2 = nn.Linear(128, 10)
        self.relu = nn.ReLU()
        self.dropout = nn.Dropout(0.5)

    def forward(self, x):
        x = self.pool(self.relu(self.conv1(x)))
        x = self.pool(self.relu(self.conv2(x)))
        x = x.view(-1, 64 * 7 * 7)
        x = self.relu(self.fc1(x))
        x = self.dropout(x)
        x = self.fc2(x)
        return x


def train_epoch(model, train_loader, criterion, optimizer, device, epoch):
    """Train for one epoch."""
    model.train()
    running_loss = 0.0
    correct = 0
    total = 0

    for batch_idx, (data, target) in enumerate(train_loader):
        data, target = data.to(device), target.to(device)

        optimizer.zero_grad()
        output = model(data)
        loss = criterion(output, target)
        loss.backward()
        optimizer.step()

        running_loss += loss.item()
        _, predicted = output.max(1)
        total += target.size(0)
        correct += predicted.eq(target).sum().item()

        if batch_idx % 100 == 0:
            print(
                f"  Batch {batch_idx}/{len(train_loader)}: "
                f"loss={loss.item():.4f}, "
                f"acc={100.*correct/total:.2f}%"
            )

    epoch_loss = running_loss / len(train_loader)
    epoch_acc = 100.0 * correct / total
    return epoch_loss, epoch_acc


def validate(model, val_loader, criterion, device):
    """Validate the model."""
    model.eval()
    val_loss = 0.0
    correct = 0
    total = 0

    with torch.no_grad():
        for data, target in val_loader:
            data, target = data.to(device), target.to(device)
            output = model(data)
            loss = criterion(output, target)

            val_loss += loss.item()
            _, predicted = output.max(1)
            total += target.size(0)
            correct += predicted.eq(target).sum().item()

    val_loss /= len(val_loader)
    val_acc = 100.0 * correct / total
    return val_loss, val_acc


def save_checkpoint(model, optimizer, epoch, loss, acc, checkpoint_dir, is_final=False):
    """Save training checkpoint."""
    checkpoint_dir = Path(checkpoint_dir)
    checkpoint_dir.mkdir(parents=True, exist_ok=True)

    checkpoint = {
        "epoch": epoch,
        "model_state_dict": model.state_dict(),
        "optimizer_state_dict": optimizer.state_dict(),
        "loss": loss,
        "accuracy": acc,
        "timestamp": time.time(),
    }

    if is_final:
        path = checkpoint_dir / "final_checkpoint.pt"
    else:
        path = checkpoint_dir / f"checkpoint_epoch_{epoch}.pt"

    torch.save(checkpoint, path)
    print(f"  Saved checkpoint: {path}")
    return path


def load_checkpoint(checkpoint_path, model, optimizer, device):
    """Load checkpoint and resume training."""
    checkpoint = torch.load(checkpoint_path, map_location=device)
    model.load_state_dict(checkpoint["model_state_dict"])
    optimizer.load_state_dict(checkpoint["optimizer_state_dict"])
    start_epoch = checkpoint["epoch"]
    print(f"  Resumed from epoch {start_epoch}")
    return start_epoch


def main():
    parser = argparse.ArgumentParser(description="Train MNIST classifier")
    parser.add_argument("--epochs", type=int, default=5, help="Number of epochs")
    parser.add_argument("--batch-size", type=int, default=64, help="Batch size")
    parser.add_argument("--lr", type=float, default=0.001, help="Learning rate")
    parser.add_argument(
        "--checkpoint-dir",
        type=Path,
        default=Path("checkpoints"),
        help="Checkpoint directory",
    )
    parser.add_argument(
        "--resume",
        type=Path,
        default=None,
        help="Resume from checkpoint",
    )
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("data"),
        help="Data directory for MNIST",
    )
    args = parser.parse_args()

    print("=" * 70)
    print("MNIST Training Example")
    print("=" * 70)
    print(f"Epochs: {args.epochs}")
    print(f"Batch size: {args.batch_size}")
    print(f"Learning rate: {args.lr}")
    print(f"Checkpoint dir: {args.checkpoint_dir}")
    print(f"Data dir: {args.data_dir}")
    if args.resume:
        print(f"Resuming from: {args.resume}")
    print()

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    print(f"Using device: {device}")
    print()

    # Load data
    print("Loading MNIST dataset...")
    transform = transforms.Compose(
        [transforms.ToTensor(), transforms.Normalize((0.1307,), (0.3081,))]
    )

    train_dataset = datasets.MNIST(
        root=args.data_dir, train=True, download=True, transform=transform
    )
    val_dataset = datasets.MNIST(
        root=args.data_dir, train=False, download=True, transform=transform
    )

    train_loader = DataLoader(
        train_dataset, batch_size=args.batch_size, shuffle=True, num_workers=2
    )
    val_loader = DataLoader(
        val_dataset, batch_size=args.batch_size, shuffle=False, num_workers=2
    )

    print(f"Training samples: {len(train_dataset)}")
    print(f"Validation samples: {len(val_dataset)}")
    print()

    # Create model
    model = SimpleCNN().to(device)
    criterion = nn.CrossEntropyLoss()
    optimizer = optim.Adam(model.parameters(), lr=args.lr)

    # Resume from checkpoint if specified
    start_epoch = 0
    if args.resume and args.resume.exists():
        print(f"Loading checkpoint: {args.resume}")
        start_epoch = load_checkpoint(args.resume, model, optimizer, device)
        start_epoch += 1  # Start from next epoch
        print()

    # Training loop
    print("Starting training...")
    print()

    best_val_acc = 0.0

    for epoch in range(start_epoch, args.epochs):
        print(f"Epoch {epoch+1}/{args.epochs}")
        print("-" * 70)

        # Train
        train_loss, train_acc = train_epoch(
            model, train_loader, criterion, optimizer, device, epoch
        )

        # Validate
        val_loss, val_acc = validate(model, val_loader, criterion, device)

        print()
        print(
            f"Epoch {epoch+1} Summary: "
            f"train_loss={train_loss:.4f}, train_acc={train_acc:.2f}%, "
            f"val_loss={val_loss:.4f}, val_acc={val_acc:.2f}%"
        )

        # Save checkpoint
        is_best = val_acc > best_val_acc
        if is_best:
            best_val_acc = val_acc

        save_checkpoint(
            model, optimizer, epoch + 1, val_loss, val_acc, args.checkpoint_dir
        )

        if is_best:
            print(f"  New best validation accuracy: {val_acc:.2f}%")

        print()

    # Save final checkpoint
    print("Training completed!")
    save_checkpoint(
        model,
        optimizer,
        args.epochs,
        val_loss,
        val_acc,
        args.checkpoint_dir,
        is_final=True,
    )
    print(f"Best validation accuracy: {best_val_acc:.2f}%")
    print(f"Final checkpoint saved to: {args.checkpoint_dir}/final_checkpoint.pt")


if __name__ == "__main__":
    main()

