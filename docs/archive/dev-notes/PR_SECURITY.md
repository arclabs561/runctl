# Pull Request Security

## Overview

This document describes the security measures in place to prevent external pull requests from accessing secrets or running untrusted code with elevated permissions.

## 🔒 Security Measures

### 1. Secret Protection in Workflows

**Problem**: External PRs from forks could potentially access repository secrets if workflows run with secrets enabled.

**Solution**: All workflows that use secrets have explicit conditions to prevent execution on PRs from forks:

```yaml
# Only run on pushes to main/develop, NOT on PRs from forks
if: |
  env.TRAINCTL_E2E == '1' &&
  (github.event_name == 'push' || 
   (github.event_name == 'pull_request' && 
    github.event.pull_request.head.repo.full_name == github.repository))
```

**What this does**:
- ✅ Allows E2E tests on pushes to main/develop branches
- ✅ Allows E2E tests on PRs from the same repository (internal PRs)
- ❌ **Blocks** E2E tests on PRs from forks (external contributors)
- ❌ **Blocks** secret access for untrusted code

### 2. Workflow Restrictions

#### `test.yml`
- **E2E Tests**: Only run on pushes or internal PRs (not forks)
- **Secrets Used**: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `TRAINCTL_E2E`
- **Protection**: Condition checks `github.event.pull_request.head.repo.full_name == github.repository`

#### `publish-check.yml`
- **Publish Checks**: Only run on tags or manual dispatch
- **Secrets Used**: `CARGO_REGISTRY_TOKEN`
- **Protection**: Only triggers on `workflow_dispatch` or tag pushes (never on PRs)

#### `ci.yml` and `security.yml`
- **No Secrets**: These workflows don't use secrets, so they're safe to run on all PRs
- **Purpose**: Linting, testing, secret scanning (read-only operations)

### 3. GitHub Branch Protection

**Recommended Settings** (configure in GitHub repository settings):

1. **Require pull request reviews**
   - At least 1 approval required
   - Dismiss stale reviews when new commits are pushed

2. **Require status checks to pass**
   - `secret-scanning` must pass
   - `lint-and-test` must pass
   - `build` must pass

3. **Require branches to be up to date**
   - PRs must be rebased/merged with latest main

4. **Do not allow bypassing**
   - Even admins must follow these rules

5. **Restrict who can push to matching branches**
   - Only trusted collaborators can push directly to main

### 4. Secret Scanning

All workflows run secret scanning **before** any other jobs:

- Scans for AWS access keys (AKIA*, ASIA*)
- Scans for private keys
- Scans for hardcoded API keys
- Verifies sensitive files not tracked
- **Blocks** all other jobs if secrets found

This prevents secrets from being committed, even accidentally.

## 🛡️ How It Works

### For Internal PRs (Same Repository)

```
PR from feature branch → Workflow runs → Secrets available → E2E tests run
```

**Condition**: `github.event.pull_request.head.repo.full_name == github.repository` ✅

### For External PRs (Forks)

```
PR from fork → Workflow runs → Secrets NOT available → E2E tests skipped
```

**Condition**: `github.event.pull_request.head.repo.full_name != github.repository` ❌

### For Direct Pushes

```
Push to main → Workflow runs → Secrets available → E2E tests run
```

**Condition**: `github.event_name == 'push'` ✅

## 📋 Verification

To verify these protections are working:

1. **Check workflow conditions**:
   ```bash
   grep -A 5 "if:" .github/workflows/*.yml
   ```

2. **Test with a fork**:
   - Create a fork of the repository
   - Open a PR from the fork
   - Verify E2E tests are skipped (no secrets exposed)

3. **Check workflow logs**:
   - External PRs should show: "Skipping E2E tests (PR from fork)"
   - Internal PRs should show: "Running E2E tests"

## ⚠️ Important Notes

1. **Secrets are NEVER exposed to forks**:
   - GitHub Actions automatically restricts secrets for fork PRs
   - Our conditions add an extra layer of protection

2. **E2E tests are opt-in**:
   - Require `TRAINCTL_E2E` secret to be set
   - Even then, only run on trusted sources

3. **Secret scanning runs first**:
   - All workflows depend on secret scanning passing
   - Prevents secrets from being committed

4. **Publish checks are restricted**:
   - Only run on tags or manual dispatch
   - Never run automatically on PRs

## 🔍 Monitoring

To monitor for security issues:

1. **Review workflow runs**:
   - Check that E2E tests are skipped on fork PRs
   - Verify secret scanning passes on all PRs

2. **Audit secret usage**:
   - Review which workflows use secrets
   - Ensure all have proper conditions

3. **Check branch protection**:
   - Verify branch protection rules are enabled
   - Ensure required status checks are configured

## 📚 References

- [GitHub Actions: Using secrets in workflows](https://docs.github.com/en/actions/security-guides/encrypted-secrets#using-secrets-in-workflows)
- [GitHub Actions: Security hardening](https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions)
- [GitHub Actions: Preventing pwn requests](https://securitylab.github.com/research/github-actions-preventing-pwn-requests/)

---

**Last Updated**: 2025-01-03  
**Status**: ✅ Protected - Secrets are not exposed to external PRs

