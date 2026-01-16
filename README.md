# glljobstat (Rust)

A high-performance Rust rewrite of [glljobstat.py](https://github.com/DDNeu/global-lustre-jobstats), based on [lljobstat](https://review.whamcloud.com/c/fs/lustre-release/+/48888) with significant enhancements for monitoring Lustre job statistics across multiple servers.

## Features

### Core Functionality
- **Aggregate stats** over multiple OSS/MDS via SSH in parallel (key and password auth)
- **Calculate rates** of each job between queries
- **Show sum of ops** over all jobs
- **Show job ops in percentage** to total ops
- **Track highest ever ops** in a persistent JSON file
- **Filter job_ids** - include or exclude specific patterns
- **Config file** for SSH, servers, filters, and settings (TOML format)
- **Configurable job_id length** for pretty printing
- **Parallel SSH connections** and async data processing

### Rust Version Enhancements
- **10-50x faster** parsing and data processing compared to Python
- **Profile-based configuration** - save complete setups for different clusters
- **Interactive TUI mode** with real-time time-series graphs
- **Multiple export formats**: Prometheus, VictoriaMetrics, Apache Parquet
- **Replay mode** - analyze historical data from log files
- **Per-server credentials** - different user/password/key per server
- **Log rotation** with configurable max file sizes
- **Group jobs by** user, group, host, job ID, or process
- **Sort by any operation** type (ops, read_bytes, write_bytes, open, etc.)
- **Native binary** - no Python runtime required

## Installation

### Building from Source

```bash
# Clone the repository
git clone https://github.com/bolausson/global-lustre-jobstats-rust.git
cd global-lustre-jobstats-rust

# Build release binary
cargo build --release

# Optional: strip for smaller binary (15MB → ~5MB)
strip target/release/glljobstat

# Install to system
sudo cp target/release/glljobstat /usr/local/bin/
```

### Requirements
- Rust 1.70+ (for building)
- SSH access to Lustre OSS/MDS servers
- Lustre `job_stats` enabled on target servers

## Quick Start

### 1. Generate Configuration File

```bash
glljobstat --init
```

This creates `~/.glljobstat.toml` with a fully documented example configuration.

### 2. Edit Configuration

Edit `~/.glljobstat.toml` and create a profile for your cluster:

```toml
[profile.mycluster]
servers = "oss1.example.com,oss2.example.com,mds1.example.com"
user = "root"
password = "your-password"
# Or use SSH key (type auto-detected):
# key = "~/.ssh/id_rsa"
rate = true
count = 10
```

### 3. Run with Profile

```bash
glljobstat -P mycluster
```

## Configuration

### Profile-Based Configuration

Profiles allow you to save complete configurations for different clusters or use cases.
All settings including servers, credentials, and options are stored per-profile.

```toml
# Production cluster monitoring
[profile.prod]
servers = "oss1,oss2,oss3,mds1,mds2"
user = "admin"
key = "~/.ssh/prod_key"
rate = true
count = 15
interval = 5

# Development cluster
[profile.dev]
servers = "dev-oss1,dev-mds1"
user = "root"
password = "devpass"
count = 5

# Interactive TUI dashboard
[profile.dashboard]
servers = "oss1,oss2,mds1"
user = "root"
key = "~/.ssh/id_rsa"
tui = "true"
rate = true
interval = 3
count = 20

# Prometheus exporter for Grafana
[profile.prometheus]
servers = "oss1,oss2,mds1"
user = "monitor"
key = "~/.ssh/monitor_key"
rate = true
interval = 60
log_data_prometheus = "/var/lib/prometheus/lustre/"
log_max_size = "100M"
```

**Usage:**
```bash
glljobstat -P prod              # Run with production profile
glljobstat --list-profiles      # List all available profiles
```

### Per-Server Credentials

Specify different credentials for each server using comma-separated lists:

```toml
[profile.mixed]
servers = "oss1,oss2,mds1"
user = "admin,root,mdsadmin"           # Different user per server
password = "pass1,pass2,pass3"         # Different password per server
# Or mix keys and passwords:
# key = "~/.ssh/oss_key,,~/.ssh/mds_key"  # Empty = use password for that server
# password = ",oss2pass,"                  # Password only for oss2
```

If fewer credentials than servers, the last value is repeated.

## Usage Examples

### Basic Usage

```bash
# Run once, show top 5 jobs
glljobstat -P mycluster -n 1

# Run continuously with rate calculation, show top 10 jobs
glljobstat -P mycluster -r -c 10

# Show top jobs sorted by write bytes
glljobstat -P mycluster -r --sortby write_bytes

# Group jobs by user and show rates
glljobstat -P mycluster -r --groupby user
```

### Filtering Jobs

```bash
# Filter OUT jobs containing "oss" or "login" in job_id
glljobstat -P mycluster -f oss,login

# Filter to ONLY show jobs matching the filter (invert with -F)
glljobstat -P mycluster -f comp -F

# Set minimum rate threshold (only show jobs with >= 100 ops/sec)
glljobstat -P mycluster -r --minrate 100
```

### TUI Mode (Interactive Dashboard)

```bash
# Launch interactive TUI with real-time graphs
glljobstat -P mycluster --tui

# Replay historical data in TUI from log files
glljobstat --tui /var/log/lustre/jobstats/
```

The TUI provides:
- Real-time time-series graph of total ops rate
- Top jobs table with live updates
- Keyboard navigation and sorting
- Pause/resume data collection

### Data Export & Logging

```bash
# Export to Prometheus format (for Grafana)
glljobstat -P mycluster -r --log-data-prometheus /var/lib/prometheus/lustre/

# Export to VictoriaMetrics JSON format
glljobstat -P mycluster -r --log-data-victoriametrics /var/log/lustre/

# Export to Apache Parquet (for data analysis)
glljobstat -P mycluster -r --log-data-parquet /var/log/lustre/

# Log raw SSH output (for debugging or replay)
glljobstat -P mycluster --log-raw-data /var/log/lustre/raw/

# Combine logging with rotation
glljobstat -P mycluster -r \
  --log-data-prometheus /var/lib/prometheus/lustre/ \
  --log-max-size 100M

# Collect data only (no console output)
glljobstat -P mycluster --log-raw-data /var/log/lustre/ --log-only
```

### Totals and Percentages

```bash
# Show total ops across all jobs
glljobstat -P mycluster -t

# Show jobs as percentage of total
glljobstat -P mycluster -p

# Track highest rate ever seen (persisted to file)
glljobstat -P mycluster -r -T
```

## Command-Line Reference

```
glljobstat [OPTIONS]

OPTIONS:
  -C, --configfile <PATH>     Config file path [default: ~/.glljobstat.toml]
  -P, --profile <NAME>        Use named profile from config
  --init                      Generate example config file and exit
  --list-profiles             List available profiles and exit

DATA COLLECTION:
  -c, --count <N>             Number of top jobs to show [default: 5]
  -i, --interval <SECS>       Query interval in seconds [default: 10]
  -n, --repeats <N>           Number of iterations, -1=unlimited [default: -1]
  -s, --servers <LIST>        Comma-separated server list
  --param <PATH>              Lustre param path [default: *.*.job_stats]
  -o, --ost                   Query only OST stats
  -m, --mdt                   Query only MDT stats

DISPLAY:
  --groupby <TYPE>            Group by: none, user, group, host, host_short, job, proc
  --sortby <OP>               Sort by operation: ops, read_bytes, write_bytes, open, etc.
  --fullname                  Show full operation names
  -l, --length <N>            Job ID display length
  -H, --humantime             Human-readable timestamps

CALCULATIONS:
  -r, --rate                  Calculate operation rates
  -d, --difference            Show counter differences
  -t, --total                 Show operation totals
  -p, --percent               Show percentages of total
  -T, --totalrate             Track highest rate ever
  --minrate <N>               Minimum rate to display [default: 1]

FILTERING:
  -f, --filter <LIST>         Job IDs to exclude (comma-separated)
  -F, --fmod                  Invert filter (show only matching)

LOGGING:
  --log-raw-data <PATH>       Log raw SSH output
  --log-data-prometheus <PATH>    Export Prometheus format
  --log-data-victoriametrics <PATH>  Export VictoriaMetrics JSON
  --log-data-parquet <PATH>   Export Apache Parquet
  --log-only                  Only log, skip console output
  --log-max-size <SIZE>       Max log size (e.g., 100M, 1G)

TUI:
  --tui [<PATH>]              Launch TUI, optionally replay from PATH

OTHER:
  --num-proc-ssh <N>          Parallel SSH connections [default: CPU count]
  --num-proc-data <N>         Parallel data parsing [default: CPU count]
  -v, --verbose               Show debug information
  -h, --help                  Show help
  -V, --version               Show version
```

## Export Formats

### Prometheus

Creates `.prom` files compatible with Prometheus node_exporter textfile collector:

```
# HELP lustre_job_ops Total operations per job
# TYPE lustre_job_ops gauge
lustre_job_ops{job_id="user@host",server="oss1"} 12345
lustre_job_read_bytes{job_id="user@host",server="oss1"} 1073741824
```

### VictoriaMetrics

Creates `.vm.json` files for direct import via `/api/v1/import`:

```json
{"metric":{"__name__":"lustre_job_ops","job_id":"user@host"},"values":[12345],"timestamps":[1699900000000]}
```

### Apache Parquet

Creates `.parquet` files for efficient columnar storage, ideal for:
- Data analysis with Python/Pandas
- Long-term archival
- Integration with data lakes

## Migration from Python Version

### Key Differences

| Feature | Python | Rust |
|---------|--------|------|
| Config format | INI | TOML |
| Config file | `~/.glljobstat.cfg` | `~/.glljobstat.toml` |
| Profiles | Not supported | Full support |
| TUI mode | Not available | Built-in |
| Export formats | None | Prometheus, VM, Parquet |
| Highest rate file | Pickle | JSON |
| Key type option | Required | Auto-detected (removed) |

### Config Migration

Python config:
```ini
[SSH]
user = root
key = ~/.ssh/id_rsa
keytype = RSA

[SERVERS]
list = oss1,oss2

[FILTER]
list = 0,cp.0
```

Rust config (profile-based):
```toml
[profile.mycluster]
servers = "oss1,oss2"
user = "root"
key = "~/.ssh/id_rsa"
# keytype not needed - auto-detected
filter = "0,cp.0"
```

## Troubleshooting

### SSH Connection Issues

```bash
# Test with verbose output
glljobstat -P mycluster -v -n 1

# Check SSH connectivity manually
ssh -i ~/.ssh/id_rsa root@oss1 "lctl get_param *.*.job_stats"
```

### No Data Returned

- Ensure `job_stats` is enabled: `lctl set_param *.*.job_stats=1`
- Check the param path matches your setup: `--param obdfilter.*.job_stats`

### TUI Not Working

- Ensure terminal supports Unicode and 256 colors
- Try a different terminal emulator
- Check `$TERM` environment variable

## License

See LICENSE file for details.

## Authors

- Bjoern Olausson
- Maxence Joulin

## See Also

- [Original Python version](https://github.com/DDNeu/global-lustre-jobstats)
- [Lustre lljobstat](https://review.whamcloud.com/c/fs/lustre-release/+/48888)
- [Lustre Job Stats documentation](https://doc.lustre.org/lustre_manual.xhtml#lustrejobstats)
