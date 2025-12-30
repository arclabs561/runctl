# Security Policy

## Supported Versions

We currently support the latest version of runctl. Security updates will be backported as needed.

## Reporting a Vulnerability

If you discover a security vulnerability, please email the maintainers directly rather than opening a public issue.

**Do not** open a public GitHub issue for security vulnerabilities.

For security issues:
1. Email the maintainers with details
2. Include steps to reproduce if possible
3. We will respond within 48 hours
4. We will work with you to coordinate disclosure

## Security Best Practices

- Never commit credentials or API keys
- Use IAM roles with temporary credentials when possible
- Review [docs/AWS_SECURITY_BEST_PRACTICES.md](docs/AWS_SECURITY_BEST_PRACTICES.md) for AWS-specific guidance
- Enable MFA on all cloud accounts
- Use least-privilege IAM policies

