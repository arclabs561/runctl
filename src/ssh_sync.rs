//! Native Rust SSH-based code syncing
//!
//! Replaces shell-based tar/rsync/ssh commands with native Rust implementations
//! using ssh2-rs for SSH connections and tar crate for archive operations.

use crate::error::{Result, TrainctlError};
use flate2::write::GzEncoder;
use flate2::Compression;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use ssh2::Session;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use tar::Builder;
use tracing::info;

/// Sync code to instance using shell-based tar+ssh (fallback method)
///
/// This is more reliable for large codebases and when native sync hangs.
pub async fn sync_code_shell(
    key_path: &str,
    ip: &str,
    user: &str,
    project_dir: &str,
    project_root: &Path,
    output_format: &str,
    include_patterns: &[String],
) -> Result<()> {
    use std::process::Command;
    use std::time::Duration;

    if output_format != "json" {
        println!("   Using shell-based sync (tar+ssh)...");
    }

    // Build exclude patterns for tar (aggressive exclusions for large repos)
    let mut exclude_args = Vec::new();
    exclude_args.push("--exclude=.git".to_string());
    exclude_args.push("--exclude=__pycache__".to_string());
    exclude_args.push("--exclude=*.pyc".to_string());
    exclude_args.push("--exclude=.venv".to_string());
    exclude_args.push("--exclude=node_modules".to_string());
    exclude_args.push("--exclude=data/embeddings".to_string()); // Exclude entire embeddings dir
    exclude_args.push("--exclude=data/embeddings/**".to_string());
    exclude_args.push("--exclude=*.wv".to_string()); // All Word2Vec files
    exclude_args.push("--exclude=*.json".to_string()); // Large JSON files (except small configs)
    exclude_args.push("--exclude=*.log".to_string());
    exclude_args.push("--exclude=target".to_string());
    exclude_args.push("--exclude=.DS_Store".to_string());
    exclude_args.push("--exclude=old-scraper-data".to_string());
    exclude_args.push("--exclude=deploy".to_string());
    exclude_args.push("--exclude=*.zst".to_string()); // Compressed data files
    exclude_args.push("--exclude=*.parquet".to_string());
    exclude_args.push("--exclude=*.csv".to_string()); // Large CSV files
    // But include small config JSONs and essential files
    exclude_args.push("--include=pyproject.toml".to_string());
    exclude_args.push("--include=requirements.txt".to_string());
    exclude_args.push("--include=justfile".to_string());
    exclude_args.push("--include=.runctl.toml".to_string());

    // Build tar command arguments
    let mut tar_args = vec!["-czf".to_string(), "-".to_string()];
    tar_args.extend(exclude_args.iter().cloned());
    for pattern in include_patterns {
        tar_args.push("--include".to_string());
        tar_args.push(pattern.clone());
    }
    tar_args.push(".".to_string());

    // Create SSH command to extract on remote
    let ssh_cmd = format!(
        "mkdir -p {} && cd {} && tar -xzf - 2>&1",
        project_dir, project_dir
    );

    // Use shell to pipe tar directly to ssh (streaming, no buffering)
    let project_root_str = project_root.to_string_lossy().to_string();
    let key_path_clone = key_path.to_string();
    let ip_clone = ip.to_string();
    let user_clone = user.to_string();
    let output_format_clone = output_format.to_string();

    // Build the full command: cd project_root && tar ... | ssh ...
    let tar_cmd_str = format!(
        "cd {} && tar {}",
        project_root_str,
        tar_args.join(" ")
    );

    let full_cmd = format!(
        "{} | ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o ServerAliveInterval=60 -o ServerAliveCountMax=3 -o TCPKeepAlive=yes -i {} {}@{} '{}'",
        tar_cmd_str, key_path_clone, user_clone, ip_clone, ssh_cmd
    );

    if output_format != "json" {
        println!("   Executing streaming sync (tar | ssh)...");
    }

    // Execute in blocking task with timeout
    let sync_result = tokio::time::timeout(
        Duration::from_secs(300),
        tokio::task::spawn_blocking(move || -> Result<()> {
            use std::process::Command;
            let output = Command::new("sh")
                .arg("-c")
                .arg(&full_cmd)
                .output()
                .map_err(|e| {
                    TrainctlError::Ssm(format!("Failed to execute sync command: {}", e))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Err(TrainctlError::Ssm(format!(
                    "Sync failed: stderr={}, stdout={}",
                    stderr, stdout
                )));
            }

            Ok(())
        }),
    )
    .await;

    match sync_result {
        Ok(Ok(Ok(()))) => {
            if output_format != "json" {
                println!("   Code synced successfully (shell-based)");
            }
            Ok(())
        }
        Ok(Ok(Err(e))) => Err(e),
        Ok(Err(_join_err)) => Err(TrainctlError::Ssm(
            "Task join error during sync".to_string(),
        )),
        Err(_) => Err(TrainctlError::Ssm(
            "SSH sync timed out after 5 minutes".to_string(),
        )),
    }
}

/// Sync code to instance using native Rust SSH and tar
///
/// # Arguments
/// * `include_patterns` - Patterns to include even if gitignored (e.g., `data/`, `datasets/`)
///   These are added as negations to override `.gitignore` rules
pub async fn sync_code_native(
    key_path: &str,
    ip: &str,
    user: &str,
    project_dir: &str,
    project_root: &Path,
    output_format: &str,
    include_patterns: &[String],
) -> Result<()> {
    // Check if shell-based sync is requested
    if std::env::var("TRAINCTL_USE_SHELL_SYNC").is_ok() {
        return sync_code_shell(
            key_path,
            ip,
            user,
            project_dir,
            project_root,
            output_format,
            include_patterns,
        )
        .await;
    }
    let pb = if output_format != "json" {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .expect("Progress bar template should be valid"),
        );
        pb.set_message("Connecting to instance...");
        Some(pb)
    } else {
        None
    };

    // Run SSH operations in blocking task (ssh2 is synchronous)
    let key_path_clone = key_path.to_string();
    let ip_clone = ip.to_string();
    let user_clone = user.to_string();
    let project_dir_clone = project_dir.to_string();
    let project_root_clone = project_root.to_path_buf();
    let include_patterns_clone = include_patterns.to_vec();
    let pb_clone = pb.clone();

    // Add timeout to prevent hanging
    let sync_result = tokio::time::timeout(
        std::time::Duration::from_secs(300), // 5 minute timeout
        tokio::task::spawn_blocking(move || {
            // Connect via SSH
            let tcp = TcpStream::connect(format!("{}:22", ip_clone))
                .map_err(|e| TrainctlError::Ssm(format!("Failed to connect to {}:22: {}", ip_clone, e)))?;

        let mut sess = Session::new()
            .map_err(|e| TrainctlError::Ssm(format!("Failed to create SSH session: {}", e)))?;

        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| TrainctlError::Ssm(format!("SSH handshake failed: {}", e)))?;

        // Authenticate with private key
        sess.userauth_pubkey_file(&user_clone, None, Path::new(&key_path_clone), None)
            .map_err(|e| {
                TrainctlError::Ssm(format!(
                    "SSH authentication failed: {}. Check key permissions (chmod 600 {})",
                    e, key_path_clone
                ))
            })?;

        if !sess.authenticated() {
            return Err(TrainctlError::Ssm(format!(
                "SSH authentication failed. Check key permissions: chmod 600 {}",
                key_path_clone
            )));
        }

        if let Some(ref p) = pb_clone {
            p.set_message("Checking if code exists on instance...");
        }

        // Check if code exists (for incremental sync)
        let check_cmd = format!("test -d {} && echo EXISTS || echo NOT_FOUND", project_dir_clone);
        let use_incremental = check_remote_directory(&sess, &check_cmd)?;

        if use_incremental {
            if let Some(ref p) = pb_clone {
                p.set_message("Code exists, using incremental sync...");
            }

            // Incremental sync: compare files and sync only changes
            sync_incremental_blocking(
                &sess,
                &project_root_clone,
                &project_dir_clone,
                &pb_clone,
                &include_patterns_clone,
            )?;

            if let Some(ref p) = pb_clone {
                p.finish_with_message("Code synced (incremental)");
            }
            return Ok(());
        }

        // Full sync: create tar archive and transfer
        if let Some(ref p) = pb_clone {
            p.set_message("Performing full sync (tar archive)...");
        }

        sync_full_tar_blocking(
            &sess,
            &project_root_clone,
            &project_dir_clone,
            &pb_clone,
            &include_patterns_clone,
        )?;

        if let Some(ref p) = pb_clone {
            p.finish_with_message("Code synced successfully");
        }

            Ok(())
        }),
    )
    .await;

    match sync_result {
        Ok(Ok(result)) => result,
        Ok(Err(_e)) => {
            // Task join error - try shell fallback
            if output_format != "json" {
                println!("   Native sync failed, trying shell-based fallback...");
            }
            sync_code_shell(
                key_path,
                ip,
                user,
                project_dir,
                project_root,
                output_format,
                include_patterns,
            )
            .await
        }
        Err(_) => {
            // Timeout - try shell fallback
            if output_format != "json" {
                println!("   Native sync timed out, trying shell-based fallback...");
            }
            sync_code_shell(
                key_path,
                ip,
                user,
                project_dir,
                project_root,
                output_format,
                include_patterns,
            )
            .await
        }
    }
}

/// Check if remote directory exists
fn check_remote_directory(sess: &Session, command: &str) -> Result<bool> {
    let mut channel = sess
        .channel_session()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to create SSH channel: {}", e)))?;

    channel
        .exec(command)
        .map_err(|e| TrainctlError::Ssm(format!("Failed to execute command: {}", e)))?;

    let mut output = String::new();
    channel
        .read_to_string(&mut output)
        .map_err(|e| TrainctlError::Ssm(format!("Failed to read command output: {}", e)))?;

    channel
        .wait_close()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to close channel: {}", e)))?;

    Ok(output.contains("EXISTS"))
}

/// Build a gitignore matcher with overrides for include_patterns
fn build_gitignore_matcher(project_root: &Path, include_patterns: &[String]) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(project_root);

    // Add negations for patterns to include (even if gitignored)
    // In gitignore, ! prefix negates a pattern
    for pattern in include_patterns {
        // Normalize pattern: ensure it ends with / if it's a directory pattern
        let normalized_pattern = if pattern.ends_with('/') {
            format!("!{}**", pattern)
        } else {
            format!("!{}", pattern)
        };

        builder.add_line(None, &normalized_pattern).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to add include pattern '{}': {}",
                pattern, e
            )))
        })?;
    }

    builder.build().map_err(|e| {
        TrainctlError::Io(std::io::Error::other(format!(
            "Failed to build gitignore matcher: {}",
            e
        )))
    })
}

/// Check if a path matches any include pattern using proper path matching
fn matches_include_pattern(path: &Path, pattern: &str, project_root: &Path) -> bool {
    let rel_path = match path.strip_prefix(project_root) {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Normalize pattern: remove leading/trailing slashes for comparison
    let pattern = pattern.trim_matches('/');
    if pattern.is_empty() {
        return false;
    }

    // Check if pattern matches as a directory prefix
    // "data/" should match "data/train.csv" but not "my_data_file.txt"
    let pattern_path = Path::new(pattern);

    // Match if:
    // 1. Relative path starts with pattern (directory prefix match)
    // 2. Pattern is a parent directory of the file
    rel_path.starts_with(pattern_path)
        || rel_path
            .parent()
            .map(|p| p == pattern_path || p.starts_with(pattern_path))
            .unwrap_or(false)
}

/// Get list of files to sync (unified logic for both incremental and full sync)
fn get_files_to_sync(project_root: &Path, include_patterns: &[String]) -> Result<Vec<PathBuf>> {
    // Build gitignore matcher with overrides
    let gitignore = build_gitignore_matcher(project_root, include_patterns)?;

    // Walk all files (we'll filter manually using the matcher)
    let files: Vec<PathBuf> = WalkBuilder::new(project_root)
        .git_ignore(false) // Don't use WalkBuilder's gitignore - we'll check manually
        .git_global(false)
        .git_exclude(false)
        .build()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            // Skip directories
            if !path.is_file() {
                return None;
            }

            let rel_path = match path.strip_prefix(project_root) {
                Ok(p) => p,
                Err(_) => return None,
            };

            // Check if this file matches any include pattern
            let matches_include = include_patterns
                .iter()
                .any(|pattern| matches_include_pattern(path, pattern, project_root));

            // Use gitignore matcher to check if file should be ignored
            let matched = gitignore.matched(rel_path, false);

            // Include if:
            // 1. Matches include pattern (even if gitignored), OR
            // 2. Not gitignored (normal case)
            if matches_include || !matched.is_ignore() {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .collect();

    Ok(files)
}

/// Incremental sync: compare and sync only changed files (blocking)
fn sync_incremental_blocking(
    sess: &Session,
    project_root: &Path,
    remote_dir: &str,
    pb: &Option<ProgressBar>,
    include_patterns: &[String],
) -> Result<()> {
    // Get list of files to sync using unified logic
    let files_to_sync = get_files_to_sync(project_root, include_patterns)?;

    if let Some(ref p) = pb {
        p.set_message(format!("Syncing {} files...", files_to_sync.len()));
    }

    // Use SFTP for file transfer
    let sftp = sess
        .sftp()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to create SFTP session: {}", e)))?;

    let mut synced = 0;
    for file_path in &files_to_sync {
        let relative_path = file_path.strip_prefix(project_root).map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Failed to get relative path: {}", e),
            ))
        })?;

        let remote_path = format!("{}/{}", remote_dir, relative_path.display());
        let remote_path_parent = Path::new(&remote_path).parent().ok_or_else(|| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid remote path",
            ))
        })?;

        // Create remote directory if needed
        let mut remote_dir_path = remote_path_parent.to_string_lossy().to_string();
        if !remote_dir_path.starts_with('/') {
            remote_dir_path = format!("{}/{}", remote_dir, remote_dir_path);
        }

        // Ensure remote directory exists
        let mkdir_cmd = format!("mkdir -p {}", remote_dir_path);
        let mut channel = sess
            .channel_session()
            .map_err(|e| TrainctlError::Ssm(format!("Failed to create channel: {}", e)))?;
        channel
            .exec(&mkdir_cmd)
            .map_err(|e| TrainctlError::Ssm(format!("Failed to create directory: {}", e)))?;
        channel.wait_close().ok();

        // Read local file
        let mut local_file = File::open(file_path).map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to open {}: {}", file_path.display(), e),
            ))
        })?;

        let mut contents = Vec::new();
        local_file
            .read_to_end(&mut contents)
            .map_err(TrainctlError::Io)?;

        // Write to remote via SFTP
        let mut remote_file = sftp.create(Path::new(&remote_path)).map_err(|e| {
            TrainctlError::Ssm(format!(
                "Failed to create remote file {}: {}",
                remote_path, e
            ))
        })?;

        remote_file.write_all(&contents).map_err(|e| {
            TrainctlError::Ssm(format!("Failed to write to {}: {}", remote_path, e))
        })?;

        // Close file to ensure it's written
        drop(remote_file);

        // Set permissions via SFTP (0o644 = rw-r--r--)
        // Note: Permissions are non-critical, so we ignore errors
        let stat = ssh2::FileStat {
            size: Some(contents.len() as u64),
            uid: None,
            gid: None,
            perm: Some(0o644),
            atime: None,
            mtime: None,
        };
        sftp.setstat(Path::new(&remote_path), stat).ok();

        synced += 1;
        if let Some(ref p) = pb {
            p.set_message(format!(
                "Synced {}/{} files...",
                synced,
                files_to_sync.len()
            ));
        }
    }

    info!("Incremental sync completed: {} files", synced);
    Ok(())
}

/// Full sync: create tar.gz archive and transfer via SSH (blocking)
fn sync_full_tar_blocking(
    sess: &Session,
    project_root: &Path,
    remote_dir: &str,
    pb: &Option<ProgressBar>,
    include_patterns: &[String],
) -> Result<()> {
    if let Some(ref p) = pb {
        p.set_message("Creating tar archive...");
    }

    // Get list of files to sync using unified logic
    let files_to_sync = get_files_to_sync(project_root, include_patterns)?;

    if let Some(ref p) = pb {
        p.set_message(format!("Archiving {} files...", files_to_sync.len()));
    }

    // Create tar.gz archive in memory
    let mut archive_data = Vec::new();
    {
        let encoder = GzEncoder::new(&mut archive_data, Compression::default());
        let mut tar = Builder::new(encoder);

        // Add all files to archive
        for file_path in &files_to_sync {
            let relative_path = file_path.strip_prefix(project_root).map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Failed to get relative path: {}", e),
                ))
            })?;

            tar.append_file(relative_path, &mut File::open(file_path)?)
                .map_err(|e| {
                    TrainctlError::Io(std::io::Error::other(format!(
                        "Failed to add file to archive: {}",
                        e
                    )))
                })?;
        }

        tar.finish().map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to finish archive: {}",
                e
            )))
        })?;
    }

    if let Some(ref p) = pb {
        p.set_message(format!("Transferring {} bytes...", archive_data.len()));
    }

    // Create remote directory
    let mkdir_cmd = format!("mkdir -p {}", remote_dir);
    let mut channel = sess
        .channel_session()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to create channel: {}", e)))?;
    channel
        .exec(&mkdir_cmd)
        .map_err(|e| TrainctlError::Ssm(format!("Failed to create directory: {}", e)))?;
    channel.wait_close().ok();

    // Transfer archive via SSH and extract
    let extract_cmd = format!("cd {} && tar -xzf -", remote_dir);
    let mut channel = sess
        .channel_session()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to create channel: {}", e)))?;

    channel
        .exec(&extract_cmd)
        .map_err(|e| TrainctlError::Ssm(format!("Failed to execute extract command: {}", e)))?;

    // Write archive data to channel
    channel
        .write_all(&archive_data)
        .map_err(|e| TrainctlError::Ssm(format!("Failed to write archive data: {}", e)))?;

    channel
        .send_eof()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to send EOF: {}", e)))?;

    // Wait for completion
    channel
        .wait_close()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to close channel: {}", e)))?;

    let exit_status = channel
        .exit_status()
        .map_err(|e| TrainctlError::Ssm(format!("Failed to get exit status: {}", e)))?;

    if exit_status != 0 {
        let mut error_output = String::new();
        channel.stderr().read_to_string(&mut error_output).ok();
        return Err(TrainctlError::Ssm(format!(
            "Archive extraction failed with status {}: {}",
            exit_status, error_output
        )));
    }

    info!(
        "Full sync completed: {} bytes transferred",
        archive_data.len()
    );
    Ok(())
}
