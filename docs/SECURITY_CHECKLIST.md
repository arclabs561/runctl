# Security Checklist

## ✅ Pre-Commit Checklist

Before committing code, verify:

- [ ] Run `./scripts/check-secrets.sh` - No secrets found
- [ ] Verify `.runctl.toml` is not staged: `git status`
- [ ] Check no `.env` files are staged
- [ ] Verify no credential files (`.pem`, `.key`) are staged
- [ ] Review diff for any hardcoded credentials: `git diff`

## ✅ Pre-Push Checklist

Before pushing to GitHub:

- [ ] Run `./scripts/check-secrets.sh` - All checks pass
- [ ] Verify repository is private (if not ready for public)
- [ ] Review recent commits: `git log --oneline -5`
- [ ] Check for accidental secret commits in history

## ✅ Pre-Public Checklist

Before making repository public:

- [ ] Run comprehensive secret scan: `./scripts/check-secrets.sh`
- [ ] Scan entire git history for secrets:
  ```bash
  git log --all --source --pretty=format:"%H" -- . | \
    xargs -I {} git show {} | \
    grep -iE "AKIA|secret|password|api.*key" | \
    grep -v "check-secrets" | \
    grep -v "docs/" || echo "No secrets found"
  ```
- [ ] Verify .gitignore is comprehensive
- [ ] Test that sensitive files are actually ignored
- [ ] Consider using BFG Repo-Cleaner if secrets found
- [ ] Review all documentation for accidental secrets

## ✅ Pre-Publish Checklist

Before publishing to crates.io:

- [ ] Update authors in Cargo.toml (remove placeholder)
- [ ] Update version if needed
- [ ] Verify repository/homepage in Cargo.toml
- [ ] Run `cargo publish --dry-run`
- [ ] Verify all tests pass: `cargo test`
- [ ] Create git tag: `git tag v0.1.0`
- [ ] Get crates.io API token
- [ ] Publish: `cargo publish`

## ✅ CI/CD Checklist

For GitHub Actions:

- [ ] Verify `.github/workflows/security.yml` is enabled
- [ ] Check that secret scanning runs on every PR
- [ ] Verify no secrets in GitHub Actions secrets
- [ ] Test that failed scans block PRs
- [ ] Review weekly scheduled scans

## Quick Commands

```bash
# Check for secrets
./scripts/check-secrets.sh

# Scan git history
git log --all --source --pretty=format:"%H" -- . | \
  xargs -I {} git show {} | \
  grep -iE "AKIA|secret|password" | \
  grep -v "check-secrets" | \
  grep -v "docs/"

# Verify .gitignore
git check-ignore -v .runctl.toml .env *.pem

# Check what's staged
git status --short

# Review recent commits
git log --oneline -10
```

