//! TUI (Terminal User Interface) module for glljobstat
//!
//! Provides an interactive terminal interface with:
//! - Time-series graph of job operation rates using Braille characters
//! - Collapsible filter panels for job ID and operation filtering
//! - Adjustable time window and refresh interval
//! - Top jobs table with sorting
//! - Replay mode for historical data analysis

mod app;
mod events;
mod filters;
pub mod replay;
mod time_series;
mod ui;
mod widgets;

pub use app::TuiApp;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::stdout;
use std::path::PathBuf;

use crate::config::Config;
use crate::job_stats::JobStatsParser;

/// Run the TUI application
///
/// # Arguments
/// * `config` - Application configuration
/// * `parser` - Job stats parser (used for live mode)
/// * `replay_path` - Optional path to replay data file (None = live mode)
pub async fn run(
    config: Config,
    mut parser: JobStatsParser,
    replay_path: Option<PathBuf>,
) -> Result<()> {
    // Check if we're in replay mode
    if let Some(ref path) = replay_path {
        return run_replay_mode(config, parser, path).await;
    }

    // Live mode - initialize the parser (get params from servers, parse jobid_name)
    if config.servers.is_empty() {
        anyhow::bail!("No servers configured");
    }

    // Print initialization message before entering TUI
    println!("Initializing TUI, connecting to servers...");

    // Get params from all servers
    parser.hosts_param = parser.get_params_public().await?;

    // Parse jobid_name pattern
    parser.parse_jobid_name_public().await?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create and run app
    let mut app = TuiApp::new(config, parser, None);
    let result = app.run(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Run TUI in replay mode
async fn run_replay_mode(
    config: Config,
    parser: JobStatsParser,
    replay_path: &PathBuf,
) -> Result<()> {
    use replay::{load_replay_data, ReplayController};

    // Print loading message
    println!("Loading replay data from {:?}...", replay_path);

    // Load replay data
    let replay_data = load_replay_data(replay_path).await?;

    let record_count = replay_data.records.len();
    let job_count = replay_data.job_ids.len();
    let (start, end) = replay_data.time_range;

    println!(
        "Loaded {} records for {} jobs (time range: {} to {})",
        record_count,
        job_count,
        chrono::DateTime::from_timestamp(start, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| start.to_string()),
        chrono::DateTime::from_timestamp(end, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| end.to_string()),
    );

    // Create replay controller
    let controller = ReplayController::new(replay_data);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create and run app with replay controller
    let mut app = TuiApp::new(config, parser, Some(controller));
    let result = app.run(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

