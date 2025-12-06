#!/usr/bin/env python3
"""
Minimal test training script for E2E testing.

This script:
1. Creates a simple dataset
2. Trains a minimal model
3. Saves checkpoints
4. Validates the training worked

Run with: python3 test_training_script.py
"""

import os
import json
import time
from pathlib import Path

def create_test_dataset(output_dir="data"):
    """Create a minimal test dataset."""
    os.makedirs(output_dir, exist_ok=True)
    
    # Create simple CSV dataset
    with open(f"{output_dir}/train.csv", "w") as f:
        f.write("x,y\n")
        for i in range(100):
            f.write(f"{i},{i*2}\n")
    
    print(f"Created test dataset in {output_dir}/")

def train_model(epochs=5, checkpoint_dir="checkpoints"):
    """Train a minimal model."""
    os.makedirs(checkpoint_dir, exist_ok=True)
    
    # Simulate training
    for epoch in range(epochs):
        # Simulate training step
        loss = 1.0 / (epoch + 1)
        accuracy = 0.5 + (epoch * 0.1)
        
        print(f"Epoch {epoch+1}/{epochs}: loss={loss:.4f}, accuracy={accuracy:.4f}")
        
        # Save checkpoint
        checkpoint = {
            "epoch": epoch + 1,
            "loss": loss,
            "accuracy": accuracy,
            "timestamp": time.time()
        }
        
        checkpoint_path = f"{checkpoint_dir}/checkpoint_epoch_{epoch+1}.json"
        with open(checkpoint_path, "w") as f:
            json.dump(checkpoint, f, indent=2)
        
        print(f"  Saved checkpoint: {checkpoint_path}")
        time.sleep(1)  # Simulate training time
    
    # Final checkpoint
    final_checkpoint = {
        "epoch": epochs,
        "loss": 1.0 / epochs,
        "accuracy": 0.5 + ((epochs - 1) * 0.1),
        "timestamp": time.time(),
        "status": "completed"
    }
    
    final_path = f"{checkpoint_dir}/final_checkpoint.json"
    with open(final_path, "w") as f:
        json.dump(final_checkpoint, f, indent=2)
    
    print(f"\nTraining completed! Final checkpoint: {final_path}")
    return final_path

def validate_training(checkpoint_dir="checkpoints"):
    """Validate that training produced checkpoints."""
    checkpoints = list(Path(checkpoint_dir).glob("*.json"))
    
    if not checkpoints:
        print(f"ERROR: No checkpoints found in {checkpoint_dir}/")
        return False
    
    print(f"\nFound {len(checkpoints)} checkpoint(s):")
    for cp in sorted(checkpoints):
        print(f"  - {cp.name}")
    
    # Validate final checkpoint
    final_cp = Path(checkpoint_dir) / "final_checkpoint.json"
    if final_cp.exists():
        with open(final_cp) as f:
            data = json.load(f)
            if data.get("status") == "completed":
                print("\n✅ Training validation passed!")
                return True
    
    print("\n⚠️  Training validation incomplete (no final checkpoint)")
    return False

if __name__ == "__main__":
    import sys
    
    # Parse args
    epochs = int(sys.argv[1]) if len(sys.argv) > 1 else 5
    data_dir = sys.argv[2] if len(sys.argv) > 2 else "data"
    checkpoint_dir = sys.argv[3] if len(sys.argv) > 3 else "checkpoints"
    
    print("=" * 60)
    print("Test Training Script")
    print("=" * 60)
    print(f"Epochs: {epochs}")
    print(f"Data dir: {data_dir}")
    print(f"Checkpoint dir: {checkpoint_dir}")
    print()
    
    # Create dataset
    create_test_dataset(data_dir)
    
    # Train model
    final_path = train_model(epochs, checkpoint_dir)
    
    # Validate
    success = validate_training(checkpoint_dir)
    
    # Write success marker
    with open("training_complete.txt", "w") as f:
        f.write(f"Training completed successfully at {time.time()}\n")
        f.write(f"Final checkpoint: {final_path}\n")
        f.write(f"Validation: {'PASSED' if success else 'FAILED'}\n")
    
    sys.exit(0 if success else 1)

