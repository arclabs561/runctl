#!/usr/bin/env python3
"""
Example: Using trainctl from Python

This demonstrates how to use trainctl programmatically from Python
using the wrapper script.
"""

import sys
from pathlib import Path

# Add scripts directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))

from trainctl_wrapper import Trainctl, TrainctlError


def main():
    """Example trainctl usage from Python."""
    tc = Trainctl()
    
    print("=== trainctl Python Wrapper Example ===\n")
    
    # Example 1: Check version
    print("1. Checking trainctl version...")
    try:
        version = tc.version()
        print(f"   Version info: {version}\n")
    except TrainctlError as e:
        print(f"   Error: {e}\n")
        return
    
    # Example 2: List resources
    print("2. Listing AWS resources...")
    try:
        resources = tc.resources.list(platform="aws", limit=5)
        print(f"   Resources: {resources}\n")
    except TrainctlError as e:
        print(f"   Error: {e}\n")
    
    # Example 3: Create instance (commented out to avoid creating real instances)
    print("3. Example: Creating instance (commented out)...")
    print("   # instance = tc.aws.create_instance(")
    print("   #     instance_type='g4dn.xlarge',")
    print("   #     spot=True,")
    print("   #     project_name='example'")
    print("   # )")
    print("   # print(f'Created instance: {instance[\"instance_id\"]}')\n")
    
    # Example 4: Training workflow (commented out)
    print("4. Example: Training workflow (commented out)...")
    print("   # instance_id = 'i-1234567890abcdef0'")
    print("   # tc.aws.train(")
    print("   #     instance_id,")
    print("   #     'training/train.py',")
    print("   #     sync_code=True,")
    print("   #     args=['--epochs', '10']")
    print("   # )")
    print("   # tc.aws.monitor(instance_id, follow=True)\n")
    
    print("=== Examples complete ===")
    print("\nTo use in your code:")
    print("  from trainctl_wrapper import Trainctl")
    print("  tc = Trainctl()")
    print("  instance = tc.aws.create_instance('g4dn.xlarge')")


if __name__ == "__main__":
    main()

