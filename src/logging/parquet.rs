//! Apache Parquet format logger
//!
//! Exports parsed job statistics to Apache Parquet format for efficient
//! columnar storage and analytics.
//!
//! Schema:
//! - timestamp (INT64, milliseconds since epoch)
//! - job_id (UTF8/STRING)
//! - operation (UTF8/STRING)
//! - value (INT64)

use anyhow::{Context, Result};
use arrow::array::{Int64Array, StringArray, TimestampMillisecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use super::path_resolver::{generate_rotated_filename, resolve_path, ExportFormat};

/// A single record for the Parquet buffer
#[derive(Clone)]
struct ParquetRecord {
    timestamp: i64,
    job_id: String,
    operation: String,
    value: i64,
}

/// Parquet logger that exports job stats with buffered row group writes
pub struct ParquetLogger {
    path: PathBuf,
    writer: Option<ArrowWriter<File>>,
    schema: Arc<Schema>,
    buffer: Vec<ParquetRecord>,
    buffer_size: usize,
    bytes_written: u64,
    max_size: Option<u64>,
    rotation_index: u32,
}

impl ParquetLogger {
    /// Create a new Parquet logger
    ///
    /// # Arguments
    /// * `path` - Optional path (file, directory, or None for cwd)
    /// * `max_size` - Optional maximum file size before rotation
    /// * `buffer_size` - Number of records to buffer before flushing (default: 10_000)
    pub fn new(
        path: Option<&PathBuf>,
        max_size: Option<u64>,
        buffer_size: Option<usize>,
    ) -> Result<Self> {
        let resolved_path = resolve_path(path, ExportFormat::Parquet);
        let schema = Self::create_schema();
        let writer = Self::create_writer(&resolved_path, schema.clone())?;

        Ok(Self {
            path: resolved_path,
            writer: Some(writer),
            schema,
            buffer: Vec::new(),
            buffer_size: buffer_size.unwrap_or(10_000),
            bytes_written: 0,
            max_size,
            rotation_index: 0,
        })
    }

    /// Create the Arrow schema for job stats
    fn create_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("job_id", DataType::Utf8, false),
            Field::new("operation", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]))
    }

    /// Create a new Parquet writer for the given path
    fn create_writer(path: &PathBuf, schema: Arc<Schema>) -> Result<ArrowWriter<File>> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create parquet file: {:?}", path))?;

        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        ArrowWriter::try_new(file, schema, Some(props))
            .context("Failed to create Parquet writer")
    }

    /// Check if rotation is needed and rotate if so
    fn maybe_rotate(&mut self) -> Result<()> {
        if let Some(max_size) = self.max_size {
            if self.bytes_written >= max_size {
                self.rotate()?;
            }
        }
        Ok(())
    }

    /// Rotate to a new Parquet file
    fn rotate(&mut self) -> Result<()> {
        // Flush remaining buffer and close current writer
        self.flush()?;
        if let Some(writer) = self.writer.take() {
            writer.close()?;
        }

        // Generate new filename
        self.rotation_index += 1;
        let new_path = generate_rotated_filename(&self.path, self.rotation_index);

        // Create new writer
        self.writer = Some(Self::create_writer(&new_path, self.schema.clone())?);
        self.bytes_written = 0;

        Ok(())
    }

    /// Flush the buffer to disk as a row group
    pub fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let batch = self.create_record_batch()?;
        let batch_size = batch.get_array_memory_size();

        if let Some(ref mut writer) = self.writer {
            writer.write(&batch)?;
        }
        self.bytes_written += batch_size as u64;
        self.buffer.clear();

        Ok(())
    }

    /// Create a RecordBatch from the buffer
    fn create_record_batch(&self) -> Result<RecordBatch> {
        let timestamps: Vec<i64> = self.buffer.iter().map(|r| r.timestamp).collect();
        let job_ids: Vec<&str> = self.buffer.iter().map(|r| r.job_id.as_str()).collect();
        let operations: Vec<&str> = self.buffer.iter().map(|r| r.operation.as_str()).collect();
        let values: Vec<i64> = self.buffer.iter().map(|r| r.value).collect();

        RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(TimestampMillisecondArray::from(timestamps)),
                Arc::new(StringArray::from(job_ids)),
                Arc::new(StringArray::from(operations)),
                Arc::new(Int64Array::from(values)),
            ],
        )
        .context("Failed to create RecordBatch")
    }

    /// Log job statistics to Parquet
    ///
    /// # Arguments
    /// * `jobs` - Map of job_id -> operation -> value
    /// * `timestamp_secs` - Unix timestamp in seconds
    pub fn log_job_stats(
        &mut self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        timestamp_secs: i64,
    ) -> Result<()> {
        self.maybe_rotate()?;

        let ts_ms = timestamp_secs * 1000;

        for (job_id, ops) in jobs {
            for (op_name, value) in ops {
                self.buffer.push(ParquetRecord {
                    timestamp: ts_ms,
                    job_id: job_id.clone(),
                    operation: op_name.clone(),
                    value: *value,
                });
            }
        }

        // Flush if buffer exceeds threshold
        if self.buffer.len() >= self.buffer_size {
            self.flush()?;
        }

        Ok(())
    }

    /// Get the current log file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Close the Parquet writer, flushing any remaining data
    pub fn close(mut self) -> Result<()> {
        self.flush()?;
        if let Some(writer) = self.writer.take() {
            writer.close()?;
        }
        Ok(())
    }
}

