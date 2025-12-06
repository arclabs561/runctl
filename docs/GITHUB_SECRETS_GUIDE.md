# GitHub Secrets Guide

## Current Status

✅ **No AWS credentials are hardcoded in GitHub Actions workflows**

✅ **No AWS credentials are in the codebase**

✅ **Workflows use GitHub Secrets (if configured)**

## How GitHub Secrets Work

GitHub Secrets are stored securely in your repository settings and are **never** exposed in:
- Workflow logs
- Pull request diffs
- Code
- Public access

### Current Secret References

The workflows reference secrets using `${{ secrets.SECRET_NAME }}`:

```yaml
env:
  AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
  AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

**This is safe** - the actual values are stored in GitHub, not in the code.

## Setting Up GitHub Secrets (If Needed)

If you want to enable E2E tests in CI/CD, you need to add secrets:

### Steps:

1. Go to your GitHub repository
2. Settings → Secrets and variables → Actions
3. Click "New repository secret"
4. Add:
   - `AWS_ACCESS_KEY_ID` - Your AWS access key
   - `AWS_SECRET_ACCESS_KEY` - Your AWS secret key
   - `TRAINCTL_E2E` - Set to `1` to enable E2E tests

### Security Best Practices:

1. **Use IAM roles with temporary credentials** (recommended)
   - Create an IAM role for CI/CD
   - Use OIDC to assume the role
   - No long-term credentials needed

2. **Use least privilege**
   - Only grant permissions needed for tests
   - Use permission boundaries
   - Tag resources created by CI/CD

3. **Rotate credentials regularly**
   - Change secrets every 90 days
   - Use temporary credentials when possible

4. **Never commit secrets**
   - ✅ Already verified - no secrets in code
   - Use GitHub Secrets for CI/CD
   - Use environment variables locally

## Current Workflow Behavior

### E2E Tests (Optional)

E2E tests in `.github/workflows/test.yml` are **opt-in**:

```yaml
- name: Run E2E tests (if enabled)
  if: env.TRAINCTL_E2E == '1'
  env:
    TRAINCTL_E2E: ${{ secrets.TRAINCTL_E2E }}
    AWS_ACCESS_KEY_ID: ${{ secrets.AWS_ACCESS_KEY_ID }}
    AWS_SECRET_ACCESS_KEY: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
```

**These only run if:**
1. `TRAINCTL_E2E` secret is set to `1`
2. AWS credentials are provided as secrets

**If secrets are not set:**
- E2E tests are skipped
- Other tests still run
- No errors occur

## Verification

### Check for Hardcoded Credentials

```bash
# Check workflows
grep -r "AKIA\|AWS_ACCESS_KEY_ID\|AWS_SECRET_ACCESS_KEY" .github/workflows/ | grep -v "secrets\."

# Check codebase
git grep -E "AKIA[0-9A-Z]{16}" -- . :!.github/
```

### Check Secret References

```bash
# See what secrets are referenced
grep -r "\${{ secrets\." .github/workflows/
```

## Recommendations

### For Local Development

✅ **Use AWS default credential chain:**
- `~/.aws/credentials`
- Environment variables
- IAM roles (if on EC2)

✅ **Use temporary credentials:**
- `scripts/assume-test-role.sh` for testing
- IAM role assumption
- No long-term keys

### For CI/CD

✅ **Option 1: Skip E2E tests** (current default)
- No secrets needed
- All other tests run
- Safe for public repos

✅ **Option 2: Use GitHub Secrets** (if E2E needed)
- Add secrets in GitHub settings
- Set `TRAINCTL_E2E=1`
- E2E tests will run

✅ **Option 3: Use OIDC** (recommended for production)
- Configure AWS OIDC provider
- Use IAM roles
- No credentials stored

## Summary

✅ **Your AWS credentials are NOT in GitHub CI**
- No hardcoded credentials
- No credentials in code
- Secrets are referenced (but not set)

✅ **E2E tests are opt-in**
- Only run if secrets are configured
- Safe to push without secrets
- Other tests still run

✅ **Current setup is secure**
- No secrets exposed
- Can be made public safely
- E2E tests optional

