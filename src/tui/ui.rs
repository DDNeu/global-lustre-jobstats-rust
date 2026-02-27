//! Main UI rendering and layout

use ratatui::prelude::*;

use super::app::TuiApp;
use super::widgets;

/// Main render function
pub fn render(f: &mut Frame, app: &mut TuiApp) {
    let area = f.area();

    // Clear clickable regions at start of each frame
    app.click_regions.clear();

    // Main layout: content area + status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(1)])
        .split(area);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    // Render content based on panel visibility
    render_content(f, content_area, app);

    // Render status bar
    widgets::status_bar::render(f, status_area, app);

    // Render help overlay if active
    if app.show_help {
        widgets::help::render(f, area, app);
    }
}

/// Render the main content area with dynamic layout
fn render_content(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    // Calculate panel widths based on visibility
    let left_width = if app.panels.left_panel { 28 } else { 0 };
    let right_width = if app.panels.right_panel { 22 } else { 0 };
    let bottom_height = if app.panels.bottom_panel { 12 } else { 0 };

    // Build horizontal constraints
    let h_constraints = if left_width > 0 && right_width > 0 {
        vec![
            Constraint::Length(left_width),
            Constraint::Min(40),
            Constraint::Length(right_width),
        ]
    } else if left_width > 0 {
        vec![Constraint::Length(left_width), Constraint::Min(40)]
    } else if right_width > 0 {
        vec![Constraint::Min(40), Constraint::Length(right_width)]
    } else {
        vec![Constraint::Min(40)]
    };

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(h_constraints)
        .split(area);

    // Determine which chunk is which
    let (left_area, center_area, right_area) = if left_width > 0 && right_width > 0 {
        (Some(h_chunks[0]), h_chunks[1], Some(h_chunks[2]))
    } else if left_width > 0 {
        (Some(h_chunks[0]), h_chunks[1], None)
    } else if right_width > 0 {
        (None, h_chunks[0], Some(h_chunks[1]))
    } else {
        (None, h_chunks[0], None)
    };

    // Split center for graph + bottom panel
    let v_constraints = if bottom_height > 0 {
        vec![Constraint::Min(10), Constraint::Length(bottom_height)]
    } else {
        vec![Constraint::Min(10)]
    };

    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(v_constraints)
        .split(center_area);

    let graph_area = v_chunks[0];
    let bottom_area = if bottom_height > 0 {
        Some(v_chunks[1])
    } else {
        None
    };

    // Render panels
    if let Some(area) = left_area {
        widgets::left_panel::render(f, area, app);
    }

    match app.graph_kind {
        super::app::GraphKind::TimeSeries => widgets::graph::render(f, graph_area, app),
        super::app::GraphKind::BarChart => widgets::bar_chart::render(f, graph_area, app),
    }

    if let Some(area) = right_area {
        widgets::legend::render(f, area, app);
    }

    if let Some(area) = bottom_area {
        widgets::job_table::render(f, area, app);
    }
}

