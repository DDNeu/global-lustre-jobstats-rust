//! Time-series graph widget using Braille markers for high resolution

use ratatui::{
    prelude::*,
    symbols::Marker,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, LegendPosition},
};

use crate::tui::app::TuiApp;
use crate::tui::time_series::{DataPoint, TimeSeriesStore};

/// Colors for different operations (public for use in other widgets)
pub const OPERATION_COLORS: &[(&str, Color)] = &[
    ("read", Color::Blue),
    ("write", Color::Red),
    ("open", Color::Green),
    ("close", Color::Yellow),
    ("getattr", Color::Cyan),
    ("setattr", Color::Magenta),
    ("punch", Color::LightRed),
    ("read_bytes", Color::LightBlue),
    ("write_bytes", Color::LightRed),
    ("statfs", Color::LightGreen),
    ("sync", Color::LightYellow),
    ("mkdir", Color::LightCyan),
    ("rmdir", Color::LightMagenta),
    ("unlink", Color::White),
    ("rename", Color::Gray),
];

/// Markers for different jobs (cycling through available markers)
pub const JOB_MARKERS: &[Marker] = &[
    Marker::Braille,
    Marker::Dot,
    Marker::Block,
    Marker::Bar,
    Marker::HalfBlock,
];

/// Get color for an operation (public for use in other widgets)
pub fn get_operation_color(operation: &str) -> Color {
    OPERATION_COLORS
        .iter()
        .find(|(op, _)| *op == operation)
        .map(|(_, color)| *color)
        .unwrap_or(Color::White)
}

/// Get marker for a job index (public for use in other widgets)
pub fn get_job_marker(job_idx: usize) -> Marker {
    JOB_MARKERS[job_idx % JOB_MARKERS.len()]
}

/// Get the character representation of a marker for display in legend/UI
pub fn marker_to_char(marker: Marker) -> char {
    match marker {
        Marker::Dot => '•',
        Marker::Block => '█',
        Marker::Bar => '▄',
        Marker::Braille => '⣿',
        Marker::HalfBlock => '▀',
    }
}

/// Render the time-series graph
pub fn render(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let now = chrono::Utc::now().timestamp();
    let window_start = now - app.time_window.as_secs() as i64;
    let time_window_secs = app.time_window.as_secs() as f64;

    // Clone enabled ops to avoid borrow issues
    let enabled_ops: Vec<String> = app
        .operation_filter
        .enabled_ops()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let enabled_ops_refs: Vec<&str> = enabled_ops.iter().map(|s| s.as_str()).collect();

    let top_jobs = app.get_filtered_top_jobs();

    let (datasets, max_value) = if app.graph_aggregate_mode {
        // Aggregated mode: one line per operation (summed across all jobs)
        build_aggregated_datasets(app, &enabled_ops_refs, window_start)
    } else {
        // Per-job mode: one line per (job, operation) pair
        build_per_job_datasets(app, &top_jobs, &enabled_ops_refs, window_start)
    };

    // Round up max value for nice axis labels
    let y_max = round_up_nice(max_value);

    // Create X axis labels
    let x_labels = vec![
        Span::raw(format!("-{}s", app.time_window.as_secs())),
        Span::raw(format!("-{}s", app.time_window.as_secs() / 2)),
        Span::raw("now"),
    ];

    // Create Y axis labels
    let y_labels = vec![
        Span::raw("0"),
        Span::raw(format_value(y_max / 2.0)),
        Span::raw(format_value(y_max)),
    ];

    let x_axis = Axis::default()
        .title("Time")
        .style(Style::default().fg(Color::Gray))
        .bounds([0.0, time_window_secs])
        .labels(x_labels);

    // Y-axis label depends on rate mode
    let y_axis_title = if app.graph_rate_mode { "ops/s" } else { "ops" };
    let y_axis = Axis::default()
        .title(y_axis_title)
        .style(Style::default().fg(Color::Gray))
        .bounds([0.0, y_max])
        .labels(y_labels);

    // Build title with mode indicators
    let title = build_title(app);

    let mut chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .x_axis(x_axis)
        .y_axis(y_axis);

    // Hide the in-graph legend if requested
    if app.graph_hide_legend {
        chart = chart.legend_position(None);
    } else {
        chart = chart.legend_position(Some(LegendPosition::TopRight));
    }

    f.render_widget(chart, area);
}

/// Build datasets for aggregated mode (one line per operation)
fn build_aggregated_datasets<'a>(
    app: &TuiApp,
    enabled_ops: &[&str],
    window_start: i64,
) -> (Vec<Dataset<'a>>, f64) {
    let mut datasets: Vec<Dataset> = Vec::new();
    let mut max_value: f64 = 100.0;

    for &op in enabled_ops {
        let series = app
            .time_series
            .get_aggregated_operation_series(op, window_start);

        if series.is_empty() {
            continue;
        }

        // Apply rate calculation if enabled
        let series = if app.graph_rate_mode {
            TimeSeriesStore::calculate_rate(&series)
        } else {
            series
        };

        if series.is_empty() {
            continue;
        }

        let data = convert_to_chart_data(&series, window_start, &mut max_value);
        let color = get_operation_color(op);

        datasets.push(
            Dataset::default()
                .name(op.to_string())
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(Box::leak(data.into_boxed_slice())),
        );
    }

    (datasets, max_value)
}

/// Build datasets for per-job mode (one line per job+operation pair)
fn build_per_job_datasets<'a>(
    app: &TuiApp,
    top_jobs: &[(String, i64)],
    enabled_ops: &[&str],
    window_start: i64,
) -> (Vec<Dataset<'a>>, f64) {
    let mut datasets: Vec<Dataset> = Vec::new();
    let mut max_value: f64 = 100.0;

    for (job_idx, (job_id, _)) in top_jobs.iter().enumerate() {
        let marker = get_job_marker(job_idx);

        for &op in enabled_ops {
            let series = app
                .time_series
                .get_job_operation_series(job_id, op, window_start);

            if series.is_empty() {
                continue;
            }

            // Apply rate calculation if enabled
            let series = if app.graph_rate_mode {
                TimeSeriesStore::calculate_rate(&series)
            } else {
                series
            };

            if series.is_empty() {
                continue;
            }

            let data = convert_to_chart_data(&series, window_start, &mut max_value);
            let color = get_operation_color(op);

            // Legend: "operation (job_id)"
            let name = format!("{} ({})", op, truncate_job_id(job_id, 12));

            datasets.push(
                Dataset::default()
                    .name(name)
                    .marker(marker)
                    .graph_type(GraphType::Line)
                    .style(Style::default().fg(color))
                    .data(Box::leak(data.into_boxed_slice())),
            );
        }
    }

    (datasets, max_value)
}

/// Convert DataPoint series to chart format
fn convert_to_chart_data(
    series: &[DataPoint],
    window_start: i64,
    max_value: &mut f64,
) -> Vec<(f64, f64)> {
    series
        .iter()
        .map(|p| {
            let x = (p.timestamp - window_start) as f64;
            let y = p.value as f64;
            *max_value = max_value.max(y);
            (x, y)
        })
        .collect()
}

/// Build the graph title with mode indicators
fn build_title(app: &TuiApp) -> String {
    let mut mode_parts = Vec::new();

    if app.graph_rate_mode {
        mode_parts.push("Rate");
    } else {
        mode_parts.push("Counter");
    }

    if app.graph_aggregate_mode {
        mode_parts.push("Aggregated");
    } else {
        mode_parts.push("Per-Job");
    }

    let mode_str = mode_parts.join(", ");

    format!(
        " Job Stats Graph ({}) [{}s window, {}s refresh, top {}] ",
        mode_str,
        app.time_window.as_secs(),
        app.refresh_interval.as_secs(),
        app.top_n
    )
}

/// Format a value for axis labels
fn format_value(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else {
        format!("{:.0}", v)
    }
}

/// Round up to a nice value for axis labels
fn round_up_nice(v: f64) -> f64 {
    if v <= 0.0 {
        return 100.0;
    }

    let magnitude = 10_f64.powf(v.log10().floor());
    let normalized = v / magnitude;

    let nice = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };

    nice * magnitude * 1.1 // Add 10% margin
}

/// Truncate job ID for display
fn truncate_job_id(job_id: &str, max_len: usize) -> String {
    if job_id.len() <= max_len {
        job_id.to_string()
    } else {
        format!("{}...", &job_id[..max_len - 3])
    }
}

