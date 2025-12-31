//! Exit code standardization for runctl
//!
//! Provides consistent exit codes for different error types to enable
//! reliable programmatic error detection by AI tools and scripts.
//!
//! ## Exit Code Convention
//!
//! - `0` = Success
//! - `1` = User error (invalid input, validation failure, resource not found)
//! - `2` = System error (AWS API failure, network error, cloud provider error)
//! - `3` = Configuration error (missing config, invalid credentials, config parse error)

use crate::error::TrainctlError;

/// Standard exit codes for runctl
pub mod codes {
    /// Success
    #[allow(dead_code)]
    pub const SUCCESS: i32 = 0;
    /// User error (invalid input, validation failure)
    pub const USER_ERROR: i32 = 1;
    /// System error (AWS API failure, network error)
    pub const SYSTEM_ERROR: i32 = 2;
    /// Configuration error (missing config, invalid credentials)
    pub const CONFIG_ERROR: i32 = 3;
}

/// Map a TrainctlError to an appropriate exit code
///
/// This function categorizes errors into user errors, system errors, and config errors
/// to provide consistent exit codes for programmatic error detection.
pub fn exit_code_for_error(error: &TrainctlError) -> i32 {
    use TrainctlError::*;
    match error {
        // Configuration errors
        Config(_) => codes::CONFIG_ERROR,
        
        // User errors (invalid input, validation failures)
        Validation { .. } => codes::USER_ERROR,
        ResourceNotFound { .. } => codes::USER_ERROR,
        ResourceExists { .. } => codes::USER_ERROR,
        
        // System errors (cloud provider, network, I/O)
        CloudProvider { .. } => codes::SYSTEM_ERROR,
        Aws(_) => codes::SYSTEM_ERROR,
        S3(_) => codes::SYSTEM_ERROR,
        Ssm(_) => codes::SYSTEM_ERROR,
        Io(_) => codes::SYSTEM_ERROR,
        Retryable { .. } => codes::SYSTEM_ERROR,
        
        // Resource errors - depends on context, default to user error
        Resource { .. } => codes::USER_ERROR,
        
        // Other errors - default to system error
        CostTracking(_) => codes::SYSTEM_ERROR,
        Cleanup(_) => codes::SYSTEM_ERROR,
        DataTransfer(_) => codes::SYSTEM_ERROR,
        Json(_) => codes::SYSTEM_ERROR,
    }
}

/// Exit with appropriate code based on error type
///
/// This is a convenience function for use in main() to exit with the correct code.
#[allow(dead_code)]
pub fn exit_with_code(error: &TrainctlError) -> ! {
    let code = exit_code_for_error(error);
    std::process::exit(code);
}

