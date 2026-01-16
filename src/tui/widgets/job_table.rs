//! Top jobs table widget

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Row, Table},
};

use crate::tui::app::{FocusArea, HitRegion, HitRegionType, SelectionMode, TuiApp};

/// Render the top jobs table
pub fn render(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let focused = app.focus == FocusArea::TopJobsTable;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    // Register panel header for click to collapse
    let header_rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    app.click_regions.add(HitRegion::new(
        header_rect,
        HitRegionType::PanelHeader("bottom".to_string()),
    ));

    // Calculate inner area for row click regions (border + header + margin)
    let inner_start_y = area.y + 3; // 1 border + 1 header + 1 margin
    let inner_area = Rect {
        x: area.x + 1,
        y: inner_start_y,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(4),
    };

    // Get operation columns to display
    let enabled_ops = app.operation_filter.enabled_ops();
    let display_ops: Vec<&str> = enabled_ops.iter().take(6).copied().collect(); // Limit columns

    // Build header
    let mut header_cells = vec![Cell::from("Job ID").style(Style::default().bold())];
    header_cells.push(Cell::from("Total").style(Style::default().bold()));
    for op in &display_ops {
        header_cells.push(Cell::from(*op).style(Style::default().bold()));
    }
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    // Collect job data first to register click regions
    let job_data: Vec<(String, std::collections::HashMap<String, i64>, i64)> =
        if let Some(stats) = &app.current_stats {
            let mut jobs: Vec<_> = stats
                .jobs
                .iter()
                .filter(|(job_id, _)| {
                    app.job_filter.matches(job_id) && app.job_passes_selection(job_id)
                })
                .map(|(job_id, ops)| {
                    let total: i64 = ops
                        .iter()
                        .filter(|(op, _)| app.operation_filter.is_enabled(op))
                        .map(|(_, &v)| v)
                        .sum();
                    (job_id.clone(), ops.clone(), total)
                })
                .filter(|(_, _, total)| *total > 0)
                .collect();

            jobs.sort_by(|a, b| b.2.cmp(&a.2));
            jobs.truncate(app.top_n);
            jobs
        } else {
            vec![]
        };

    // Register click regions for each visible row
    for (idx, (job_id, _, _)) in job_data.iter().enumerate().skip(app.table_scroll) {
        let row_idx = idx - app.table_scroll;
        if row_idx < inner_area.height as usize {
            let row_rect = Rect {
                x: inner_area.x,
                y: inner_area.y + row_idx as u16,
                width: inner_area.width,
                height: 1,
            };
            app.click_regions.add(HitRegion::new(
                row_rect,
                HitRegionType::TableJob(job_id.clone()),
            ));
        }
    }

    // Build rows
    let rows: Vec<Row> = if job_data.is_empty() && app.current_stats.is_none() {
        vec![Row::new(vec![Cell::from("No data yet...")])]
    } else if job_data.is_empty() {
        vec![Row::new(vec![Cell::from("No matching jobs")])]
    } else {
        job_data
            .iter()
            .enumerate()
            .skip(app.table_scroll)
            .map(|(i, (job_id, ops, total))| {
                let color = app
                    .job_color_map
                    .get_assigned_color(job_id)
                    .unwrap_or(Color::White);

                let is_selected = app.is_job_selected(job_id);

                // Selection indicator
                let job_display = if is_selected {
                    format!("◆ {}", truncate_job_id(job_id, 20))
                } else {
                    format!("  {}", truncate_job_id(job_id, 20))
                };

                let job_style = if is_selected {
                    Style::default().fg(color).bold()
                } else {
                    Style::default().fg(color)
                };

                let mut cells = vec![
                    Cell::from(job_display).style(job_style),
                    Cell::from(format_value(*total)),
                ];

                for op in &display_ops {
                    let value = ops.get(*op).copied().unwrap_or(0);
                    cells.push(Cell::from(format_value(value)));
                }

                let style = if focused && i == app.table_scroll {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Row::new(cells).style(style)
            })
            .collect()
    };

    // Calculate column widths
    let mut widths = vec![Constraint::Length(24), Constraint::Length(10)];
    for _ in &display_ops {
        widths.push(Constraint::Length(10));
    }

    // Show selection info in title
    let title = match (app.selection_mode, app.selected_jobs.len()) {
        (SelectionMode::None, _) => " Top Jobs ".to_string(),
        (SelectionMode::Inclusive, n) => format!(" Top Jobs [+{}] ", n),
        (SelectionMode::Exclusive, n) => format!(" Top Jobs [-{}] ", n),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(Style::default().bg(Color::DarkGray));

    f.render_widget(table, area);
}

/// Format a value for table display
fn format_value(v: i64) -> String {
    if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        v.to_string()
    }
}

/// Truncate job ID for table display
fn truncate_job_id(job_id: &str, max_len: usize) -> String {
    if job_id.len() <= max_len {
        job_id.to_string()
    } else {
        format!("{}...", &job_id[..max_len - 3])
    }
}

