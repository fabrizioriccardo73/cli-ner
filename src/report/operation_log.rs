use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionType {
    Trash,
    PermanentDelete,
    ExternalCommand,
    DryRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionStatus {
    Success,
    Failed(String),
    Skipped(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanedItem {
    pub path: String,
    pub size_bytes: u64,
    pub action: ActionType,
    pub status: ActionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub dry_run: bool,
    pub category: String,
    pub total_bytes_freed: u64,
    pub total_items_count: usize,
    pub duration_ms: u64,
    pub items: Vec<CleanedItem>,
}

impl OperationRecord {
    pub fn new(command: &str, category: &str, dry_run: bool) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            command: command.to_string(),
            dry_run,
            category: category.to_string(),
            total_bytes_freed: 0,
            total_items_count: 0,
            duration_ms: 0,
            items: Vec::new(),
        }
    }

    pub fn add_item(&mut self, path: String, size_bytes: u64, action: ActionType, status: ActionStatus) {
        if matches!(status, ActionStatus::Success) {
            self.total_bytes_freed += size_bytes;
        }
        self.total_items_count += 1;
        self.items.push(CleanedItem {
            path,
            size_bytes,
            action,
            status,
        });
    }

    pub fn set_duration(&mut self, duration: std::time::Duration) {
        self.duration_ms = duration.as_millis() as u64;
    }
}

/// Get the path to the log directory `~/.cli-ner/logs/`
pub fn get_log_dir() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".cli-ner").join("logs")
}

/// Save an operation record to a daily JSON Lines log file
pub fn save_operation_log(record: &OperationRecord) -> Result<PathBuf> {
    let log_dir = get_log_dir();
    fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    let date_str = record.timestamp.format("%Y-%m-%d").to_string();
    let log_file = log_dir.join(format!("operations_{}.jsonl", date_str));

    let json_line = serde_json::to_string(record).context("Failed to serialize operation record")?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .context("Failed to open log file")?;

    writeln!(file, "{}", json_line).context("Failed to write to log file")?;

    Ok(log_file)
}

/// Read all logged operations sorted by timestamp descending
pub fn read_recent_operations(limit: usize) -> Result<Vec<OperationRecord>> {
    let log_dir = get_log_dir();
    if !log_dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();

    let mut log_files: Vec<PathBuf> = fs::read_dir(&log_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "jsonl"))
        .collect();

    log_files.sort_by(|a, b| b.cmp(a)); // Newest first

    for file in log_files {
        if let Ok(content) = fs::read_to_string(&file) {
            for line in content.lines().rev() {
                if let Ok(record) = serde_json::from_str::<OperationRecord>(line) {
                    records.push(record);
                    if records.len() >= limit {
                        return Ok(records);
                    }
                }
            }
        }
    }

    Ok(records)
}
