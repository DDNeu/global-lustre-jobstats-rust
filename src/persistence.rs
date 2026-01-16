//! Persistence module for storing highest operation rates (replaces pickle)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::op_keys::OP_KEYS;

/// Stored data for top operations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopOpsEntry {
    pub rate: i64,
    pub timestamp: i64,
}

/// Stored data for a job with top operation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopJobEntry {
    pub job_id: String,
    pub ops: HashMap<String, i64>,
    pub timestamp: i64,
}

/// The persisted database structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopDb {
    pub top_ops: HashMap<String, TopOpsEntry>,
    pub top_job_per_op: HashMap<String, TopJobEntry>,
}

impl TopDb {
    /// Load from file or create new
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path)
                .context("Failed to read persistence file")?;
            serde_json::from_str(&content)
                .context("Failed to parse persistence file")
        } else {
            Ok(TopDb::default())
        }
    }

    /// Save to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize persistence data")?;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        
        fs::write(path, content)
            .context("Failed to write persistence file")?;
        
        Ok(())
    }

    /// Update with new data and return the updated database
    pub fn update(
        &mut self,
        total_ops: &HashMap<String, i64>,
        jobs: &HashMap<String, HashMap<String, i64>>,
        query_time: i64,
    ) -> &Self {
        // Update top_ops
        for (op, &rate) in total_ops {
            let entry = self.top_ops.entry(op.clone()).or_insert_with(|| TopOpsEntry {
                rate: 0,
                timestamp: query_time,
            });
            
            if rate > entry.rate {
                entry.rate = rate;
                entry.timestamp = query_time;
            }
        }

        // Update top_job_per_op
        for short_key in OP_KEYS.keys() {
            let long_key = OP_KEYS.get(short_key).unwrap();
            
            for (job_id, job_data) in jobs {
                if let Some(&rate) = job_data.get(*long_key) {
                    let entry = self.top_job_per_op
                        .entry(long_key.to_string())
                        .or_insert_with(|| TopJobEntry {
                            job_id: String::new(),
                            ops: HashMap::new(),
                            timestamp: query_time,
                        });
                    
                    let current_rate = entry.ops.get(*long_key).copied().unwrap_or(0);
                    
                    if rate > current_rate {
                        entry.job_id = job_id.clone();
                        entry.ops = job_data.clone();
                        entry.timestamp = query_time;
                    }
                }
            }
        }

        self
    }
}

