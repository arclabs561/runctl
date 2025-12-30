# Changelog

All notable changes to runctl will be documented in this file.

## [Unreleased]

### Added
- Resource management commands (`resources list`, `resources summary`, `resources insights`, `resources cleanup`)
- S3 operations module (upload, download, sync, list, cleanup, watch, review)
- Checkpoint cleanup command
- E2E test framework
- Comprehensive test suite (29 passing tests)
- Documentation organization and archiving
- CI/CD workflow for testing
- ResourceTracker with automatic cost calculation
- Property-based tests for ResourceTracker
- Comprehensive architecture documentation

### Changed
- Organized documentation into docs/ directory
- Archived older documentation
- Improved code organization with lib.rs
- **Split `aws.rs` (2689 lines) into modular `src/aws/` structure (6 modules)**
- **Split `resources.rs` (2287 lines) into modular `src/resources/` structure (11 modules)**
- Updated all documentation to reflect new module structure
- Standardized error handling with `TrainctlError`
- Integrated retry logic for cloud API calls
- Added comprehensive module-level documentation

### Fixed
- Compilation errors in resources module
- Type mismatches with AWS SDK
- String formatting issues
- ResourceTracker accumulated cost calculation bug
- Chrono API usage (timestamp_opt → from_timestamp)

## [0.1.0] - Initial Release

### Added
- Basic CLI structure
- Local training execution
- RunPod integration
- AWS EC2 integration (stubbed)
- Checkpoint management
- Monitoring capabilities
- Configuration management

