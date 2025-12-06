#!/usr/bin/env python3
"""
Simple Python wrapper around trainctl CLI.

This provides a convenient Python interface to trainctl without requiring
PyO3 bindings. It uses subprocess to call the CLI with JSON output.

Usage:
    from trainctl_wrapper import Trainctl
    
    tc = Trainctl()
    instance = tc.aws.create_instance(instance_type="g4dn.xlarge")
    tc.aws.train(instance["instance_id"], "train.py", sync_code=True)
"""

import subprocess
import json
import sys
from pathlib import Path
from typing import Optional, Dict, List, Any


class TrainctlError(Exception):
    """Error from trainctl command."""
    pass


class Trainctl:
    """Python wrapper for trainctl CLI."""
    
    def __init__(self, binary: Optional[str] = None, output_format: str = "json"):
        """
        Initialize trainctl wrapper.
        
        Args:
            binary: Path to trainctl binary (default: auto-detect)
            output_format: Output format ("json" or "text")
        """
        if binary is None:
            # Try to find trainctl binary
            import shutil
            if shutil.which("trainctl"):
                self.binary = "trainctl"
            else:
                # Try target directory (for development)
                target_bin = Path(__file__).parent.parent / "target" / "release" / "trainctl"
                if target_bin.exists():
                    self.binary = str(target_bin)
                else:
                    target_bin = Path(__file__).parent.parent / "target" / "debug" / "trainctl"
                    if target_bin.exists():
                        self.binary = str(target_bin)
                    else:
                        self.binary = "trainctl"  # Will fail with clear error
        else:
            self.binary = binary
        self.output_format = output_format
        self.aws = AWSCommands(self)
        self.resources = ResourceCommands(self)
        self.checkpoint = CheckpointCommands(self)
    
    def _run(self, args: List[str], check: bool = True) -> Dict[str, Any]:
        """
        Run trainctl command and return JSON output.
        
        Args:
            args: Command arguments (without "trainctl")
            check: Raise exception on non-zero exit
            
        Returns:
            Parsed JSON output or empty dict if no output
        """
        cmd = [self.binary] + args
        if self.output_format == "json":
            cmd.extend(["--output", "json"])
        
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=check
            )
            
            if result.returncode != 0:
                if check:
                    raise TrainctlError(
                        f"trainctl failed: {result.stderr}\n"
                        f"Command: {' '.join(cmd)}"
                    )
                return {"error": result.stderr, "exit_code": result.returncode}
            
            if not result.stdout.strip():
                return {}
            
            if self.output_format == "json":
                try:
                    return json.loads(result.stdout)
                except json.JSONDecodeError:
                    # Fallback: return text output as string
                    return {"output": result.stdout}
            else:
                return {"output": result.stdout}
        
        except FileNotFoundError:
            raise TrainctlError(
                f"trainctl binary not found: {self.binary}\n"
                "Install with: cargo install --path ."
            )
        except subprocess.CalledProcessError as e:
            raise TrainctlError(
                f"trainctl command failed: {e.stderr}\n"
                f"Command: {' '.join(cmd)}"
            )
    
    def version(self) -> Dict[str, Any]:
        """Get trainctl version."""
        return self._run(["--version"], check=False)


class AWSCommands:
    """AWS EC2 commands."""
    
    def __init__(self, trainctl: Trainctl):
        self.trainctl = trainctl
    
    def create_instance(
        self,
        instance_type: str,
        spot: bool = False,
        spot_max_price: Optional[float] = None,
        ami_id: Optional[str] = None,
        project_name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """
        Create an EC2 instance.
        
        Args:
            instance_type: Instance type (e.g., "g4dn.xlarge")
            spot: Use spot instance
            spot_max_price: Maximum spot price
            ami_id: Custom AMI ID
            project_name: Project name for tagging
            
        Returns:
            Instance information including instance_id
        """
        args = ["aws", "create", "--instance-type", instance_type]
        if spot:
            args.append("--spot")
        if spot_max_price:
            args.extend(["--spot-max-price", str(spot_max_price)])
        if ami_id:
            args.extend(["--ami-id", ami_id])
        if project_name:
            args.extend(["--project-name", project_name])
        
        return self.trainctl._run(args)
    
    def train(
        self,
        instance_id: str,
        script: str,
        data_s3: Optional[str] = None,
        sync_code: bool = False,
        project_name: Optional[str] = None,
        args: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """
        Start training on an instance.
        
        Args:
            instance_id: EC2 instance ID
            script: Training script path
            data_s3: S3 path for data
            sync_code: Sync code to instance
            project_name: Project name
            args: Additional script arguments
            
        Returns:
            Training status
        """
        cmd_args = ["aws", "train", instance_id, script]
        if data_s3:
            cmd_args.extend(["--data-s3", data_s3])
        if sync_code:
            cmd_args.append("--sync-code")
        if project_name:
            cmd_args.extend(["--project-name", project_name])
        if args:
            cmd_args.extend(["--"] + args)
        
        return self.trainctl._run(cmd_args)
    
    def stop(self, instance_id: str, force: bool = False) -> Dict[str, Any]:
        """Stop an instance."""
        args = ["aws", "stop", instance_id]
        if force:
            args.append("--force")
        return self.trainctl._run(args)
    
    def terminate(self, instance_id: str, force: bool = False) -> Dict[str, Any]:
        """Terminate an instance."""
        args = ["aws", "terminate", instance_id]
        if force:
            args.append("--force")
        return self.trainctl._run(args)
    
    def monitor(self, instance_id: str, follow: bool = False) -> Dict[str, Any]:
        """Monitor an instance."""
        args = ["aws", "monitor", instance_id]
        if follow:
            args.append("--follow")
        return self.trainctl._run(args)
    
    def processes(self, instance_id: str, watch: bool = False) -> Dict[str, Any]:
        """List processes on an instance."""
        args = ["aws", "processes", instance_id]
        if watch:
            args.append("--watch")
        return self.trainctl._run(args)


class ResourceCommands:
    """Resource management commands."""
    
    def __init__(self, trainctl: Trainctl):
        self.trainctl = trainctl
    
    def list(
        self,
        platform: Optional[str] = None,
        detailed: bool = False,
        limit: Optional[int] = None,
    ) -> Dict[str, Any]:
        """
        List resources.
        
        Args:
            platform: Filter by platform ("aws", "runpod", "local", "all")
            detailed: Show detailed information
            limit: Limit number of results
            
        Returns:
            List of resources
        """
        args = ["resources", "list"]
        if platform:
            args.extend(["--platform", platform])
        if detailed:
            args.append("--detailed")
        if limit:
            args.extend(["--limit", str(limit)])
        
        return self.trainctl._run(args)
    
    def summary(self) -> Dict[str, Any]:
        """Get resource summary."""
        return self.trainctl._run(["resources", "summary"])
    
    def stop_all(self, force: bool = False) -> Dict[str, Any]:
        """Stop all running resources."""
        args = ["resources", "stop-all"]
        if force:
            args.append("--force")
        return self.trainctl._run(args)


class CheckpointCommands:
    """Checkpoint management commands."""
    
    def __init__(self, trainctl: Trainctl):
        self.trainctl = trainctl
    
    def list(self, directory: str) -> Dict[str, Any]:
        """List checkpoints in directory."""
        return self.trainctl._run(["checkpoint", "list", directory])
    
    def info(self, path: str) -> Dict[str, Any]:
        """Get checkpoint information."""
        return self.trainctl._run(["checkpoint", "info", path])


# Convenience function for quick usage
def create_instance(instance_type: str, **kwargs) -> Dict[str, Any]:
    """Quick function to create an instance."""
    tc = Trainctl()
    return tc.aws.create_instance(instance_type, **kwargs)


if __name__ == "__main__":
    # Example usage
    if len(sys.argv) < 2:
        print("Usage: python trainctl_wrapper.py <command> [args...]")
        print("\nExample:")
        print("  python trainctl_wrapper.py aws create --instance-type g4dn.xlarge")
        sys.exit(1)
    
    tc = Trainctl()
    result = tc._run(sys.argv[1:], check=False)
    print(json.dumps(result, indent=2))

