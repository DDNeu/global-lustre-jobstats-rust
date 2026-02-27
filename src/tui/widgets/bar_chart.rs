//! Bar chart widget showing current operation values

use ratatui::{
    prelude::*,
    widgets::{Bar, BarChart, BarGroup, Block, Borders},
};

use crate::op_keys::OP_KEYS_REV;
use crate::tui::app::TuiApp;
use crate::tui::time_series::TimeSeriesStore;
use crate::tui::widgets::graph::get_operation_color;

/// Render the bar chart in the graph area
pub fn render(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    if app.graph_aggregate_mode {
        render_aggregated(f, area, app);
    } else {
        render_per_job(f, area, app);
    }
}

/// Render aggregated mode: one bar per operation, summed across all visible jobs
fn render_aggregated(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let Some(ref stats) = app.current_stats else {
        render_empty(f, area);
        return;
    };

    let enabled_ops: Vec<String> = app
        .operation_filter
        .enabled_ops()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    if enabled_ops.is_empty() {
        render_empty(f, area);
        return;
    }

    // Compute values per operation
    let op_values: Vec<(String, i64)> = if app.graph_rate_mode {
        compute_rate_values_aggregated(app, &enabled_ops)
    } else {
        enabled_ops
            .iter()
            .map(|op| {
                let total: i64 = stats
                    .jobs
                    .iter()
                    .filter(|(job_id, _)| {
                        app.job_filter.matches(job_id) && app.job_passes_selection(job_id)
                    })
                    .map(|(_, ops)| ops.get(op.as_str()).copied().unwrap_or(0))
                    .sum();
                (op.clone(), total)
            })
            .collect()
    };

    let use_log = app.bar_chart_log_scale;

    let bars: Vec<Bar> = op_values
        .iter()
        .map(|(op, value)| {
            let display_value = to_display_value(*value, use_log);
            let color = get_operation_color(op);
            Bar::default()
                .value(display_value)
                .label(Line::from(short_op_label(op)))
                .text_value(format_value(*value))
                .style(Style::default().fg(color))
                .value_style(Style::default().fg(Color::White).bold())
        })
        .collect();

    let title = build_title(app);
    let bar_width = compute_bar_width(area.width, bars.len());

    let chart = BarChart::default()
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(bar_width)
        .bar_gap(1)
        .value_style(Style::default().fg(Color::White).bold())
        .label_style(Style::default().fg(Color::Gray));

    f.render_widget(chart, area);
}

/// Render per-job mode: one bar group per operation, one bar per job in each group
fn render_per_job(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let Some(ref stats) = app.current_stats else {
        render_empty(f, area);
        return;
    };

    let enabled_ops: Vec<String> = app
        .operation_filter
        .enabled_ops()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let top_jobs = app.get_filtered_top_jobs();

    if enabled_ops.is_empty() || top_jobs.is_empty() {
        render_empty(f, area);
        return;
    }

    let use_log = app.bar_chart_log_scale;

    // Build one BarGroup per operation
    let groups: Vec<BarGroup> = enabled_ops
        .iter()
        .map(|op| {
            let bars: Vec<Bar> = top_jobs
                .iter()
                .map(|(job_id, _)| {
                    let value = if app.graph_rate_mode {
                        compute_rate_value_for_job(app, job_id, op)
                    } else {
                        stats
                            .jobs
                            .get(job_id)
                            .and_then(|ops| ops.get(op.as_str()))
                            .copied()
                            .unwrap_or(0)
                    };

                    let display_value = to_display_value(value, use_log);
                    let color = get_operation_color(op);

                    Bar::default()
                        .value(display_value)
                        .text_value(format_value(value))
                        .style(Style::default().fg(color))
                        .value_style(Style::default().fg(Color::White).bold())
                })
                .collect();

            BarGroup::default()
                .label(Line::from(short_op_label(op)))
                .bars(&bars)
        })
        .collect();

    let title = build_title(app);
    let bar_width = compute_grouped_bar_width(area.width, groups.len(), top_jobs.len());

    let mut chart = BarChart::default()
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .bar_width(bar_width)
        .bar_gap(0)
        .group_gap(2)
        .value_style(Style::default().fg(Color::White).bold())
        .label_style(Style::default().fg(Color::Gray));

    for group in groups {
        chart = chart.data(group);
    }

    f.render_widget(chart, area);
}

/// Convert a raw value to a display value for bar height
fn to_display_value(value: i64, use_log: bool) -> u64 {
    if use_log && value > 0 {
        ((value as f64 + 1.0).log10() * 1000.0) as u64
    } else {
        value.max(0) as u64
    }
}

/// Compute rate values for aggregated mode from time series
fn compute_rate_values_aggregated(app: &TuiApp, enabled_ops: &[String]) -> Vec<(String, i64)> {
    let now = chrono::Utc::now().timestamp();
    let window_start = now - 60;

    enabled_ops
        .iter()
        .map(|op| {
            let series = app
                .time_series
                .get_aggregated_operation_series(op, window_start);
            let rate_series = TimeSeriesStore::calculate_rate(&series);
            let value = rate_series.last().map(|p| p.value).unwrap_or(0);
            (op.clone(), value)
        })
        .collect()
}

/// Compute rate value for a single job/operation pair
fn compute_rate_value_for_job(app: &TuiApp, job_id: &str, operation: &str) -> i64 {
    let now = chrono::Utc::now().timestamp();
    let window_start = now - 60;
    let series = app
        .time_series
        .get_job_operation_series(job_id, operation, window_start);
    let rate_series = TimeSeriesStore::calculate_rate(&series);
    rate_series.last().map(|p| p.value).unwrap_or(0)
}

/// Build the bar chart title
fn build_title(app: &TuiApp) -> String {
    let rate_mode = if app.graph_rate_mode {
        "Rate"
    } else {
        "Counter"
    };

    let agg_mode = if app.graph_aggregate_mode {
        "Aggregated"
    } else {
        "Per-Job"
    };

    let scale = if app.bar_chart_log_scale {
        "Log"
    } else {
        "Linear"
    };

    format!(
        " Operation Bar Chart ({}, {}, {}) [top {}] ",
        rate_mode, agg_mode, scale, app.top_n
    )
}

/// Render an empty bar chart with a placeholder
fn render_empty(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Operation Bar Chart (no data) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    f.render_widget(block, area);
}

/// Get a short label for an operation (use 2-char abbreviation)
fn short_op_label(op: &str) -> String {
    OP_KEYS_REV
        .get(op)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if op.len() > 4 {
                op[..4].to_string()
            } else {
                op.to_string()
            }
        })
}

/// Format value for display on/above bars
fn format_value(v: i64) -> String {
    if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        v.to_string()
    }
}

/// Compute bar width so bars + gaps fill the entire available width.
/// Layout: num_bars * bar_width + (num_bars - 1) * bar_gap = available
/// With bar_gap = 1: bar_width = (available - (num_bars - 1)) / num_bars
fn compute_bar_width(area_width: u16, num_bars: usize) -> u16 {
    if num_bars == 0 {
        return 3;
    }
    let available = area_width.saturating_sub(2) as usize; // borders
    let gaps = num_bars.saturating_sub(1); // bar_gap = 1 between bars
    let for_bars = available.saturating_sub(gaps);
    let width = for_bars / num_bars;
    (width as u16).max(1)
}

/// Compute bar width for grouped mode so groups fill the available width.
/// Layout: total_bars * bar_width + (num_groups - 1) * group_gap = available
/// With bar_gap = 0 within groups and group_gap = 2 between groups.
fn compute_grouped_bar_width(area_width: u16, num_groups: usize, bars_per_group: usize) -> u16 {
    if num_groups == 0 || bars_per_group == 0 {
        return 2;
    }
    let available = area_width.saturating_sub(2) as usize; // borders
    let group_gaps = num_groups.saturating_sub(1) * 2; // group_gap = 2
    let total_bars = num_groups * bars_per_group;
    let for_bars = available.saturating_sub(group_gaps);
    let width = for_bars / total_bars;
    (width as u16).max(1)
}
