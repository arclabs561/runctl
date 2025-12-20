# Quick Start: Resource Management

## See What's Running Right Now

```bash
# Quick overview
runctl resources list

# Detailed view
runctl resources list --detailed

# Just AWS
runctl resources list --platform aws

# Just RunPod
runctl resources list --platform runpod

# Just local processes
runctl resources list --platform local
```

## Get Insights & Recommendations

```bash
# Get insights about your resources
runctl resources insights
```

This shows:
- What's running
- Cost estimates
- Recommendations
- Potential zombies

## Check for Zombies

```bash
# Preview what would be cleaned up
runctl resources cleanup --dry-run

# Actually cleanup (with confirmation)
runctl resources cleanup

# Force cleanup (no confirmation)
runctl resources cleanup --force
```

## Daily Workflow

```bash
# Morning check
runctl resources summary

# Get recommendations
runctl resources insights

# Cleanup if needed
runctl resources cleanup --dry-run
```

## What Gets Tracked

- **AWS EC2**: All instances (running, stopped, etc.)
- **RunPod**: All pods
- **Local**: Training processes

## Cost Monitoring

```bash
# See current costs
runctl resources summary

# Get cost insights
runctl resources insights
```

## Safety

- Always use `--dry-run` first
- Confirmation prompts prevent accidents
- Only cleans up old, untagged resources
- Won't touch resources < 24 hours old

