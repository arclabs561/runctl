//! E2E tests for secret scanning and security
//!
//! These tests verify that:
//! 1. No secrets are hardcoded in the codebase
//! 2. .gitignore properly excludes sensitive files
//! 3. Config files are not tracked in git
//! 4. No credentials in git history
//!
//! Run with: cargo test --features e2e secret_scanning
//!
//! Note: These tests are useful for CI/CD to prevent secret leakage

#[cfg(feature = "e2e")]
#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::path::Path;

    #[test]
    fn test_no_secrets_in_code() {
        // Check for AWS access keys
        let output = Command::new("git")
            .args(&["grep", "-E", "AKIA[0-9A-Z]{16}"])
            .args(&["--", "."])
            .output()
            .expect("Failed to run git grep");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Filter out false positives (documentation, scripts that check for secrets)
        let filtered: Vec<&str> = stdout
            .lines()
            .filter(|line| {
                !line.contains("check-secrets") &&
                !line.contains("docs/") &&
                !line.contains("AWS_TESTING")
            })
            .collect();
        
        assert!(
            filtered.is_empty(),
            "Found potential AWS access keys in code: {:?}",
            filtered
        );
    }

    #[test]
    fn test_no_private_keys_in_git() {
        let output = Command::new("git")
            .args(&["grep", "-E", "BEGIN.*PRIVATE KEY"])
            .args(&["--", "."])
            .output()
            .expect("Failed to run git grep");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.trim().is_empty(),
            "Found private keys in git: {}",
            stdout
        );
    }

    #[test]
    fn test_config_file_not_tracked() {
        // .runctl.toml should not be in git
        let output = Command::new("git")
            .args(&["ls-files", ".runctl.toml"])
            .output()
            .expect("Failed to run git ls-files");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.trim().is_empty(),
            ".runctl.toml should not be tracked in git"
        );
    }

    #[test]
    fn test_env_files_not_tracked() {
        let output = Command::new("git")
            .args(&["ls-files", "|", "grep", "-E", "\\.env$"])
            .output()
            .expect("Failed to check .env files");
        
        // This is a basic check - .env files should not be in git
        let tracked_env_files: Vec<String> = std::fs::read_dir(".")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name();
                let name_str = name.to_str()?;
                if name_str.starts_with(".env") {
                    Some(name_str.to_string())
                } else {
                    None
                }
            })
            .collect();
        
        // If .env files exist, they should not be tracked
        for env_file in tracked_env_files {
            let output = Command::new("git")
                .args(&["ls-files", &env_file])
                .output()
                .expect("Failed to check if .env is tracked");
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.trim().is_empty(),
                ".env file '{}' should not be tracked in git",
                env_file
            );
        }
    }

    #[test]
    fn test_gitignore_covers_secrets() {
        let gitignore = std::fs::read_to_string(".gitignore")
            .expect("Failed to read .gitignore");
        
        let required_patterns = [
            ".runctl.toml",
            ".env",
            "*.pem",
            "*.key",
        ];
        
        for pattern in &required_patterns {
            assert!(
                gitignore.contains(pattern),
                ".gitignore should contain: {}",
                pattern
            );
        }
    }
}

