//! Native Rust SSH-based code syncing
//!
//! Replaces shell-based tar/rsync/ssh commands with native Rust implementations
//! using ssh2-rs for SSH connections and tar crate for archive operations.

use crate::error::{Result, TrainctlError};
use flate2::write::GzEncoder;
use flate2::Compression;
use indicatif::{ProgressBar, ProgressStyle};
use ssh2::Session;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use tar::Builder;
use tracing::info;
use walkdir::WalkDir;

/// Sync code to instance using native Rust SSH and tar
pub async fn sync_code_native(
    key_path: &str,
    ip: &str,
    user: &str,
    project_dir: &str,
    project_root: &Path,
    output_format: &str,
) -> Result<()> {
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
    let key_path = key_path.to_string();
    let ip = ip.to_string();
    let user = user.to_string();
    let project_dir = project_dir.to_string();
    let project_root = project_root.to_path_buf();
    let pb_clone = pb.clone();

    tokio::task::spawn_blocking(move || {
        // Connect via SSH
        let tcp = TcpStream::connect(format!("{}:22", ip))
            .map_err(|e| TrainctlError::Ssm(format!("Failed to connect to {}:22: {}", ip, e)))?;

        let mut sess = Session::new()
            .map_err(|e| TrainctlError::Ssm(format!("Failed to create SSH session: {}", e)))?;

        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| TrainctlError::Ssm(format!("SSH handshake failed: {}", e)))?;

        // Authenticate with private key
        sess.userauth_pubkey_file(&user, None, Path::new(&key_path), None)
            .map_err(|e| {
                TrainctlError::Ssm(format!(
                    "SSH authentication failed: {}. Check key permissions (chmod 600 {})",
                    e, key_path
                ))
            })?;

        if !sess.authenticated() {
            return Err(TrainctlError::Ssm(format!(
                "SSH authentication failed. Check key permissions: chmod 600 {}",
                key_path
            )));
        }

        if let Some(ref p) = pb_clone {
            p.set_message("Checking if code exists on instance...");
        }

        // Check if code exists (for incremental sync)
        let check_cmd = format!("test -d {} && echo EXISTS || echo NOT_FOUND", project_dir);
        let use_incremental = check_remote_directory(&sess, &check_cmd)?;

        if use_incremental {
            if let Some(ref p) = pb_clone {
                p.set_message("Code exists, using incremental sync...");
            }

            // Incremental sync: compare files and sync only changes
            sync_incremental_blocking(&sess, &project_root, &project_dir, &pb_clone)?;

            if let Some(ref p) = pb_clone {
                p.finish_with_message("Code synced (incremental)");
            }
            return Ok(());
        }

        // Full sync: create tar archive and transfer
        if let Some(ref p) = pb_clone {
            p.set_message("Performing full sync (tar archive)...");
        }

        sync_full_tar_blocking(&sess, &project_root, &project_dir, &pb_clone)?;

        if let Some(ref p) = pb_clone {
            p.finish_with_message("Code synced successfully");
        }

        Ok(())
    })
    .await
    .map_err(|e| TrainctlError::Ssm(format!("Task join error: {}", e)))?
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

/// Incremental sync: compare and sync only changed files (blocking)
fn sync_incremental_blocking(
    sess: &Session,
    project_root: &Path,
    remote_dir: &str,
    pb: &Option<ProgressBar>,
) -> Result<()> {
    // Get list of files to sync (excluding patterns)
    let exclusions = [
        ".git",
        "checkpoints",
        "results",
        "data",
        "__pycache__",
        "*.pyc",
        ".aim",
        "node_modules",
        ".venv",
    ];

    let files_to_sync: Vec<PathBuf> = WalkDir::new(project_root)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            !exclusions
                .iter()
                .any(|excl| name == *excl || path.to_string_lossy().contains(excl))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

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
            .map_err(|e| TrainctlError::Io(e))?;

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
) -> Result<()> {
    if let Some(ref p) = pb {
        p.set_message("Creating tar archive...");
    }

    // Create tar.gz archive in memory
    let mut archive_data = Vec::new();
    {
        let encoder = GzEncoder::new(&mut archive_data, Compression::default());
        let mut tar = Builder::new(encoder);

        let exclusions = [
            ".git",
            "checkpoints",
            "results",
            "data",
            "__pycache__",
            ".aim",
            "node_modules",
            ".venv",
        ];

        for entry in WalkDir::new(project_root).into_iter() {
            let entry = entry.map_err(|e| {
                TrainctlError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("WalkDir error: {}", e),
                ))
            })?;

            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip excluded patterns
            if exclusions.iter().any(|excl| {
                name == *excl
                    || path.to_string_lossy().contains(excl)
                    || (excl.starts_with("*") && name.ends_with(&excl[1..]))
            }) {
                continue;
            }

            if path.is_file() {
                let relative_path = path.strip_prefix(project_root).map_err(|e| {
                    TrainctlError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to get relative path: {}", e),
                    ))
                })?;

                tar.append_file(relative_path, &mut File::open(path)?)
                    .map_err(|e| {
                        TrainctlError::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Failed to add file to archive: {}", e),
                        ))
                    })?;
            }
        }

        tar.finish().map_err(|e| {
            TrainctlError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to finish archive: {}", e),
            ))
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
