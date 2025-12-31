//! PEP 723 inline script dependencies support
//!
//! PEP 723 allows Python scripts to declare dependencies inline using special comment blocks.
//! This module provides detection and parsing of PEP 723 dependencies.

use crate::error::{Result, TrainctlError};
use std::path::Path;

/// Detect if a Python script has PEP 723 inline dependencies
///
/// Returns `Some(dependencies)` if PEP 723 block is found, `None` otherwise.
pub fn detect_pep723_dependencies(script_path: &Path) -> Result<Option<Vec<String>>> {
    let content = std::fs::read_to_string(script_path)
        .map_err(|e| TrainctlError::Io(std::io::Error::other(format!(
            "Failed to read script {}: {}",
            script_path.display(),
            e
        ))))?;

    // PEP 723 format:
    // # /// script
    // # requires-python = ">=3.9"
    // # dependencies = [
    // #     "torch>=2.0.0",
    // #     "torchvision>=0.15.0",
    // # ]
    // # ///
    
    if !content.contains("# /// script") {
        return Ok(None);
    }

    // Extract the PEP 723 block
    let mut in_block = false;
    let mut block_lines = Vec::new();
    
    for line in content.lines() {
        if line.trim() == "# /// script" {
            in_block = true;
            continue;
        }
        if line.trim() == "# ///" {
            break; // End of block
        }
        if in_block && line.trim().starts_with("# ") {
            block_lines.push(line.trim_start_matches("# ").trim().to_string());
        }
    }

    if block_lines.is_empty() {
        return Ok(None);
    }

    // Parse dependencies from the block (simple TOML-like parsing)
    // Format: dependencies = ["package1", "package2>=1.0"]
    let mut dependencies = Vec::new();
    let mut in_dependencies = false;

    for line in &block_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("dependencies = [") {
            in_dependencies = true;
            // Check if it's a single-line array: dependencies = ["pkg1", "pkg2"]
            if trimmed.contains(']') {
                // Single line format
                let deps_str = trimmed
                    .strip_prefix("dependencies = [")
                    .and_then(|s| s.strip_suffix(']'))
                    .unwrap_or("");
                for dep in deps_str.split(',') {
                    let dep = dep
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim()
                        .to_string();
                    if !dep.is_empty() {
                        dependencies.push(dep);
                    }
                }
                break;
            }
            continue;
        }
        if in_dependencies {
            if trimmed == "]" {
                break;
            }
            // Extract dependency string (handles quotes and commas)
            let dep = trimmed
                .trim_start_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches(',')
                .trim_end_matches('"')
                .trim_end_matches('\'')
                .trim()
                .to_string();
            
            if !dep.is_empty() {
                dependencies.push(dep);
            }
        }
    }

    if dependencies.is_empty() {
        Ok(None)
    } else {
        Ok(Some(dependencies))
    }
}

/// Check if `uv` is available in PATH
pub fn uv_available() -> bool {
    std::process::Command::new("uv")
        .arg("--version")
        .output()
        .is_ok()
}

/// Check if script should use `uv run` (has PEP 723 or uv is preferred)
pub fn should_use_uv_run(script_path: &Path) -> Result<bool> {
    if !uv_available() {
        return Ok(false);
    }

    // Use uv run if PEP 723 dependencies detected
    if detect_pep723_dependencies(script_path)?.is_some() {
        return Ok(true);
    }

    // Could also use uv run for requirements.txt, but for now only for PEP 723
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_pep723_no_block() {
        let temp_dir = TempDir::new().unwrap();
        let script = temp_dir.path().join("script.py");
        fs::write(&script, "print('hello')").unwrap();

        let deps = detect_pep723_dependencies(&script).unwrap();
        assert_eq!(deps, None);
    }

    #[test]
    fn test_detect_pep723_with_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let script = temp_dir.path().join("script.py");
        let content = r#"# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "torch>=2.0.0",
#     "torchvision>=0.15.0",
# ]
# ///
import torch
print('hello')
"#;
        fs::write(&script, content).unwrap();

        let deps = detect_pep723_dependencies(&script).unwrap();
        assert!(deps.is_some());
        let deps = deps.unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"torch>=2.0.0".to_string()));
        assert!(deps.contains(&"torchvision>=0.15.0".to_string()));
    }
}

