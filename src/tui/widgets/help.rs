//! Help overlay widget

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::app::TuiApp;

/// Render the help overlay
pub fn render(f: &mut Frame, area: Rect, app: &TuiApp) {
    // Calculate centered popup area
    let popup_area = centered_rect(60, 70, area);

    // Clear the area behind the popup
    f.render_widget(Clear, popup_area);

    let help_text = if app.is_replay_mode() {
        build_replay_help()
    } else {
        build_live_help()
    };

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, popup_area);
}

/// Build help text for live mode
fn build_live_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Keyboard Shortcuts",
            Style::default().bold().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled("Panel Visibility", Style::default().bold())),
        Line::from("  h, F11     Toggle all panels (full-screen graph)"),
        Line::from("  1/2/3      Toggle left/right/bottom panel"),
        Line::from("  v          Cycle view modes"),
        Line::from(""),
        Line::from(Span::styled("Navigation", Style::default().bold())),
        Line::from("  Tab        Move focus to next panel"),
        Line::from("  ↑/↓ or j/k Navigate within panel"),
        Line::from("  Enter      Select/toggle item"),
        Line::from("  Esc        Cancel editing"),
        Line::from(""),
        Line::from(Span::styled("Settings", Style::default().bold())),
        Line::from("  +/-        Adjust time window (±30s)"),
        Line::from("  [/]        Adjust refresh interval (±1s)"),
        Line::from("  n/N        Adjust top N jobs (+1/-1)"),
        Line::from(""),
        Line::from(Span::styled("Graph Modes", Style::default().bold())),
        Line::from("  g          Toggle aggregate/per-job mode"),
        Line::from("  r          Toggle rate/counter display"),
        Line::from("  l          Toggle in-graph legend"),
        Line::from(""),
        Line::from(Span::styled("Filtering", Style::default().bold())),
        Line::from("  /          Edit job filter"),
        Line::from("  c          Clear all filters & selection"),
        Line::from("  a/A        Enable/disable all operations (in filter panel)"),
        Line::from(""),
        Line::from(Span::styled(
            "Mouse Support",
            Style::default().bold().fg(Color::Green),
        )),
        Line::from("  Click      Select/toggle job or operation"),
        Line::from("  Ctrl+Click Add to selection (multi-select)"),
        Line::from("  Shift+Click Select only this job"),
        Line::from("  Right-Click Clear job selection"),
        Line::from("  Scroll     Navigate within focused panel"),
        Line::from(""),
        Line::from(Span::styled("Legend: ", Style::default().bold())),
        Line::from("  [+] = Show only selected jobs"),
        Line::from("  [-] = Hide selected jobs"),
        Line::from(""),
        Line::from(Span::styled("General", Style::default().bold())),
        Line::from("  ?/F1  Help | q/Ctrl+C  Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or ? to close",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

/// Build help text for replay mode
fn build_replay_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Replay Mode - Keyboard Shortcuts",
            Style::default().bold().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Playback Controls",
            Style::default().bold().fg(Color::Green),
        )),
        Line::from("  Space/p    Play/Pause playback"),
        Line::from("  o          Stop and reset to start"),
        Line::from("  ←/→        Seek backward/forward 10 seconds"),
        Line::from("  PgUp/PgDn  Seek backward/forward 60 seconds"),
        Line::from("  Home       Jump to start"),
        Line::from("  End        Jump to end"),
        Line::from("  </,        Slow down playback (0.5x)"),
        Line::from("  >/.        Speed up playback (2x)"),
        Line::from(""),
        Line::from(Span::styled("Panel Visibility", Style::default().bold())),
        Line::from("  h, F11     Toggle all panels (full-screen graph)"),
        Line::from("  1/2/3      Toggle left/right/bottom panel"),
        Line::from("  v          Cycle view modes"),
        Line::from(""),
        Line::from(Span::styled("Graph Modes", Style::default().bold())),
        Line::from("  g          Toggle aggregate/per-job mode"),
        Line::from("  r          Toggle rate/counter display"),
        Line::from("  l          Toggle in-graph legend"),
        Line::from(""),
        Line::from(Span::styled("Filtering", Style::default().bold())),
        Line::from("  /          Edit job filter"),
        Line::from("  c          Clear all filters & selection"),
        Line::from(""),
        Line::from(Span::styled("General", Style::default().bold())),
        Line::from("  ?/F1  Help | q/Ctrl+C  Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or ? to close",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

