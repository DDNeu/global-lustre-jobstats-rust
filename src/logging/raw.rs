//! Raw SSH data logger
//!
//! Logs unprocessed SSH output from `lctl get_param -n` commands before parsing.
//! Each entry is timestamped and clearly delimited.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::path_resolver::{generate_rotated_filename, resolve_path, ExportFormat};

/// Raw data logger that saves SSH output before parsing
pub struct RawLogger {
    path: PathBuf,
    file: tokio::fs::File,
    bytes_written: u64,
    max_size: Option<u64>,
    rotation_index: u32,
}

impl RawLogger {
    /// Create a new raw data logger
    pub async fn new(path: Option<&PathBuf>, max_size: Option<u64>) -> Result<Self> {
        let resolved_path = resolve_path(path, ExportFormat::Raw);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved_path)
            .await?;

        // Get current file size for rotation tracking
        let metadata = file.metadata().await?;
        let bytes_written = metadata.len();

        Ok(Self {
            path: resolved_path,
            file,
            bytes_written,
            max_size,
            rotation_index: 0,
        })
    }

    /// Check if rotation is needed and rotate if so
    async fn maybe_rotate(&mut self) -> Result<()> {
        if let Some(max_size) = self.max_size {
            if self.bytes_written >= max_size {
                self.rotate().await?;
            }
        }
        Ok(())
    }

    /// Rotate to a new log file
    async fn rotate(&mut self) -> Result<()> {
        // Flush and close current file
        self.file.flush().await?;

        // Generate new filename
        self.rotation_index += 1;
        let new_path = generate_rotated_filename(&self.path, self.rotation_index);

        // Open new file
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await?;

        self.bytes_written = 0;
        Ok(())
    }

    /// Log raw SSH data from a host
    ///
    /// # Arguments
    /// * `host` - The hostname that provided the data
    /// * `param` - The lctl param that was queried
    /// * `data` - The raw SSH output
    /// * `timestamp` - The time the data was collected
    pub async fn log_raw_data(
        &mut self,
        host: &str,
        param: &str,
        data: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        self.maybe_rotate().await?;

        let entry = format!(
            "### HOST: {} | PARAM: {} | TIMESTAMP: {} ###\n{}\n### END ###\n\n",
            host,
            param,
            timestamp.to_rfc3339(),
            data
        );

        let bytes = entry.as_bytes();
        self.file.write_all(bytes).await?;
        self.bytes_written += bytes.len() as u64;

        // Flush to ensure data is written immediately
        self.file.flush().await?;

        Ok(())
    }

    /// Get the current log file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get total bytes written (across all rotations in this session)
    #[allow(dead_code)]
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Flush and close the logger
    pub async fn close(mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }
}

