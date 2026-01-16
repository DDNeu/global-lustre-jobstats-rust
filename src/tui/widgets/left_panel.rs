//! Left panel widget containing settings, operation filter, and job filter

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::tui::app::{FocusArea, HitRegion, HitRegionType, InputMode, TuiApp};
use crate::tui::widgets::graph::get_operation_color;

/// Render the left panel
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
        HitRegionType::PanelHeader("left".to_string()),
    ));

    // Split into three sections: settings, operation filter, job filter
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // Settings
            Constraint::Min(10),    // Operation filter
            Constraint::Length(8),  // Job filter
        ])
        .split(area);

    render_settings(f, chunks[0], app);
    render_operation_filter(f, chunks[1], app);
    render_job_filter(f, chunks[2], app);
}

/// Render settings section
fn render_settings(f: &mut Frame, area: Rect, app: &TuiApp) {
    let focused = app.focus == FocusArea::Settings;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    let text = vec![
        Line::from(vec![
            Span::raw("Refresh: "),
            Span::styled(
                format!("{}s", app.refresh_interval.as_secs()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" [/]"),
        ]),
        Line::from(vec![
            Span::raw("Window:  "),
            Span::styled(
                format!("{}s", app.time_window.as_secs()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" [+/-]"),
        ]),
        Line::from(vec![
            Span::raw("Top N:   "),
            Span::styled(
                format!("{}", app.top_n),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" [n/N]"),
        ]),
    ];

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

/// Render operation filter section
fn render_operation_filter(f: &mut Frame, area: Rect, app: &mut TuiApp) {
    let focused = app.focus == FocusArea::OperationFilter;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    // Calculate inner area for click regions
    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let ops_with_state = app.operation_filter.all_with_state();

    // Register click regions for each operation
    for (idx, (op, _)) in ops_with_state.iter().enumerate() {
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
    }

    let items: Vec<ListItem> = ops_with_state
        .iter()
        .enumerate()
        .map(|(i, (op, enabled))| {
            let checkbox = if *enabled { "[×]" } else { "[ ]" };
            let op_color = get_operation_color(op);

            // Build styled line: checkbox + operation name with operation color
            let is_selected = i == app.operation_filter.selected_idx && focused;

            let checkbox_style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if *enabled {
                Style::default().fg(op_color)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let op_style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(op_color).bold()
            } else if *enabled {
                Style::default().fg(op_color)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", checkbox), checkbox_style),
                Span::styled(op.to_string(), op_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title(" Operations [a/A] ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// Render job filter section
fn render_job_filter(f: &mut Frame, area: Rect, app: &TuiApp) {
    let focused = app.focus == FocusArea::JobIdFilter;
    let editing = app.input_mode == InputMode::Editing && focused;
    
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    let mut lines = vec![];

    // Full filter line
    let full_style = if app.job_filter.editing_full && editing {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::raw("Full: "),
        Span::styled(
            if app.job_filter.full_filter.is_empty() {
                "_____________".to_string()
            } else {
                format!("{}_", app.job_filter.full_filter)
            },
            full_style,
        ),
    ]));

    // Component filters
    for (i, name) in app.job_filter.component_names.iter().enumerate() {
        let value = app
            .job_filter
            .component_filters
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or("");
        
        let is_selected = !app.job_filter.editing_full 
            && app.job_filter.selected_component == i 
            && editing;
        
        let style = if is_selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::raw(format!("{}: ", name)),
            Span::styled(
                if value.is_empty() {
                    "________".to_string()
                } else {
                    format!("{}_", value)
                },
                style,
            ),
        ]));
    }

    let title = if editing {
        " Job Filter [Enter] "
    } else {
        " Job Filter [/] "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

