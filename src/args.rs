//! Command-line argument parsing for glljobstat

use anyhow::Result;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use std::path::PathBuf;

use crate::config::ProfileConfig;

/// List top jobs from Lustre job_stats across multiple servers
#[derive(Parser, Debug, Clone)]
#[command(name = "glljobstat")]
#[command(author = "Bjoern Olausson, Maxence Joulin")]
#[command(version = "1.0")]
#[command(about = "Read job_stats files, parse and aggregate data of every job on multiple OSS/MDS via SSH")]
pub struct Args {
    /// Full path to config file
    #[arg(short = 'C', long, default_value = "~/.glljobstat.toml")]
    pub configfile: String,

    /// Use a named profile from the config file.
    /// Profile settings override defaults; CLI arguments override profile settings.
    #[arg(short = 'P', long)]
    pub profile: Option<String>,

    /// The number of top jobs to be listed
    #[arg(short = 'c', long, default_value_t = 5)]
    pub count: usize,

    /// The interval in seconds to check job stats again
    #[arg(short = 'i', long, default_value_t = 10)]
    pub interval: u64,

    /// The times to repeat the parsing (-1 for unlimited)
    #[arg(short = 'n', long, default_value_t = -1)]
    pub repeats: i64,

    /// The param path to be checked
    #[arg(long, default_value = "*.*.job_stats")]
    pub param: String,

    /// Sort by user/group/host/host_short/job/proc according to jobid_name Lustre pattern
    #[arg(long, default_value = "none")]
    pub groupby: String,

    /// Sort top_jobs by operation type (ops, open, close, rename...)
    #[arg(long, default_value = "ops")]
    pub sortby: String,

    /// Check only OST job stats
    #[arg(short = 'o', long)]
    pub ost: bool,

    /// Check only MDT job stats
    #[arg(short = 'm', long)]
    pub mdt: bool,

    /// Comma separated list of OSS/MDS to query
    #[arg(short = 's', long)]
    pub servers: Option<String>,

    /// Show full operation name
    #[arg(long)]
    pub fullname: bool,

    /// Comma separated list of job_ids to ignore
    #[arg(short = 'f', long)]
    pub filter: Option<String>,

    /// Modify the filter to only show job_ids that match the filter
    #[arg(short = 'F', long)]
    pub fmod: bool,

    /// Set job_id filename length for pretty printing
    #[arg(short = 'l', long)]
    pub length: Option<usize>,

    /// Show sum over all jobs for each operation
    #[arg(short = 't', long)]
    pub total: bool,

    /// Keep track of the highest rate ever in a persistent file
    #[arg(short = 'T', long)]
    pub totalrate: bool,

    /// The minimal ops rate number a job needs to be shown in top jobs
    #[arg(long, default_value_t = 1)]
    pub minrate: i64,

    /// Path to a file which will keep track of the highest rate
    #[arg(long)]
    pub totalratefile: Option<String>,

    /// Show top jobs in percentage to total ops
    #[arg(short = 'p', long)]
    pub percent: bool,

    /// Show human readable time instead of timestamp
    #[arg(short = 'H', long)]
    pub humantime: bool,

    /// Number of parallel SSH connections
    #[arg(long, default_value_t = num_cpus())]
    pub num_proc_ssh: usize,

    /// Number of parallel data parsing tasks
    #[arg(long, default_value_t = num_cpus())]
    pub num_proc_data: usize,

    /// Explicitly enable read_bytes & write_bytes histogram
    #[arg(long)]
    pub hist: bool,

    /// Show some debug and timing information
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Show change in counters between two queries
    #[arg(short = 'd', long, conflicts_with = "rate")]
    pub difference: bool,

    /// Calculate the rate between two queries
    #[arg(short = 'r', long, conflicts_with = "difference")]
    pub rate: bool,

    // ===== Logging Options =====

    /// Log raw SSH output to file before parsing (path can be file, directory, or empty for cwd)
    #[arg(long, value_name = "PATH")]
    pub log_raw_data: Option<PathBuf>,

    /// Export to VictoriaMetrics JSON format compatible with /api/v1/import
    #[arg(long, value_name = "PATH")]
    pub log_data_victoriametrics: Option<PathBuf>,

    /// Export to Prometheus exposition format
    #[arg(long, value_name = "PATH")]
    pub log_data_prometheus: Option<PathBuf>,

    /// Export to Apache Parquet format for columnar storage
    #[arg(long, value_name = "PATH")]
    pub log_data_parquet: Option<PathBuf>,

    /// Only collect raw data, skip analysis and console output (requires --log-raw-data)
    #[arg(long)]
    pub log_only: bool,

    /// Maximum log file size before rotation (e.g., 100M, 1G, 10T)
    #[arg(long, value_name = "SIZE")]
    pub log_max_size: Option<String>,

    // ===== TUI Options =====

    /// Launch interactive TUI mode with time-series graph.
    /// Optionally specify a path to replay historical data from log files.
    /// Supports: raw logs (.raw.log), Parquet (.parquet), Prometheus (.prom), VictoriaMetrics (.vm.json)
    #[arg(long, value_name = "REPLAY_PATH", num_args = 0..=1, default_missing_value = "")]
    pub tui: Option<String>,

    /// List available profiles from the config file and exit
    #[arg(long)]
    pub list_profiles: bool,

    /// Generate an example config file with documentation and exit.
    /// Creates the file at the path specified by --configfile (default: ~/.glljobstat.toml)
    #[arg(long)]
    pub init: bool,
}

/// Track which arguments were explicitly provided on the command line
#[derive(Debug, Clone, Default)]
pub struct ArgsProvided {
    pub count: bool,
    pub interval: bool,
    pub repeats: bool,
    pub param: bool,
    pub groupby: bool,
    pub sortby: bool,
    pub ost: bool,
    pub mdt: bool,
    pub servers: bool,
    pub fullname: bool,
    pub filter: bool,
    pub fmod: bool,
    pub length: bool,
    pub total: bool,
    pub totalrate: bool,
    pub minrate: bool,
    pub totalratefile: bool,
    pub percent: bool,
    pub humantime: bool,
    pub num_proc_ssh: bool,
    pub num_proc_data: bool,
    pub hist: bool,
    pub verbose: bool,
    pub difference: bool,
    pub rate: bool,
    pub log_raw_data: bool,
    pub log_data_victoriametrics: bool,
    pub log_data_prometheus: bool,
    pub log_data_parquet: bool,
    pub log_only: bool,
    pub log_max_size: bool,
    pub tui: bool,
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

impl ArgsProvided {
    /// Build from clap ArgMatches to detect which args were explicitly provided
    fn from_matches(matches: &ArgMatches) -> Self {
        Self {
            count: matches.value_source("count") == Some(clap::parser::ValueSource::CommandLine),
            interval: matches.value_source("interval")
                == Some(clap::parser::ValueSource::CommandLine),
            repeats: matches.value_source("repeats")
                == Some(clap::parser::ValueSource::CommandLine),
            param: matches.value_source("param") == Some(clap::parser::ValueSource::CommandLine),
            groupby: matches.value_source("groupby")
                == Some(clap::parser::ValueSource::CommandLine),
            sortby: matches.value_source("sortby") == Some(clap::parser::ValueSource::CommandLine),
            ost: matches.value_source("ost") == Some(clap::parser::ValueSource::CommandLine),
            mdt: matches.value_source("mdt") == Some(clap::parser::ValueSource::CommandLine),
            servers: matches.value_source("servers")
                == Some(clap::parser::ValueSource::CommandLine),
            fullname: matches.value_source("fullname")
                == Some(clap::parser::ValueSource::CommandLine),
            filter: matches.value_source("filter") == Some(clap::parser::ValueSource::CommandLine),
            fmod: matches.value_source("fmod") == Some(clap::parser::ValueSource::CommandLine),
            length: matches.value_source("length") == Some(clap::parser::ValueSource::CommandLine),
            total: matches.value_source("total") == Some(clap::parser::ValueSource::CommandLine),
            totalrate: matches.value_source("totalrate")
                == Some(clap::parser::ValueSource::CommandLine),
            minrate: matches.value_source("minrate")
                == Some(clap::parser::ValueSource::CommandLine),
            totalratefile: matches.value_source("totalratefile")
                == Some(clap::parser::ValueSource::CommandLine),
            percent: matches.value_source("percent")
                == Some(clap::parser::ValueSource::CommandLine),
            humantime: matches.value_source("humantime")
                == Some(clap::parser::ValueSource::CommandLine),
            num_proc_ssh: matches.value_source("num_proc_ssh")
                == Some(clap::parser::ValueSource::CommandLine),
            num_proc_data: matches.value_source("num_proc_data")
                == Some(clap::parser::ValueSource::CommandLine),
            hist: matches.value_source("hist") == Some(clap::parser::ValueSource::CommandLine),
            verbose: matches.value_source("verbose")
                == Some(clap::parser::ValueSource::CommandLine),
            difference: matches.value_source("difference")
                == Some(clap::parser::ValueSource::CommandLine),
            rate: matches.value_source("rate") == Some(clap::parser::ValueSource::CommandLine),
            log_raw_data: matches.value_source("log_raw_data")
                == Some(clap::parser::ValueSource::CommandLine),
            log_data_victoriametrics: matches.value_source("log_data_victoriametrics")
                == Some(clap::parser::ValueSource::CommandLine),
            log_data_prometheus: matches.value_source("log_data_prometheus")
                == Some(clap::parser::ValueSource::CommandLine),
            log_data_parquet: matches.value_source("log_data_parquet")
                == Some(clap::parser::ValueSource::CommandLine),
            log_only: matches.value_source("log_only")
                == Some(clap::parser::ValueSource::CommandLine),
            log_max_size: matches.value_source("log_max_size")
                == Some(clap::parser::ValueSource::CommandLine),
            tui: matches.value_source("tui") == Some(clap::parser::ValueSource::CommandLine),
        }
    }
}

impl Args {
    /// Parse args and return both Args and which args were explicitly provided
    pub fn parse_with_provided() -> Result<(Self, ArgsProvided)> {
        let matches = Args::command().get_matches();
        let provided = ArgsProvided::from_matches(&matches);
        let args = Args::from_arg_matches(&matches)?;
        Ok((args, provided))
    }

    #[allow(dead_code)]
    pub fn parse_args() -> Result<Self> {
        let mut args = Args::parse();

        // Handle OST/MDT shortcuts
        if args.ost {
            args.param = "obdfilter.*.job_stats".to_string();
        } else if args.mdt {
            args.param = "mdt.*.job_stats".to_string();
        }

        // If totalrate is set, enable rate and total
        if args.totalrate {
            args.rate = true;
            args.total = true;
            args.difference = false;
            args.percent = false;
        }

        // If rate or difference and repeats is 1, set to 2
        if (args.rate || args.difference) && args.repeats > 0 && args.repeats < 2 {
            args.repeats = 2;
        }

        // If percent is set, enable total
        if args.percent {
            args.total = true;
        }

        Ok(args)
    }

    /// Apply profile settings to Args, only for values not explicitly provided on CLI
    pub fn apply_profile(&mut self, profile: &ProfileConfig, provided: &ArgsProvided) {
        // Data collection settings
        if !provided.count {
            if let Some(v) = profile.count {
                self.count = v;
            }
        }
        if !provided.interval {
            if let Some(v) = profile.interval {
                self.interval = v;
            }
        }
        if !provided.repeats {
            if let Some(v) = profile.repeats {
                self.repeats = v;
            }
        }
        if !provided.param {
            if let Some(ref v) = profile.param {
                self.param = v.clone();
            }
        }
        if !provided.groupby {
            if let Some(ref v) = profile.groupby {
                self.groupby = v.clone();
            }
        }
        if !provided.sortby {
            if let Some(ref v) = profile.sortby {
                self.sortby = v.clone();
            }
        }
        if !provided.ost {
            if let Some(v) = profile.ost {
                self.ost = v;
            }
        }
        if !provided.mdt {
            if let Some(v) = profile.mdt {
                self.mdt = v;
            }
        }
        if !provided.servers {
            if let Some(ref v) = profile.servers {
                self.servers = Some(v.clone());
            }
        }

        // Display settings
        if !provided.fullname {
            if let Some(v) = profile.fullname {
                self.fullname = v;
            }
        }
        if !provided.length {
            if let Some(v) = profile.length {
                self.length = Some(v);
            }
        }
        if !provided.total {
            if let Some(v) = profile.total {
                self.total = v;
            }
        }
        if !provided.totalrate {
            if let Some(v) = profile.totalrate {
                self.totalrate = v;
            }
        }
        if !provided.minrate {
            if let Some(v) = profile.minrate {
                self.minrate = v;
            }
        }
        if !provided.totalratefile {
            if let Some(ref v) = profile.totalratefile {
                self.totalratefile = Some(v.clone());
            }
        }
        if !provided.percent {
            if let Some(v) = profile.percent {
                self.percent = v;
            }
        }
        if !provided.humantime {
            if let Some(v) = profile.humantime {
                self.humantime = v;
            }
        }

        // Processing settings
        if !provided.num_proc_ssh {
            if let Some(v) = profile.num_proc_ssh {
                self.num_proc_ssh = v;
            }
        }
        if !provided.num_proc_data {
            if let Some(v) = profile.num_proc_data {
                self.num_proc_data = v;
            }
        }
        if !provided.hist {
            if let Some(v) = profile.hist {
                self.hist = v;
            }
        }
        if !provided.verbose {
            if let Some(v) = profile.verbose {
                self.verbose = v;
            }
        }
        if !provided.difference {
            if let Some(v) = profile.difference {
                self.difference = v;
            }
        }
        if !provided.rate {
            if let Some(v) = profile.rate {
                self.rate = v;
            }
        }

        // Filter settings
        if !provided.filter {
            if let Some(ref v) = profile.filter {
                self.filter = Some(v.clone());
            }
        }
        if !provided.fmod {
            if let Some(v) = profile.fmod {
                self.fmod = v;
            }
        }

        // Logging settings
        if !provided.log_raw_data {
            if let Some(ref v) = profile.log_raw_data {
                self.log_raw_data = Some(PathBuf::from(v));
            }
        }
        if !provided.log_data_victoriametrics {
            if let Some(ref v) = profile.log_data_victoriametrics {
                self.log_data_victoriametrics = Some(PathBuf::from(v));
            }
        }
        if !provided.log_data_prometheus {
            if let Some(ref v) = profile.log_data_prometheus {
                self.log_data_prometheus = Some(PathBuf::from(v));
            }
        }
        if !provided.log_data_parquet {
            if let Some(ref v) = profile.log_data_parquet {
                self.log_data_parquet = Some(PathBuf::from(v));
            }
        }
        if !provided.log_only {
            if let Some(v) = profile.log_only {
                self.log_only = v;
            }
        }
        if !provided.log_max_size {
            if let Some(ref v) = profile.log_max_size {
                self.log_max_size = Some(v.clone());
            }
        }

        // TUI settings
        if !provided.tui {
            if let Some(ref v) = profile.tui {
                // Handle "true" as empty string (just enable TUI)
                if v.to_lowercase() == "true" {
                    self.tui = Some(String::new());
                } else if v.to_lowercase() != "false" {
                    self.tui = Some(v.clone());
                }
            }
        }
    }

    /// Get the effective param path
    #[allow(dead_code)]
    pub fn get_param(&self) -> &str {
        &self.param
    }

    /// Apply post-processing logic after profile merging
    pub fn finalize(&mut self) {
        // Handle OST/MDT shortcuts
        if self.ost {
            self.param = "obdfilter.*.job_stats".to_string();
        } else if self.mdt {
            self.param = "mdt.*.job_stats".to_string();
        }

        // If totalrate is set, enable rate and total
        if self.totalrate {
            self.rate = true;
            self.total = true;
            self.difference = false;
            self.percent = false;
        }

        // If rate or difference and repeats is 1, set to 2
        if (self.rate || self.difference) && self.repeats > 0 && self.repeats < 2 {
            self.repeats = 2;
        }

        // If percent is set, enable total
        if self.percent {
            self.total = true;
        }
    }
}

