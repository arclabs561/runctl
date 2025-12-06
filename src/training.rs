use crate::error::{Result, TrainctlError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Training session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSession {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub platform: String, // "local", "runpod", "aws"
    pub script: PathBuf,
    pub checkpoint_dir: PathBuf,
    pub log_file: Option<PathBuf>,
    pub status: TrainingStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrainingStatus {
    Running,
    Completed,
    Failed(String),
    Interrupted,
}

impl TrainingSession {
    pub fn new(platform: String, script: PathBuf, checkpoint_dir: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            platform,
            script,
            checkpoint_dir,
            log_file: None,
            status: TrainingStatus::Running,
        }
    }

    pub fn save(&self, sessions_dir: &Path) -> Result<()> {
        let sessions_dir = sessions_dir.join("sessions");
        fs::create_dir_all(&sessions_dir)?;

        let session_file = sessions_dir.join(format!("{}.json", self.id));
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to serialize session: {}",
                e
            )))
        })?;
        fs::write(&session_file, content).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to write session file: {}",
                e
            )))
        })?;

        Ok(())
    }

    pub fn load(sessions_dir: &Path, session_id: &str) -> Result<Self> {
        let session_file = sessions_dir
            .join("sessions")
            .join(format!("{}.json", session_id));
        let content = fs::read_to_string(&session_file).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to read session file {}: {}",
                session_file.display(),
                e
            )))
        })?;
        let session: Self = serde_json::from_str(&content).map_err(|e| {
            TrainctlError::Io(std::io::Error::other(format!(
                "Failed to parse session {}: {}",
                session_file.display(),
                e
            )))
        })?;
        Ok(session)
    }

    pub fn list_sessions(sessions_dir: &Path) -> Result<Vec<Self>> {
        let sessions_path = sessions_dir.join("sessions");
        if !sessions_path.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&sessions_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Self>(&content) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(sessions)
    }
}

/// Checkpoint metadata helper
pub fn extract_checkpoint_info(checkpoint_path: &Path) -> Result<CheckpointInfo> {
    // For PyTorch checkpoints, we'd need torch-sys or similar
    // For now, return basic file info
    let metadata = fs::metadata(checkpoint_path)?;

    Ok(CheckpointInfo {
        path: checkpoint_path.to_path_buf(),
        size: metadata.len(),
        modified: metadata.modified()?,
        epoch: None, // Would extract from PyTorch checkpoint
        loss: None,
    })
}

#[derive(Debug)]
pub struct CheckpointInfo {
    pub path: PathBuf,
    pub size: u64,
    pub modified: std::time::SystemTime,
    pub epoch: Option<u32>,
    pub loss: Option<f64>,
}
