//! VictoriaMetrics JSON format logger
//!
//! Exports parsed job statistics in VictoriaMetrics native JSON format
//! compatible with the `/api/v1/import` endpoint.
//!
//! Format: JSON Lines (JSONL) with one metric per line:
//! {"metric":{"__name__":"lustre_job_op","job_id":"123:user","operation":"read"},"values":[1234],"timestamps":[1704067200000]}

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::path_resolver::{generate_rotated_filename, resolve_path, ExportFormat};

/// VictoriaMetrics metric structure for JSON export
#[derive(Serialize)]
struct VictoriaMetric {
    metric: HashMap<String, String>,
    values: Vec<i64>,
    timestamps: Vec<i64>,
}

/// VictoriaMetrics logger that exports job stats in JSONL format
pub struct VictoriaMetricsLogger {
    path: PathBuf,
    file: tokio::fs::File,
    bytes_written: u64,
    max_size: Option<u64>,
    rotation_index: u32,
}

impl VictoriaMetricsLogger {
    /// Create a new VictoriaMetrics logger
    pub async fn new(path: Option<&PathBuf>, max_size: Option<u64>) -> Result<Self> {
        let resolved_path = resolve_path(path, ExportFormat::VictoriaMetrics);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved_path)
            .await?;

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
        self.file.flush().await?;
        self.rotation_index += 1;
        let new_path = generate_rotated_filename(&self.path, self.rotation_index);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await?;
        self.bytes_written = 0;
        Ok(())
    }

    /// Log job statistics in VictoriaMetrics format
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

        let ts_ms = timestamp_secs * 1000; // Convert to milliseconds

        for (job_id, ops) in jobs {
            for (op_name, value) in ops {
                // Skip the aggregate "ops" counter if you want, or include it
                // Including all operations for completeness
                
                let mut metric_labels = HashMap::new();
                metric_labels.insert("__name__".to_string(), "lustre_job_op".to_string());
                metric_labels.insert("job_id".to_string(), job_id.clone());
                metric_labels.insert("operation".to_string(), op_name.clone());

                let vm_metric = VictoriaMetric {
                    metric: metric_labels,
                    values: vec![*value],
                    timestamps: vec![ts_ms],
                };

                let json_line = serde_json::to_string(&vm_metric)?;
                let line = format!("{}\n", json_line);
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

