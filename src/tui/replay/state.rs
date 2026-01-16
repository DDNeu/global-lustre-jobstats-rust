//! Replay state management and playback controls

use std::collections::HashMap;
use std::time::Instant;

/// A single data record from replay data
#[derive(Debug, Clone)]
pub struct ReplayRecord {
    pub timestamp: i64,  // Unix timestamp in seconds
    pub job_id: String,
    pub operation: String,
    pub value: i64,
}

/// Container for all replay data
#[derive(Debug, Clone)]
pub struct ReplayData {
    /// All records sorted by timestamp
    pub records: Vec<ReplayRecord>,
    /// Time range of the data
    pub time_range: (i64, i64),
    /// All unique job IDs found
    pub job_ids: Vec<String>,
    /// All unique operations found
    pub operations: Vec<String>,
}

impl ReplayData {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            time_range: (0, 0),
            job_ids: Vec::new(),
            operations: Vec::new(),
        }
    }

    /// Build indexes after loading data
    pub fn finalize(&mut self) {
        if self.records.is_empty() {
            return;
        }

        // Sort by timestamp
        self.records.sort_by_key(|r| r.timestamp);

        // Calculate time range
        self.time_range = (
            self.records.first().unwrap().timestamp,
            self.records.last().unwrap().timestamp,
        );

        // Extract unique job IDs and operations
        let mut job_set = std::collections::HashSet::new();
        let mut op_set = std::collections::HashSet::new();
        for record in &self.records {
            job_set.insert(record.job_id.clone());
            op_set.insert(record.operation.clone());
        }
        self.job_ids = job_set.into_iter().collect();
        self.job_ids.sort();
        self.operations = op_set.into_iter().collect();
        self.operations.sort();
    }

    /// Get records within a time window
    #[allow(dead_code)]
    pub fn get_window(&self, start: i64, end: i64) -> Vec<&ReplayRecord> {
        self.records
            .iter()
            .filter(|r| r.timestamp >= start && r.timestamp <= end)
            .collect()
    }

    /// Get aggregated stats at a specific timestamp (or nearest before)
    /// Returns: job_id -> operation -> value
    pub fn get_stats_at(&self, timestamp: i64) -> HashMap<String, HashMap<String, i64>> {
        let mut result: HashMap<String, HashMap<String, i64>> = HashMap::new();

        // Find all records at or before this timestamp, keeping only the latest per job/op
        for record in &self.records {
            if record.timestamp > timestamp {
                break;
            }
            result
                .entry(record.job_id.clone())
                .or_default()
                .insert(record.operation.clone(), record.value);
        }

        result
    }

    /// Downsample data to fit a target number of points
    #[allow(dead_code)]
    pub fn downsample(&self, start: i64, end: i64, target_points: usize) -> Vec<ReplayRecord> {
        let window_records: Vec<_> = self.get_window(start, end);
        if window_records.len() <= target_points {
            return window_records.into_iter().cloned().collect();
        }

        // Calculate bucket size
        let duration = (end - start).max(1);
        let bucket_size = duration / target_points as i64;

        // Group by bucket and take last value per job/operation in each bucket
        let mut buckets: HashMap<i64, HashMap<(String, String), ReplayRecord>> = HashMap::new();

        for record in window_records {
            let bucket = (record.timestamp - start) / bucket_size;
            let key = (record.job_id.clone(), record.operation.clone());
            buckets.entry(bucket).or_default().insert(key, record.clone());
        }

        // Flatten buckets into records
        let mut result: Vec<ReplayRecord> = buckets
            .into_values()
            .flat_map(|bucket| bucket.into_values())
            .collect();
        result.sort_by_key(|r| r.timestamp);
        result
    }
}

impl Default for ReplayData {
    fn default() -> Self {
        Self::new()
    }
}

/// Playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

/// Replay controller managing playback
pub struct ReplayController {
    /// The loaded replay data
    pub data: ReplayData,
    /// Current playback state
    pub state: PlaybackState,
    /// Current virtual timestamp in the replay
    pub current_time: i64,
    /// Playback speed multiplier (1.0 = real-time, 2.0 = 2x, 0.5 = half speed)
    pub speed: f64,
    /// Time window to display
    #[allow(dead_code)]
    pub window_size: i64,
    /// Last real-world update time
    last_update: Instant,
}

impl ReplayController {
    pub fn new(data: ReplayData) -> Self {
        let start_time = data.time_range.0;
        Self {
            data,
            state: PlaybackState::Paused,
            current_time: start_time,
            speed: 1.0,
            window_size: 300, // 5 minutes default
            last_update: Instant::now(),
        }
    }

    /// Start or resume playback
    pub fn play(&mut self) {
        self.state = PlaybackState::Playing;
        self.last_update = Instant::now();
    }

    /// Pause playback
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }

    /// Toggle play/pause
    pub fn toggle_playback(&mut self) {
        match self.state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.play(),
            PlaybackState::Stopped => {
                self.current_time = self.data.time_range.0;
                self.play();
            }
        }
    }

    /// Stop playback and reset to start
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.current_time = self.data.time_range.0;
    }

    /// Seek to a specific timestamp
    pub fn seek(&mut self, timestamp: i64) {
        self.current_time = timestamp.clamp(self.data.time_range.0, self.data.time_range.1);
        self.last_update = Instant::now();
    }

    /// Seek by a relative amount
    pub fn seek_relative(&mut self, delta_secs: i64) {
        self.seek(self.current_time + delta_secs);
    }

    /// Jump to start
    pub fn jump_to_start(&mut self) {
        self.seek(self.data.time_range.0);
    }

    /// Jump to end
    pub fn jump_to_end(&mut self) {
        self.seek(self.data.time_range.1);
    }

    /// Increase playback speed
    pub fn speed_up(&mut self) {
        self.speed = (self.speed * 2.0).min(64.0);
    }

    /// Decrease playback speed
    pub fn slow_down(&mut self) {
        self.speed = (self.speed / 2.0).max(0.0625);
    }

    /// Update current time based on playback (call each tick)
    pub fn tick(&mut self) {
        if self.state != PlaybackState::Playing {
            return;
        }

        let elapsed = self.last_update.elapsed();
        self.last_update = Instant::now();

        // Advance virtual time by elapsed * speed
        let advance = (elapsed.as_secs_f64() * self.speed) as i64;
        self.current_time += advance;

        // Stop at end
        if self.current_time >= self.data.time_range.1 {
            self.current_time = self.data.time_range.1;
            self.state = PlaybackState::Paused;
        }
    }

    /// Get the current window start time
    #[allow(dead_code)]
    pub fn window_start(&self) -> i64 {
        (self.current_time - self.window_size).max(self.data.time_range.0)
    }

    /// Get the current window end time
    #[allow(dead_code)]
    pub fn window_end(&self) -> i64 {
        self.current_time
    }

    /// Get progress as a fraction (0.0 to 1.0)
    #[allow(dead_code)]
    pub fn progress(&self) -> f64 {
        let range = self.data.time_range.1 - self.data.time_range.0;
        if range == 0 {
            return 0.0;
        }
        let pos = self.current_time - self.data.time_range.0;
        (pos as f64 / range as f64).clamp(0.0, 1.0)
    }

    /// Format current timestamp as human-readable string
    pub fn current_time_str(&self) -> String {
        format_timestamp(self.current_time)
    }

    /// Get stats for the current window, suitable for TUI display
    pub fn get_current_stats(&self) -> HashMap<String, HashMap<String, i64>> {
        self.data.get_stats_at(self.current_time)
    }
}

/// Format a Unix timestamp as a human-readable string
fn format_timestamp(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| format!("{}", ts))
}

