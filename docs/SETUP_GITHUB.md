# GitHub Repository Setup

## Repository Created

The repository should be created at: `https://github.com/arclabs561/trainctl`

## Initial Setup

Since the GitHub API had permission issues, you can create the repo manually or use GitHub CLI:

### Option 1: Using GitHub CLI (gh)

```bash
cd /Users/arc/Documents/dev/infra-utils

# Create the repository
gh repo create trainctl --private --description "Modern training orchestration CLI for ML workloads" --source=. --remote=origin

# Push the code
git add .
git commit -m "Initial commit: trainctl - ML training orchestration CLI"
git branch -M main
git push -u origin main
```

### Option 2: Manual Creation

1. Go to https://github.com/new
2. Repository name: `trainctl`
3. Description: "Modern training orchestration CLI for ML workloads. Supports local, RunPod, and AWS EC2 training with unified checkpoint management and monitoring."
4. Set to **Private**
5. **Don't** initialize with README, .gitignore, or license (we already have these)
6. Click "Create repository"

Then connect it:

```bash
cd /Users/arc/Documents/dev/infra-utils

# Add remote
git remote add origin https://github.com/arclabs561/trainctl.git

# Or if using SSH:
# git remote add origin git@github.com:arclabs561/trainctl.git

# Initial commit and push
git add .
git commit -m "Initial commit: trainctl - ML training orchestration CLI"
git branch -M main
git push -u origin main
```

## Repository Settings

After creating, consider:

1. **Topics/Tags**: Add topics like `rust`, `cli`, `ml`, `training`, `aws`, `runpod`
2. **Description**: Already set above
3. **Website**: If you have one
4. **Visibility**: Keep private for now, make public when ready

## Next Steps

After pushing:

1. Set up branch protection (optional)
2. Configure GitHub Actions secrets (for E2E tests):
   - `TRAINCTL_E2E` (set to "1" to enable)
   - `AWS_ACCESS_KEY_ID`
   - `AWS_SECRET_ACCESS_KEY`
3. Add collaborators if needed
4. Create issues for tracking features/bugs

