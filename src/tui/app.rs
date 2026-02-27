//! TUI Application state and main event loop

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::events::{handle_key_event, handle_mouse_event};
use super::filters::{JobIdFilter, OperationFilter};
use super::time_series::TimeSeriesStore;
use super::ui;
use crate::args::Args;
use crate::config::Config;
use crate::job_stats::JobStatsParser;
use crate::ssh;
use crate::stats_processor::{ProcessingConfig, StatsProcessor, TimestampInfo};

/// Types of clickable regions in the UI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitRegionType {
    /// A job entry in the legend (job_id)
    LegendJob(String),
    /// A job row in the table (job_id)
    TableJob(String),
    /// An operation filter checkbox (operation name)
    OperationFilter(String),
    /// Panel header (panel name for collapse/expand)
    PanelHeader(String),
}

/// A clickable region on the screen
#[derive(Debug, Clone)]
pub struct HitRegion {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub region_type: HitRegionType,
}

impl HitRegion {
    pub fn new(rect: Rect, region_type: HitRegionType) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            region_type,
        }
    }

    /// Check if a point is within this region
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Collection of all clickable regions (rebuilt each frame)
#[derive(Debug, Default)]
pub struct ClickableRegions {
    pub regions: Vec<HitRegion>,
}

impl ClickableRegions {
    pub fn new() -> Self {
        Self { regions: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.regions.clear();
    }

    pub fn add(&mut self, region: HitRegion) {
        self.regions.push(region);
    }

    /// Find the region at a given point
    pub fn find_at(&self, x: u16, y: u16) -> Option<&HitRegion> {
        self.regions.iter().find(|r| r.contains(x, y))
    }
}

/// Panel visibility state
#[derive(Debug, Clone)]
pub struct PanelVisibility {
    pub left_panel: bool,
    pub right_panel: bool,
    pub bottom_panel: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            left_panel: true,
            right_panel: true,
            bottom_panel: true,
        }
    }
}

impl PanelVisibility {
    pub fn all_hidden(&self) -> bool {
        !self.left_panel && !self.right_panel && !self.bottom_panel
    }

    pub fn toggle_all(&mut self) {
        if self.all_hidden() {
            self.left_panel = true;
            self.right_panel = true;
            self.bottom_panel = true;
        } else {
            self.left_panel = false;
            self.right_panel = false;
            self.bottom_panel = false;
        }
    }
}

/// View mode presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Full,
    GraphFocused,
    Custom,
}

/// Which panel/widget has focus for keyboard input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Graph,
    OperationFilter,
    JobIdFilter,
    TopJobsTable,
    Settings,
}

/// Input mode for text fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// Color assignment for jobs in the graph
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct JobColorMap {
    colors: Vec<Color>,
    assignments: HashMap<String, usize>,
    next_idx: usize,
}

impl Default for JobColorMap {
    fn default() -> Self {
        Self {
            colors: vec![
                Color::Cyan,
                Color::Yellow,
                Color::Magenta,
                Color::Green,
                Color::Red,
                Color::Blue,
                Color::LightCyan,
                Color::LightYellow,
                Color::LightMagenta,
                Color::LightGreen,
                Color::LightRed,
                Color::LightBlue,
            ],
            assignments: HashMap::new(),
            next_idx: 0,
        }
    }
}

#[allow(dead_code)]
impl JobColorMap {
    pub fn get_color(&mut self, job_id: &str) -> Color {
        if let Some(&idx) = self.assignments.get(job_id) {
            self.colors[idx % self.colors.len()]
        } else {
            let idx = self.next_idx;
            self.assignments.insert(job_id.to_string(), idx);
            self.next_idx += 1;
            self.colors[idx % self.colors.len()]
        }
    }

    pub fn get_assigned_color(&self, job_id: &str) -> Option<Color> {
        self.assignments.get(job_id).map(|&idx| self.colors[idx % self.colors.len()])
    }
}

/// Aggregated stats for TUI display
#[derive(Debug, Clone)]
pub struct TuiStats {
    pub timestamp: i64,
    pub jobs: HashMap<String, HashMap<String, i64>>,
    pub servers_queried: usize,
}

/// Message from data collection task
#[derive(Debug)]
pub enum DataMessage {
    Stats(TuiStats),
    Error(String),
}

/// Job selection mode for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// No explicit selection - show all (subject to text filters)
    None,
    /// Show only selected jobs (inclusive)
    Inclusive,
    /// Hide selected jobs (exclusive)
    Exclusive,
}

/// Which type of graph to display in the graph area
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphKind {
    /// Time-series line chart (existing)
    TimeSeries,
    /// Bar chart showing current operation values
    BarChart,
}

/// Main TUI application state
pub struct TuiApp {
    // Configuration
    pub config: Config,
    #[allow(dead_code)]
    pub args: Args,
    pub hosts_param: HashMap<String, Vec<String>>,
    #[allow(dead_code)]
    pub jobid_var: HashMap<String, usize>,
    #[allow(dead_code)]
    pub jobid_separator: char,
    /// Processing configuration for stats processor
    pub processing_config: ProcessingConfig,

    // Data
    pub time_series: TimeSeriesStore,
    pub current_stats: Option<TuiStats>,
    pub job_color_map: JobColorMap,

    // Filters
    pub operation_filter: OperationFilter,
    pub job_filter: JobIdFilter,

    // Job selection (for mouse-based filtering)
    pub selected_jobs: HashSet<String>,
    pub selection_mode: SelectionMode,

    // Clickable regions (rebuilt each frame)
    pub click_regions: ClickableRegions,

    // Display settings
    pub refresh_interval: Duration,
    pub time_window: Duration,
    pub top_n: usize,

    // Panel visibility
    pub panels: PanelVisibility,
    pub view_mode: ViewMode,

    // UI state
    pub focus: FocusArea,
    pub input_mode: InputMode,
    pub should_quit: bool,
    pub show_help: bool,

    // Scroll state for table
    pub table_scroll: usize,

    // Status message
    pub status_message: Option<(String, Instant)>,

    // Graph display modes
    /// If true, aggregate same operation across all jobs; if false, show per-job lines
    pub graph_aggregate_mode: bool,
    /// If true, show rate (ops/sec); if false, show raw counter values
    pub graph_rate_mode: bool,
    /// If true, hide the legend inside the graph plot area
    pub graph_hide_legend: bool,
    /// Which graph type to display (line chart vs bar chart)
    pub graph_kind: GraphKind,
    /// If true, use log10 scale for bar chart; if false, use linear
    pub bar_chart_log_scale: bool,

    // Replay mode
    /// Replay controller (Some = replay mode, None = live mode)
    pub replay: Option<super::replay::ReplayController>,
}

impl TuiApp {
    pub fn new(
        config: Config,
        parser: JobStatsParser,
        replay: Option<super::replay::ReplayController>,
    ) -> Self {
        // Get jobid components from parser for filter setup
        let jobid_components: Vec<String> = parser.jobid_var.keys().cloned().collect();
        let jobid_separator = parser.jobid_separator;
        let hosts_param = parser.hosts_param.clone();
        let jobid_var = parser.jobid_var.clone();
        let args = parser.args.clone();

        // Create processing config from parser
        let processing_config = parser.create_processing_config();

        // Use refresh interval from args if specified, otherwise default
        let refresh_interval = Duration::from_secs(args.interval.max(1) as u64);

        Self {
            config,
            args,
            hosts_param,
            jobid_var,
            jobid_separator,
            processing_config,
            time_series: TimeSeriesStore::new(Duration::from_secs(600)), // 10 min max
            current_stats: None,
            job_color_map: JobColorMap::default(),
            operation_filter: OperationFilter::default(),
            job_filter: JobIdFilter::new(jobid_components, jobid_separator),
            selected_jobs: HashSet::new(),
            selection_mode: SelectionMode::None,
            click_regions: ClickableRegions::new(),
            refresh_interval,
            time_window: Duration::from_secs(300), // 5 minutes
            top_n: 10,
            panels: PanelVisibility::default(),
            view_mode: ViewMode::Full,
            focus: FocusArea::Graph,
            input_mode: InputMode::Normal,
            should_quit: false,
            show_help: false,
            table_scroll: 0,
            status_message: None,
            graph_aggregate_mode: false, // Default: per-job mode
            graph_rate_mode: false,      // Default: show raw counters
            graph_hide_legend: false,    // Default: show legend in graph
            graph_kind: GraphKind::TimeSeries,
            bar_chart_log_scale: true,
            replay,
        }
    }

    /// Check if we're in replay mode
    pub fn is_replay_mode(&self) -> bool {
        self.replay.is_some()
    }

    /// Toggle graph aggregate mode
    pub fn toggle_graph_aggregate_mode(&mut self) {
        self.graph_aggregate_mode = !self.graph_aggregate_mode;
    }

    /// Toggle graph rate mode
    pub fn toggle_graph_rate_mode(&mut self) {
        self.graph_rate_mode = !self.graph_rate_mode;
    }

    /// Toggle graph legend visibility
    pub fn toggle_graph_legend(&mut self) {
        self.graph_hide_legend = !self.graph_hide_legend;
    }

    /// Toggle between time-series and bar chart graph
    pub fn toggle_graph_kind(&mut self) {
        self.graph_kind = match self.graph_kind {
            GraphKind::TimeSeries => GraphKind::BarChart,
            GraphKind::BarChart => GraphKind::TimeSeries,
        };
    }

    /// Toggle bar chart log/linear scale
    pub fn toggle_bar_chart_scale(&mut self) {
        self.bar_chart_log_scale = !self.bar_chart_log_scale;
    }

    /// Check if rate/difference mode is enabled
    #[allow(dead_code)]
    pub fn is_rate_mode(&self) -> bool {
        self.args.rate || self.args.difference
    }

    /// Get the sortby field
    #[allow(dead_code)]
    pub fn sortby(&self) -> &str {
        &self.args.sortby
    }

    /// Toggle job selection
    pub fn toggle_job_selection(&mut self, job_id: &str) {
        if self.selected_jobs.contains(job_id) {
            self.selected_jobs.remove(job_id);
        } else {
            self.selected_jobs.insert(job_id.to_string());
        }

        // Only reset to None when selection becomes empty
        if self.selected_jobs.is_empty() {
            self.selection_mode = SelectionMode::None;
        }
        // Don't auto-switch to Inclusive - let user press 's' to activate filter
    }

    /// Select only this job (clear others) and activate inclusive filter
    pub fn select_job_only(&mut self, job_id: &str) {
        self.selected_jobs.clear();
        self.selected_jobs.insert(job_id.to_string());
        // Shift+click explicitly activates inclusive mode
        self.selection_mode = SelectionMode::Inclusive;
    }

    /// Clear all job selections
    pub fn clear_job_selection(&mut self) {
        self.selected_jobs.clear();
        self.selection_mode = SelectionMode::None;
    }

    /// Toggle selection mode (None -> Inclusive -> Exclusive -> None)
    pub fn cycle_selection_mode(&mut self) {
        self.selection_mode = match self.selection_mode {
            SelectionMode::None => SelectionMode::Inclusive,
            SelectionMode::Inclusive => SelectionMode::Exclusive,
            SelectionMode::Exclusive => SelectionMode::None,
        };
    }

    /// Check if a job is selected
    pub fn is_job_selected(&self, job_id: &str) -> bool {
        self.selected_jobs.contains(job_id)
    }

    /// Check if a job passes selection filter (separate from text filters)
    pub fn job_passes_selection(&self, job_id: &str) -> bool {
        match self.selection_mode {
            SelectionMode::None => true,
            SelectionMode::Inclusive => self.selected_jobs.contains(job_id),
            SelectionMode::Exclusive => !self.selected_jobs.contains(job_id),
        }
    }

    /// Main run loop
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        if self.is_replay_mode() {
            return self.run_replay_loop(terminal).await;
        }

        // Live mode: Create channel for data collection
        let (tx, mut rx) = mpsc::channel::<DataMessage>(32);

        // Clone what we need for the data collection task
        let config = self.config.clone();
        let hosts_param = self.hosts_param.clone();
        let refresh_interval = self.refresh_interval;
        let processing_config = self.processing_config.clone();

        // Spawn data collection task
        let collector_handle = tokio::spawn(async move {
            Self::data_collector(config, hosts_param, processing_config, tx, refresh_interval).await
        });

        // Main UI loop
        let tick_rate = Duration::from_millis(100);
        let mut last_tick = Instant::now();

        while !self.should_quit {
            // Draw UI
            terminal.draw(|f| ui::render(f, self))?;

            // Calculate timeout for event polling
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            // Poll for keyboard and mouse events
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        handle_key_event(self, key);
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_event(self, mouse);
                    }
                    _ => {}
                }
            }

            // Check for new data from collector
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    DataMessage::Stats(stats) => {
                        self.update_from_stats(stats);
                    }
                    DataMessage::Error(e) => {
                        self.set_status(format!("Error: {}", e));
                    }
                }
            }

            // Tick handling
            if last_tick.elapsed() >= tick_rate {
                self.on_tick();
                last_tick = Instant::now();
            }
        }

        // Clean up collector task
        collector_handle.abort();

        Ok(())
    }

    /// Replay mode run loop
    async fn run_replay_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        let tick_rate = Duration::from_millis(50); // Faster tick for smooth playback
        let mut last_tick = Instant::now();

        // Initialize with first frame of data
        self.update_replay_stats();

        while !self.should_quit {
            // Draw UI
            terminal.draw(|f| ui::render(f, self))?;

            // Calculate timeout for event polling
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::ZERO);

            // Poll for keyboard and mouse events
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        handle_key_event(self, key);
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_event(self, mouse);
                    }
                    _ => {}
                }
            }

            // Tick handling for replay
            if last_tick.elapsed() >= tick_rate {
                self.on_replay_tick();
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    /// Update stats from replay controller
    pub fn update_replay_stats(&mut self) {
        if let Some(ref controller) = self.replay {
            let stats = controller.get_current_stats();
            let timestamp = controller.current_time;

            // Update time series with replay data
            for (job_id, ops) in &stats {
                for (op_name, value) in ops {
                    self.time_series.insert(job_id, op_name, timestamp, *value);
                }
            }

            // Update current stats
            self.current_stats = Some(TuiStats {
                timestamp,
                jobs: stats,
                servers_queried: 0, // Not applicable in replay mode
            });
        }
    }

    /// Tick handler for replay mode
    fn on_replay_tick(&mut self) {
        // Advance replay time if playing
        if let Some(ref mut controller) = self.replay {
            let was_playing = controller.state == super::replay::PlaybackState::Playing;
            controller.tick();

            // Update stats if playing
            if was_playing {
                // Get current stats from replay
                let stats = controller.get_current_stats();
                let timestamp = controller.current_time;

                // Update time series
                for (job_id, ops) in &stats {
                    for (op_name, value) in ops {
                        self.time_series.insert(job_id, op_name, timestamp, *value);
                    }
                }

                // Update current stats
                self.current_stats = Some(TuiStats {
                    timestamp,
                    jobs: stats,
                    servers_queried: 0,
                });
            }
        }
    }

    /// Data collection task that runs in background
    async fn data_collector(
        config: Config,
        hosts_param: HashMap<String, Vec<String>>,
        processing_config: ProcessingConfig,
        tx: mpsc::Sender<DataMessage>,
        refresh_interval: Duration,
    ) {
        let config = Arc::new(config);
        // Create a stats processor for this collector (holds reference data for rate calc)
        let mut processor = StatsProcessor::new(processing_config);

        loop {
            // Collect stats from all servers in parallel
            let mut join_set = JoinSet::new();

            for (host, params) in &hosts_param {
                for param in params {
                    let host = host.clone();
                    let param = param.clone();
                    let cfg = Arc::clone(&config);

                    join_set.spawn(async move {
                        let result = ssh::get_stats(&host, &param, &cfg);
                        (host, param, result)
                    });
                }
            }

            // Collect raw data from all servers
            let mut raw_data = String::new();
            let mut servers_queried = 0;
            let mut errors = Vec::new();

            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok((_host, _param, Ok(stats_result))) => {
                        servers_queried += 1;
                        raw_data.push_str(&stats_result.data);
                        raw_data.push('\n');
                    }
                    Ok((host, _param, Err(e))) => {
                        errors.push(format!("{}: {}", host, e));
                    }
                    Err(e) => {
                        errors.push(format!("Task error: {}", e));
                    }
                }
            }

            // Send error if all servers failed
            if servers_queried == 0 && !errors.is_empty() {
                let _ = tx.send(DataMessage::Error(errors.join("; "))).await;
            } else {
                let timestamp = chrono::Utc::now().timestamp();

                // Use StatsProcessor for parsing and aggregation (includes groupby)
                let mut jobs: HashMap<String, HashMap<String, i64>> = HashMap::new();
                let mut timestamp_dict: HashMap<String, TimestampInfo> = HashMap::new();

                // Parse all jobs with groupby transformation
                let parsed_jobs = processor.parse_job_stats(&raw_data);
                for parsed_job in &parsed_jobs {
                    processor.merge_job(&mut jobs, parsed_job, &mut timestamp_dict);
                }

                // If rate mode, calculate rates
                let final_jobs = if processor.config.rate || processor.config.difference {
                    let rate_result =
                        processor.calculate_rates(&jobs, timestamp, &timestamp_dict);
                    if rate_result.is_first_sample {
                        // First sample - no rates yet, send empty to indicate waiting
                        HashMap::new()
                    } else {
                        rate_result.job_rates
                    }
                } else {
                    jobs
                };

                let stats = TuiStats {
                    timestamp,
                    jobs: final_jobs,
                    servers_queried,
                };

                if tx.send(DataMessage::Stats(stats)).await.is_err() {
                    break; // Receiver dropped
                }
            }

            tokio::time::sleep(refresh_interval).await;
        }
    }

    /// Update state from new stats
    fn update_from_stats(&mut self, stats: TuiStats) {
        let timestamp = stats.timestamp;

        // Store in time series
        for (job_id, ops) in &stats.jobs {
            for (op, &value) in ops {
                self.time_series.insert(job_id, op, timestamp, value);
            }
        }

        // Update current stats
        self.current_stats = Some(stats);
    }

    /// Called every tick
    fn on_tick(&mut self) {
        // Prune old time series data
        let cutoff = chrono::Utc::now().timestamp() - self.time_window.as_secs() as i64 * 2;
        self.time_series.prune_before(cutoff);

        // Clear old status messages
        if let Some((_, created)) = &self.status_message {
            if created.elapsed() > Duration::from_secs(5) {
                self.status_message = None;
            }
        }
    }

    /// Set a status message
    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some((msg, Instant::now()));
    }

    /// Get filtered and sorted top jobs
    pub fn get_filtered_top_jobs(&self) -> Vec<(String, i64)> {
        let Some(stats) = &self.current_stats else {
            return vec![];
        };

        let mut jobs: Vec<(String, i64)> = stats
            .jobs
            .iter()
            .filter(|(job_id, _)| {
                self.job_filter.matches(job_id) && self.job_passes_selection(job_id)
            })
            .map(|(job_id, ops)| {
                // Sum up filtered operations
                let total: i64 = ops
                    .iter()
                    .filter(|(op, _)| self.operation_filter.is_enabled(op))
                    .map(|(_, &v)| v)
                    .sum();
                (job_id.clone(), total)
            })
            .filter(|(_, total)| *total > 0)
            .collect();

        jobs.sort_by(|a, b| b.1.cmp(&a.1));
        jobs.truncate(self.top_n);
        jobs
    }

    /// Cycle through view modes
    pub fn cycle_view_mode(&mut self) {
        match self.view_mode {
            ViewMode::Full => {
                self.view_mode = ViewMode::GraphFocused;
                self.panels.left_panel = false;
                self.panels.right_panel = false;
                self.panels.bottom_panel = false;
            }
            ViewMode::GraphFocused => {
                self.view_mode = ViewMode::Full;
                self.panels.left_panel = true;
                self.panels.right_panel = true;
                self.panels.bottom_panel = true;
            }
            ViewMode::Custom => {
                self.view_mode = ViewMode::Full;
                self.panels.left_panel = true;
                self.panels.right_panel = true;
                self.panels.bottom_panel = true;
            }
        }
    }

    /// Adjust refresh interval
    pub fn adjust_refresh(&mut self, delta_secs: i64) {
        let current = self.refresh_interval.as_secs() as i64;
        let new_val = (current + delta_secs).clamp(1, 300);
        self.refresh_interval = Duration::from_secs(new_val as u64);
    }

    /// Adjust time window
    pub fn adjust_time_window(&mut self, delta_secs: i64) {
        let current = self.time_window.as_secs() as i64;
        let new_val = (current + delta_secs).clamp(30, 3600);
        self.time_window = Duration::from_secs(new_val as u64);
    }

    /// Adjust top N
    pub fn adjust_top_n(&mut self, delta: i32) {
        let new_val = (self.top_n as i32 + delta).clamp(1, 50);
        self.top_n = new_val as usize;
    }
}
