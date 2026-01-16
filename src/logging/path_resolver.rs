//! Path resolution and size parsing utilities for logging

use anyhow::{anyhow, Result};
use chrono::Local;
use std::path::PathBuf;

/// Export format types for logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Raw,
    VictoriaMetrics,
    Prometheus,
    Parquet,
}

impl ExportFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Raw => "txt",
            ExportFormat::VictoriaMetrics => "json",
            ExportFormat::Prometheus => "prom",
            ExportFormat::Parquet => "parquet",
        }
    }

    /// Get the format name for filename generation
    pub fn name(&self) -> &'static str {
        match self {
            ExportFormat::Raw => "raw",
            ExportFormat::VictoriaMetrics => "victoriametrics",
            ExportFormat::Prometheus => "prometheus",
            ExportFormat::Parquet => "parquet",
        }
    }
}

/// Generate an auto-named filename for the given format
pub fn generate_filename(format: ExportFormat) -> String {
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    format!(
        "glljobstat-{}-{}.{}",
        timestamp,
        format.name(),
        format.extension()
    )
}

/// Generate a rotated filename with rotation index
pub fn generate_rotated_filename(base_path: &PathBuf, rotation_index: u32) -> PathBuf {
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let stem = base_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("glljobstat");
    let ext = base_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("log");
    
    let new_filename = format!("{}-{}-{}.{}", stem, timestamp, rotation_index, ext);
    
    if let Some(parent) = base_path.parent() {
        parent.join(new_filename)
    } else {
        PathBuf::from(new_filename)
    }
}

/// Resolve the path for logging based on the input:
/// - If path is None or empty: use current working directory with auto-generated name
/// - If path is a directory: use that directory with auto-generated name
/// - If path is a file: use that exact path
pub fn resolve_path(path: Option<&PathBuf>, format: ExportFormat) -> PathBuf {
    let filename = generate_filename(format);

    match path {
        None => PathBuf::from(&filename),
        Some(p) if p.as_os_str().is_empty() => PathBuf::from(&filename),
        Some(p) if p.is_dir() => p.join(&filename),
        Some(p) => p.clone(),
    }
}

/// Parse a human-readable size string into bytes
/// Supports: k/K (1024), M (1024²), G (1024³), T (1024⁴), P (1024⁵)
/// Examples: "1k", "100M", "1G", "10T", "1P"
pub fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim();
    if size_str.is_empty() {
        return Err(anyhow!("Empty size string"));
    }

    // Check if last character is a unit suffix
    let last_char = size_str.chars().last().unwrap();
    let (num_part, multiplier) = if last_char.is_ascii_digit() {
        // No suffix, treat as bytes
        (size_str, 1u64)
    } else {
        let num_part = &size_str[..size_str.len() - 1];
        let multiplier = match last_char.to_ascii_uppercase() {
            'K' => 1024u64,
            'M' => 1024u64 * 1024,
            'G' => 1024u64 * 1024 * 1024,
            'T' => 1024u64 * 1024 * 1024 * 1024,
            'P' => 1024u64 * 1024 * 1024 * 1024 * 1024,
            _ => return Err(anyhow!("Unknown size unit: {}", last_char)),
        };
        (num_part, multiplier)
    };

    let value: f64 = num_part
        .parse()
        .map_err(|_| anyhow!("Invalid number in size: {}", num_part))?;

    if value < 0.0 {
        return Err(anyhow!("Size cannot be negative"));
    }

    Ok((value * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1k").unwrap(), 1024);
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("100M").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("1T").unwrap(), 1024u64 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1P").unwrap(), 1024u64 * 1024 * 1024 * 1024 * 1024);
        assert!(parse_size("").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("1X").is_err());
    }
}

