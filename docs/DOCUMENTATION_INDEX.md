# Documentation Index

**Last Updated**: 2026-01-01  
**Status**: ✅ Organized and up to date  
**Active Docs**: 25 core documentation files  
**Archived**: ~180 historical files in `docs/archive/`

## Quick Navigation

### Getting Started
- **[README.md](../README.md)** - Main project documentation and quick start
- **[EXAMPLES.md](EXAMPLES.md)** - Usage examples and workflows
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Complete architecture overview

### Architecture & Design
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - Complete architecture overview
- **[PROVIDER_ARCHITECTURE.md](PROVIDER_ARCHITECTURE.md)** - Provider abstraction design
- **[PROVIDER_TRAIT_DECISION.md](PROVIDER_TRAIT_DECISION.md)** - Provider trait integration status

### Development
- **[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)** - Implementation roadmap
- **[TESTING.md](TESTING.md)** - Comprehensive testing guide
- **[E2E_TESTS.md](E2E_TESTS.md)** - End-to-end testing

### AWS & Cloud
- **[AWS_TESTING_SETUP.md](AWS_TESTING_SETUP.md)** - AWS testing environment setup
- **[AWS_SECURITY_BEST_PRACTICES.md](AWS_SECURITY_BEST_PRACTICES.md)** - Security best practices
- **[AWS_ROOT_CREDENTIALS_MIGRATION.md](AWS_ROOT_CREDENTIALS_MIGRATION.md)** - Migrating from root credentials
- **[SSM_SETUP.md](SSM_SETUP.md)** - AWS Systems Manager setup
- **[SSM_QUICK_START.md](SSM_QUICK_START.md)** - SSM quick start guide
- **[SSM_CODE_SYNC.md](SSM_CODE_SYNC.md)** - SSM-based code synchronization
- **[SSM_REFINEMENTS.md](SSM_REFINEMENTS.md)** - SSM improvements and refinements
- **[S3_OPERATIONS.md](S3_OPERATIONS.md)** - S3 upload, download, sync, cleanup

### Features
- **[RESOURCE_MANAGEMENT.md](RESOURCE_MANAGEMENT.md)** - Resource tracking and cleanup
- **[ADDITIONAL_PROVIDERS.md](ADDITIONAL_PROVIDERS.md)** - RunPod and other providers

### Security
- **[SECURITY_QUICK_START.md](SECURITY_QUICK_START.md)** - Security quick start
- **[SECURITY_AND_SECRETS.md](SECURITY_AND_SECRETS.md)** - Secrets management
- **[GITHUB_SECRETS_GUIDE.md](GITHUB_SECRETS_GUIDE.md)** - GitHub Actions secrets setup
- **[SETUP_GITHUB.md](SETUP_GITHUB.md)** - GitHub repository setup

### Python Integration
- **[PYTHON_USAGE.md](PYTHON_USAGE.md)** - Using runctl with Python
- **[README_PYTHON.md](README_PYTHON.md)** - Python utilities

### Code Reviews & Analysis
- **[DEEP_REVIEW_2025-01-03.md](DEEP_REVIEW_2025-01-03.md)** - Comprehensive code review
- **[RESOURCES_RS_ANALYSIS.md](RESOURCES_RS_ANALYSIS.md)** - Resources module analysis (historical)

## Documentation by Category

### For Users
1. README.md - Quick start
2. EXAMPLES.md - Usage examples
3. AWS_TESTING_SETUP.md - AWS setup
4. SECURITY_QUICK_START.md - Security setup

### For Developers
1. ARCHITECTURE.md - Architecture overview
2. PROVIDER_ARCHITECTURE.md - Provider design
3. TESTING.md - Testing guide
4. IMPLEMENTATION_PLAN.md - Roadmap

### For Contributors
1. TESTING.md - Testing guide
2. IMPLEMENTATION_PLAN.md - Implementation roadmap

## Documentation Status

### ✅ Up to Date
- Architecture documentation
- Module structure documentation
- Testing documentation
- User guides
- Security guides

### ⚠️ Historical (Archived)
- RESOURCES_RS_ANALYSIS.md - Marked as completed, see RESOURCES_SPLIT_COMPLETE.md
- Some archived docs reference old file structure

## Finding Documentation

### By Topic

**Architecture**: ARCHITECTURE.md, PROVIDER_ARCHITECTURE.md, PROVIDER_TRAIT_DECISION.md  
**Testing**: TESTING.md, E2E_TESTS.md, AWS_TESTING_SETUP.md  
**Security**: SECURITY_QUICK_START.md, SECURITY_AND_SECRETS.md, AWS_SECURITY_BEST_PRACTICES.md, SECURITY_AUDIT_GIT_HISTORY.md  
**AWS**: AWS_TESTING_SETUP.md, SSM_SETUP.md, SSM_QUICK_START.md, SSM_CODE_SYNC.md, S3_OPERATIONS.md  
**Development**: IMPLEMENTATION_PLAN.md, PROVIDER_ARCHITECTURE.md

### By Audience

**New Users**: README.md, EXAMPLES.md, SECURITY_QUICK_START.md, SSM_QUICK_START.md  
**Developers**: ARCHITECTURE.md, PROVIDER_ARCHITECTURE.md, TESTING.md  
**Contributors**: TESTING.md, IMPLEMENTATION_PLAN.md

## API Documentation

Generate with:

```bash
cargo doc --open
```

This generates comprehensive API documentation from code comments.

