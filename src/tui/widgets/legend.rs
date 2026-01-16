//! Legend widget showing job colors and markers

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use crate::tui::app::{HitRegion, HitRegionType, SelectionMode, TuiApp};
use crate::tui::widgets::graph::{get_job_marker, get_operation_color, marker_to_char};

/// Render the legend panel
pub fn render(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Register panel header for click to collapse
    let header_rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    app.click_regions.add(HitRegion::new(
        header_rect,
        HitRegionType::PanelHeader("right".to_string()),
    ));

    // Render based on graph mode
    if app.graph_aggregate_mode {
        render_aggregated_legend(f, area, app);
    } else {
        render_per_job_legend(f, area, app);
    }
}

/// Render legend for aggregated mode (one entry per operation)
fn render_aggregated_legend(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let enabled_ops = app.operation_filter.enabled_ops();
    let braille_char = marker_to_char(ratatui::symbols::Marker::Braille);

    // Calculate the inner area (after border)
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let items: Vec<ListItem> = enabled_ops
        .iter()
        .enumerate()
        .map(|(idx, op)| {
            let color = get_operation_color(op);

            // Register clickable region for this item (toggle operation)
            if idx < inner_area.height as usize {
                let item_rect = Rect {
                    x: inner_area.x,
                    y: inner_area.y + idx as u16,
                    width: inner_area.width,
                    height: 1,
                };
                app.click_regions.add(HitRegion::new(
                    item_rect,
                    HitRegionType::OperationFilter(op.to_string()),
                ));
            }

            let text = Line::from(vec![
                Span::styled(format!("{} ", braille_char), Style::default().fg(color)),
                Span::styled(op.to_string(), Style::default().fg(color)),
            ]);

            ListItem::new(text)
        })
        .collect();

    let title = format!(" Legend ({} ops) ", items.len());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Gray));

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Render legend for per-job mode (one entry per job with job-specific marker)
fn render_per_job_legend(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let top_jobs = app.get_filtered_top_jobs();

    // Calculate the inner area (after border)
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Build items and register click regions
    let items: Vec<ListItem> = top_jobs
        .iter()
        .enumerate()
        .map(|(idx, (job_id, _total))| {
            // Get the marker for this job index (same as in graph)
            let marker = get_job_marker(idx);
            let marker_char = marker_to_char(marker);

            let color = app
                .job_color_map
                .get_assigned_color(job_id)
                .unwrap_or(Color::White);

            let is_selected = app.is_job_selected(job_id);

            // Register clickable region for this item
            if idx < inner_area.height as usize {
                let item_rect = Rect {
                    x: inner_area.x,
                    y: inner_area.y + idx as u16,
                    width: inner_area.width,
                    height: 1,
                };
                app.click_regions.add(HitRegion::new(
                    item_rect,
                    HitRegionType::LegendJob(job_id.clone()),
                ));
            }

            // Show marker with selection indicator
            let (prefix, prefix_style) = if is_selected {
                (format!("◆{} ", marker_char), Style::default().fg(Color::White).bold())
            } else {
                (format!("{} ", marker_char), Style::default().fg(color))
            };

            let text_style = if is_selected {
                Style::default().fg(color).bold().underlined()
            } else {
                Style::default()
            };

            let text = Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(truncate_job_id(job_id, 14), text_style),
            ]);

            ListItem::new(text)
        })
        .collect();

    // Show selection mode in title if active
    let title = match app.selection_mode {
        SelectionMode::None => format!(" Legend ({}) ", items.len()),
        SelectionMode::Inclusive => format!(" Legend ({}) [+] ", items.len()),
        SelectionMode::Exclusive => format!(" Legend ({}) [-] ", items.len()),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Gray));

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Truncate job ID for legend display
fn truncate_job_id(job_id: &str, max_len: usize) -> String {
    if job_id.len() <= max_len {
        job_id.to_string()
    } else {
        format!("{}...", &job_id[..max_len - 3])
    }
}
