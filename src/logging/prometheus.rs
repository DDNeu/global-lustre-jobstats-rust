//! Prometheus exposition format logger
//!
//! Exports parsed job statistics in Prometheus text exposition format (0.0.4).
//!
//! Format example:
//! # HELP lustre_job_op_total Total operations per job and operation type
//! # TYPE lustre_job_op_total counter
//! lustre_job_op_total{job_id="123:user",operation="read"} 1234 1704067200000

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::path_resolver::{generate_rotated_filename, resolve_path, ExportFormat};

/// Prometheus logger that exports job stats in text exposition format
pub struct PrometheusLogger {
    path: PathBuf,
    file: tokio::fs::File,
    bytes_written: u64,
    max_size: Option<u64>,
    rotation_index: u32,
    header_written: bool,
}

impl PrometheusLogger {
    /// Create a new Prometheus logger
    pub async fn new(path: Option<&PathBuf>, max_size: Option<u64>) -> Result<Self> {
        let resolved_path = resolve_path(path, ExportFormat::Prometheus);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved_path)
            .await?;

        let metadata = file.metadata().await?;
        let bytes_written = metadata.len();
        
        // If file is empty, we need to write headers
        let header_written = bytes_written > 0;

        Ok(Self {
            path: resolved_path,
            file,
            bytes_written,
            max_size,
            rotation_index: 0,
            header_written,
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
        self.file.flush().await?;
        self.rotation_index += 1;
        let new_path = generate_rotated_filename(&self.path, self.rotation_index);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await?;
        self.bytes_written = 0;
        self.header_written = false; // Need to write headers in new file
        Ok(())
    }

    /// Write the HELP and TYPE headers if not already written
    async fn write_headers(&mut self) -> Result<()> {
        if !self.header_written {
            let headers = concat!(
                "# HELP lustre_job_op_total Total operations per job and operation type\n",
                "# TYPE lustre_job_op_total counter\n"
            );
            let bytes = headers.as_bytes();
            self.file.write_all(bytes).await?;
            self.bytes_written += bytes.len() as u64;
            self.header_written = true;
        }
        Ok(())
    }

    /// Log job statistics in Prometheus format
    ///
    /// # Arguments
    /// * `jobs` - Map of job_id -> operation -> value
    /// * `timestamp_secs` - Unix timestamp in seconds
    pub async fn log_job_stats(
        &mut self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        timestamp_secs: i64,
    ) -> Result<()> {
        self.maybe_rotate().await?;
        self.write_headers().await?;

        let ts_ms = timestamp_secs * 1000;

        for (job_id, ops) in jobs {
            for (op_name, value) in ops {
                let escaped_job_id = escape_label_value(job_id);
                let escaped_op = escape_label_value(op_name);

                let line = format!(
                    "lustre_job_op_total{{job_id=\"{}\",operation=\"{}\"}} {} {}\n",
                    escaped_job_id, escaped_op, value, ts_ms
                );
                let bytes = line.as_bytes();
                self.file.write_all(bytes).await?;
                self.bytes_written += bytes.len() as u64;
            }
        }

        self.file.flush().await?;
        Ok(())
    }

    /// Get the current log file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Flush and close the logger
    pub async fn close(mut self) -> Result<()> {
        self.file.flush().await?;
        Ok(())
    }
}

/// Escape label values according to Prometheus spec
fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

