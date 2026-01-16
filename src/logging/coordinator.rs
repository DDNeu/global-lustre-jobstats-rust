//! Logging coordinator that manages all loggers
//!
//! Provides a unified interface for:
//! - Raw SSH data logging
//! - VictoriaMetrics JSON export
//! - Prometheus exposition format export
//! - Apache Parquet columnar export

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::args::Args;

use super::parquet::ParquetLogger;
use super::path_resolver::parse_size;
use super::prometheus::PrometheusLogger;
use super::raw::RawLogger;
use super::victoriametrics::VictoriaMetricsLogger;

/// Coordinates all logging operations
pub struct LoggingCoordinator {
    raw_logger: Option<RawLogger>,
    vm_logger: Option<VictoriaMetricsLogger>,
    prom_logger: Option<PrometheusLogger>,
    parquet_logger: Option<ParquetLogger>,
    verbose: bool,
}

impl LoggingCoordinator {
    /// Create a new logging coordinator from command-line arguments
    pub async fn from_args(args: &Args) -> Result<Self> {
        // Parse max size if specified
        let max_size = if let Some(ref size_str) = args.log_max_size {
            Some(parse_size(size_str)?)
        } else {
            None
        };

        // Initialize raw logger if requested
        let raw_logger = if let Some(ref path) = args.log_raw_data {
            let logger = RawLogger::new(Some(path), max_size).await?;
            if args.verbose {
                eprintln!("Raw data logging to: {:?}", logger.path());
            }
            Some(logger)
        } else {
            None
        };

        // Initialize VictoriaMetrics logger if requested
        let vm_logger = if let Some(ref path) = args.log_data_victoriametrics {
            let logger = VictoriaMetricsLogger::new(Some(path), max_size).await?;
            if args.verbose {
                eprintln!("VictoriaMetrics logging to: {:?}", logger.path());
            }
            Some(logger)
        } else {
            None
        };

        // Initialize Prometheus logger if requested
        let prom_logger = if let Some(ref path) = args.log_data_prometheus {
            let logger = PrometheusLogger::new(Some(path), max_size).await?;
            if args.verbose {
                eprintln!("Prometheus logging to: {:?}", logger.path());
            }
            Some(logger)
        } else {
            None
        };

        // Initialize Parquet logger if requested
        let parquet_logger = if let Some(ref path) = args.log_data_parquet {
            let logger = ParquetLogger::new(Some(path), max_size, None)?;
            if args.verbose {
                eprintln!("Parquet logging to: {:?}", logger.path());
            }
            Some(logger)
        } else {
            None
        };

        Ok(Self {
            raw_logger,
            vm_logger,
            prom_logger,
            parquet_logger,
            verbose: args.verbose,
        })
    }

    /// Check if any logging is enabled
    #[allow(dead_code)]
    pub fn is_logging_enabled(&self) -> bool {
        self.raw_logger.is_some()
            || self.vm_logger.is_some()
            || self.prom_logger.is_some()
            || self.parquet_logger.is_some()
    }

    /// Check if raw logging is enabled
    #[allow(dead_code)]
    pub fn has_raw_logger(&self) -> bool {
        self.raw_logger.is_some()
    }

    /// Log raw SSH data before parsing
    pub async fn log_raw(
        &mut self,
        host: &str,
        param: &str,
        data: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        if let Some(ref mut logger) = self.raw_logger {
            if let Err(e) = logger.log_raw_data(host, param, data, timestamp).await {
                if self.verbose {
                    eprintln!("Warning: Failed to write raw log: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Log parsed job statistics to all configured formats
    pub async fn log_parsed(
        &mut self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        timestamp_secs: i64,
    ) -> Result<()> {
        // Log to VictoriaMetrics format
        if let Some(ref mut logger) = self.vm_logger {
            if let Err(e) = logger.log_job_stats(jobs, timestamp_secs).await {
                if self.verbose {
                    eprintln!("Warning: Failed to write VictoriaMetrics log: {}", e);
                }
            }
        }

        // Log to Prometheus format
        if let Some(ref mut logger) = self.prom_logger {
            if let Err(e) = logger.log_job_stats(jobs, timestamp_secs).await {
                if self.verbose {
                    eprintln!("Warning: Failed to write Prometheus log: {}", e);
                }
            }
        }

        // Log to Parquet format (synchronous)
        if let Some(ref mut logger) = self.parquet_logger {
            if let Err(e) = logger.log_job_stats(jobs, timestamp_secs) {
                if self.verbose {
                    eprintln!("Warning: Failed to write Parquet log: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Flush any buffered data and close all loggers
    pub async fn close(self) -> Result<()> {
        // Close raw logger
        if let Some(logger) = self.raw_logger {
            if let Err(e) = logger.close().await {
                if self.verbose {
                    eprintln!("Warning: Failed to close raw logger: {}", e);
                }
            }
        }

        // Close VictoriaMetrics logger
        if let Some(logger) = self.vm_logger {
            if let Err(e) = logger.close().await {
                if self.verbose {
                    eprintln!("Warning: Failed to close VictoriaMetrics logger: {}", e);
                }
            }
        }

        // Close Prometheus logger
        if let Some(logger) = self.prom_logger {
            if let Err(e) = logger.close().await {
                if self.verbose {
                    eprintln!("Warning: Failed to close Prometheus logger: {}", e);
                }
            }
        }

        // Close Parquet logger (flushes remaining buffer)
        if let Some(logger) = self.parquet_logger {
            if let Err(e) = logger.close() {
                if self.verbose {
                    eprintln!("Warning: Failed to close Parquet logger: {}", e);
                }
            }
        }

        Ok(())
    }
}

