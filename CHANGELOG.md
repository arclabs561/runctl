# Changelog

All notable changes to trainctl will be documented in this file.

## [Unreleased]

### Added
- Resource management commands (`resources list`, `resources summary`, `resources insights`, `resources cleanup`)
- S3 operations module (upload, download, sync, list, cleanup, watch, review)
- Checkpoint cleanup command
- E2E test framework
- Comprehensive test suite
- Documentation organization and archiving
- CI/CD workflow for testing

### Changed
- Organized documentation into docs/ directory
- Archived older documentation
- Improved code organization with lib.rs

### Fixed
- Compilation errors in resources module
- Type mismatches with AWS SDK
- String formatting issues

## [0.1.0] - Initial Release

### Added
- Basic CLI structure
- Local training execution
- RunPod integration
- AWS EC2 integration (stubbed)
- Checkpoint management
- Monitoring capabilities
- Configuration management

