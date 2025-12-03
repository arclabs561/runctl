# Quick Start: Resource Management

## See What's Running Right Now

```bash
# Quick overview
trainctl resources list

# Detailed view
trainctl resources list --detailed

# Just AWS
trainctl resources list --platform aws

# Just RunPod
trainctl resources list --platform runpod

# Just local processes
trainctl resources list --platform local
```

## Get Insights & Recommendations

```bash
# Get insights about your resources
trainctl resources insights
```

This shows:
- What's running
- Cost estimates
- Recommendations
- Potential zombies

## Check for Zombies

```bash
# Preview what would be cleaned up
trainctl resources cleanup --dry-run

# Actually cleanup (with confirmation)
trainctl resources cleanup

# Force cleanup (no confirmation)
trainctl resources cleanup --force
```

## Daily Workflow

```bash
# Morning check
trainctl resources summary

# Get recommendations
trainctl resources insights

# Cleanup if needed
trainctl resources cleanup --dry-run
```

## What Gets Tracked

- **AWS EC2**: All instances (running, stopped, etc.)
- **RunPod**: All pods
- **Local**: Training processes

## Cost Monitoring

```bash
# See current costs
trainctl resources summary

# Get cost insights
trainctl resources insights
```

## Safety

- Always use `--dry-run` first
- Confirmation prompts prevent accidents
- Only cleans up old, untagged resources
- Won't touch resources < 24 hours old

