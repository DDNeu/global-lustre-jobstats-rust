//! File format readers for replay data

use anyhow::{Context, Result};
use std::path::Path;

use super::state::{ReplayData, ReplayRecord};

/// Supported data formats for replay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Raw,
    Parquet,
    Prometheus,
    VictoriaMetrics,
}

/// Reader for replay data files
pub struct ReplayReader {
    format: DataFormat,
}

impl ReplayReader {
    pub fn new(format: DataFormat) -> Self {
        Self { format }
    }

    /// Read replay data from a file
    pub async fn read(&self, path: &Path) -> Result<ReplayData> {
        match self.format {
            DataFormat::Raw => self.read_raw(path).await,
            DataFormat::Parquet => self.read_parquet(path),
            DataFormat::Prometheus => self.read_prometheus(path).await,
            DataFormat::VictoriaMetrics => self.read_victoriametrics(path).await,
        }
    }

    /// Read raw log format
    async fn read_raw(&self, path: &Path) -> Result<ReplayData> {
        use chrono::DateTime;
        use tokio::fs;

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read raw log: {:?}", path))?;

        let mut data = ReplayData::new();

        // Parse raw log format:
        // ### HOST: xxx | PARAM: xxx | TIMESTAMP: 2024-01-01T00:00:00+00:00 ###
        // <job stats data>
        // ### END ###
        let mut current_timestamp: Option<i64> = None;
        let mut current_block = String::new();
        let mut in_block = false;

        for line in content.lines() {
            if line.starts_with("### HOST:") {
                // Parse timestamp from header
                if let Some(ts_start) = line.find("TIMESTAMP: ") {
                    let ts_str = &line[ts_start + 11..];
                    if let Some(ts_end) = ts_str.find(" ###") {
                        let ts_part = &ts_str[..ts_end];
                        if let Ok(dt) = DateTime::parse_from_rfc3339(ts_part) {
                            current_timestamp = Some(dt.timestamp());
                            in_block = true;
                            current_block.clear();
                        }
                    }
                }
            } else if line.starts_with("### END ###") {
                // Parse the block content
                if let Some(ts) = current_timestamp {
                    parse_job_stats_block(&current_block, ts, &mut data);
                }
                in_block = false;
                current_timestamp = None;
            } else if in_block {
                current_block.push_str(line);
                current_block.push('\n');
            }
        }

        data.finalize();
        Ok(data)
    }

    /// Read Parquet format
    fn read_parquet(&self, path: &Path) -> Result<ReplayData> {
        use arrow::array::{Array, Int64Array, StringArray, TimestampMillisecondArray};
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        use std::fs::File;

        let file = File::open(path)
            .with_context(|| format!("Failed to open parquet file: {:?}", path))?;

        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let reader = builder.build()?;

        let mut data = ReplayData::new();

        for batch_result in reader {
            let batch = batch_result?;

            // Get columns by name
            let ts_col = batch
                .column_by_name("timestamp")
                .context("Missing timestamp column")?;
            let job_col = batch
                .column_by_name("job_id")
                .context("Missing job_id column")?;
            let op_col = batch
                .column_by_name("operation")
                .context("Missing operation column")?;
            let val_col = batch
                .column_by_name("value")
                .context("Missing value column")?;

            let timestamps = ts_col
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .context("Invalid timestamp column type")?;
            let job_ids = job_col
                .as_any()
                .downcast_ref::<StringArray>()
                .context("Invalid job_id column type")?;
            let operations = op_col
                .as_any()
                .downcast_ref::<StringArray>()
                .context("Invalid operation column type")?;
            let values = val_col
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("Invalid value column type")?;

            for i in 0..batch.num_rows() {
                if timestamps.is_null(i) || job_ids.is_null(i) || operations.is_null(i) || values.is_null(i) {
                    continue;
                }

                data.records.push(ReplayRecord {
                    timestamp: timestamps.value(i) / 1000, // Convert ms to seconds
                    job_id: job_ids.value(i).to_string(),
                    operation: operations.value(i).to_string(),
                    value: values.value(i),
                });
            }
        }

        data.finalize();
        Ok(data)
    }

    /// Read Prometheus exposition format
    async fn read_prometheus(&self, path: &Path) -> Result<ReplayData> {
        use regex::Regex;
        use tokio::fs;

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read prometheus file: {:?}", path))?;

        let mut data = ReplayData::new();

        // Parse Prometheus format:
        // lustre_job_op_total{job_id="123:user",operation="read"} 1234 1704067200000
        let re = Regex::new(
            r#"lustre_job_op_total\{job_id="([^"]+)",operation="([^"]+)"\}\s+(\d+)\s+(\d+)"#,
        )?;

        for line in content.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }

            if let Some(caps) = re.captures(line) {
                let job_id = unescape_prometheus_label(&caps[1]);
                let operation = unescape_prometheus_label(&caps[2]);
                let value: i64 = caps[3].parse().unwrap_or(0);
                let timestamp_ms: i64 = caps[4].parse().unwrap_or(0);

                data.records.push(ReplayRecord {
                    timestamp: timestamp_ms / 1000,
                    job_id,
                    operation,
                    value,
                });
            }
        }

        data.finalize();
        Ok(data)
    }

    /// Read VictoriaMetrics JSON Lines format
    async fn read_victoriametrics(&self, path: &Path) -> Result<ReplayData> {
        use serde::Deserialize;
        use std::collections::HashMap;
        use tokio::fs;

        #[derive(Deserialize)]
        struct VmMetric {
            metric: HashMap<String, String>,
            values: Vec<i64>,
            timestamps: Vec<i64>,
        }

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read VictoriaMetrics file: {:?}", path))?;

        let mut data = ReplayData::new();

        for line in content.lines() {
            if line.is_empty() {
                continue;
            }

            let vm_metric: VmMetric = match serde_json::from_str(line) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let job_id = vm_metric.metric.get("job_id").cloned().unwrap_or_default();
            let operation = vm_metric.metric.get("operation").cloned().unwrap_or_default();

            // Each metric can have multiple values/timestamps
            for (i, &value) in vm_metric.values.iter().enumerate() {
                let timestamp_ms = vm_metric.timestamps.get(i).copied().unwrap_or(0);
                data.records.push(ReplayRecord {
                    timestamp: timestamp_ms / 1000,
                    job_id: job_id.clone(),
                    operation: operation.clone(),
                    value,
                });
            }
        }

        data.finalize();
        Ok(data)
    }
}

/// Parse a block of job stats data (from raw log format)
fn parse_job_stats_block(block: &str, timestamp: i64, data: &mut ReplayData) {
    // Simple parser for job_stats format
    // Looking for patterns like:
    // - job_id: 123:user
    // - read_bytes: { samples: 10, ... }
    // - write_bytes: { samples: 5, ... }

    let mut current_job_id: Option<String> = None;

    for line in block.lines() {
        let line = line.trim();

        if line.starts_with("job_id:") {
            current_job_id = line.strip_prefix("job_id:").map(|s| s.trim().to_string());
        } else if let Some(ref job_id) = current_job_id {
            // Try to parse operation lines
            // Format: operation_name: { samples: N, ... } or operation_name: N
            if let Some((op_name, rest)) = line.split_once(':') {
                let op_name = op_name.trim();
                let rest = rest.trim();

                // Skip non-operation lines
                if op_name.is_empty() || op_name.contains(' ') {
                    continue;
                }

                // Try to extract value
                let value = if rest.starts_with('{') {
                    // Parse samples from { samples: N, ... }
                    extract_samples_value(rest)
                } else {
                    // Direct numeric value
                    rest.parse::<i64>().ok()
                };

                if let Some(val) = value {
                    data.records.push(ReplayRecord {
                        timestamp,
                        job_id: job_id.clone(),
                        operation: op_name.to_string(),
                        value: val,
                    });
                }
            }
        }
    }
}

/// Extract the samples value from a YAML-like struct
fn extract_samples_value(s: &str) -> Option<i64> {
    // Looking for: { samples: N, ... }
    if let Some(start) = s.find("samples:") {
        let rest = &s[start + 8..];
        let rest = rest.trim_start();
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest[..end].parse().ok()
    } else {
        None
    }
}

/// Unescape Prometheus label values
fn unescape_prometheus_label(s: &str) -> String {
    s.replace("\\\\", "\x00")
        .replace("\\\"", "\"")
        .replace("\\n", "\n")
        .replace('\x00', "\\")
}
