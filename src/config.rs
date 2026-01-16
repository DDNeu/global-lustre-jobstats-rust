//! Configuration file handling for glljobstat

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::args::Args;

/// SSH authentication configuration
///
/// Supports both single values (backward compatible) and comma-separated lists
/// for per-server credentials. Lists are matched positionally to the server list.
/// If there are more servers than credentials, the last credential is repeated.
///
/// Example config:
/// ```toml
/// [ssh]
/// user = "admin,root"       # admin for first server, root for rest
/// key = "~/.ssh/oss_key"    # same key for all servers
/// password = ",secret"       # no password for first, "secret" for rest
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshConfig {
    /// SSH user(s) - comma-separated list, matched positionally to servers
    pub user: Option<String>,
    /// Path(s) to SSH key file(s) - comma-separated list
    pub key: Option<String>,
    /// SSH password(s) - comma-separated list (use empty string between commas for no password)
    pub password: Option<String>,
}

/// Resolved SSH credentials for a specific server
#[derive(Debug, Clone)]
pub struct ServerCredentials {
    pub host: String,
    pub user: String,
    pub key: Option<String>,
    pub password: Option<String>,
}

impl ServerCredentials {
    /// Check if this credential set has a valid authentication method
    #[allow(dead_code)]
    pub fn has_auth(&self) -> bool {
        self.key.is_some() || self.password.is_some()
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServersConfig {
    /// Comma separated list of OSS/MDS to query
    pub list: Option<String>,
}

/// Filter configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterConfig {
    /// Comma separated list of job_ids to ignore
    pub list: Option<String>,
}

/// Miscellaneous configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MiscConfig {
    /// Job ID length for pretty printing
    pub jobid_length: Option<usize>,
    /// Path to the total rate tracking file
    pub totalratefile: Option<String>,
}

/// Profile configuration - pre-defined sets of command-line arguments
///
/// Profiles allow saving commonly-used argument combinations in the config file.
/// Priority: CLI arguments > Profile settings > Defaults
///
/// Example config:
/// ```toml
/// [profile.tui-all-ops]
/// tui = true
/// rate = true
/// interval = 5
/// count = 20
/// groupby = "user"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileConfig {
    // Data collection settings
    /// The number of top jobs to be listed
    pub count: Option<usize>,
    /// The interval in seconds to check job stats again
    pub interval: Option<u64>,
    /// The times to repeat the parsing (-1 for unlimited)
    pub repeats: Option<i64>,
    /// The param path to be checked
    pub param: Option<String>,
    /// Sort by user/group/host/host_short/job/proc
    pub groupby: Option<String>,
    /// Sort top_jobs by operation type
    pub sortby: Option<String>,
    /// Check only OST job stats
    pub ost: Option<bool>,
    /// Check only MDT job stats
    pub mdt: Option<bool>,
    /// Comma separated list of servers to override [servers] section
    pub servers: Option<String>,

    // Display settings
    /// Show full operation name
    pub fullname: Option<bool>,
    /// Set job_id filename length for pretty printing
    pub length: Option<usize>,
    /// Show sum over all jobs for each operation
    pub total: Option<bool>,
    /// Keep track of the highest rate ever
    pub totalrate: Option<bool>,
    /// The minimal ops rate number a job needs to be shown
    pub minrate: Option<i64>,
    /// Path to the total rate tracking file
    pub totalratefile: Option<String>,
    /// Show top jobs in percentage to total ops
    pub percent: Option<bool>,
    /// Show human readable time instead of timestamp
    pub humantime: Option<bool>,

    // Processing settings
    /// Number of parallel SSH connections
    pub num_proc_ssh: Option<usize>,
    /// Number of parallel data parsing tasks
    pub num_proc_data: Option<usize>,
    /// Enable read_bytes & write_bytes histogram
    pub hist: Option<bool>,
    /// Show debug and timing information
    pub verbose: Option<bool>,
    /// Show change in counters between two queries
    pub difference: Option<bool>,
    /// Calculate the rate between two queries
    pub rate: Option<bool>,

    // Filter settings
    /// Comma separated list of job_ids to ignore
    pub filter: Option<String>,
    /// Modify the filter to only show job_ids that match the filter
    pub fmod: Option<bool>,

    // Logging settings
    /// Log raw SSH output to file before parsing
    pub log_raw_data: Option<String>,
    /// Export to VictoriaMetrics JSON format
    pub log_data_victoriametrics: Option<String>,
    /// Export to Prometheus exposition format
    pub log_data_prometheus: Option<String>,
    /// Export to Apache Parquet format
    pub log_data_parquet: Option<String>,
    /// Only collect raw data, skip analysis
    pub log_only: Option<bool>,
    /// Maximum log file size before rotation
    pub log_max_size: Option<String>,

    // TUI settings
    /// Launch interactive TUI mode (can be true or a path for replay)
    pub tui: Option<String>,

    // SSH settings (profile-specific)
    /// SSH user
    pub user: Option<String>,
    /// SSH password
    pub password: Option<String>,
    /// Path to SSH key file
    pub key: Option<String>,
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    #[serde(default)]
    pub ssh: SshConfig,
    #[serde(default)]
    pub servers: ServersConfig,
    #[serde(default)]
    pub filter: FilterConfig,
    #[serde(default)]
    pub misc: MiscConfig,
    /// Named profiles - [profile.NAME] sections
    #[serde(default)]
    pub profile: std::collections::HashMap<String, ProfileConfig>,
}

impl ConfigFile {
    /// Get a profile by name
    pub fn get_profile(&self, name: &str) -> Option<&ProfileConfig> {
        self.profile.get(name)
    }

    /// List all available profile names
    pub fn list_profiles(&self) -> Vec<&String> {
        self.profile.keys().collect()
    }
}

/// Runtime configuration (merged from args and config file)
#[derive(Debug, Clone)]
pub struct Config {
    /// Ordered list of servers (preserves order for credential matching)
    pub servers: HashSet<String>,
    /// Ordered list of servers (for credential indexing)
    pub servers_ordered: Vec<String>,
    pub filter: HashSet<String>,
    /// List of users (parallel to servers_ordered)
    users: Vec<String>,
    /// List of keys (parallel to servers_ordered)
    keys: Vec<Option<String>>,
    /// List of passwords (parallel to servers_ordered)
    passwords: Vec<Option<String>>,
    pub jobid_length: usize,
    pub totalratefile: PathBuf,
}

impl Config {
    /// Load configuration from file or create a default one
    /// If a profile is provided, its SSH settings override the global [ssh] section
    pub fn load_or_create(args: &Args, profile: Option<&ProfileConfig>) -> Result<Self> {
        let config_path = expand_tilde(&args.configfile);

        let config_file = if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            toml::from_str::<ConfigFile>(&content)
                .context("Failed to parse config file")?
        } else {
            // Create a default config file with example profiles
            let content = default_config_template();

            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&config_path, &content)
                .context("Failed to write default config file")?;

            println!("Example configuration file {} created!", config_path.display());
            std::process::exit(0);
        };

        // Merge SSH config: profile settings override global [ssh] section
        let ssh_user = profile
            .and_then(|p| p.user.clone())
            .or(config_file.ssh.user.clone());
        let ssh_password = profile
            .and_then(|p| p.password.clone())
            .or(config_file.ssh.password.clone());
        let ssh_key = profile
            .and_then(|p| p.key.clone())
            .or(config_file.ssh.key.clone());

        // Parse servers - keep both ordered list and set
        let servers_ordered: Vec<String> = if let Some(ref servers_arg) = args.servers {
            servers_arg
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else if let Some(ref list) = config_file.servers.list {
            list.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        let servers: HashSet<String> = servers_ordered.iter().cloned().collect();
        let server_count = servers_ordered.len();

        let filter: HashSet<String> = if let Some(ref filter_arg) = args.filter {
            filter_arg
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        } else if let Some(ref list) = config_file.filter.list {
            list.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            HashSet::new()
        };

        let jobid_length = args.length.or(config_file.misc.jobid_length).unwrap_or(17);

        let totalratefile = args
            .totalratefile
            .clone()
            .or(config_file.misc.totalratefile.clone())
            .map(|s| expand_tilde(&s))
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".glljobstatdb.json")
            });

        // Parse credential lists with repeat-last-value behavior
        // Use merged SSH settings (profile overrides global)
        let users = parse_credential_list(ssh_user.as_deref(), server_count, "root");

        let keys = parse_optional_credential_list(
            ssh_key.as_deref(),
            server_count,
            true, // expand paths
        );

        // Determine if we need to prompt for password
        // Only prompt if at least one server has no key and no password configured
        let needs_password_prompt = keys.iter().any(|k| k.is_none()) && ssh_password.is_none();

        let prompted_password = if needs_password_prompt {
            Some(
                rpassword::prompt_password("SSH Password: ")
                    .context("Failed to read password")?,
            )
        } else {
            None
        };

        let passwords = parse_optional_credential_list_with_default(
            ssh_password.as_deref(),
            server_count,
            prompted_password.as_deref(),
        );

        Ok(Config {
            servers,
            servers_ordered,
            filter,
            users,
            keys,
            passwords,
            jobid_length,
            totalratefile,
        })
    }

    /// Get credentials for a specific server by hostname
    pub fn get_credentials(&self, host: &str) -> ServerCredentials {
        // Find the server's index in the ordered list
        let index = self
            .servers_ordered
            .iter()
            .position(|s| s == host)
            .unwrap_or(0);

        self.get_credentials_by_index(index, host)
    }

    /// Get credentials for a specific server by index
    pub fn get_credentials_by_index(&self, index: usize, host: &str) -> ServerCredentials {
        // Use repeat-last-value if index exceeds list length
        let user_idx = index.min(self.users.len().saturating_sub(1));
        let key_idx = index.min(self.keys.len().saturating_sub(1));
        let pwd_idx = index.min(self.passwords.len().saturating_sub(1));

        ServerCredentials {
            host: host.to_string(),
            user: self
                .users
                .get(user_idx)
                .cloned()
                .unwrap_or_else(|| "root".to_string()),
            key: self.keys.get(key_idx).cloned().flatten(),
            password: self.passwords.get(pwd_idx).cloned().flatten(),
        }
    }
}

/// Parse a comma-separated credential list with repeat-last-value behavior
fn parse_credential_list(input: Option<&str>, count: usize, default: &str) -> Vec<String> {
    let input = input.unwrap_or(default);
    let parts: Vec<String> = input.split(',').map(|s| s.trim().to_string()).collect();

    if parts.is_empty() || count == 0 {
        return vec![default.to_string()];
    }

    // Extend to server count by repeating last value
    let mut result = parts.clone();
    if result.len() < count {
        let last = result.last().cloned().unwrap_or_else(|| default.to_string());
        result.resize(count, last);
    }

    // Replace empty strings with default
    result
        .into_iter()
        .map(|s| if s.is_empty() { default.to_string() } else { s })
        .collect()
}

/// Parse an optional comma-separated credential list (for keys/passwords)
/// Empty values between commas become None
fn parse_optional_credential_list(
    input: Option<&str>,
    count: usize,
    expand_paths: bool,
) -> Vec<Option<String>> {
    let Some(input) = input else {
        return vec![None; count.max(1)];
    };

    let parts: Vec<Option<String>> = input
        .split(',')
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else if expand_paths {
                Some(expand_tilde(trimmed).to_string_lossy().to_string())
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    if parts.is_empty() || count == 0 {
        return vec![None];
    }

    // Extend to server count by repeating last value
    let mut result = parts.clone();
    if result.len() < count {
        let last = result.last().cloned().flatten();
        result.resize(count, last);
    }

    result
}

/// Parse optional credential list with a default value for None entries
fn parse_optional_credential_list_with_default(
    input: Option<&str>,
    count: usize,
    default: Option<&str>,
) -> Vec<Option<String>> {
    let Some(input) = input else {
        // No input - use default for all servers
        return vec![default.map(|s| s.to_string()); count.max(1)];
    };

    let parts: Vec<Option<String>> = input
        .split(',')
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                default.map(|s| s.to_string())
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    if parts.is_empty() || count == 0 {
        return vec![default.map(|s| s.to_string())];
    }

    // Extend to server count by repeating last value
    let mut result = parts.clone();
    if result.len() < count {
        let last = result.last().cloned().flatten();
        result.resize(count, last);
    }

    result
}

/// Expand ~ to home directory
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Generate a default config file template with example profiles
pub fn default_config_template() -> String {
    r#"# glljobstat Configuration File
# ==============================
#
# This file uses PROFILES to define complete configurations for different systems.
# Each profile contains all settings needed: servers, SSH credentials, and options.
#
# Usage:
#   glljobstat -P <profile_name>       # Run with a specific profile
#   glljobstat --list-profiles         # List available profiles
#
# Priority: CLI arguments > Profile settings > Defaults
#
# To get started:
#   1. Copy the [profile.example] section below
#   2. Rename it to [profile.your-system-name]
#   3. Fill in your server addresses and credentials
#   4. Uncomment and adjust options as needed

# =============================================================================
# EXAMPLE PROFILE - Complete reference with all available options
# =============================================================================
# Uncomment and customize this profile for your system

# [profile.example]
# #---------------------------------------------------------------------------
# # CONNECTION SETTINGS (required)
# #---------------------------------------------------------------------------
# # Comma-separated list of OSS/MDS servers to query
# servers = "oss1.example.com,oss2.example.com,mds1.example.com"
#
# # SSH credentials (use key OR password)
# # Credentials can be specified PER-SERVER using comma-separated lists.
# # If fewer values than servers, the last value is repeated for remaining servers.
# #
# # Examples with 3 servers (oss1, oss2, mds1):
# #   user = "root"                    # "root" for all servers
# #   user = "admin,root"              # "admin" for oss1, "root" for oss2 and mds1
# #   user = "admin,root,mdsuser"      # different user for each server
# #
# #   password = "pass1,pass2,pass3"   # different password per server
# #   password = "pass1,,pass3"        # empty password for oss2 (uses key instead)
# #
# #   key = "~/.ssh/id_rsa,~/.ssh/id_ed25519"  # different keys per server
# #
# user = "root"
# # key = "~/.ssh/id_rsa"           # Path to SSH private key (key type auto-detected)
# password = "your-password"        # SSH password (if not using key)
#
# #---------------------------------------------------------------------------
# # DATA COLLECTION SETTINGS
# #---------------------------------------------------------------------------
# # count = 5                       # Number of top jobs to display (default: 5)
# # interval = 10                   # Seconds between queries (default: 10)
# # repeats = -1                    # Number of iterations, -1=unlimited (default: -1)
# # param = "*.*.job_stats"         # Lustre param path (default: *.*.job_stats)
# # ost = false                     # Query only OST stats (sets param=obdfilter.*.job_stats)
# # mdt = false                     # Query only MDT stats (sets param=mdt.*.job_stats)
#
# #---------------------------------------------------------------------------
# # DISPLAY & SORTING SETTINGS
# #---------------------------------------------------------------------------
# # groupby = "none"                # Group jobs by: none, user, group, host, host_short, job, proc
# # sortby = "ops"                  # Sort by operation: ops, open, close, read_bytes, write_bytes,
# #                                 #   mknod, link, unlink, mkdir, rmdir, rename, getattr, setattr,
# #                                 #   getxattr, setxattr, statfs, sync, samedir_rename, crossdir_rename, punch
# # fullname = false                # Show full operation names instead of abbreviations
# # length = 17                     # Job ID display length for formatting
# # humantime = false               # Show human-readable timestamps
#
# #---------------------------------------------------------------------------
# # RATE & CALCULATION SETTINGS
# #---------------------------------------------------------------------------
# # rate = false                    # Calculate and show operation rates (ops/sec)
# # difference = false              # Show counter differences between queries
# # total = false                   # Show sum totals for each operation
# # percent = false                 # Show percentages relative to total (enables total)
# # totalrate = false               # Track highest rate ever seen (enables rate+total)
# # minrate = 1                     # Minimum ops rate to display a job (default: 1)
# # totalratefile = "~/.glljobstatdb.json"  # File to persist highest rate tracking
#
# #---------------------------------------------------------------------------
# # FILTER SETTINGS
# #---------------------------------------------------------------------------
# # filter = "0,cp.0"               # Comma-separated job_ids to ignore
# # fmod = false                    # Invert filter: only SHOW matching job_ids
#
# #---------------------------------------------------------------------------
# # PROCESSING SETTINGS
# #---------------------------------------------------------------------------
# # num_proc_ssh = 14               # Parallel SSH connections (default: CPU count)
# # num_proc_data = 14              # Parallel data parsing tasks (default: CPU count)
# # hist = false                    # Enable read/write bytes histogram
# # verbose = false                 # Show debug and timing information
#
# #---------------------------------------------------------------------------
# # LOGGING SETTINGS - Export data to files
# #---------------------------------------------------------------------------
# # log_raw_data = "/var/log/lustre/"           # Log raw SSH output (for debugging/replay)
# # log_data_prometheus = "/var/log/lustre/"    # Export to Prometheus format (.prom)
# # log_data_victoriametrics = "/var/log/lustre/"  # Export to VictoriaMetrics JSON (.vm.json)
# # log_data_parquet = "/var/log/lustre/"       # Export to Apache Parquet (.parquet)
# # log_only = false                            # Only log, skip console output
# # log_max_size = "100M"                       # Max log size before rotation (e.g., 100M, 1G, 10T)
#
# #---------------------------------------------------------------------------
# # TUI (Terminal User Interface) SETTINGS
# #---------------------------------------------------------------------------
# # tui = "true"                    # Launch interactive TUI mode
# # tui = "/path/to/logs"           # Launch TUI in replay mode from log files

# =============================================================================
# QUICK-START PROFILES - Common use cases
# =============================================================================

# # Simple monitoring with rate calculation
# [profile.monitor]
# servers = "oss1,oss2"
# user = "root"
# password = "changeme"
# rate = true
# count = 10

# # Interactive TUI dashboard
# [profile.dashboard]
# servers = "oss1,oss2"
# user = "root"
# password = "changeme"
# tui = "true"
# rate = true
# interval = 5
# count = 20

# # Log to Prometheus for Grafana integration
# [profile.prometheus-export]
# servers = "oss1,oss2"
# user = "root"
# password = "changeme"
# rate = true
# interval = 60
# log_data_prometheus = "/var/log/lustre/jobstats/"
# log_max_size = "100M"
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credential_list_single_value() {
        // Single value should be used for all servers
        let result = parse_credential_list(Some("admin"), 3, "root");
        assert_eq!(result, vec!["admin", "admin", "admin"]);
    }

    #[test]
    fn test_parse_credential_list_exact_match() {
        // Exact number of values
        let result = parse_credential_list(Some("admin,root,user"), 3, "default");
        assert_eq!(result, vec!["admin", "root", "user"]);
    }

    #[test]
    fn test_parse_credential_list_repeat_last() {
        // Fewer values than servers - repeat last
        let result = parse_credential_list(Some("admin,root"), 4, "default");
        assert_eq!(result, vec!["admin", "root", "root", "root"]);
    }

    #[test]
    fn test_parse_credential_list_empty_uses_default() {
        // Empty string uses default
        let result = parse_credential_list(Some(",root"), 2, "default");
        assert_eq!(result, vec!["default", "root"]);
    }

    #[test]
    fn test_parse_credential_list_none_uses_default() {
        // None uses default for all
        let result = parse_credential_list(None, 3, "root");
        assert_eq!(result, vec!["root", "root", "root"]);
    }

    #[test]
    fn test_parse_optional_credential_list_single_value() {
        let result = parse_optional_credential_list(Some("/path/to/key"), 3, false);
        assert_eq!(
            result,
            vec![
                Some("/path/to/key".to_string()),
                Some("/path/to/key".to_string()),
                Some("/path/to/key".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_optional_credential_list_with_empty() {
        // Empty value between commas becomes None
        let result = parse_optional_credential_list(Some("key1,,key3"), 3, false);
        assert_eq!(
            result,
            vec![
                Some("key1".to_string()),
                None,
                Some("key3".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_optional_credential_list_repeat_last_none() {
        // Last value is empty, so None is repeated
        let result = parse_optional_credential_list(Some("key1,"), 3, false);
        assert_eq!(result, vec![Some("key1".to_string()), None, None]);
    }

    #[test]
    fn test_parse_optional_credential_list_repeat_last_some() {
        // Last value is set, so it's repeated
        let result = parse_optional_credential_list(Some(",key2"), 4, false);
        assert_eq!(
            result,
            vec![
                None,
                Some("key2".to_string()),
                Some("key2".to_string()),
                Some("key2".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_optional_credential_list_none_input() {
        let result = parse_optional_credential_list(None, 3, false);
        assert_eq!(result, vec![None, None, None]);
    }

    #[test]
    fn test_parse_optional_credential_list_with_default() {
        // Empty values get the default
        let result =
            parse_optional_credential_list_with_default(Some("pass1,,pass3"), 3, Some("prompted"));
        assert_eq!(
            result,
            vec![
                Some("pass1".to_string()),
                Some("prompted".to_string()),
                Some("pass3".to_string())
            ]
        );
    }

    #[test]
    fn test_parse_optional_credential_list_with_default_none_input() {
        // No input, all servers get default
        let result = parse_optional_credential_list_with_default(None, 3, Some("prompted"));
        assert_eq!(
            result,
            vec![
                Some("prompted".to_string()),
                Some("prompted".to_string()),
                Some("prompted".to_string())
            ]
        );
    }
}
