# Resource Management

List, monitor, and cleanup resources across AWS, RunPod, and local.

## Commands

```bash
runctl resources list [--platform aws|runpod|local] [--detailed] [--watch]
runctl resources summary
runctl resources insights
runctl resources cleanup [--dry-run] [--force]
```

## Usage

```bash
# Daily check
runctl resources summary
runctl resources insights

# Before training
runctl resources list
runctl resources cleanup --dry-run

# Cleanup
runctl resources cleanup --force
```

## Zombie Resources

- Orphaned instances: Running >24h without runctl tags
- Stopped instances: Consuming storage costs
- Old training processes: No longer needed

## Safety

- Dry-run mode: Preview before cleanup
- Confirmation prompts: Prevent accidental deletion
- Tag checking: Only cleanup untagged resources
- Age filtering: Only cleanup old resources (>24h)

## Cost Estimation

Cost estimates are approximate, based on instance type and on-demand pricing. For accurate costs, use AWS Cost Explorer or RunPod billing.

