# Resource Management & Zombie Detection

## Overview

trainctl now includes comprehensive resource management to help you:
- **See what's running** across AWS, RunPod, and local
- **Identify zombies** (orphaned resources)
- **Get cost insights** and recommendations
- **Cleanup automatically** with safety checks

## Commands

### List All Resources

```bash
# List all resources (AWS, RunPod, local)
trainctl resources list

# Detailed view
trainctl resources list --detailed

# Filter by platform
trainctl resources list --platform aws
trainctl resources list --platform runpod
trainctl resources list --platform local
```

### Resource Summary

```bash
# Quick summary with cost estimate
trainctl resources summary
```

Shows:
- Total running instances/pods
- Estimated hourly cost
- Resource breakdown

### Insights & Recommendations

```bash
# Get insights and recommendations
trainctl resources insights
```

Provides:
- Current state analysis
- Cost warnings
- Recommendations for cleanup
- Action suggestions

### Cleanup Zombies

```bash
# Preview what would be cleaned up (dry run)
trainctl resources cleanup --dry-run

# Actually cleanup (with confirmation)
trainctl resources cleanup

# Force cleanup (skip confirmation)
trainctl resources cleanup --force
```

## What Are "Zombies"?

Zombie resources are:
- **Orphaned instances**: Running > 24 hours without trainctl tags
- **Stopped instances**: Consuming storage costs
- **Old training processes**: No longer needed

## Examples

### Daily Check

```bash
# Quick check of what's running
trainctl resources summary

# Get recommendations
trainctl resources insights
```

### Before Training

```bash
# See what's already running
trainctl resources list

# Cleanup old resources
trainctl resources cleanup --dry-run  # Preview
trainctl resources cleanup --force    # Cleanup
```

### Cost Monitoring

```bash
# Check current costs
trainctl resources summary

# Get cost insights
trainctl resources insights
```

## Resource Types Tracked

### AWS EC2 Instances
- Instance ID, type, state
- Launch time
- Tags
- Cost per hour estimate

### RunPod Pods
- Pod ID, name, status
- GPU type
- Creation time
- Cost per hour estimate

### Local Processes
- Training scripts
- trainctl processes
- GPU-using processes

## Cost Estimation

Cost estimates are approximate and based on:
- Instance type
- On-demand pricing
- Running time

For accurate costs, use AWS Cost Explorer or RunPod billing.

## Safety Features

1. **Dry-run mode**: Preview before cleanup
2. **Confirmation prompts**: Prevent accidental deletion
3. **Tag checking**: Only cleanup untagged resources
4. **Age filtering**: Only cleanup old resources (>24h)

## Integration

### With Training Workflows

```bash
# Before training
trainctl resources list
trainctl resources cleanup --dry-run

# After training
trainctl resources summary
trainctl resources insights
```

### Automated Cleanup

Add to your workflow:
```bash
# Weekly cleanup script
trainctl resources cleanup --force
```

## Best Practices

1. **Regular checks**: Run `resources insights` daily
2. **Tag resources**: Use trainctl tags for tracking
3. **Monitor costs**: Check `resources summary` regularly
4. **Cleanup promptly**: Remove unused resources
5. **Use dry-run**: Always preview before cleanup

## Troubleshooting

### "No resources found"
- Check AWS credentials
- Verify runpodctl is installed
- Check local process filters

### "Failed to list instances"
- Check AWS permissions
- Verify region configuration
- Check network connectivity

### High costs
- Use `resources insights` for recommendations
- Review instance types
- Consider spot instances
- Cleanup old resources

