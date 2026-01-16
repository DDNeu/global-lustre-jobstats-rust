//! Status bar widget with keyboard hints

use ratatui::{prelude::*, widgets::Paragraph};

use crate::tui::app::{InputMode, SelectionMode, TuiApp};
use crate::tui::replay::PlaybackState;

/// Render the status bar
pub fn render(f: &mut Frame, area: Rect, app: &TuiApp) {
    // Check if we're in replay mode
    if app.is_replay_mode() {
        render_replay_status(f, area, app);
    } else {
        render_live_status(f, area, app);
    }
}

/// Render status bar for live mode
fn render_live_status(f: &mut Frame, area: Rect, app: &TuiApp) {
    let mode_indicator = match app.input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Editing => "EDITING",
    };

    let hints = if app.panels.all_hidden() {
        "[h] show panels  [?] help  [q] quit  [+/-] zoom  [n/N] top-n"
    } else {
        "[?] help  [q] quit  [Click] select job  [Ctrl+Click] multi-select"
    };

    let status_msg = app
        .status_message
        .as_ref()
        .map(|(msg, _)| msg.as_str())
        .unwrap_or("");

    // Selection info
    let selection_info = match app.selection_mode {
        SelectionMode::None => String::new(),
        SelectionMode::Inclusive => format!(" | Selected: {} (show only)", app.selected_jobs.len()),
        SelectionMode::Exclusive => format!(" | Excluded: {} (hide)", app.selected_jobs.len()),
    };

    let data_info = if let Some(stats) = &app.current_stats {
        format!(
            "Jobs: {} | Servers: {} | {}{}",
            stats.jobs.len(),
            stats.servers_queried,
            format_timestamp(stats.timestamp),
            selection_info
        )
    } else {
        format!("Waiting for data...{}", selection_info)
    };

    let content = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_indicator),
            Style::default().bg(Color::Blue).fg(Color::White),
        ),
        Span::raw(" "),
        Span::styled(data_info, Style::default().fg(Color::Gray)),
        Span::raw(" | "),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
        if !status_msg.is_empty() {
            Span::styled(
                format!(" | {} ", status_msg),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::raw("")
        },
    ]);

    let paragraph = Paragraph::new(content);
    f.render_widget(paragraph, area);
}

/// Render status bar for replay mode
fn render_replay_status(f: &mut Frame, area: Rect, app: &TuiApp) {
    let Some(ref controller) = app.replay else {
        return;
    };

    // Playback state indicator
    let (state_icon, state_color) = match controller.state {
        PlaybackState::Playing => ("▶ PLAY", Color::Green),
        PlaybackState::Paused => ("⏸ PAUSE", Color::Yellow),
        PlaybackState::Stopped => ("⏹ STOP", Color::Red),
    };

    // Speed indicator
    let speed_str = format!("{}x", controller.speed);

    // Time info
    let current_time = controller.current_time_str();
    let (start, end) = controller.data.time_range;
    let duration = end - start;
    let position = controller.current_time - start;
    let progress_pct = if duration > 0 {
        (position as f64 / duration as f64 * 100.0) as u32
    } else {
        0
    };

    // Progress bar (simple text-based)
    let progress_width = 20;
    let filled = (progress_pct as usize * progress_width / 100).min(progress_width);
    let progress_bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(progress_width - filled)
    );

    // Hints for replay mode
    let hints = "[Space] play/pause  [←/→] seek  [</>] speed  [Home/End] jump  [q] quit";

    let status_msg = app
        .status_message
        .as_ref()
        .map(|(msg, _)| msg.as_str())
        .unwrap_or("");

    let content = Line::from(vec![
        Span::styled(
            format!(" {} ", state_icon),
            Style::default().bg(state_color).fg(Color::Black),
        ),
        Span::raw(" "),
        Span::styled(
            format!(" {} ", speed_str),
            Style::default().bg(Color::DarkGray).fg(Color::White),
        ),
        Span::raw(" "),
        Span::styled(current_time, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled(progress_bar, Style::default().fg(Color::Blue)),
        Span::styled(format!(" {}%", progress_pct), Style::default().fg(Color::Gray)),
        Span::raw(" | "),
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
        if !status_msg.is_empty() {
            Span::styled(
                format!(" | {} ", status_msg),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::raw("")
        },
    ]);

    let paragraph = Paragraph::new(content);
    f.render_widget(paragraph, area);
}

/// Format timestamp for display
fn format_timestamp(ts: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "?".to_string())
}

