//! Data replay module for TUI
//!
//! Supports reading and replaying historical job statistics data from:
//! - Raw log files (.raw.log)
//! - Parquet files (.parquet)
//! - Prometheus metrics exports (.prom)
//! - VictoriaMetrics time series data (.vm.json)

mod readers;
mod state;

pub use readers::{DataFormat, ReplayReader};
pub use state::{PlaybackState, ReplayController, ReplayData};

use anyhow::{Context, Result};
use std::path::Path;

/// Detect the data format from file extension
pub fn detect_format(path: &Path) -> Result<DataFormat> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Invalid file path")?;

    if filename.ends_with(".parquet") {
        Ok(DataFormat::Parquet)
    } else if filename.ends_with(".raw.log") || filename.ends_with(".raw") {
        Ok(DataFormat::Raw)
    } else if filename.ends_with(".prom") || filename.ends_with(".prometheus") {
        Ok(DataFormat::Prometheus)
    } else if filename.ends_with(".vm.json") || filename.ends_with(".victoriametrics") {
        Ok(DataFormat::VictoriaMetrics)
    } else {
        // Try to detect from content or default to raw
        anyhow::bail!(
            "Cannot detect format from file extension. \
             Supported: .parquet, .raw.log, .prom, .vm.json"
        )
    }
}

/// Load replay data from a file
pub async fn load_replay_data(path: &Path) -> Result<ReplayData> {
    let format = detect_format(path)?;
    let reader = ReplayReader::new(format);
    reader.read(path).await
}

