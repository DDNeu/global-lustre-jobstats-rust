//! Shared statistics processing logic for both CLI and TUI modes
//!
//! This module provides the core parsing, aggregation, groupby transformation,
//! rate calculation, and sorting logic that is shared between CLI and TUI.

use std::collections::HashMap;

use crate::op_keys::is_op_key;

/// Configuration for stats processing
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProcessingConfig {
    /// Group by this jobid component (e.g., "user", "group", "none")
    pub groupby: String,
    /// Sort by this operation type (e.g., "ops", "open", "read")
    pub sortby: String,
    /// Calculate rates (ops/sec)
    pub rate: bool,
    /// Calculate differences (delta between samples)
    pub difference: bool,
    /// Minimum rate to include in results
    pub minrate: i64,
    /// Enable histogram processing
    pub enable_hist: bool,
    /// Job ID component positions (e.g., {"user": 0, "group": 1})
    pub jobid_var: HashMap<String, usize>,
    /// Job ID separator character
    pub jobid_separator: char,
    /// Filter set - job IDs to exclude
    pub filter: std::collections::HashSet<String>,
}

impl Default for ProcessingConfig {
    fn default() -> Self {
        Self {
            groupby: "none".to_string(),
            sortby: "ops".to_string(),
            rate: false,
            difference: false,
            minrate: 0,
            enable_hist: false,
            jobid_var: HashMap::new(),
            jobid_separator: '.',
            filter: std::collections::HashSet::new(),
        }
    }
}

/// Timestamp information for a job
#[derive(Debug, Clone, Default)]
pub struct TimestampInfo {
    pub snapshot_time: i64,
    pub start_time: i64,
    pub elapsed_time: i64,
}

/// Parsed metric data
#[derive(Debug, Clone, Default)]
pub struct JobMetric {
    pub samples: i64,
    pub unit: String,
    pub min: i64,
    pub max: i64,
    pub sum: i64,
    pub hist: HashMap<String, i64>,
}

/// Parsed job data
#[derive(Debug, Clone, Default)]
pub struct ParsedJob {
    pub job_id: String,
    pub snapshot_time: i64,
    pub start_time: i64,
    pub elapsed_time: i64,
    pub metrics: HashMap<String, JobMetric>,
}

/// Reference data for rate calculation (stores previous sample)
#[derive(Debug, Clone, Default)]
pub struct ReferenceData {
    pub jobs: HashMap<String, HashMap<String, i64>>,
    pub timestamps: HashMap<String, TimestampInfo>,
    pub query_time: i64,
}

/// Result of rate calculation
#[derive(Debug, Clone)]
pub struct RateResult {
    /// Job rates/differences
    pub job_rates: HashMap<String, HashMap<String, i64>>,
    /// Sampling window per job
    pub sampling_windows: HashMap<String, i64>,
    /// Duration since last query
    pub query_duration: i64,
    /// Whether this is the first sample (no rates yet)
    pub is_first_sample: bool,
}

/// Stats processor that handles parsing, groupby, rate calculation, and sorting
#[derive(Debug, Clone)]
pub struct StatsProcessor {
    pub config: ProcessingConfig,
    pub reference: ReferenceData,
}

impl StatsProcessor {
    /// Create a new StatsProcessor with the given configuration
    pub fn new(config: ProcessingConfig) -> Self {
        Self {
            config,
            reference: ReferenceData::default(),
        }
    }

    /// Apply groupby transformation to a job ID
    pub fn apply_groupby(&self, job_id: &str) -> String {
        if self.config.groupby == "none" {
            return job_id.to_string();
        }

        let clean_id = job_id.trim_matches('"');
        if clean_id.contains(self.config.jobid_separator) {
            let parts: Vec<&str> = clean_id.split(self.config.jobid_separator).collect();
            if let Some(&idx) = self.config.jobid_var.get(&self.config.groupby) {
                if idx < parts.len() {
                    return format!("\"{}\"", parts[idx]);
                }
            }
        }
        job_id.to_string()
    }

    /// Check if a job should be filtered out
    #[allow(dead_code)]
    pub fn should_filter(&self, job_id: &str) -> bool {
        self.config.filter.contains(job_id)
    }

    /// Merge a parsed job into the jobs map with groupby transformation
    pub fn merge_job(
        &self,
        jobs: &mut HashMap<String, HashMap<String, i64>>,
        job: &ParsedJob,
        timestamp_dict: &mut HashMap<String, TimestampInfo>,
    ) {
        let job_id = self.apply_groupby(&job.job_id);

        // Update timestamp info
        let ts_info = timestamp_dict.entry(job_id.clone()).or_default();
        if job.snapshot_time > ts_info.snapshot_time {
            ts_info.snapshot_time = job.snapshot_time;
        }
        if job.start_time > ts_info.start_time {
            ts_info.start_time = job.start_time;
        }
        if job.elapsed_time > ts_info.elapsed_time {
            ts_info.elapsed_time = job.elapsed_time;
        }

        // Merge metrics
        let job_data = jobs.entry(job_id.clone()).or_default();

        for (metric_name, metric) in &job.metrics {
            if metric.samples == 0 {
                continue;
            }

            // Skip histogram metrics if not enabled
            if (metric_name == "read_bytes" || metric_name == "write_bytes")
                && !self.config.enable_hist
            {
                continue;
            }

            // Accumulate samples
            *job_data.entry(metric_name.clone()).or_insert(0) += metric.samples;
            *job_data.entry("ops".to_string()).or_insert(0) += metric.samples;
        }

        job_data.insert("job_id".to_string(), 0); // Marker that this is a valid job
    }

    /// Calculate rate/difference between current and reference data
    pub fn calculate_rates(
        &mut self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        query_time: i64,
        timestamp_dict: &HashMap<String, TimestampInfo>,
    ) -> RateResult {
        // If no reference data, store current as reference
        if self.reference.jobs.is_empty() {
            self.reference.jobs = jobs.clone();
            self.reference.query_time = query_time;
            self.reference.timestamps = timestamp_dict.clone();
            return RateResult {
                job_rates: HashMap::new(),
                sampling_windows: HashMap::new(),
                query_duration: 0,
                is_first_sample: true,
            };
        }

        let mut job_rates: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut sampling_windows: HashMap<String, i64> = HashMap::new();
        let query_duration = query_time - self.reference.query_time;

        for (job_id, ref_data) in &self.reference.jobs {
            let mut rate_data: HashMap<String, i64> = HashMap::new();

            let new_snap = match timestamp_dict.get(job_id) {
                Some(ts) => ts.snapshot_time,
                None => continue,
            };

            let ref_snap = match self.reference.timestamps.get(job_id) {
                Some(ts) => ts.snapshot_time,
                None => continue,
            };

            let duration = new_snap - ref_snap;
            if duration <= 0 {
                continue;
            }

            sampling_windows.insert(job_id.clone(), duration);

            for (metric, &old_val) in ref_data {
                if !is_op_key(metric) && metric != "ops" {
                    continue;
                }

                let new_val = jobs
                    .get(job_id)
                    .and_then(|j| j.get(metric))
                    .copied()
                    .unwrap_or(0);

                let diff = (new_val - old_val).max(0);

                let rate = if self.config.rate {
                    if duration == 0 || diff == 0 {
                        0
                    } else {
                        diff / duration
                    }
                } else if self.config.difference {
                    diff
                } else {
                    0
                };

                rate_data.insert(metric.clone(), rate);
            }

            if !rate_data.is_empty() {
                job_rates.insert(job_id.clone(), rate_data);
            }
        }

        // Update reference
        self.reference.jobs = jobs.clone();
        self.reference.timestamps = timestamp_dict.clone();
        self.reference.query_time = query_time;

        RateResult {
            job_rates,
            sampling_windows,
            query_duration,
            is_first_sample: false,
        }
    }

    /// Calculate total operations across all jobs
    pub fn calculate_totals(
        &self,
        jobs: &HashMap<String, HashMap<String, i64>>,
    ) -> HashMap<String, i64> {
        let mut total: HashMap<String, i64> = HashMap::new();

        for job_data in jobs.values() {
            for (metric, &value) in job_data {
                if is_op_key(metric) || metric == "ops" {
                    *total.entry(metric.clone()).or_insert(0) += value;
                }
            }
        }

        total
    }

    /// Get top N jobs sorted by the configured sortby field
    #[allow(dead_code)]
    pub fn get_top_jobs(
        &self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        count: usize,
    ) -> Vec<(String, HashMap<String, i64>)> {
        let mut top_jobs: Vec<(String, HashMap<String, i64>)> = Vec::new();

        for (job_id, job_data) in jobs {
            // Skip filtered jobs
            if self.should_filter(job_id) {
                continue;
            }

            // Check minimum rate
            let ops = job_data.get("ops").copied().unwrap_or(0);
            if ops <= self.config.minrate {
                continue;
            }

            top_jobs.push((job_id.clone(), job_data.clone()));
        }

        // Sort by the specified metric
        let sortby = &self.config.sortby;
        top_jobs.sort_by(|a, b| {
            let val_a = a.1.get(sortby).copied().unwrap_or(0);
            let val_b = b.1.get(sortby).copied().unwrap_or(0);
            val_b.cmp(&val_a)
        });

        // Take top N
        top_jobs.into_iter().take(count).collect()
    }

    /// Reset reference data (useful when starting fresh)
    #[allow(dead_code)]
    pub fn reset_reference(&mut self) {
        self.reference = ReferenceData::default();
    }

    /// Check if we have reference data for rate calculation
    #[allow(dead_code)]
    pub fn has_reference(&self) -> bool {
        !self.reference.jobs.is_empty()
    }

    /// Parse job stats data into ParsedJob structures
    pub fn parse_job_stats(&self, data: &str) -> Vec<ParsedJob> {
        let mut jobs = Vec::new();
        let mut current_job: Option<ParsedJob> = None;
        let lines = data.lines();

        for line in lines {
            let line = line.trim();

            if line == "job_stats:" {
                continue;
            }

            if line.starts_with("- job_id:") {
                // Save previous job if exists
                if let Some(job) = current_job.take() {
                    jobs.push(job);
                }

                // Start new job
                let mut job = ParsedJob::default();
                if let Some(id) = line.strip_prefix("- job_id:") {
                    job.job_id = id.trim().to_string();
                }
                current_job = Some(job);
                continue;
            }

            if let Some(ref mut job) = current_job {
                // Parse timestamp lines
                if line.contains("snapshot_time:")
                    || line.contains("start_time:")
                    || line.contains("elapsed_time:")
                {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let key = parts[0].trim_end_matches(':');
                        // Remove trailing .nsecs from snapshot_time (Lustre 2.15+)
                        let value_str = parts[1].split('.').next().unwrap_or(parts[1]);
                        if let Ok(value) = value_str.parse::<i64>() {
                            match key {
                                "snapshot_time" => job.snapshot_time = value,
                                "start_time" => job.start_time = value,
                                "elapsed_time" => job.elapsed_time = value,
                                _ => {}
                            }
                        }
                    }
                    continue;
                }

                // Parse metric lines (format: metric: {samples:N, unit:X, ...})
                if line.contains('{') && line.contains('}') {
                    Self::parse_metric_line(line, job);
                }
            }
        }

        // Save last job
        if let Some(job) = current_job {
            jobs.push(job);
        }

        jobs
    }

    /// Parse a metric line into a ParsedJob
    fn parse_metric_line(line: &str, job: &mut ParsedJob) {
        let clean = line.replace(' ', "");

        if let Some(idx) = clean.find('{') {
            let metric = clean[..idx].trim_end_matches(':');
            let values_part = &clean[idx + 1..];
            let values_part = values_part.trim_end_matches('}');

            let mut metric_data = JobMetric::default();

            // Check for histogram
            if values_part.contains("hist:") {
                // Parse histogram separately
                if let Some(hist_idx) = values_part.find("hist:{") {
                    let before_hist = &values_part[..hist_idx];
                    let hist_part = &values_part[hist_idx + 6..];

                    // Parse regular values
                    for item in before_hist.split(',') {
                        if let Some((key, val)) = item.split_once(':') {
                            Self::parse_metric_value(key, val, &mut metric_data);
                        }
                    }

                    // Parse histogram
                    let hist_end = hist_part.find('}').unwrap_or(hist_part.len());
                    let hist_values = &hist_part[..hist_end];
                    for item in hist_values.split(',') {
                        if let Some((bucket, count)) = item.split_once(':') {
                            if let Ok(c) = count.parse::<i64>() {
                                metric_data.hist.insert(bucket.to_string(), c);
                            }
                        }
                    }
                }
            } else {
                // Parse simple values
                for item in values_part.split(',') {
                    if let Some((key, val)) = item.split_once(':') {
                        Self::parse_metric_value(key, val, &mut metric_data);
                    }
                }
            }

            job.metrics.insert(metric.to_string(), metric_data);
        }
    }

    fn parse_metric_value(key: &str, val: &str, metric: &mut JobMetric) {
        match key {
            "samples" => metric.samples = val.parse().unwrap_or(0),
            "unit" => metric.unit = val.to_string(),
            "min" => metric.min = val.parse().unwrap_or(0),
            "max" => metric.max = val.parse().unwrap_or(0),
            "sum" => metric.sum = val.parse().unwrap_or(0),
            _ => {}
        }
    }

    /// Full processing pipeline: parse data, merge with groupby, optionally calculate rates
    /// Returns (jobs, timestamp_dict) or (rate_jobs, timestamp_dict) if rate mode
    #[allow(dead_code)]
    pub fn process_stats_data(
        &mut self,
        data: &str,
        query_time: i64,
    ) -> (HashMap<String, HashMap<String, i64>>, HashMap<String, TimestampInfo>, Option<RateResult>) {
        let mut jobs: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut timestamp_dict: HashMap<String, TimestampInfo> = HashMap::new();

        // Parse and merge jobs
        let parsed_jobs = self.parse_job_stats(data);
        for parsed_job in &parsed_jobs {
            self.merge_job(&mut jobs, parsed_job, &mut timestamp_dict);
        }

        // Calculate rates if configured
        let rate_result = if self.config.rate || self.config.difference {
            Some(self.calculate_rates(&jobs, query_time, &timestamp_dict))
        } else {
            None
        };

        (jobs, timestamp_dict, rate_result)
    }
}

