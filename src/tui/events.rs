//! Keyboard and mouse event handling

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::app::{FocusArea, HitRegionType, InputMode, TuiApp, ViewMode};

/// Handle a key event
pub fn handle_key_event(app: &mut TuiApp, key: KeyEvent) {
    // Check for quit shortcuts first (always active)
    match key.code {
        KeyCode::Char('q') if app.input_mode == InputMode::Normal => {
            app.should_quit = true;
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return;
        }
        KeyCode::Esc if app.input_mode == InputMode::Editing => {
            app.input_mode = InputMode::Normal;
            return;
        }
        KeyCode::Esc if app.show_help => {
            app.show_help = false;
            return;
        }
        _ => {}
    }

    // Handle input mode separately
    if app.input_mode == InputMode::Editing {
        handle_editing_mode(app, key);
        return;
    }

    // Handle replay-specific keys first
    if app.is_replay_mode() {
        if handle_replay_key(app, key) {
            return;
        }
    }

    // Normal mode key handling
    match key.code {
        // Help toggle
        KeyCode::Char('?') | KeyCode::F(1) => {
            app.show_help = !app.show_help;
        }

        // Panel visibility toggles
        KeyCode::Char('h') | KeyCode::F(11) => {
            app.panels.toggle_all();
            app.view_mode = if app.panels.all_hidden() {
                ViewMode::GraphFocused
            } else {
                ViewMode::Full
            };
        }
        KeyCode::Char('1') => {
            app.panels.left_panel = !app.panels.left_panel;
            app.view_mode = ViewMode::Custom;
        }
        KeyCode::Char('2') => {
            app.panels.right_panel = !app.panels.right_panel;
            app.view_mode = ViewMode::Custom;
        }
        KeyCode::Char('3') => {
            app.panels.bottom_panel = !app.panels.bottom_panel;
            app.view_mode = ViewMode::Custom;
        }
        KeyCode::Char('v') => {
            app.cycle_view_mode();
        }

        // Focus switching
        KeyCode::Tab => {
            app.focus = next_focus(app.focus, &app.panels);
        }
        KeyCode::BackTab => {
            app.focus = prev_focus(app.focus, &app.panels);
        }

        // Navigation and actions based on focus
        KeyCode::Up | KeyCode::Char('k') => {
            handle_up(app);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            handle_down(app);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            handle_select(app);
        }

        // Settings adjustments (when in settings or graph focus)
        KeyCode::Char('+') | KeyCode::Char('=') => {
            match app.focus {
                FocusArea::Graph | FocusArea::Settings => {
                    app.adjust_time_window(30);
                }
                _ => {}
            }
        }
        KeyCode::Char('-') => {
            match app.focus {
                FocusArea::Graph | FocusArea::Settings => {
                    app.adjust_time_window(-30);
                }
                _ => {}
            }
        }
        KeyCode::Char('[') => {
            app.adjust_refresh(-1);
        }
        KeyCode::Char(']') => {
            app.adjust_refresh(1);
        }
        KeyCode::Char('n') => {
            app.adjust_top_n(1);
        }
        KeyCode::Char('N') => {
            app.adjust_top_n(-1);
        }

        // Clear filters and selection
        KeyCode::Char('c') => {
            app.job_filter.clear();
            app.clear_job_selection();
            app.set_status("Filters and selection cleared".to_string());
        }

        // Toggle selection mode (when jobs are selected)
        KeyCode::Char('s') => {
            if !app.selected_jobs.is_empty() {
                app.cycle_selection_mode();
                let mode_str = match app.selection_mode {
                    crate::tui::app::SelectionMode::None => "Selection disabled",
                    crate::tui::app::SelectionMode::Inclusive => "Showing only selected",
                    crate::tui::app::SelectionMode::Exclusive => "Hiding selected",
                };
                app.set_status(mode_str.to_string());
            }
        }

        // Enable/disable all operations
        KeyCode::Char('a') if app.focus == FocusArea::OperationFilter => {
            app.operation_filter.enable_all();
        }
        KeyCode::Char('A') if app.focus == FocusArea::OperationFilter => {
            app.operation_filter.disable_all();
        }

        // Graph mode toggles (when not in OperationFilter focus)
        KeyCode::Char('g') => {
            app.toggle_graph_aggregate_mode();
            let mode = if app.graph_aggregate_mode {
                "Aggregated"
            } else {
                "Per-Job"
            };
            app.set_status(format!("Graph mode: {}", mode));
        }
        KeyCode::Char('r') => {
            app.toggle_graph_rate_mode();
            let mode = if app.graph_rate_mode {
                "Rate (ops/s)"
            } else {
                "Counter (raw)"
            };
            app.set_status(format!("Display mode: {}", mode));
        }
        KeyCode::Char('l') => {
            app.toggle_graph_legend();
            let state = if app.graph_hide_legend {
                "hidden"
            } else {
                "visible"
            };
            app.set_status(format!("Graph legend: {}", state));
        }

        // Start editing job filter
        KeyCode::Char('/') => {
            app.focus = FocusArea::JobIdFilter;
            app.input_mode = InputMode::Editing;
        }

        _ => {}
    }
}

/// Handle key events in editing mode
fn handle_editing_mode(app: &mut TuiApp, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            app.job_filter.pop_char();
        }
        KeyCode::Tab => {
            app.job_filter.next_component();
        }
        KeyCode::BackTab => {
            app.job_filter.prev_component();
        }
        KeyCode::Char(c) => {
            app.job_filter.push_char(c);
        }
        _ => {}
    }
}

/// Get next focus area (cycling through visible panels)
fn next_focus(current: FocusArea, panels: &super::app::PanelVisibility) -> FocusArea {
    let order = focus_order(panels);
    let current_idx = order.iter().position(|&f| f == current).unwrap_or(0);
    let next_idx = (current_idx + 1) % order.len();
    order[next_idx]
}

/// Get previous focus area
fn prev_focus(current: FocusArea, panels: &super::app::PanelVisibility) -> FocusArea {
    let order = focus_order(panels);
    let current_idx = order.iter().position(|&f| f == current).unwrap_or(0);
    let prev_idx = if current_idx == 0 {
        order.len() - 1
    } else {
        current_idx - 1
    };
    order[prev_idx]
}

/// Get ordered list of focusable areas based on visible panels
fn focus_order(panels: &super::app::PanelVisibility) -> Vec<FocusArea> {
    let mut order = vec![FocusArea::Graph];

    if panels.left_panel {
        order.push(FocusArea::Settings);
        order.push(FocusArea::OperationFilter);
        order.push(FocusArea::JobIdFilter);
    }

    if panels.bottom_panel {
        order.push(FocusArea::TopJobsTable);
    }

    order
}

/// Handle up arrow / k key
fn handle_up(app: &mut TuiApp) {
    match app.focus {
        FocusArea::OperationFilter => {
            app.operation_filter.select_prev();
        }
        FocusArea::JobIdFilter => {
            app.job_filter.prev_component();
        }
        FocusArea::TopJobsTable => {
            if app.table_scroll > 0 {
                app.table_scroll -= 1;
            }
        }
        _ => {}
    }
}

/// Handle down arrow / j key
fn handle_down(app: &mut TuiApp) {
    match app.focus {
        FocusArea::OperationFilter => {
            app.operation_filter.select_next();
        }
        FocusArea::JobIdFilter => {
            app.job_filter.next_component();
        }
        FocusArea::TopJobsTable => {
            app.table_scroll += 1;
        }
        _ => {}
    }
}

/// Handle enter/space key
fn handle_select(app: &mut TuiApp) {
    match app.focus {
        FocusArea::OperationFilter => {
            app.operation_filter.toggle_selected();
        }
        FocusArea::JobIdFilter => {
            app.input_mode = InputMode::Editing;
        }
        _ => {}
    }
}

/// Handle a mouse event
pub fn handle_mouse_event(app: &mut TuiApp, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            handle_left_click(app, mouse.column, mouse.row, mouse.modifiers);
        }
        MouseEventKind::Down(MouseButton::Right) => {
            handle_right_click(app, mouse.column, mouse.row);
        }
        MouseEventKind::ScrollUp => {
            // Scroll up in current focus area
            handle_scroll(app, -1);
        }
        MouseEventKind::ScrollDown => {
            // Scroll down in current focus area
            handle_scroll(app, 1);
        }
        _ => {}
    }
}

/// Handle left mouse click
fn handle_left_click(app: &mut TuiApp, x: u16, y: u16, modifiers: KeyModifiers) {
    // Find what was clicked
    let region = app.click_regions.find_at(x, y).cloned();

    if let Some(region) = region {
        match region.region_type {
            HitRegionType::LegendJob(job_id) | HitRegionType::TableJob(job_id) => {
                if modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+click: toggle selection (multi-select)
                    app.toggle_job_selection(&job_id);
                    let selected = app.selected_jobs.len();
                    if selected > 0 {
                        app.set_status(format!("{} job(s) selected - press 's' to filter", selected));
                    } else {
                        app.set_status("Selection cleared".to_string());
                    }
                } else if modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+click: select only this job and immediately filter
                    app.select_job_only(&job_id);
                    app.set_status(format!("Showing only: {}", job_id));
                } else {
                    // Regular click: toggle this job
                    app.toggle_job_selection(&job_id);
                    if app.is_job_selected(&job_id) {
                        let count = app.selected_jobs.len();
                        app.set_status(format!("Selected {} job(s) - press 's' to filter", count));
                    } else {
                        let count = app.selected_jobs.len();
                        if count > 0 {
                            app.set_status(format!("{} job(s) selected", count));
                        } else {
                            app.set_status("Selection cleared".to_string());
                        }
                    }
                }
            }
            HitRegionType::OperationFilter(op) => {
                // Toggle operation filter
                app.operation_filter.toggle(&op);
                let enabled = app.operation_filter.is_enabled(&op);
                app.set_status(format!("{}: {}", op, if enabled { "ON" } else { "OFF" }));
            }
            HitRegionType::PanelHeader(panel) => {
                // Toggle panel visibility
                match panel.as_str() {
                    "left" => {
                        app.panels.left_panel = !app.panels.left_panel;
                        app.view_mode = ViewMode::Custom;
                    }
                    "right" => {
                        app.panels.right_panel = !app.panels.right_panel;
                        app.view_mode = ViewMode::Custom;
                    }
                    "bottom" => {
                        app.panels.bottom_panel = !app.panels.bottom_panel;
                        app.view_mode = ViewMode::Custom;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Handle right mouse click (context menu / clear selection)
fn handle_right_click(app: &mut TuiApp, x: u16, y: u16) {
    let region = app.click_regions.find_at(x, y).cloned();

    if let Some(region) = region {
        match region.region_type {
            HitRegionType::LegendJob(_) | HitRegionType::TableJob(_) => {
                // Right click on job: clear all selections
                app.clear_job_selection();
                app.set_status("Selection cleared".to_string());
            }
            _ => {}
        }
    }
}

/// Handle scroll wheel
fn handle_scroll(app: &mut TuiApp, delta: i32) {
    match app.focus {
        FocusArea::TopJobsTable => {
            if delta > 0 {
                app.table_scroll += 1;
            } else if app.table_scroll > 0 {
                app.table_scroll -= 1;
            }
        }
        FocusArea::OperationFilter => {
            if delta > 0 {
                app.operation_filter.select_next();
            } else {
                app.operation_filter.select_prev();
            }
        }
        _ => {}
    }
}

/// Handle replay-specific key events
/// Returns true if the key was handled
fn handle_replay_key(app: &mut TuiApp, key: KeyEvent) -> bool {
    use super::replay::PlaybackState;

    // First, determine what action to take and collect any needed data
    enum ReplayAction {
        TogglePlayback,
        Stop,
        SpeedUp,
        SlowDown,
        SeekRelative(i64),
        JumpToStart,
        JumpToEnd,
        None,
    }

    let action = match key.code {
        KeyCode::Char('p') | KeyCode::Char(' ') if app.focus == FocusArea::Graph => {
            ReplayAction::TogglePlayback
        }
        KeyCode::Char('o') => ReplayAction::Stop,
        KeyCode::Char('>') | KeyCode::Char('.') => ReplayAction::SpeedUp,
        KeyCode::Char('<') | KeyCode::Char(',') => ReplayAction::SlowDown,
        KeyCode::Left => ReplayAction::SeekRelative(-10),
        KeyCode::Right => ReplayAction::SeekRelative(10),
        KeyCode::Home => ReplayAction::JumpToStart,
        KeyCode::End => ReplayAction::JumpToEnd,
        KeyCode::PageUp => ReplayAction::SeekRelative(-60),
        KeyCode::PageDown => ReplayAction::SeekRelative(60),
        _ => ReplayAction::None,
    };

    // Now execute the action
    match action {
        ReplayAction::None => false,
        ReplayAction::TogglePlayback => {
            if let Some(ref mut controller) = app.replay {
                controller.toggle_playback();
                let state = match controller.state {
                    PlaybackState::Playing => "Playing",
                    PlaybackState::Paused => "Paused",
                    PlaybackState::Stopped => "Stopped",
                };
                app.set_status(format!("Playback: {}", state));
            }
            true
        }
        ReplayAction::Stop => {
            if let Some(ref mut controller) = app.replay {
                controller.stop();
            }
            app.set_status("Playback stopped, reset to start".to_string());
            app.update_replay_stats();
            true
        }
        ReplayAction::SpeedUp => {
            let speed = if let Some(ref mut controller) = app.replay {
                controller.speed_up();
                controller.speed
            } else {
                1.0
            };
            app.set_status(format!("Playback speed: {}x", speed));
            true
        }
        ReplayAction::SlowDown => {
            let speed = if let Some(ref mut controller) = app.replay {
                controller.slow_down();
                controller.speed
            } else {
                1.0
            };
            app.set_status(format!("Playback speed: {}x", speed));
            true
        }
        ReplayAction::SeekRelative(delta) => {
            let time_str = if let Some(ref mut controller) = app.replay {
                controller.seek_relative(delta);
                controller.current_time_str()
            } else {
                String::new()
            };
            app.set_status(format!("Seek: {}", time_str));
            app.update_replay_stats();
            true
        }
        ReplayAction::JumpToStart => {
            if let Some(ref mut controller) = app.replay {
                controller.jump_to_start();
            }
            app.set_status("Jumped to start".to_string());
            app.update_replay_stats();
            true
        }
        ReplayAction::JumpToEnd => {
            if let Some(ref mut controller) = app.replay {
                controller.jump_to_end();
            }
            app.set_status("Jumped to end".to_string());
            app.update_replay_stats();
            true
        }
    }
}
