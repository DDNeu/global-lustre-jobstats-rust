//! Job statistics parsing and aggregation

use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;

use crate::args::Args;
use crate::config::Config;
use crate::logging::LoggingCoordinator;
use crate::op_keys::{is_op_key, JOBID_NAME_KEYS, OP_KEYS_REV};
use crate::output::{print_top_jobs, print_total_ops, print_total_ops_logged, JobOutput};
use crate::persistence::TopDb;
use crate::ssh;
use crate::stats_processor::{ParsedJob, ProcessingConfig, StatsProcessor, TimestampInfo};

/// OST/MDT counts
#[derive(Debug, Clone, Default)]
pub struct OstMdtCounts {
    pub obdfilter: usize,
    pub mdt: usize,
}

/// Raw stats data with metadata for logging
#[derive(Debug, Clone)]
pub struct RawStatsData {
    pub host: String,
    pub param: String,
    pub data: String,
}

/// Main parser struct
pub struct JobStatsParser {
    pub args: Args,
    pub config: Config,
    pub hosts_param: HashMap<String, Vec<String>>,
    pub osts_mdts: OstMdtCounts,
    pub jobid_var: HashMap<String, usize>,
    pub jobid_separator: char,
    pub enable_hist: bool,
    pub logging_coordinator: Option<LoggingCoordinator>,
    /// Shared stats processor for parsing, groupby, rate calculation
    pub stats_processor: StatsProcessor,
}

impl JobStatsParser {
    pub fn new(args: Args, config: Config) -> Self {
        // Create processing config from args
        let processing_config = ProcessingConfig {
            groupby: args.groupby.clone(),
            sortby: args.sortby.clone(),
            rate: args.rate,
            difference: args.difference,
            minrate: args.minrate,
            enable_hist: args.hist,
            jobid_var: HashMap::new(), // Will be populated after parse_jobid_name
            jobid_separator: '.',       // Will be updated after parse_jobid_name
            filter: config.filter.clone(),
        };

        JobStatsParser {
            args,
            config,
            hosts_param: HashMap::new(),
            osts_mdts: OstMdtCounts::default(),
            jobid_var: HashMap::new(),
            jobid_separator: '.',
            enable_hist: false,
            logging_coordinator: None,
            stats_processor: StatsProcessor::new(processing_config),
        }
    }

    /// Update the stats processor with jobid configuration after parsing
    fn update_processor_config(&mut self) {
        self.stats_processor.config.jobid_var = self.jobid_var.clone();
        self.stats_processor.config.jobid_separator = self.jobid_separator;
        self.stats_processor.config.enable_hist = self.enable_hist;
    }

    /// Create a ProcessingConfig from current parser state (for TUI use)
    pub fn create_processing_config(&self) -> ProcessingConfig {
        ProcessingConfig {
            groupby: self.args.groupby.clone(),
            sortby: self.args.sortby.clone(),
            rate: self.args.rate,
            difference: self.args.difference,
            minrate: self.args.minrate,
            enable_hist: self.enable_hist,
            jobid_var: self.jobid_var.clone(),
            jobid_separator: self.jobid_separator,
            filter: self.config.filter.clone(),
        }
    }

    /// Initialize logging if any logging options are enabled
    pub async fn init_logging(&mut self) -> Result<()> {
        // Check if any logging is requested
        if self.args.log_raw_data.is_some()
            || self.args.log_data_victoriametrics.is_some()
            || self.args.log_data_prometheus.is_some()
            || self.args.log_data_parquet.is_some()
        {
            let coordinator = LoggingCoordinator::from_args(&self.args).await?;
            self.logging_coordinator = Some(coordinator);
        }

        // Validate --log-only requires --log-raw-data
        if self.args.log_only && self.args.log_raw_data.is_none() {
            anyhow::bail!("--log-only requires --log-raw-data to be specified");
        }

        Ok(())
    }

    /// Get current Unix timestamp
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Main run loop
    pub async fn run(&mut self) -> Result<()> {
        self.enable_hist = self.args.hist;

        if self.config.servers.is_empty() {
            anyhow::bail!("No servers configured");
        }

        // Initialize logging if requested
        self.init_logging().await?;

        // Get params from all servers
        self.hosts_param = self.get_params().await?;
        self.calculate_ost_mdt_counts();

        // Parse jobid_name pattern (skip in log-only mode)
        if !self.args.log_only {
            self.parse_jobid_name().await?;

            // Validate sortby
            if !OP_KEYS_REV.contains_key(self.args.sortby.as_str()) {
                anyhow::bail!(
                    "sortby argument key '{}' is not in ops key list: {:?}",
                    self.args.sortby,
                    OP_KEYS_REV.keys().collect::<Vec<_>>()
                );
            }
        }

        let total_start = Instant::now();
        let mut iteration = 0i64;

        loop {
            if let Err(e) = self.run_once().await {
                eprintln!("Error in iteration: {}", e);
            }

            iteration += 1;
            if self.args.repeats != -1 && iteration >= self.args.repeats {
                break;
            }

            tokio::time::sleep(Duration::from_secs(self.args.interval)).await;
        }

        // Close logging coordinator
        if let Some(coordinator) = self.logging_coordinator.take() {
            coordinator.close().await?;
        }

        if self.args.verbose {
            println!("Total runtime    : {:?}", total_start.elapsed());
        }

        Ok(())
    }

    /// Calculate OST and MDT counts from hosts_param
    fn calculate_ost_mdt_counts(&mut self) {
        let mut obdfilter = 0;
        let mut mdt = 0;

        for params in self.hosts_param.values() {
            for param in params {
                if let Some(prefix) = param.split('.').next() {
                    match prefix {
                        "obdfilter" => obdfilter += 1,
                        "mdt" => mdt += 1,
                        _ => {}
                    }
                }
            }
        }

        self.osts_mdts = OstMdtCounts { obdfilter, mdt };
    }

    /// Get parameters from all servers in parallel
    async fn get_params(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut join_set = JoinSet::new();
        let config = Arc::new(self.config.clone());
        let param_pattern = self.args.param.clone();

        for host in &self.config.servers {
            let host = host.clone();
            let config = Arc::clone(&config);
            let param = param_pattern.clone();

            join_set.spawn(async move {
                ssh::get_params(&host, &param, &config)
            });
        }

        let mut results = HashMap::new();
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(param_result)) => {
                    results.insert(param_result.host, param_result.params);
                }
                Ok(Err(e)) => {
                    if self.args.verbose {
                        eprintln!("Error getting params: {}", e);
                    }
                }
                Err(e) => {
                    if self.args.verbose {
                        eprintln!("Task error: {}", e);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Parse jobid_name pattern from a server
    async fn parse_jobid_name(&mut self) -> Result<()> {
        if self.args.groupby == "none" {
            return Ok(());
        }

        let host = self.config.servers.iter().next()
            .context("No servers available")?;

        let jobid_name = ssh::get_jobid_name(host, &self.config)?;

        // Find positions of each key pattern
        let mut positions: Vec<(&str, usize)> = Vec::new();
        for (pattern, name) in JOBID_NAME_KEYS.iter() {
            if let Some(pos) = jobid_name.find(pattern) {
                positions.push((*name, pos));
            }
        }

        // Sort by position
        positions.sort_by_key(|&(_, pos)| pos);

        // Build jobid_var mapping
        for (i, (name, _)) in positions.iter().enumerate() {
            self.jobid_var.insert(name.to_string(), i);
        }

        // Find separator (first non-pattern character)
        let mut stripped = jobid_name.clone();
        for (pattern, _) in JOBID_NAME_KEYS.iter() {
            stripped = stripped.replace(pattern, "");
        }

        if let Some(sep) = stripped.chars().next() {
            self.jobid_separator = sep;
        }

        // Update the stats processor config
        self.update_processor_config();

        // Validate groupby key
        if !self.jobid_var.contains_key(&self.args.groupby) {
            anyhow::bail!(
                "groupby key '{}' has not been found in jobid_name pattern\n\
                current jobid_name: {}\n\
                available values: {:?}",
                self.args.groupby,
                jobid_name,
                JOBID_NAME_KEYS.values().collect::<Vec<_>>()
            );
        }

        Ok(())
    }

    /// Get stats data from all servers in parallel (returns data with metadata for logging)
    async fn get_stats_data_with_metadata(&self) -> Result<Vec<RawStatsData>> {
        let mut join_set = JoinSet::new();
        let config = Arc::new(self.config.clone());

        for (host, params) in &self.hosts_param {
            for param in params {
                let host = host.clone();
                let param = param.clone();
                let config = Arc::clone(&config);

                join_set.spawn(async move {
                    let result = ssh::get_stats(&host, &param, &config);
                    (host, param, result)
                });
            }
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((host, param, Ok(stats_result))) => {
                    results.push(RawStatsData {
                        host,
                        param,
                        data: stats_result.data,
                    });
                }
                Ok((_, _, Err(e))) => {
                    if self.args.verbose {
                        eprintln!("Error getting stats: {}", e);
                    }
                }
                Err(e) => {
                    if self.args.verbose {
                        eprintln!("Task error: {}", e);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Parse single job stats data (delegates to StatsProcessor)
    fn parse_job_stats(&self, data: &str) -> Vec<ParsedJob> {
        self.stats_processor.parse_job_stats(data)
    }

    /// Merge a parsed job into the jobs map (delegates to StatsProcessor)
    fn merge_job(
        &self,
        jobs: &mut HashMap<String, HashMap<String, i64>>,
        job: &ParsedJob,
        timestamp_dict: &mut HashMap<String, TimestampInfo>,
    ) {
        self.stats_processor.merge_job(jobs, job, timestamp_dict);
    }

    /// Calculate rate between two queries (delegates to StatsProcessor)
    fn rate_calc(
        &mut self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        query_time: i64,
        timestamp_dict: &HashMap<String, TimestampInfo>,
    ) -> (HashMap<String, HashMap<String, i64>>, HashMap<String, i64>, i64) {
        let result = self
            .stats_processor
            .calculate_rates(jobs, query_time, timestamp_dict);
        (
            result.job_rates,
            result.sampling_windows,
            result.query_duration,
        )
    }

    /// Calculate total operations across all jobs (delegates to StatsProcessor)
    fn total_calc(&self, jobs: &HashMap<String, HashMap<String, i64>>) -> HashMap<String, i64> {
        self.stats_processor.calculate_totals(jobs)
    }

    /// Calculate percentage of each job's ops relative to total
    fn pct_calc(
        &self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        total_ops: &HashMap<String, i64>,
    ) -> HashMap<String, HashMap<String, i64>> {
        let mut result: HashMap<String, HashMap<String, i64>> = HashMap::new();

        for (job_id, job_data) in jobs {
            let mut pct_data: HashMap<String, i64> = HashMap::new();

            for (metric, &value) in job_data {
                if is_op_key(metric) || metric == "ops" {
                    let total = total_ops.get(metric).copied().unwrap_or(0);
                    let pct = if total == 0 { 0 } else { value * 100 / total };
                    pct_data.insert(metric.clone(), pct);
                } else {
                    pct_data.insert(metric.clone(), value);
                }
            }

            result.insert(job_id.clone(), pct_data);
        }

        result
    }

    /// Pick top N jobs sorted by the specified metric
    fn pick_top_jobs(
        &self,
        jobs: &HashMap<String, HashMap<String, i64>>,
        count: usize,
    ) -> Vec<JobOutput> {
        let mut top_jobs: Vec<(String, HashMap<String, i64>)> = Vec::new();

        for (job_id, job_data) in jobs {
            // Check if job has any non-zero values
            let has_values = job_data.iter()
                .any(|(k, &v)| v != 0 && k != "job_id");

            if !has_values {
                continue;
            }

            // Apply filter
            let matches_filter = self.config.filter.iter()
                .any(|f| job_id.contains(f));

            let should_include = if self.args.fmod {
                matches_filter
            } else {
                !matches_filter || self.config.filter.is_empty()
            };

            if !should_include {
                continue;
            }

            // Check minimum rate
            let ops = job_data.get("ops").copied().unwrap_or(0);
            if ops <= self.args.minrate {
                continue;
            }

            top_jobs.push((job_id.clone(), job_data.clone()));
        }

        // Sort by the specified metric
        top_jobs.sort_by(|a, b| {
            let val_a = a.1.get(&self.args.sortby).copied().unwrap_or(0);
            let val_b = b.1.get(&self.args.sortby).copied().unwrap_or(0);
            val_b.cmp(&val_a)
        });

        // Take top N and convert to JobOutput
        top_jobs.into_iter()
            .take(count)
            .map(|(job_id, ops)| JobOutput {
                job_id,
                ops,
                sampling_window: None,
            })
            .collect()
    }

    /// Run one iteration of stats collection
    async fn run_once(&mut self) -> Result<()> {
        let query_time = Self::now();
        let ssh_start = Instant::now();

        // Get stats from all servers with metadata for logging
        let stats_data = self.get_stats_data_with_metadata().await?;

        if self.args.verbose {
            println!("SSH time         : {:?}", ssh_start.elapsed());
        }

        // Log raw data before parsing (if enabled)
        if let Some(ref mut coordinator) = self.logging_coordinator {
            let timestamp = Utc::now();
            for raw_data in &stats_data {
                if let Err(e) = coordinator
                    .log_raw(&raw_data.host, &raw_data.param, &raw_data.data, timestamp)
                    .await
                {
                    if self.args.verbose {
                        eprintln!("Warning: Failed to log raw data: {}", e);
                    }
                }
            }
        }

        // If log-only mode, stop here without parsing or analysis
        if self.args.log_only {
            if self.args.verbose {
                println!("Raw data logged (--log-only mode, skipping analysis)");
            }
            return Ok(());
        }

        let parser_start = Instant::now();

        // Parse all stats data
        let mut jobs: HashMap<String, HashMap<String, i64>> = HashMap::new();
        let mut timestamp_dict: HashMap<String, TimestampInfo> = HashMap::new();

        for raw_data in &stats_data {
            let parsed_jobs = self.parse_job_stats(&raw_data.data);
            for job in parsed_jobs {
                self.merge_job(&mut jobs, &job, &mut timestamp_dict);
            }
        }

        if self.args.verbose {
            println!("Parser time      : {:?}", parser_start.elapsed());
            println!("Loop time        : {:?}", ssh_start.elapsed());
        }

        // Log parsed data to enabled formats (VictoriaMetrics, Prometheus, Parquet)
        if let Some(ref mut coordinator) = self.logging_coordinator {
            if let Err(e) = coordinator.log_parsed(&jobs, query_time).await {
                if self.args.verbose {
                    eprintln!("Warning: Failed to log parsed data: {}", e);
                }
            }
        }

        let total_jobs = jobs.len();

        // Handle rate/difference mode
        if self.args.rate || self.args.difference {
            let (job_rates, job_sampling_window, query_duration) =
                self.rate_calc(&jobs, query_time, &timestamp_dict);

            // First iteration just stores reference
            if job_rates.is_empty() && query_duration == 0 {
                return Ok(());
            }

            let mut display_jobs = job_rates.clone();
            let mut total_ops = HashMap::new();

            if self.args.total || self.args.percent || self.args.totalrate {
                total_ops = self.total_calc(&display_jobs);
            }

            let mut top_ops_ever: Option<TopDb> = None;
            if self.args.totalrate && self.args.total {
                let mut top_db = TopDb::load_or_create(&self.config.totalratefile)?;
                top_db.update(&total_ops, &display_jobs, query_time);
                top_db.save(&self.config.totalratefile)?;
                top_ops_ever = Some(top_db);
            }

            if self.args.percent {
                display_jobs = self.pct_calc(&display_jobs, &total_ops);
            }

            let mut top_jobs = self.pick_top_jobs(&display_jobs, self.args.count);

            // Add sampling windows
            for job in &mut top_jobs {
                if let Some(&sw) = job_sampling_window.get(&job.job_id) {
                    job.sampling_window = Some(sw);
                }
            }

            print_top_jobs(
                &top_jobs,
                total_jobs,
                self.args.count,
                query_time,
                query_duration,
                self.config.servers.len(),
                self.osts_mdts.obdfilter,
                self.osts_mdts.mdt,
                &self.args,
                self.config.jobid_length,
            );

            if self.args.total {
                print_total_ops(&total_ops, &self.args);
            }

            if let Some(ref top_db) = top_ops_ever {
                print_total_ops_logged(top_db, &self.args);
            }
        } else {
            // Simple mode (no rate/difference)
            let mut display_jobs = jobs.clone();
            let mut total_ops = HashMap::new();

            if self.args.total || self.args.percent {
                total_ops = self.total_calc(&display_jobs);
            }

            if self.args.percent {
                display_jobs = self.pct_calc(&display_jobs, &total_ops);
            }

            let top_jobs = self.pick_top_jobs(&display_jobs, self.args.count);

            print_top_jobs(
                &top_jobs,
                total_jobs,
                self.args.count,
                query_time,
                0,
                self.config.servers.len(),
                self.osts_mdts.obdfilter,
                self.osts_mdts.mdt,
                &self.args,
                self.config.jobid_length,
            );

            if self.args.total {
                print_total_ops(&total_ops, &self.args);
            }
        }

        Ok(())
    }

    /// Public wrapper for get_params (for TUI initialization)
    pub async fn get_params_public(&self) -> Result<HashMap<String, Vec<String>>> {
        self.get_params().await
    }

    /// Public wrapper for parse_jobid_name (for TUI initialization)
    pub async fn parse_jobid_name_public(&mut self) -> Result<()> {
        // For TUI, we don't require groupby validation
        if self.config.servers.is_empty() {
            return Ok(());
        }

        let host = self.config.servers.iter().next()
            .context("No servers available")?;

        let jobid_name = ssh::get_jobid_name(host, &self.config)?;

        // Find positions of each key pattern
        let mut positions: Vec<(&str, usize)> = Vec::new();
        for (pattern, name) in JOBID_NAME_KEYS.iter() {
            if let Some(pos) = jobid_name.find(pattern) {
                positions.push((*name, pos));
            }
        }

        // Sort by position
        positions.sort_by_key(|&(_, pos)| pos);

        // Build jobid_var mapping
        for (i, (name, _)) in positions.iter().enumerate() {
            self.jobid_var.insert(name.to_string(), i);
        }

        // Find separator (first non-pattern character)
        let mut stripped = jobid_name.clone();
        for (pattern, _) in JOBID_NAME_KEYS.iter() {
            stripped = stripped.replace(pattern, "");
        }

        if let Some(sep) = stripped.chars().next() {
            self.jobid_separator = sep;
        }

        // Update the stats processor config
        self.update_processor_config();

        Ok(())
    }
}

