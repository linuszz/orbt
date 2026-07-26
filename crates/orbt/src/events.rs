use anyhow::Result;
use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind,
};
use futures::StreamExt;
use orbt_protocol::{ClientMessage, SplitDir};
use tracing::debug;

use crate::ipc::{IpcReader, IpcWriter};
use orbt_tui::app::{
    AgentHover, AgentPanelMode, App, ContextMenuItem, ContextMenuTarget, InputMode, MobileCloseConfirm,
    MobileCloseTarget, MobileColFocus, MobileView, COMMANDS,
};
use orbt_tui::tui::{
    agent_panel_width, render, render_mobile, OrbitTerminal, SIDEBAR_COLLAPSED_W, SIDEBAR_W,
};

fn is_prefix_key(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b')
}

fn filtered_indices(search: &str) -> Vec<usize> {
    if search.is_empty() {
        return (0..COMMANDS.len()).collect();
    }
    let s = search.to_lowercase();
    COMMANDS
        .iter()
        .enumerate()
        .filter(|(_, c)| c.label.to_lowercase().contains(&s))
        .map(|(i, _)| i)
        .collect()
}

fn key_to_pty_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    match (key.modifiers, key.code) {
        (m, KeyCode::Char(c))
            if m.contains(KeyModifiers::CONTROL) && !m.contains(KeyModifiers::ALT) =>
        {
            Some(vec![(c as u8) & 0x1f])
        }
        (m, KeyCode::Char(c)) if m.contains(KeyModifiers::ALT) => {
            let mut bytes = vec![0x1b];
            bytes.extend(c.to_string().as_bytes());
            Some(bytes)
        }
        (_, KeyCode::Char(c)) => Some(c.to_string().into_bytes()),
        (_, KeyCode::Enter) => Some(b"\r".to_vec()),
        (_, KeyCode::Backspace) => Some(b"\x7f".to_vec()),
        (_, KeyCode::Tab) => Some(b"\t".to_vec()),
        (_, KeyCode::Up) => Some(b"\x1b[A".to_vec()),
        (_, KeyCode::Down) => Some(b"\x1b[B".to_vec()),
        (_, KeyCode::Right) => Some(b"\x1b[C".to_vec()),
        (_, KeyCode::Left) => Some(b"\x1b[D".to_vec()),
        (_, KeyCode::Home) => Some(b"\x1b[H".to_vec()),
        (_, KeyCode::End) => Some(b"\x1b[F".to_vec()),
        (_, KeyCode::PageUp) => Some(b"\x1b[5~".to_vec()),
        (_, KeyCode::PageDown) => Some(b"\x1b[6~".to_vec()),
        (_, KeyCode::Delete) => Some(b"\x1b[3~".to_vec()),
        (_, KeyCode::Esc) => Some(b"\x1b".to_vec()),
        _ => None,
    }
}

fn content_area(term_size: ratatui::layout::Rect, app: &App) -> ratatui::layout::Rect {
    compute_pane_area(term_size.width, term_size.height, app)
}

fn compute_pane_area(term_cols: u16, term_rows: u16, app: &App) -> ratatui::layout::Rect {
    let sidebar_w: u16 = if term_cols < 80 {
        SIDEBAR_COLLAPSED_W
    } else if app.sidebar_visible {
        SIDEBAR_W
    } else {
        SIDEBAR_COLLAPSED_W
    };
    let agent_w = agent_panel_width(term_cols, app.agent_panel_mode);
    let total_cols = term_cols.saturating_sub(sidebar_w + agent_w).max(20);
    let total_rows = term_rows.saturating_sub(3).max(5);
    ratatui::layout::Rect {
        x: sidebar_w,
        y: 1,
        width: total_cols,
        height: total_rows,
    }
}

async fn execute_command(id: &str, app: &mut App, writer: &IpcWriter, term_h: u16) {
    match id {
        "split_h" => {
            app.pending_split = Some((app.active_pane, SplitDir::Horizontal));
            let _ = writer
                .send(ClientMessage::SplitPane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                    direction: SplitDir::Horizontal,
                })
                .await;
        }
        "split_v" => {
            app.pending_split = Some((app.active_pane, SplitDir::Vertical));
            let _ = writer
                .send(ClientMessage::SplitPane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                    direction: SplitDir::Vertical,
                })
                .await;
        }
        "close_pane" => {
            if app.pane_tree().leaves().len() <= 1 {
                app.should_quit = true;
            }
            let _ = writer
                .send(ClientMessage::ClosePane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                })
                .await;
        }
        "cycle_pane" => {
            app.cycle_focus();
            let _ = writer
                .send(ClientMessage::FocusPane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                })
                .await;
        }
        "zoom_pane" => {
            // Zoom is a placeholder — future: toggle pane fullscreen
        }
        "scroll_mode" => {
            app.selection = None;
            app.mode = InputMode::Scroll { offset: 1 };
        }
        "new_tab" => {
            let _ = writer.send(ClientMessage::NewTab { name: None }).await;
        }
        "next_tab" => {
            app.next_tab();
            let _ = writer
                .send(ClientMessage::SwitchTab {
                    tab_id: app.active_tab_id,
                })
                .await;
        }
        "prev_tab" => {
            app.prev_tab();
            let _ = writer
                .send(ClientMessage::SwitchTab {
                    tab_id: app.active_tab_id,
                })
                .await;
        }
        "toggle_sidebar" => {
            app.sidebar_visible = !app.sidebar_visible;
            orbt_tui::app::save_settings(app);
        }
        "toggle_agent" => {
            app.agent_panel_mode = app.agent_panel_mode.cycle();
            if app.agent_panel_mode.is_visible() {
                let sel = if let InputMode::AgentPanel { selected } = app.mode {
                    selected
                } else {
                    0
                };
                app.mode = InputMode::AgentPanel { selected: sel };
                app.agent_scroll_offset = app.agent_scroll_offset.min(sel);
            } else {
                app.mode = InputMode::Normal;
                app.agent_hovered = None;
            }
            orbt_tui::app::save_settings(app);
        }
        "agent_scroll_up" => {
            if app.agent_panel_mode.is_visible() {
                app.agent_scroll_offset = app.agent_scroll_offset.saturating_sub(1);
            }
        }
        "agent_scroll_down" => {
            if app.agent_panel_mode.is_visible() {
                let banner_rows: u16 = if app
                    .agents
                    .iter()
                    .any(|a| a.status == orbt_protocol::AgentStatus::Blocked)
                {
                    2
                } else {
                    0
                };
                let above_row: u16 = if app.agent_scroll_offset > 0 { 1 } else { 0 };
                let visible =
                    ((term_h.saturating_sub(5 + banner_rows + above_row)) / 6).max(1) as usize;
                let max = app.agents.len().saturating_sub(visible);
                app.agent_scroll_offset = (app.agent_scroll_offset + 1).min(max);
            }
        }
        "paste_image" => {
            let writer_clone = writer.clone();
            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
                    let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
                    let rgba = img.bytes.into_owned();
                    let dyn_img =
                        image::RgbaImage::from_raw(img.width as u32, img.height as u32, rgba)?;
                    let mut buf = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::from(dyn_img)
                        .write_to(&mut buf, image::ImageFormat::Png)
                        .ok()?;
                    let data = buf.into_inner();
                    if data.len() > orbt_protocol::MAX_MSG_BYTES {
                        return None;
                    }
                    Some(data)
                })
                .await;
                if let Ok(Some(data)) = result {
                    let _ = writer_clone
                        .send(ClientMessage::UploadPayload {
                            data,
                            filename: "screenshot.png".to_string(),
                        })
                        .await;
                }
            });
        }
        "detach" => app.should_quit = true,
        "toggle_theme" => {
            let themes = orbt_tui::tui::theme::ALL_THEMES;
            let idx = themes
                .iter()
                .position(|&t| t == app.theme_name)
                .unwrap_or(0);
            app.theme_name = themes[(idx + 1) % themes.len()].to_string();
            orbt_tui::tui::theme::set_theme(&app.theme_name);
            orbt_tui::app::save_settings(app);
        }
        "settings" => {
            app.settings_open = true;
            app.settings_selected = 0;
        }
        "help" => app.show_help = true,
        _ => {}
    }

    // Mobile: after executing any command from the Actions view, return to Terminal
    // so the user immediately sees the result full-screen. Exclude commands that open
    // their own overlay (settings, help) — those stay in Actions until Esc.
    if app.mobile_mode
        && app.mobile_view == MobileView::Actions
        && !matches!(id, "settings" | "help")
    {
        app.mobile_view = MobileView::Terminal;
        app.mode = InputMode::Normal;
    }

    app.needs_redraw = true;
}

async fn execute_context_action(
    id: &str,
    target: &ContextMenuTarget,
    app: &mut App,
    writer: &IpcWriter,
) {
    match id {
        "new_tab" => {
            let _ = writer.send(ClientMessage::NewTab { name: None }).await;
        }
        "close_tab" => {
            if let ContextMenuTarget::Tab(tab_id) = target {
                if app.tabs.len() > 1 {
                    let _ = writer
                        .send(ClientMessage::CloseTab { tab_id: *tab_id })
                        .await;
                }
            }
        }
        "split_h" => {
            app.pending_split = Some((app.active_pane, SplitDir::Horizontal));
            let _ = writer
                .send(ClientMessage::SplitPane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                    direction: SplitDir::Horizontal,
                })
                .await;
        }
        "split_v" => {
            app.pending_split = Some((app.active_pane, SplitDir::Vertical));
            let _ = writer
                .send(ClientMessage::SplitPane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                    direction: SplitDir::Vertical,
                })
                .await;
        }
        "close_pane" => {
            if app.pane_tree().leaves().len() <= 1 {
                app.should_quit = true;
            }
            let _ = writer
                .send(ClientMessage::ClosePane {
                    tab_id: app.active_tab_id,
                    pane_id: app.active_pane,
                })
                .await;
        }
        "copy_selection" => {
            if let Some(sel) = app.selection.clone() {
                let pane_id = sel.pane_id;
                if let Some(pane_state) = app.panes.get(&pane_id) {
                    let grid = &pane_state.parser.grid;
                    let (min_col, max_col) = if sel.start.0 <= sel.end.0 {
                        (sel.start.0 as usize, sel.end.0 as usize)
                    } else {
                        (sel.end.0 as usize, sel.start.0 as usize)
                    };
                    let (min_row, max_row) = if sel.start.1 <= sel.end.1 {
                        (sel.start.1 as usize, sel.end.1 as usize)
                    } else {
                        (sel.end.1 as usize, sel.start.1 as usize)
                    };
                    let cols = grid.cols as usize;
                    let max_row_clamped = max_row.min(grid.rows as usize - 1);
                    let min_col_clamped = min_col.min(cols.saturating_sub(1));
                    let max_col_clamped = max_col.min(cols.saturating_sub(1));
                    let mut lines: Vec<String> = Vec::new();
                    for row in min_row..=max_row_clamped {
                        let row_start = row * cols;
                        let line: String = grid.cells
                            [row_start + min_col_clamped..=row_start + max_col_clamped]
                            .iter()
                            .map(|c| if c.ch == '\0' { ' ' } else { c.ch })
                            .collect::<String>()
                            .trim_end()
                            .to_string();
                        lines.push(line);
                    }
                    let text = lines.join("\n");
                    let _ = writer
                        .send(orbt_protocol::ClientMessage::CopyToClipboard { text })
                        .await;
                }
                app.selection = None;
            }
        }
        "maximize" | "rename_space" => {}
        "move_up" | "move_down" => {
            if let ContextMenuTarget::Space(space_id) = target {
                if let Some(idx) = app.spaces.iter().position(|s| s.space_id == *space_id) {
                    let to_index = if id == "move_up" {
                        idx.saturating_sub(1)
                    } else {
                        (idx + 1).min(app.spaces.len().saturating_sub(1))
                    };
                    if to_index != idx {
                        app.spaces.swap(idx, to_index);
                        let _ = writer
                            .send(ClientMessage::ReorderSpace {
                                space_id: *space_id,
                                to_index,
                            })
                            .await;
                    }
                }
            }
        }
        "close_space" => {
            if let ContextMenuTarget::Space(space_id) = target {
                let _ = writer
                    .send(ClientMessage::CloseSpace {
                        space_id: *space_id,
                    })
                    .await;
            }
        }
        "new_space" => {
            let _ = writer.send(ClientMessage::CreateSpace { name: None }).await;
        }
        _ => {}
    }
    app.needs_redraw = true;
}

async fn handle_eclipse_key(key: KeyEvent, app: &mut App, writer: &IpcWriter) {
    let Some(modal) = &app.eclipse_modal else {
        return;
    };
    let agent_id = modal.agent_id;
    match key.code {
        KeyCode::Esc => {
            app.eclipse_modal = None;
            app.needs_redraw = true;
        }
        KeyCode::Enter => {
            let response = modal.response.clone();
            app.eclipse_modal = None;
            let _ = writer
                .send(ClientMessage::AgentRespond { agent_id, response })
                .await;
            app.needs_redraw = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.eclipse_modal = None;
            let _ = writer.send(ClientMessage::AgentAbort { agent_id }).await;
            app.needs_redraw = true;
        }
        KeyCode::Backspace => {
            if let Some(m) = &mut app.eclipse_modal {
                m.response.pop();
            }
            app.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            if let Some(m) = &mut app.eclipse_modal {
                m.response.push(c);
            }
            app.needs_redraw = true;
        }
        _ => {}
    }
}

/// Execute a confirmed close action from the mobile close-confirmation modal.
async fn apply_mobile_close(confirm: MobileCloseConfirm, app: &mut App, writer: &IpcWriter) {
    match confirm.target {
        MobileCloseTarget::Space(idx) => {
            if app.spaces.len() > 1 {
                if let Some(space) = app.spaces.get(idx) {
                    let space_id = space.space_id;
                    let _ = writer.send(ClientMessage::CloseSpace { space_id }).await;
                }
            }
        }
        MobileCloseTarget::Tab(idx) => {
            if app.tabs.len() > 1 {
                if let Some(tab) = app.tabs.get(idx) {
                    let tab_id = tab.id;
                    let _ = writer.send(ClientMessage::CloseTab { tab_id }).await;
                }
            }
        }
    }
}

/// Handle keyboard input when the close-confirmation modal is open.
/// Always consumes the key.
async fn handle_mobile_close_confirm_key(
    key: KeyEvent,
    app: &mut App,
    writer: &IpcWriter,
) {
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut c) = app.mobile_close_confirm {
                c.confirm_focused = false;
            }
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
            if let Some(ref mut c) = app.mobile_close_confirm {
                c.confirm_focused = !c.confirm_focused;
            }
        }
        KeyCode::Enter => {
            if let Some(confirm) = app.mobile_close_confirm.take() {
                if confirm.confirm_focused {
                    apply_mobile_close(confirm, app, writer).await;
                }
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.mobile_close_confirm = None;
        }
        _ => {}
    }
    app.needs_redraw = true;
}

/// Mobile-mode key handler. Returns true if the key was consumed.
async fn handle_mobile_key(key: KeyEvent, app: &mut App, writer: &IpcWriter, _term_h: u16) -> bool {
    // Modals still capture input in mobile mode.
    if app.launch_modal.is_some() || app.eclipse_modal.is_some() {
        return false;
    }

    // Close-confirmation modal intercepts all keys.
    if app.mobile_close_confirm.is_some() {
        handle_mobile_close_confirm_key(key, app, writer).await;
        return true;
    }

    // Tab cycles through mobile views.
    if key.code == KeyCode::Tab {
        app.mobile_view = app.mobile_view.next();
        // Entering Actions view: open command palette.
        if app.mobile_view == MobileView::Actions {
            app.mode = InputMode::CommandPalette {
                search: String::new(),
                selected: 0,
                search_focused: false,
            };
        } else {
            app.mode = InputMode::Normal;
        }
        app.needs_redraw = true;
        return true;
    }

    match app.mobile_view {
        MobileView::Terminal => {
            // Esc when in an overlay returns to Normal/Terminal.
            if key.code == KeyCode::Esc {
                if app.settings_open {
                    app.settings_open = false;
                    app.needs_redraw = true;
                    return true;
                }
                if matches!(app.mode, InputMode::CommandPalette { .. }) {
                    app.mode = InputMode::Normal;
                    app.needs_redraw = true;
                    return true;
                }
            }
            // Fall through to normal key handler for PTY passthrough.
            false
        }
        MobileView::Agents => {
            // Mirror AgentPanel mode key handling.
            let n = app.agents.len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if n > 0 {
                        let sel = if let InputMode::AgentPanel { selected } = app.mode {
                            selected.saturating_sub(1)
                        } else {
                            0
                        };
                        app.mode = InputMode::AgentPanel { selected: sel };
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if n > 0 {
                        let sel = if let InputMode::AgentPanel { selected } = app.mode {
                            (selected + 1).min(n - 1)
                        } else {
                            0
                        };
                        app.mode = InputMode::AgentPanel { selected: sel };
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Char('n') => {
                    orbt_tui::tui::widgets::launch_modal::open(app);
                    true
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.mobile_view = MobileView::Terminal;
                    app.mode = InputMode::Normal;
                    app.needs_redraw = true;
                    true
                }
                _ => false,
            }
        }
        MobileView::Windows => {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    match app.mobile_col_focus {
                        MobileColFocus::Left => {
                            app.mobile_spaces_cursor =
                                app.mobile_spaces_cursor.saturating_sub(1);
                        }
                        MobileColFocus::Right => {
                            app.mobile_tabs_cursor =
                                app.mobile_tabs_cursor.saturating_sub(1);
                        }
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    match app.mobile_col_focus {
                        MobileColFocus::Left => {
                            let max = app.spaces.len(); // cursor=max → "+Space" button
                            app.mobile_spaces_cursor =
                                (app.mobile_spaces_cursor + 1).min(max);
                        }
                        MobileColFocus::Right => {
                            let max = app.tabs.len(); // cursor=max → "+ New Tab" button
                            app.mobile_tabs_cursor =
                                (app.mobile_tabs_cursor + 1).min(max);
                        }
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    app.mobile_col_focus = MobileColFocus::Left;
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    app.mobile_col_focus = MobileColFocus::Right;
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Enter => {
                    match app.mobile_col_focus {
                        MobileColFocus::Left => {
                            let cursor = app.mobile_spaces_cursor;
                            if cursor < app.spaces.len() {
                                // Switch space, stay in SPACES to pick a tab
                                let space_id = app.spaces[cursor].space_id;
                                let _ = writer
                                    .send(ClientMessage::SwitchSpace { space_id })
                                    .await;
                                app.active_space_idx = cursor;
                                app.mobile_tabs_cursor = 0;
                            } else {
                                // "+Space" button
                                let _ = writer
                                    .send(ClientMessage::CreateSpace { name: None })
                                    .await;
                            }
                        }
                        MobileColFocus::Right => {
                            let cursor = app.mobile_tabs_cursor;
                            if cursor < app.tabs.len() {
                                // Switch tab, go to Terminal
                                app.selection = None;
                                app.active_tab = cursor;
                                app.active_tab_id = app.tabs[cursor].id;
                                if let Some(&first) = app.pane_tree().leaves().first() {
                                    app.active_pane = first;
                                }
                                let _ = writer
                                    .send(ClientMessage::SwitchTab {
                                        tab_id: app.active_tab_id,
                                    })
                                    .await;
                                app.mobile_view = MobileView::Terminal;
                            } else {
                                // "+ New Tab" button
                                let _ = writer
                                    .send(ClientMessage::NewTab { name: None })
                                    .await;
                            }
                        }
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Delete | KeyCode::Char('x') => {
                    match app.mobile_col_focus {
                        MobileColFocus::Left => {
                            let cursor = app.mobile_spaces_cursor;
                            if cursor < app.spaces.len() && app.spaces.len() > 1 {
                                app.mobile_close_confirm = Some(MobileCloseConfirm {
                                    target: MobileCloseTarget::Space(cursor),
                                    confirm_focused: false,
                                });
                            }
                        }
                        MobileColFocus::Right => {
                            let cursor = app.mobile_tabs_cursor;
                            if cursor < app.tabs.len() && app.tabs.len() > 1 {
                                app.mobile_close_confirm = Some(MobileCloseConfirm {
                                    target: MobileCloseTarget::Tab(cursor),
                                    confirm_focused: false,
                                });
                            }
                        }
                    }
                    app.needs_redraw = true;
                    true
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.mobile_view = MobileView::Terminal;
                    app.needs_redraw = true;
                    true
                }
                _ => false,
            }
        }
        MobileView::Actions => {
            // Esc closes the palette and returns to Terminal.
            if key.code == KeyCode::Esc {
                app.mobile_view = MobileView::Terminal;
                app.mode = InputMode::Normal;
                app.settings_open = false;
                app.needs_redraw = true;
                return true;
            }
            // All other keys (including Enter on a command) fall through to the normal
            // CommandPalette / settings handler, which will call execute_command and
            // then the auto-return logic above fires.
            false
        }
    }
}

async fn handle_key(key: KeyEvent, app: &mut App, writer: &IpcWriter, term_h: u16) {
    // Mobile-mode navigation intercepts before normal key dispatch.
    if app.mobile_mode && handle_mobile_key(key, app, writer, term_h).await {
        return;
    }

    // Launch modal captures all keyboard input when open.
    if app.launch_modal.is_some() {
        handle_launch_key(key, app, writer).await;
        return;
    }

    // Eclipse modal captures all keyboard input when open.
    if app.eclipse_modal.is_some() {
        handle_eclipse_key(key, app, writer).await;
        return;
    }

    if app.show_help {
        app.show_help = false;
        app.needs_redraw = true;
        return;
    }

    if app.settings_open {
        let num_settings = 3usize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                app.settings_open = false;
            }
            KeyCode::Up => app.settings_selected = app.settings_selected.saturating_sub(1),
            KeyCode::Down => {
                app.settings_selected = (app.settings_selected + 1).min(num_settings - 1)
            }
            KeyCode::Enter | KeyCode::Char(' ') => match app.settings_selected {
                0 => {
                    let themes = orbt_tui::tui::theme::ALL_THEMES;
                    let idx = themes
                        .iter()
                        .position(|&t| t == app.theme_name)
                        .unwrap_or(0);
                    app.theme_name = themes[(idx + 1) % themes.len()].to_string();
                    orbt_tui::tui::theme::set_theme(&app.theme_name);
                    orbt_tui::app::save_settings(app);
                }
                1 => {
                    app.sidebar_visible = !app.sidebar_visible;
                    app.needs_resize = true;
                    orbt_tui::app::save_settings(app);
                }
                2 => {
                    app.agent_panel_mode = app.agent_panel_mode.cycle();
                    orbt_tui::app::save_settings(app);
                }
                _ => {}
            },
            _ => {}
        }
        app.needs_redraw = true;
        return;
    }

    if app.context_menu.is_some() && key.code == KeyCode::Esc {
        app.close_context_menu();
        return;
    }

    match &mut app.mode {
        InputMode::Normal => {
            if app.selection.is_some() {
                app.selection = None;
                app.needs_redraw = true;
            }
            if is_prefix_key(&key) {
                app.mode = InputMode::CommandPalette {
                    search: String::new(),
                    selected: 0,
                    search_focused: false,
                };
                app.needs_redraw = true;
                return;
            }
            // Tab key is forwarded to PTY (pane cycling is prefix+o, tmux-style)

            if let Some(bytes) = key_to_pty_bytes(&key) {
                let _ = writer
                    .send(ClientMessage::PaneInput {
                        tab_id: app.active_tab_id,
                        pane_id: app.active_pane,
                        data: bytes,
                    })
                    .await;
            }
        }
        InputMode::CommandPalette {
            search, selected, ..
        } => {
            if is_prefix_key(&key) || key.code == KeyCode::Esc {
                if search.is_empty() {
                    app.mode = InputMode::Normal;
                } else {
                    search.clear();
                    *selected = 0;
                }
                app.needs_redraw = true;
                return;
            }

            // In mobile Actions view the palette is always fullscreen — pane navigation
            // would close it without switching panes visibly, so skip this block there.
            if search.is_empty()
                && !(app.mobile_mode && app.mobile_view == MobileView::Actions)
            {
                let Some(tab) = app.tabs.get(app.active_tab) else {
                    app.mode = InputMode::Normal;
                    return;
                };
                let layout = &tab.pane_tree;
                let target = match key.code {
                    KeyCode::Left => layout.find_pane_in_direction(
                        app.active_pane,
                        orbt_protocol::SplitDir::Horizontal,
                        false,
                    ),
                    KeyCode::Right => layout.find_pane_in_direction(
                        app.active_pane,
                        orbt_protocol::SplitDir::Horizontal,
                        true,
                    ),
                    KeyCode::Up => layout.find_pane_in_direction(
                        app.active_pane,
                        orbt_protocol::SplitDir::Vertical,
                        false,
                    ),
                    KeyCode::Down => layout.find_pane_in_direction(
                        app.active_pane,
                        orbt_protocol::SplitDir::Vertical,
                        true,
                    ),
                    _ => None,
                };
                if let Some(target_pane) = target {
                    app.active_pane = target_pane;
                    app.mode = InputMode::Normal;
                    let _ = writer
                        .send(ClientMessage::FocusPane {
                            tab_id: app.active_tab_id,
                            pane_id: target_pane,
                        })
                        .await;
                    app.needs_redraw = true;
                    return;
                }
            }

            let filtered = filtered_indices(search);

            match key.code {
                KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                }
                KeyCode::Down if !filtered.is_empty() => {
                    *selected = (*selected + 1).min(filtered.len() - 1);
                }
                KeyCode::Enter => {
                    if let Some(&cmd_idx) = filtered.get(*selected) {
                        let cmd_id = COMMANDS[cmd_idx].id;
                        app.mode = InputMode::Normal;
                        execute_command(cmd_id, app, writer, term_h).await;
                        return;
                    }
                }
                KeyCode::Backspace => {
                    search.pop();
                    *selected = 0;
                }
                KeyCode::Char(c) => {
                    // tmux: 0-9 switches to window N
                    if search.is_empty() && c.is_ascii_digit() {
                        let idx = (c as u8 - b'0') as usize;
                        if idx < app.tabs.len() {
                            app.selection = None;
                            app.active_tab = idx;
                            app.active_tab_id = app.tabs[idx].id;
                            if let Some(&first) = app.pane_tree().leaves().first() {
                                app.active_pane = first;
                            }
                            let _ = writer
                                .send(ClientMessage::SwitchTab {
                                    tab_id: app.active_tab_id,
                                })
                                .await;
                        }
                        app.mode = InputMode::Normal;
                        app.needs_redraw = true;
                        return;
                    }
                    let sc = c.to_string();
                    let shortcut_cmd = search
                        .is_empty()
                        .then(|| COMMANDS.iter().find(|cmd| cmd.shortcut == sc))
                        .flatten();
                    if let Some(cmd) = shortcut_cmd {
                        app.mode = InputMode::Normal;
                        execute_command(cmd.id, app, writer, term_h).await;
                        return;
                    }
                    search.push(c);
                    *selected = 0;
                }
                _ => {}
            }
            app.needs_redraw = true;
        }
        InputMode::Scroll { offset } => {
            let pane_height = app
                .panes
                .get(&app.active_pane)
                .map(|p| p.parser.grid.rows as usize)
                .unwrap_or(24);
            let scrollback_len = app
                .panes
                .get(&app.active_pane)
                .map(|p| p.scrollback.len())
                .unwrap_or(0);
            let max_offset = scrollback_len;

            match key.code {
                KeyCode::Up | KeyCode::Char('k') => *offset = (*offset + 1).min(max_offset),
                KeyCode::Down | KeyCode::Char('j') => {
                    *offset = offset.saturating_sub(1);
                    if *offset == 0 {
                        app.mode = InputMode::Normal;
                    }
                }
                KeyCode::PageUp => *offset = (*offset + pane_height).min(max_offset),
                KeyCode::PageDown => {
                    *offset = offset.saturating_sub(pane_height);
                    if *offset == 0 {
                        app.mode = InputMode::Normal;
                    }
                }
                KeyCode::Char('G') => *offset = 0,
                KeyCode::Char('g') => *offset = max_offset,
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.mode = InputMode::Normal;
                }
                _ => {}
            }
            app.needs_redraw = true;
        }
        InputMode::AgentPanel { selected } => {
            let n = app.agents.len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if n > 0 {
                        *selected = selected.saturating_sub(1);
                        // Auto-scroll so selected card is visible.
                        app.agent_scroll_offset = app.agent_scroll_offset.min(*selected);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if n > 0 {
                        *selected = (*selected + 1).min(n - 1);
                        // Auto-scroll: keep selected card in the visible window.
                        // Overhead: tab_bar(1) + status_bar(1) + header(1) + divider(1) + footer(1) = 5
                        // Eclipse banner when any agent Blocked: +2 rows
                        // "N above" scroll indicator: +1 when scrolled
                        let banner_rows: u16 = if app
                            .agents
                            .iter()
                            .any(|a| a.status == orbt_protocol::AgentStatus::Blocked)
                        {
                            2
                        } else {
                            0
                        };
                        let above_row: u16 = if app.agent_scroll_offset > 0 { 1 } else { 0 };
                        let visible =
                            ((term_h.saturating_sub(5 + banner_rows + above_row)) / 6) as usize;
                        let visible = visible.max(1);
                        if *selected >= app.agent_scroll_offset + visible {
                            app.agent_scroll_offset =
                                selected.saturating_sub(visible.saturating_sub(1));
                        }
                    }
                }
                KeyCode::Tab => {
                    if n > 0 {
                        let old = *selected;
                        *selected = (*selected + 1) % n;
                        if *selected < old {
                            // Wrapped to first agent — scroll to top.
                            app.agent_scroll_offset = 0;
                        } else {
                            // Moved forward — same visible-window logic as Down.
                            let banner_rows: u16 = if app
                                .agents
                                .iter()
                                .any(|a| a.status == orbt_protocol::AgentStatus::Blocked)
                            {
                                2
                            } else {
                                0
                            };
                            let above_row: u16 = if app.agent_scroll_offset > 0 { 1 } else { 0 };
                            let visible = ((term_h.saturating_sub(5 + banner_rows + above_row)) / 6)
                                .max(1) as usize;
                            if *selected >= app.agent_scroll_offset + visible {
                                app.agent_scroll_offset =
                                    selected.saturating_sub(visible.saturating_sub(1));
                            }
                        }
                    }
                }
                KeyCode::Enter => {
                    let sel = *selected;
                    if let Some(agent) = app.agents.get(sel) {
                        if let Some(pane_id) = agent.pane_id {
                            let found = app
                                .tabs
                                .iter()
                                .enumerate()
                                .find(|(_, t)| t.pane_tree.leaves().contains(&pane_id));
                            let tab_id = found.map(|(_, t)| t.id).unwrap_or(app.active_tab_id);
                            let tab_idx = found.map(|(i, _)| i).unwrap_or(app.active_tab);
                            app.active_pane = pane_id;
                            app.active_tab = tab_idx;
                            app.active_tab_id = tab_id;
                            app.selection = None;
                            app.mode = InputMode::Normal;
                            let _ = writer
                                .send(ClientMessage::FocusPane { tab_id, pane_id })
                                .await;
                        }
                    }
                }
                // n: open Launch Agent picker
                KeyCode::Char('n') => {
                    orbt_tui::tui::widgets::launch_modal::open(app);
                }
                // r: respond to blocked agent (opens Eclipse modal)
                KeyCode::Char('r') => {
                    let sel = *selected;
                    if let Some(agent) = app.agents.get(sel) {
                        let agent_id = agent.id;
                        match agent.status {
                            orbt_protocol::AgentStatus::Blocked => {
                                orbt_tui::tui::widgets::eclipse_modal::open(app, agent_id);
                            }
                            orbt_protocol::AgentStatus::Error => {
                                let _ = writer.send(ClientMessage::AgentRestart { agent_id }).await;
                            }
                            _ => {}
                        }
                    }
                }
                // s: stop/abort a working or blocked agent
                KeyCode::Char('s') => {
                    let sel = *selected;
                    if let Some(agent) = app.agents.get(sel) {
                        if matches!(
                            agent.status,
                            orbt_protocol::AgentStatus::Working
                                | orbt_protocol::AgentStatus::Blocked
                        ) {
                            let agent_id = agent.id;
                            let _ = writer.send(ClientMessage::AgentAbort { agent_id }).await;
                        }
                    }
                }
                // d: dismiss (remove) idle/done/error agent from list
                KeyCode::Char('d') => {
                    let sel = *selected;
                    if let Some(agent) = app.agents.get(sel) {
                        if matches!(
                            agent.status,
                            orbt_protocol::AgentStatus::Idle
                                | orbt_protocol::AgentStatus::Done
                                | orbt_protocol::AgentStatus::Error
                        ) {
                            let agent_id = agent.id;
                            let _ = writer.send(ClientMessage::AgentRemove { agent_id }).await;
                        }
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.mode = InputMode::Normal;
                }
                _ => {}
            }
            app.needs_redraw = true;
        }
    }
}

fn resize_local_grids_for_areas(
    app: &mut App,
    areas: &[(orbt_protocol::PaneId, ratatui::layout::Rect)],
) {
    for (pid, rect) in areas {
        let pc = rect.width.saturating_sub(2).max(1);
        let pr = rect.height.saturating_sub(2).max(1);
        if let Some(pane) = app.panes.get_mut(pid) {
            pane.parser.grid.resize(pc, pr);
        }
    }
}

fn resize_local_grids(app: &mut App, term_cols: u16, term_rows: u16) {
    let pane_area = compute_pane_area(term_cols, term_rows, app);
    let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
    resize_local_grids_for_areas(app, &areas);
}

async fn send_pane_resizes(app: &mut App, writer: &IpcWriter, term_cols: u16, term_rows: u16) {
    resize_local_grids(app, term_cols, term_rows);
    let pane_area = compute_pane_area(term_cols, term_rows, app);
    let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
    for (pid, rect) in areas {
        let pc = rect.width.saturating_sub(2).max(1);
        let pr = rect.height.saturating_sub(2).max(1);
        let _ = writer
            .send(ClientMessage::ResizePane {
                tab_id: app.active_tab_id,
                pane_id: pid,
                cols: pc,
                rows: pr,
            })
            .await;
    }
}

pub async fn run(
    app: &mut App,
    writer: IpcWriter,
    mut reader: IpcReader,
    terminal: &mut OrbitTerminal,
) -> Result<()> {
    let mut event_stream = EventStream::new();

    app.needs_redraw = true;

    loop {
        orbt_tui::tui::theme::set_theme(&app.theme_name);

        if app.needs_redraw {
            if app.mobile_mode {
                terminal.draw(|frame| render_mobile(frame, app))?;
            } else {
                terminal.draw(|frame| render(frame, app))?;
            }
            app.needs_redraw = false;
        }

        if app.needs_resize {
            app.needs_resize = false;
            let term_size = terminal.size().unwrap_or_default();
            send_pane_resizes(app, &writer, term_size.width, term_size.height).await;
        }

        if app.should_quit {
            eprintln!("Exiting: should_quit=true");
            break;
        }

        tokio::select! {
            biased;

            // Animation tick: 16 ms, only when agents need it (§6.5 redraw-on-demand).
            _ = tokio::time::sleep(std::time::Duration::from_millis(16)),
                if app.has_active_agents() =>
            {
                app.tick_count = app.tick_count.wrapping_add(1);
                app.needs_redraw = true;
            }

            event_result = reader.recv() => {
                match event_result {
                    Ok(event) => {
                        app.handle_server_event(&event);
                        // Drain payload path: inject file path into active PTY.
                        if let Some(path) = app.pending_payload_path.take() {
                            let data = format!("{path}\n").into_bytes();
                            let _ = writer.send(ClientMessage::PaneInput {
                                tab_id: app.active_tab_id,
                                pane_id: app.active_pane,
                                data,
                            }).await;
                        }
                    }
                    Err(e) => {
                        eprintln!("Exiting: server disconnected: {e:#}");
                        app.server_connected = false;
                        app.should_quit = true;
                    }
                }
            }

            raw = event_stream.next() => {
                match raw {
                    Some(Ok(Event::Key(key))) => {
                        let term_h = terminal.size().map(|s| s.height).unwrap_or(40);
                        handle_key(key, app, &writer, term_h).await;
                    }
                    Some(Ok(Event::Resize(cols, rows))) => {
                        let was_mobile = app.mobile_mode;
                        app.mobile_mode = cols < 80 || rows < 25;
                        if was_mobile != app.mobile_mode {
                            // Mode switched — reset mobile view to Terminal.
                            app.mobile_view = MobileView::Terminal;
                        }
                        send_pane_resizes(app, &writer, cols, rows).await;
                        app.needs_redraw = true;
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        let term_size = terminal.size().unwrap_or_default();
                        let term_rect = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
                        handle_mouse(mouse, app, &writer, term_rect).await;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        if matches!(app.mode, InputMode::Normal) {
                            if text.is_empty() {
                                // Empty paste event means clipboard has no text (likely an image).
                                // Attempt image upload via the same path as paste_image command.
                                let writer_clone = writer.clone();
                                tokio::spawn(async move {
                                    let result = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
                                        let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
                                        let rgba = img.bytes.into_owned();
                                        let dyn_img = image::RgbaImage::from_raw(
                                            img.width as u32,
                                            img.height as u32,
                                            rgba,
                                        )?;
                                        let mut buf = std::io::Cursor::new(Vec::new());
                                        image::DynamicImage::from(dyn_img)
                                            .write_to(&mut buf, image::ImageFormat::Png)
                                            .ok()?;
                                        let data = buf.into_inner();
                                        if data.len() > orbt_protocol::MAX_MSG_BYTES {
                                            return None;
                                        }
                                        Some(data)
                                    })
                                    .await;
                                    if let Ok(Some(data)) = result {
                                        let _ = writer_clone
                                            .send(ClientMessage::UploadPayload {
                                                data,
                                                filename: "screenshot.png".to_string(),
                                            })
                                            .await;
                                    }
                                });
                            } else {
                                // Bracketed paste: wrap in ESC[200~ / ESC[201~ so the PTY app
                                // receives it as a paste rather than simulated keystrokes.
                                let mut data = Vec::with_capacity(text.len() + 12);
                                data.extend_from_slice(b"\x1b[200~");
                                data.extend_from_slice(text.as_bytes());
                                data.extend_from_slice(b"\x1b[201~");
                                let _ = writer
                                    .send(ClientMessage::PaneInput {
                                        tab_id: app.active_tab_id,
                                        pane_id: app.active_pane,
                                        data,
                                    })
                                    .await;
                            }
                        }
                    }
                    Some(Err(e)) => debug!("event stream error: {e}"),
                    None => {
                        eprintln!("Exiting: event stream closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

async fn do_launch(app: &mut App, writer: &IpcWriter) {
    let Some(modal) = &app.launch_modal else {
        return;
    };
    let cmd = orbt_tui::app::LAUNCH_AGENTS
        .get(modal.selected_agent)
        .map(|(cmd, _, _)| cmd.to_string())
        .unwrap_or_else(|| "claude".to_string());
    let name = modal.name.trim().to_string();
    let model = modal.model.trim().to_string();
    let cwd = if modal.cwd.trim().is_empty() {
        app.space_path.clone()
    } else {
        modal.cwd.trim().to_string()
    };
    let space_id = app
        .spaces
        .get(app.active_space_idx)
        .map(|s| s.space_id)
        .unwrap_or_default();
    app.launch_modal = None;
    let _ = writer
        .send(ClientMessage::AgentLaunch {
            config: orbt_protocol::AgentLaunchRequest {
                name: if name.is_empty() { cmd.clone() } else { name },
                model,
                cwd,
                space_id,
            },
        })
        .await;
    app.needs_redraw = true;
}

async fn handle_launch_key(key: KeyEvent, app: &mut App, writer: &IpcWriter) {
    use orbt_tui::app::LaunchFocus;
    let Some(modal) = &app.launch_modal else {
        return;
    };
    let n = orbt_tui::app::LAUNCH_AGENTS.len();
    let focus = modal.focus;

    match key.code {
        KeyCode::Esc => {
            app.launch_modal = None;
            app.needs_redraw = true;
        }
        KeyCode::Enter => {
            do_launch(app, writer).await;
        }
        // Tab / Shift-Tab cycle focus between the 4 sections
        KeyCode::Tab => {
            if let Some(m) = &mut app.launch_modal {
                m.focus = m.focus.next();
            }
            app.needs_redraw = true;
        }
        KeyCode::BackTab => {
            if let Some(m) = &mut app.launch_modal {
                m.focus = m.focus.prev();
            }
            app.needs_redraw = true;
        }
        // Arrow keys only navigate the agent list when that section is focused
        KeyCode::Up if focus == LaunchFocus::AgentList => {
            let sel = modal.selected_agent;
            if let Some(m) = &mut app.launch_modal {
                m.selected_agent = if sel == 0 { n - 1 } else { sel - 1 };
            }
            app.needs_redraw = true;
        }
        KeyCode::Down if focus == LaunchFocus::AgentList => {
            let sel = modal.selected_agent;
            if let Some(m) = &mut app.launch_modal {
                m.selected_agent = (sel + 1) % n;
            }
            app.needs_redraw = true;
        }
        // Backspace: delete last char from focused text field
        KeyCode::Backspace => {
            if let Some(m) = &mut app.launch_modal {
                match m.focus {
                    LaunchFocus::Name  => { m.name.pop(); }
                    LaunchFocus::Model => { m.model.pop(); }
                    LaunchFocus::Cwd   => { m.cwd.pop(); }
                    LaunchFocus::AgentList => {}
                }
            }
            app.needs_redraw = true;
        }
        // Printable chars: append to focused text field
        KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            if let Some(m) = &mut app.launch_modal {
                match m.focus {
                    LaunchFocus::Name  => m.name.push(c),
                    LaunchFocus::Model => m.model.push(c),
                    LaunchFocus::Cwd   => m.cwd.push(c),
                    LaunchFocus::AgentList => {}
                }
            }
            app.needs_redraw = true;
        }
        _ => {}
    }
}

async fn handle_launch_modal_mouse(
    mouse: crossterm::event::MouseEvent,
    app: &mut App,
    _writer: &IpcWriter,
    term_size: ratatui::layout::Rect,
) {
    use orbt_tui::app::LaunchFocus;
    use orbt_tui::tui::widgets::launch_modal::{MODAL_H, MODAL_W};

    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    let Some(_modal) = &app.launch_modal else {
        return;
    };

    let modal_w = MODAL_W.min(term_size.width.saturating_sub(4));
    let modal_h = MODAL_H.min(term_size.height.saturating_sub(4));
    let modal_x = (term_size.width.saturating_sub(modal_w)) / 2;
    let modal_y = (term_size.height.saturating_sub(modal_h)) / 2;

    // Dismiss on click outside.
    if mouse.column < modal_x
        || mouse.column >= modal_x + modal_w
        || mouse.row < modal_y
        || mouse.row >= modal_y + modal_h
    {
        app.launch_modal = None;
        app.needs_redraw = true;
        return;
    }

    let n = orbt_tui::app::LAUNCH_AGENTS.len() as u16;
    // Agent list box: rows modal_y+3 .. modal_y+3+n  (border+label+box-border = 3 offset; +1 inner start)
    let agents_start = modal_y + 3;
    let agents_end = agents_start + n;

    if mouse.row >= agents_start && mouse.row < agents_end {
        let idx = (mouse.row - agents_start) as usize;
        if let Some(m) = &mut app.launch_modal {
            m.selected_agent = idx;
            m.focus = LaunchFocus::AgentList;
        }
        app.needs_redraw = true;
        return;
    }

    // Name field inner row: modal_y + 3+n+2 + 1 (box border) + 1 (label) + 1 (box top) = agents_end+4
    let name_row = agents_end + 4;
    let model_row = name_row + 4;
    let cwd_row = model_row + 4;

    for (field_row, focus) in [
        (name_row, LaunchFocus::Name),
        (model_row, LaunchFocus::Model),
        (cwd_row, LaunchFocus::Cwd),
    ] {
        if mouse.row >= field_row.saturating_sub(2) && mouse.row <= field_row + 1 {
            if let Some(m) = &mut app.launch_modal {
                m.focus = focus;
            }
            app.needs_redraw = true;
            return;
        }
    }
}

async fn handle_eclipse_modal_mouse(
    mouse: crossterm::event::MouseEvent,
    app: &mut App,
    writer: &IpcWriter,
    term_size: ratatui::layout::Rect,
) {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return;
    }
    let Some(modal) = &app.eclipse_modal else {
        return;
    };
    let agent_id = modal.agent_id;

    // Compute modal geometry (mirrors eclipse_modal widget).
    let modal_w = 64u16.min(term_size.width.saturating_sub(4));
    let modal_h = 18u16.min(term_size.height.saturating_sub(4));
    let modal_x = (term_size.width.saturating_sub(modal_w)) / 2;
    let modal_y = (term_size.height.saturating_sub(modal_h)) / 2;
    let inner_x = modal_x + 1;

    // Dismiss on click outside the modal.
    if mouse.column < modal_x
        || mouse.column >= modal_x + modal_w
        || mouse.row < modal_y
        || mouse.row >= modal_y + modal_h
    {
        app.eclipse_modal = None;
        app.needs_redraw = true;
        return;
    }

    // Buttons are at modal_y + modal_h - 2 (second-to-last row, inside bottom border).
    if modal_h < 15 {
        return;
    }
    let btn_row = modal_y + modal_h.saturating_sub(2);
    if mouse.row != btn_row {
        return;
    }

    // Column ranges relative to inner_x (mirrors render: " [Send]  [Cancel]  [Abort Eclipse]").
    let col_in = mouse.column.saturating_sub(inner_x);
    if (1..=6).contains(&col_in) {
        // [Send]
        let response = modal.response.clone();
        app.eclipse_modal = None;
        let _ = writer
            .send(ClientMessage::AgentRespond { agent_id, response })
            .await;
        app.needs_redraw = true;
    } else if (9..=16).contains(&col_in) {
        // [Cancel]
        app.eclipse_modal = None;
        app.needs_redraw = true;
    } else if (19..=33).contains(&col_in) {
        // [Abort Eclipse]
        app.eclipse_modal = None;
        let _ = writer.send(ClientMessage::AgentAbort { agent_id }).await;
        app.needs_redraw = true;
    }
}

async fn handle_mobile_mouse(
    mouse: crossterm::event::MouseEvent,
    app: &mut App,
    writer: &IpcWriter,
    term_size: ratatui::layout::Rect,
) {
    let term_h = term_size.height;
    let term_w = term_size.width;
    let nav_row = term_h.saturating_sub(1);

    // Modals take priority.
    if app.eclipse_modal.is_some() {
        handle_eclipse_modal_mouse(mouse, app, writer, term_size).await;
        return;
    }
    if app.launch_modal.is_some() {
        handle_launch_modal_mouse(mouse, app, writer, term_size).await;
        return;
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.row == nav_row && term_w >= 8 {
                // Bottom nav bar click: determine which of the 4 tabs was clicked.
                let tab_w = term_w / 4;
                let col = mouse.column;
                let view = if col < tab_w {
                    MobileView::Terminal
                } else if col < tab_w * 2 {
                    MobileView::Windows
                } else if col < tab_w * 3 {
                    MobileView::Actions
                } else {
                    MobileView::Agents
                };
                app.mobile_view = view;
                if view == MobileView::Actions {
                    app.mode = InputMode::CommandPalette {
                        search: String::new(),
                        selected: 0,
                        search_focused: false,
                    };
                } else {
                    app.mode = InputMode::Normal;
                }
                app.needs_redraw = true;
                return;
            }

            // Content area clicks (row 1..nav_row).
            if mouse.row > 0 && mouse.row < nav_row {
                match app.mobile_view {
                    MobileView::Terminal => {
                        // Forward click to PTY.
                        let pane_area = ratatui::layout::Rect {
                            x: 0,
                            y: 1,
                            width: term_w,
                            height: nav_row.saturating_sub(1),
                        };
                        // Check for pane click — same logic as desktop but with mobile area.
                        let leaves = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
                        for (pid, rect) in &leaves {
                            if mouse.column >= rect.x
                                && mouse.column < rect.x + rect.width
                                && mouse.row >= rect.y
                                && mouse.row < rect.y + rect.height
                            {
                                if app.active_pane != *pid {
                                    app.active_pane = *pid;
                                    let _ = writer
                                        .send(ClientMessage::FocusPane {
                                            tab_id: app.active_tab_id,
                                            pane_id: *pid,
                                        })
                                        .await;
                                    app.needs_redraw = true;
                                }
                                break;
                            }
                        }
                    }
                    MobileView::Agents => {
                        // Mobile agents header at row 1 (first content row):
                        // "[+ New]" occupies the last 7 cols.
                        if mouse.row == 1 && mouse.column + 7 >= term_w {
                            orbt_tui::tui::widgets::launch_modal::open(app);
                            app.needs_redraw = true;
                        }
                        // Footer row ("[+] Add Agent") is pinned to the last content row.
                        let content_h = nav_row.saturating_sub(1);
                        let footer_row = 1u16 + content_h.saturating_sub(1);
                        if mouse.row == footer_row {
                            orbt_tui::tui::widgets::launch_modal::open(app);
                        }
                    }
                    MobileView::Windows => {
                        use orbt_tui::tui::widgets::mobile_confirm::{
                            hit_test as confirm_hit_test, ConfirmHit,
                        };
                        use orbt_tui::tui::widgets::mobile_spaces::{hit_test, SpacesHit};
                        let content_area = ratatui::layout::Rect {
                            x: 0,
                            y: 1,
                            width: term_w,
                            height: nav_row.saturating_sub(1),
                        };
                        // If the confirm modal is open, route all clicks to it.
                        if app.mobile_close_confirm.is_some() {
                            match confirm_hit_test(mouse.column, mouse.row, content_area) {
                                ConfirmHit::Confirm => {
                                    if let Some(confirm) = app.mobile_close_confirm.take() {
                                        apply_mobile_close(confirm, app, writer).await;
                                    }
                                }
                                ConfirmHit::Cancel | ConfirmHit::Outside => {
                                    app.mobile_close_confirm = None;
                                }
                            }
                            app.needs_redraw = true;
                        } else {
                            match hit_test(mouse.column, mouse.row, content_area, app) {
                                SpacesHit::Space(idx) => {
                                    app.mobile_spaces_cursor = idx;
                                    app.mobile_col_focus = MobileColFocus::Left;
                                    let space_id = app.spaces[idx].space_id;
                                    let _ = writer
                                        .send(ClientMessage::SwitchSpace { space_id })
                                        .await;
                                    app.active_space_idx = idx;
                                    app.mobile_tabs_cursor = 0;
                                }
                                SpacesHit::SpaceClose(idx) => {
                                    if app.spaces.len() > 1 {
                                        app.mobile_close_confirm = Some(MobileCloseConfirm {
                                            target: MobileCloseTarget::Space(idx),
                                            confirm_focused: false,
                                        });
                                    }
                                }
                                SpacesHit::NewSpace => {
                                    let _ = writer
                                        .send(ClientMessage::CreateSpace { name: None })
                                        .await;
                                }
                                SpacesHit::Tab(idx) => {
                                    app.mobile_tabs_cursor = idx;
                                    app.mobile_col_focus = MobileColFocus::Right;
                                    app.selection = None;
                                    app.active_tab = idx;
                                    app.active_tab_id = app.tabs[idx].id;
                                    if let Some(&first) = app.pane_tree().leaves().first() {
                                        app.active_pane = first;
                                    }
                                    let _ = writer
                                        .send(ClientMessage::SwitchTab {
                                            tab_id: app.active_tab_id,
                                        })
                                        .await;
                                    app.mobile_view = MobileView::Terminal;
                                }
                                SpacesHit::TabClose(idx) => {
                                    if app.tabs.len() > 1 {
                                        app.mobile_close_confirm = Some(MobileCloseConfirm {
                                            target: MobileCloseTarget::Tab(idx),
                                            confirm_focused: false,
                                        });
                                    }
                                }
                                SpacesHit::NewTab => {
                                    let _ = writer
                                        .send(ClientMessage::NewTab { name: None })
                                        .await;
                                }
                                SpacesHit::None => {}
                            }
                            app.needs_redraw = true;
                        }
                    }
                    MobileView::Actions => {
                        // Forward click to the command palette.
                        // Compute palette geometry (mirrors command_palette::render).
                        let content_h = nav_row.saturating_sub(1);
                        let content_area = ratatui::layout::Rect {
                            x: 0,
                            y: 1,
                            width: term_w,
                            height: content_h,
                        };
                        let palette_w = 50u16.min(content_area.width.saturating_sub(4));
                        let palette_h = 20u16.min(content_area.height.saturating_sub(4));
                        let px = content_area.x + (content_area.width - palette_w) / 2;
                        let py = content_area.y + (content_area.height - palette_h) / 2;
                        // Inner list starts after border(1) + search(1) + separator(1) = row 3
                        let list_start_y = py + 3;
                        let list_h = palette_h.saturating_sub(4) as usize;

                        if mouse.column >= px
                            && mouse.column < px + palette_w
                            && mouse.row >= list_start_y
                            && mouse.row < list_start_y + list_h as u16
                        {
                            let clicked_row = (mouse.row - list_start_y) as usize;
                            if let InputMode::CommandPalette { search, selected, .. } =
                                &mut app.mode
                            {
                                let filtered = filtered_indices(search);
                                // Build row offsets accounting for group headers
                                // (group headers only appear when search is empty)
                                let mut row = 0usize;
                                let mut last_group = "";
                                for (vis_idx, &cmd_idx) in filtered.iter().enumerate() {
                                    let cmd = &COMMANDS[cmd_idx];
                                    if cmd.group != last_group && search.is_empty() {
                                        if row == clicked_row {
                                            // clicked a group header — ignore
                                            break;
                                        }
                                        row += 1;
                                        last_group = cmd.group;
                                    }
                                    if row == clicked_row {
                                        *selected = vis_idx;
                                        let cmd_id = COMMANDS[cmd_idx].id;
                                        app.mode = InputMode::Normal;
                                        execute_command(cmd_id, app, writer, term_h).await;
                                        return;
                                    }
                                    row += 1;
                                }
                            }
                        }
                        app.needs_redraw = true;
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if app.mobile_view == MobileView::Windows {
                match app.mobile_col_focus {
                    MobileColFocus::Left => {
                        app.mobile_spaces_cursor =
                            app.mobile_spaces_cursor.saturating_sub(1);
                    }
                    MobileColFocus::Right => {
                        app.mobile_tabs_cursor = app.mobile_tabs_cursor.saturating_sub(1);
                    }
                }
                app.needs_redraw = true;
            } else if app.mobile_view == MobileView::Actions {
                if let InputMode::CommandPalette { selected, .. } = &mut app.mode {
                    *selected = selected.saturating_sub(1);
                    app.needs_redraw = true;
                }
            } else if app.mobile_view == MobileView::Terminal
                && matches!(app.mode, InputMode::Scroll { .. })
            {
                if let InputMode::Scroll { offset } = &mut app.mode {
                    *offset = (*offset + 3).min(
                        app.panes
                            .get(&app.active_pane)
                            .map(|p| p.scrollback.len())
                            .unwrap_or(0),
                    );
                }
                app.needs_redraw = true;
            }
        }
        MouseEventKind::ScrollDown => {
            if app.mobile_view == MobileView::Windows {
                match app.mobile_col_focus {
                    MobileColFocus::Left => {
                        let max = app.spaces.len();
                        app.mobile_spaces_cursor =
                            (app.mobile_spaces_cursor + 1).min(max);
                    }
                    MobileColFocus::Right => {
                        let max = app.tabs.len();
                        app.mobile_tabs_cursor = (app.mobile_tabs_cursor + 1).min(max);
                    }
                }
                app.needs_redraw = true;
            } else if app.mobile_view == MobileView::Actions {
                if let InputMode::CommandPalette { search, selected, .. } = &mut app.mode {
                    let max = filtered_indices(search).len();
                    if max > 0 {
                        *selected = (*selected + 1).min(max - 1);
                    }
                    app.needs_redraw = true;
                }
            } else if app.mobile_view == MobileView::Terminal {
                if let InputMode::Scroll { offset } = &mut app.mode {
                    *offset = offset.saturating_sub(3);
                    if *offset == 0 {
                        app.mode = InputMode::Normal;
                    }
                    app.needs_redraw = true;
                }
            }
        }
        _ => {}
    }
}

async fn handle_mouse(
    mouse: crossterm::event::MouseEvent,
    app: &mut App,
    writer: &IpcWriter,
    term_size: ratatui::layout::Rect,
) {
    // Mobile mode has its own simplified mouse handler.
    if app.mobile_mode {
        handle_mobile_mouse(mouse, app, writer, term_size).await;
        return;
    }

    if app.settings_open {
        return;
    }

    if let orbt_tui::app::InputMode::CommandPalette {
        search, selected, ..
    } = &mut app.mode
    {
        match mouse.kind {
            crossterm::event::MouseEventKind::ScrollUp => {
                *selected = selected.saturating_sub(1);
                app.needs_redraw = true;
            }
            crossterm::event::MouseEventKind::ScrollDown => {
                let s = search.to_lowercase();
                let max = if s.is_empty() {
                    COMMANDS.len()
                } else {
                    COMMANDS
                        .iter()
                        .filter(|c| c.label.to_lowercase().contains(&s))
                        .count()
                };
                if max > 0 {
                    *selected = (*selected + 1).min(max - 1);
                }
                app.needs_redraw = true;
            }
            _ => {}
        }
        return;
    }

    let ctx_info = app.context_menu.as_ref().map(|menu| {
        let menu_w: u16 = menu
            .items
            .iter()
            .filter_map(|i| match i {
                ContextMenuItem::Action {
                    label, shortcut, ..
                } => Some(label.len() + shortcut.len() + 2),
                _ => None,
            })
            .max()
            .unwrap_or(16) as u16
            + 4;
        let is_action: Vec<bool> = menu
            .items
            .iter()
            .map(|item| matches!(item, ContextMenuItem::Action { .. }))
            .collect();
        (menu.x, menu.y, menu_w, is_action)
    });

    if let Some((menu_x, menu_y, menu_w, is_action)) = ctx_info {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let mut clicked: Option<(&'static str, orbt_tui::app::ContextMenuTarget)> = None;
                for (i, &is_act) in is_action.iter().enumerate() {
                    if is_act
                        && mouse.row == menu_y + 1 + i as u16
                        && mouse.column >= menu_x
                        && mouse.column < menu_x + menu_w
                    {
                        if let Some(menu) = &app.context_menu {
                            if let Some(ContextMenuItem::Action { id, .. }) = menu.items.get(i) {
                                clicked = Some((*id, menu.target.clone()));
                            }
                        }
                        break;
                    }
                }
                if let Some((id, target)) = clicked {
                    app.context_menu = None;
                    app.needs_redraw = true;
                    execute_context_action(id, &target, app, writer).await;
                    return;
                }
                app.close_context_menu();
            }
            MouseEventKind::Down(MouseButton::Right) => {
                app.close_context_menu();
            }
            MouseEventKind::Moved => {
                let in_menu = mouse.row >= menu_y
                    && mouse.row < menu_y + 1 + is_action.len() as u16
                    && mouse.column >= menu_x
                    && mouse.column < menu_x + menu_w;
                if in_menu {
                    let mut new_selected =
                        app.context_menu.as_ref().map(|m| m.selected).unwrap_or(0);
                    for (i, &is_act) in is_action.iter().enumerate() {
                        if is_act && mouse.row == menu_y + 1 + i as u16 {
                            new_selected = i;
                        }
                    }
                    if let Some(ref mut m) = app.context_menu {
                        m.selected = new_selected;
                    }
                }
                app.needs_redraw = true;
            }
            _ => {}
        }
        return;
    }

    // Launch modal is open — only forward clicks to modal.
    if app.launch_modal.is_some() {
        handle_launch_modal_mouse(mouse, app, writer, term_size).await;
        return;
    }

    // Eclipse modal is open — only forward clicks to modal buttons.
    if app.eclipse_modal.is_some() {
        handle_eclipse_modal_mouse(mouse, app, writer, term_size).await;
        return;
    }

    // §6.7: Compact (<80 cols) collapses sidebar and hides agent panel.
    let sidebar_w: u16 = if term_size.width < 80 {
        SIDEBAR_COLLAPSED_W
    } else if app.sidebar_visible {
        SIDEBAR_W
    } else {
        SIDEBAR_COLLAPSED_W
    };
    let agent_w = agent_panel_width(term_size.width, app.agent_panel_mode);
    let term_w = term_size.width;
    let term_h = term_size.height;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.row == 0 {
                // Expand collapsed sidebar when clicking the » row
                if !app.sidebar_visible && mouse.column < SIDEBAR_COLLAPSED_W {
                    app.sidebar_visible = true;
                    app.needs_redraw = true;
                    app.needs_resize = true;
                    return;
                }
                if app.sidebar_visible && mouse.column < sidebar_w {
                    app.sidebar_visible = false;
                    app.needs_redraw = true;
                    app.needs_resize = true;
                    return;
                }
                let tab_x_start = sidebar_w;
                if mouse.column >= tab_x_start {
                    let mut x = tab_x_start;
                    for (i, tab) in app.tabs.iter().enumerate() {
                        let label_len = tab.name.len() as u16 + 2;
                        if mouse.column >= x && mouse.column < x + label_len {
                            app.selection = None;
                            app.active_tab = i;
                            app.active_tab_id = tab.id;
                            if let Some(&first) = app.pane_tree().leaves().first() {
                                app.active_pane = first;
                            }
                            app.drag_tab = Some(i);
                            app.needs_redraw = true;
                            return;
                        }
                        x += label_len;
                    }
                    if mouse.column >= x && mouse.column < x + 3 {
                        let _ = writer.send(ClientMessage::NewTab { name: None }).await;
                        app.needs_redraw = true;
                        return;
                    }
                    // "[A] Agent Fleet[badge] " — width varies with fleet badge.
                    let sats_badge_len: u16 =
                        if !app.agent_panel_mode.is_visible() && !app.agents.is_empty() {
                            if app.agents.len() < 10 {
                                3
                            } else {
                                4
                            }
                        } else {
                            0
                        };
                    let sats_w = 16u16 + sats_badge_len;
                    let sats_start = term_w.saturating_sub(agent_w + sats_w);
                    if mouse.column >= sats_start && mouse.column < term_w.saturating_sub(agent_w) {
                        app.agent_panel_mode = app.agent_panel_mode.cycle();
                        if app.agent_panel_mode.is_visible() {
                            let sel = if let InputMode::AgentPanel { selected } = app.mode {
                                selected
                            } else {
                                0
                            };
                            app.mode = InputMode::AgentPanel { selected: sel };
                        } else {
                            app.agent_hovered = None;
                            app.mode = InputMode::Normal;
                        }
                        app.needs_redraw = true;
                        return;
                    }
                }
            }

            if !app.sidebar_visible && mouse.column < SIDEBAR_COLLAPSED_W && mouse.row > 0 {
                let last = term_h.saturating_sub(2);
                let btn2 = term_h.saturating_sub(3);
                if mouse.row == last {
                    app.mode = InputMode::CommandPalette {
                        search: String::new(),
                        selected: 0,
                        search_focused: false,
                    };
                    app.needs_redraw = true;
                    return;
                }
                if mouse.row == btn2 {
                    let _ = writer
                        .send(orbt_protocol::ClientMessage::CreateSpace { name: None })
                        .await;
                    return;
                }
                let space_idx = (mouse.row - 1) as usize;
                if space_idx < app.spaces.len() {
                    app.active_space_idx = space_idx;
                    let space_id = app.spaces[space_idx].space_id;
                    let _ = writer
                        .send(orbt_protocol::ClientMessage::SwitchSpace { space_id })
                        .await;
                    app.needs_redraw = true;
                    return;
                }
            }
            // Sidebar: click a space card
            if app.sidebar_visible && mouse.column < SIDEBAR_W {
                let mut y: u16 = 2;
                for (i, space) in app.spaces.iter().enumerate() {
                    if mouse.row >= y && mouse.row < y + 4 {
                        let space_id = space.space_id;
                        app.active_space_idx = i;
                        let _ = writer
                            .send(orbt_protocol::ClientMessage::SwitchSpace { space_id })
                            .await;
                        app.needs_redraw = true;
                        return;
                    }
                    y += 4;
                }
                // Bottom bar: [+] New (left half) | ≡ Command (right half)
                // Rendered at sidebar_area.y + sidebar_area.height - 1
                // = 0 + (term_h - 1) - 1 = term_h - 2
                if mouse.row + 2 == term_h {
                    if mouse.column < SIDEBAR_W / 2 {
                        let _ = writer
                            .send(orbt_protocol::ClientMessage::CreateSpace { name: None })
                            .await;
                    } else {
                        app.mode = InputMode::CommandPalette {
                            search: String::new(),
                            selected: 0,
                            search_focused: false,
                        };
                    }
                    app.needs_redraw = true;
                    return;
                }
                return;
            }

            // Agent panel clicks
            if agent_w > 0 && mouse.column >= term_w.saturating_sub(agent_w) {
                let panel_x = term_w.saturating_sub(agent_w);
                let inner_x = panel_x + 1;
                let col_in_inner = mouse.column.saturating_sub(inner_x);

                // Footer row (last row): "[+] Add Satellite"
                if mouse.row + 1 == term_h {
                    orbt_tui::tui::widgets::launch_modal::open(app);
                    return;
                }

                // Header row (row 0): [+] and × buttons
                if mouse.row == 0 {
                    if mouse.column == term_w.saturating_sub(1) {
                        // × close button
                        app.agent_panel_mode = AgentPanelMode::Hidden;
                        app.mode = InputMode::Normal;
                        app.agent_hovered = None;
                        app.selection = None;
                        app.needs_redraw = true;
                        return;
                    }
                    if mouse.column >= term_w.saturating_sub(4)
                        && mouse.column <= term_w.saturating_sub(2)
                    {
                        // [+] add — open the Launch Agent picker overlay
                        orbt_tui::tui::widgets::launch_modal::open(app);
                        return;
                    }
                }

                // Eclipse banner [Respond] — row shifts when "N above" indicator is shown.
                let any_blocked = app
                    .agents
                    .iter()
                    .any(|a| a.status == orbt_protocol::AgentStatus::Blocked);
                // Eclipse banner spans 2 rows: text (banner_row) + [Respond] (banner_row+1).
                // Clicking anywhere on either row opens the Eclipse modal.
                let banner_row = 2u16 + if app.agent_scroll_offset > 0 { 1 } else { 0 };
                let respond_row = banner_row + 1;
                if any_blocked && (mouse.row == banner_row || mouse.row == respond_row) {
                    if let Some(blocked) = app
                        .agents
                        .iter()
                        .find(|a| a.status == orbt_protocol::AgentStatus::Blocked)
                    {
                        let agent_id = blocked.id;
                        orbt_tui::tui::widgets::eclipse_modal::open(app, agent_id);
                    }
                    return;
                }

                // Card button clicks — iterate only the visible (scrolled) agents.
                let base_row = orbt_tui::tui::widgets::agent_monitor::card_start_row(
                    0,
                    app.agent_scroll_offset,
                    any_blocked,
                    0,
                );
                let mut card_row_start = base_row;
                let scroll = app.agent_scroll_offset;
                // Collect (id, pane_id, status) so we can drop the borrow before mutations.
                let visible: Vec<(
                    orbt_protocol::AgentId,
                    Option<orbt_protocol::PaneId>,
                    orbt_protocol::AgentStatus,
                )> = app
                    .agents
                    .iter()
                    .skip(scroll)
                    .map(|a| (a.id, a.pane_id, a.status.clone()))
                    .collect();
                for (agent_id, agent_pane, agent_status) in visible {
                    // Narrow card slot: separator at slot+0, buttons at slot+5.
                    let btn_row = card_row_start + 5;
                    if mouse.row == btn_row {
                        // Button row of this card
                        let slot = if (1..=6).contains(&col_in_inner) {
                            Some(0u8)
                        } else if (8..=13).contains(&col_in_inner) {
                            Some(1)
                        } else if (15..=20).contains(&col_in_inner) {
                            Some(2)
                        } else {
                            None
                        };
                        if let Some(s) = slot {
                            match (s, &agent_status) {
                                // Slot 0: [View] — focus agent's pane, switching tabs if needed
                                (0, _) => {
                                    if let Some(pane_id) = agent_pane {
                                        let found =
                                            app.tabs.iter().enumerate().find(|(_, t)| {
                                                t.pane_tree.leaves().contains(&pane_id)
                                            });
                                        let tab_id =
                                            found.map(|(_, t)| t.id).unwrap_or(app.active_tab_id);
                                        let tab_idx =
                                            found.map(|(i, _)| i).unwrap_or(app.active_tab);
                                        app.active_pane = pane_id;
                                        app.active_tab = tab_idx;
                                        app.active_tab_id = tab_id;
                                        app.selection = None;
                                        let _ = writer
                                            .send(ClientMessage::FocusPane { tab_id, pane_id })
                                            .await;
                                    }
                                }
                                // Slot 1: [Stop] (Working)
                                (1, orbt_protocol::AgentStatus::Working) => {
                                    let _ =
                                        writer.send(ClientMessage::AgentAbort { agent_id }).await;
                                }
                                // Slot 2: [Chat] (Working) — focus pane to interact
                                (2, orbt_protocol::AgentStatus::Working) => {
                                    if let Some(pane_id) = agent_pane {
                                        let found =
                                            app.tabs.iter().enumerate().find(|(_, t)| {
                                                t.pane_tree.leaves().contains(&pane_id)
                                            });
                                        let tab_id =
                                            found.map(|(_, t)| t.id).unwrap_or(app.active_tab_id);
                                        let tab_idx =
                                            found.map(|(i, _)| i).unwrap_or(app.active_tab);
                                        app.active_pane = pane_id;
                                        app.active_tab = tab_idx;
                                        app.active_tab_id = tab_id;
                                        app.selection = None;
                                        let _ = writer
                                            .send(ClientMessage::FocusPane { tab_id, pane_id })
                                            .await;
                                    }
                                }
                                // Slot 1: [Resp] (Blocked) — open Eclipse modal
                                (1, orbt_protocol::AgentStatus::Blocked) => {
                                    orbt_tui::tui::widgets::eclipse_modal::open(app, agent_id);
                                }
                                // Slot 2: [Abrt] (Blocked)
                                (2, orbt_protocol::AgentStatus::Blocked) => {
                                    let _ =
                                        writer.send(ClientMessage::AgentAbort { agent_id }).await;
                                }
                                // Slot 1: [Chat] (Idle) / Slot 1: [Chat] (Done) — focus pane
                                (1, orbt_protocol::AgentStatus::Idle)
                                | (1, orbt_protocol::AgentStatus::Done) => {
                                    if let Some(pane_id) = agent_pane {
                                        let found =
                                            app.tabs.iter().enumerate().find(|(_, t)| {
                                                t.pane_tree.leaves().contains(&pane_id)
                                            });
                                        let tab_id =
                                            found.map(|(_, t)| t.id).unwrap_or(app.active_tab_id);
                                        let tab_idx =
                                            found.map(|(i, _)| i).unwrap_or(app.active_tab);
                                        app.active_pane = pane_id;
                                        app.active_tab = tab_idx;
                                        app.active_tab_id = tab_id;
                                        app.selection = None;
                                        let _ = writer
                                            .send(ClientMessage::FocusPane { tab_id, pane_id })
                                            .await;
                                    }
                                }
                                // Slot 2: [Rmov] (Idle / Done) — dismiss from list
                                (2, orbt_protocol::AgentStatus::Idle)
                                | (2, orbt_protocol::AgentStatus::Done) => {
                                    let _ =
                                        writer.send(ClientMessage::AgentRemove { agent_id }).await;
                                }
                                // Slot 1: [Rstr] (Error) — reset agent to Idle
                                (1, orbt_protocol::AgentStatus::Error) => {
                                    let _ =
                                        writer.send(ClientMessage::AgentRestart { agent_id }).await;
                                }
                                // Slot 2: [Rmov] (Error) — dismiss from list
                                (2, orbt_protocol::AgentStatus::Error) => {
                                    let _ =
                                        writer.send(ClientMessage::AgentRemove { agent_id }).await;
                                }
                                _ => {}
                            }
                        }
                        app.needs_redraw = true;
                        return;
                    }
                    if mouse.row > card_row_start && mouse.row < card_row_start + 6 {
                        // Click on card body (not separator, not buttons) — focus pane
                        if let Some(pane_id) = agent_pane {
                            let found = app
                                .tabs
                                .iter()
                                .enumerate()
                                .find(|(_, t)| t.pane_tree.leaves().contains(&pane_id));
                            let tab_id = found.map(|(_, t)| t.id).unwrap_or(app.active_tab_id);
                            let tab_idx = found.map(|(i, _)| i).unwrap_or(app.active_tab);
                            app.active_pane = pane_id;
                            app.active_tab = tab_idx;
                            app.active_tab_id = tab_id;
                            app.selection = None;
                            let _ = writer
                                .send(ClientMessage::FocusPane { tab_id, pane_id })
                                .await;
                        }
                        app.needs_redraw = true;
                        return;
                    }
                    card_row_start += 6;
                }
                app.needs_redraw = true;
                return;
            }

            let pane_area = content_area(term_size, app);
            if let Some((first_pane, second_pane, dir)) = orbt_tui::tui::find_split_at_cursor(
                app.pane_tree(),
                pane_area,
                mouse.column,
                mouse.row,
            ) {
                app.drag_split = Some((first_pane, second_pane, dir, -1.0));
                app.selection = None;
                return;
            }
            let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
            for (pid, rect) in &areas {
                if mouse.column >= rect.x
                    && mouse.column < rect.x + rect.width
                    && mouse.row >= rect.y
                    && mouse.row < rect.y + rect.height
                {
                    app.active_pane = *pid;
                    let _ = writer
                        .send(ClientMessage::FocusPane {
                            tab_id: app.active_tab_id,
                            pane_id: *pid,
                        })
                        .await;

                    // If the pane has mouse reporting enabled, forward the click
                    // as an SGR mouse escape sequence to the PTY.
                    let inner_x = rect.x + 1;
                    let inner_y = rect.y + 1;
                    let has_mouse = app
                        .panes
                        .get(pid)
                        .is_some_and(|p| p.parser.grid.mouse_reporting);
                    if has_mouse
                        && mouse.column >= inner_x
                        && mouse.row >= inner_y
                        && matches!(app.mode, InputMode::Normal)
                    {
                        let col = mouse.column - inner_x + 1; // 1-based for SGR
                        let row = mouse.row - inner_y + 1;
                        // SGR mouse press: \x1b[<0;col;rowM
                        let seq = format!("\x1b[<0;{col};{row}M");
                        let _ = writer
                            .send(ClientMessage::PaneInput {
                                tab_id: app.active_tab_id,
                                pane_id: *pid,
                                data: seq.into_bytes(),
                            })
                            .await;
                        app.selection = None;
                    } else if matches!(app.mode, InputMode::Normal) {
                        // Start selection at inner cell coords (account for border)
                        if mouse.column >= inner_x && mouse.row >= inner_y {
                            let col = mouse.column - inner_x;
                            let row = mouse.row - inner_y;
                            app.selection = Some(orbt_tui::app::Selection {
                                pane_id: *pid,
                                start: (col, row),
                                end: (col, row),
                                active: true,
                            });
                        } else {
                            app.selection = None;
                        }
                    }
                    app.needs_redraw = true;
                    return;
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            if mouse.row == 0 && mouse.column >= sidebar_w {
                // Tab bar right-click: show tab context menu
                let mut x = sidebar_w;
                for tab in app.tabs.iter() {
                    let label_len = tab.name.len() as u16 + 2;
                    if mouse.column >= x && mouse.column < x + label_len {
                        app.open_context_menu(mouse.column, 1, ContextMenuTarget::Tab(tab.id));
                        return;
                    }
                    x += label_len;
                }
                return;
            }
            if mouse.row == 0 {
                return;
            }
            if agent_w > 0 && mouse.column >= term_w.saturating_sub(agent_w) {
                // Right-click inside the agent panel — no context menu for agent cards yet.
            } else if app.sidebar_visible && mouse.column < sidebar_w {
                if mouse.row >= 2 {
                    let mut y: u16 = 2;
                    let mut clicked_space: Option<orbt_protocol::SpaceId> = None;
                    for space in &app.spaces {
                        if mouse.row >= y && mouse.row < y + 4 {
                            clicked_space = Some(space.space_id);
                            break;
                        }
                        y += 4;
                    }
                    if let Some(space_id) = clicked_space {
                        app.open_context_menu(
                            mouse.column,
                            mouse.row,
                            ContextMenuTarget::Space(space_id),
                        );
                    } else {
                        app.open_context_menu(mouse.column, mouse.row, ContextMenuTarget::Sidebar);
                    }
                } else {
                    app.open_context_menu(mouse.column, mouse.row, ContextMenuTarget::Sidebar);
                }
            } else {
                let pane_area = ratatui::layout::Rect {
                    x: sidebar_w,
                    y: 1,
                    width: term_w.saturating_sub(sidebar_w + agent_w),
                    height: term_h.saturating_sub(3),
                };
                let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
                let mut found_pane = None;
                for (pid, rect) in &areas {
                    if mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width
                        && mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height
                    {
                        found_pane = Some(*pid);
                        break;
                    }
                }
                let target = if let Some(pid) = found_pane {
                    ContextMenuTarget::Pane(pid)
                } else {
                    ContextMenuTarget::Pane(app.active_pane)
                };
                app.drag_tab = None;
                app.drag_split = None;
                app.open_context_menu(mouse.column, mouse.row, target);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let pane_area = content_area(term_size, app);
            if pane_area.width == 0 || pane_area.height == 0 {
                return;
            }
            let drag_update = if let Some(drag) = app.drag_split.as_mut() {
                let ratio = match drag.2 {
                    orbt_protocol::SplitDir::Horizontal => {
                        let total = pane_area.width as f32;
                        ((mouse.column as f32 - pane_area.x as f32) / total).clamp(0.1, 0.9)
                    }
                    orbt_protocol::SplitDir::Vertical => {
                        let total = pane_area.height as f32;
                        ((mouse.row as f32 - pane_area.y as f32) / total).clamp(0.1, 0.9)
                    }
                };
                if (ratio - drag.3).abs() >= 0.02 {
                    drag.3 = ratio;
                    Some((drag.0, drag.1, ratio))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((first_pane, second_pane, ratio)) = drag_update {
                if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                    tab.pane_tree
                        .set_split_ratio(first_pane, second_pane, ratio);
                }
                resize_local_grids(app, term_size.width, term_size.height);
                app.needs_redraw = true;
                return;
            }
            // Forward mouse drag to PTY if mouse reporting is active
            if app.drag_split.is_none() && app.drag_tab.is_none() {
                let has_mouse = app
                    .panes
                    .get(&app.active_pane)
                    .is_some_and(|p| p.parser.grid.mouse_reporting);
                if has_mouse {
                    let pane_area = content_area(term_size, app);
                    let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
                    for (pid, rect) in &areas {
                        if *pid == app.active_pane
                            && mouse.column > rect.x
                            && mouse.row > rect.y
                            && mouse.column < rect.x + rect.width
                            && mouse.row < rect.y + rect.height
                        {
                            let col = mouse.column - rect.x; // 1-based
                            let row = mouse.row - rect.y;
                            // SGR mouse drag: \x1b[<32;col;rowM (button 0 + 32 = motion)
                            let seq = format!("\x1b[<32;{col};{row}M");
                            let _ = writer
                                .send(ClientMessage::PaneInput {
                                    tab_id: app.active_tab_id,
                                    pane_id: *pid,
                                    data: seq.into_bytes(),
                                })
                                .await;
                            return;
                        }
                    }
                }
            }
            let drag_info = app
                .selection
                .as_ref()
                .filter(|s| s.active)
                .map(|s| s.pane_id);
            if let Some(sel_pane_id) = drag_info {
                let pane_area = content_area(term_size, app);
                let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
                for (pid, rect) in &areas {
                    if *pid == sel_pane_id {
                        let inner_x = rect.x + 1;
                        let inner_y = rect.y + 1;
                        let inner_w = rect.width.saturating_sub(2);
                        let inner_h = rect.height.saturating_sub(2);
                        let col = mouse
                            .column
                            .saturating_sub(inner_x)
                            .min(inner_w.saturating_sub(1));
                        let row = mouse
                            .row
                            .saturating_sub(inner_y)
                            .min(inner_h.saturating_sub(1));
                        if let Some(sel) = &mut app.selection {
                            sel.end = (col, row);
                            app.needs_redraw = true;
                        }
                        break;
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            // Forward mouse release to PTY if mouse reporting is active
            if app.drag_split.is_none() && app.drag_tab.is_none() {
                let pane_area = content_area(term_size, app);
                let areas = orbt_tui::tui::compute_leaf_areas(app.pane_tree(), pane_area);
                for (pid, rect) in &areas {
                    if *pid == app.active_pane
                        && mouse.column > rect.x
                        && mouse.row > rect.y
                        && mouse.column < rect.x + rect.width
                        && mouse.row < rect.y + rect.height
                    {
                        let has_mouse = app
                            .panes
                            .get(pid)
                            .is_some_and(|p| p.parser.grid.mouse_reporting);
                        if has_mouse {
                            let col = mouse.column - rect.x; // 1-based
                            let row = mouse.row - rect.y;
                            // SGR mouse release: \x1b[<0;col;rowm (lowercase m)
                            let seq = format!("\x1b[<0;{col};{row}m");
                            let _ = writer
                                .send(ClientMessage::PaneInput {
                                    tab_id: app.active_tab_id,
                                    pane_id: *pid,
                                    data: seq.into_bytes(),
                                })
                                .await;
                        }
                        break;
                    }
                }
            }
            if let Some((first_pane, second_pane, _dir, _)) = app.drag_split.take() {
                let pane_area = content_area(term_size, app);
                if pane_area.width > 0 && pane_area.height > 0 {
                    let ratio = match _dir {
                        orbt_protocol::SplitDir::Horizontal => {
                            let total = pane_area.width as f32;
                            ((mouse.column as f32 - pane_area.x as f32) / total).clamp(0.1, 0.9)
                        }
                        orbt_protocol::SplitDir::Vertical => {
                            let total = pane_area.height as f32;
                            ((mouse.row as f32 - pane_area.y as f32) / total).clamp(0.1, 0.9)
                        }
                    };
                    let _ = writer
                        .send(ClientMessage::ResizeSplit {
                            tab_id: app.active_tab_id,
                            first_pane,
                            second_pane,
                            ratio,
                        })
                        .await;
                }
            }
            if let Some(sel) = &mut app.selection {
                sel.active = false;
                if sel.start == sel.end {
                    app.selection = None;
                }
                app.needs_redraw = true;
            }
            if let Some(from_idx) = app.drag_tab.take() {
                if mouse.row == 0 && mouse.column >= sidebar_w {
                    if let Some(from_tab) = app.tabs.get(from_idx) {
                        let from_tab_id = from_tab.id;
                        let mut x = sidebar_w;
                        for (i, tab) in app.tabs.iter().enumerate() {
                            let label_len = tab.name.len() as u16 + 2;
                            if mouse.column >= x && mouse.column < x + label_len && i != from_idx {
                                let _ = writer
                                    .send(ClientMessage::ReorderTab {
                                        tab_id: from_tab_id,
                                        to_index: i,
                                    })
                                    .await;
                                break;
                            }
                            x += label_len;
                        }
                    }
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if agent_w > 0 && mouse.column >= term_w.saturating_sub(agent_w) {
                app.agent_scroll_offset = app.agent_scroll_offset.saturating_sub(1);
                app.needs_redraw = true;
            } else if let InputMode::Scroll { offset } = &mut app.mode {
                *offset += 3;
                app.needs_redraw = true;
            }
        }
        MouseEventKind::ScrollDown => {
            if agent_w > 0 && mouse.column >= term_w.saturating_sub(agent_w) {
                let banner_rows: u16 = if app
                    .agents
                    .iter()
                    .any(|a| a.status == orbt_protocol::AgentStatus::Blocked)
                {
                    2
                } else {
                    0
                };
                // Use above_row=1 for max_scroll ceiling: after any scroll the "N above"
                // indicator will be present, consuming one row.
                let visible = ((term_h.saturating_sub(5 + banner_rows + 1)) / 6).max(1) as usize;
                let max_scroll = app.agents.len().saturating_sub(visible);
                app.agent_scroll_offset = (app.agent_scroll_offset + 1).min(max_scroll);
                app.needs_redraw = true;
            } else if let InputMode::Scroll { offset } = &mut app.mode {
                *offset = offset.saturating_sub(3);
                if *offset == 0 {
                    app.mode = InputMode::Normal;
                }
                app.needs_redraw = true;
            }
        }
        MouseEventKind::Moved => {
            let toggle_hovered = if app.sidebar_visible {
                mouse.row == 0 && mouse.column < SIDEBAR_W
            } else {
                mouse.row == 0 && mouse.column < SIDEBAR_COLLAPSED_W
            };
            if app.sidebar_toggle_hovered != toggle_hovered {
                app.sidebar_toggle_hovered = toggle_hovered;
                app.needs_redraw = true;
            }

            let sb_w = if app.sidebar_visible {
                SIDEBAR_W
            } else {
                SIDEBAR_COLLAPSED_W
            };
            if mouse.column >= sb_w && app.sidebar_hovered.is_some() {
                app.sidebar_hovered = None;
                app.needs_redraw = true;
            }

            // Tab bar hover (row 0 of the frame, after the sidebar).
            if mouse.row == 0 && mouse.column >= sb_w {
                let col = mouse.column - sb_w;
                let mut acc: u16 = 0;
                let mut hovered = None;
                for (i, tab) in app.tabs.iter().enumerate() {
                    let w = tab.name.len() as u16 + 2; // " name "
                    if col < acc + w {
                        hovered = Some(i);
                        break;
                    }
                    acc += w;
                }
                // Check new-tab button
                if hovered.is_none() && col < acc + 3 {
                    hovered = Some(app.tabs.len());
                }
                // Check [A] Agent Fleet button (right-aligned, width varies with fleet badge).
                if hovered.is_none() {
                    let sats_badge_len: u16 =
                        if !app.agent_panel_mode.is_visible() && !app.agents.is_empty() {
                            if app.agents.len() < 10 {
                                3
                            } else {
                                4
                            }
                        } else {
                            0
                        };
                    let badge_start = term_w.saturating_sub(agent_w + 16 + sats_badge_len);
                    let badge_end = term_w.saturating_sub(agent_w);
                    if mouse.column >= badge_start && mouse.column < badge_end {
                        hovered = Some(app.tabs.len() + 1);
                    }
                }
                if app.tab_hovered != hovered {
                    app.tab_hovered = hovered;
                    app.needs_redraw = true;
                }
            } else if app.tab_hovered.is_some() {
                app.tab_hovered = None;
                app.needs_redraw = true;
            }

            // Agent panel hover
            if agent_w > 0 && mouse.column >= term_w.saturating_sub(agent_w) {
                let panel_x = term_w.saturating_sub(agent_w);
                let inner_x = panel_x + 1;
                let col_in_inner = mouse.column.saturating_sub(inner_x);
                let any_blocked = app
                    .agents
                    .iter()
                    .any(|a| a.status == orbt_protocol::AgentStatus::Blocked);

                let new_hover = if mouse.row == 0 {
                    if mouse.column == term_w.saturating_sub(1) {
                        Some(AgentHover::HeaderClose)
                    } else if (term_w.saturating_sub(4)..=term_w.saturating_sub(2))
                        .contains(&mouse.column)
                    {
                        Some(AgentHover::HeaderAdd)
                    } else {
                        None
                    }
                } else if mouse.row + 1 == term_h {
                    Some(AgentHover::PanelFooter)
                } else if any_blocked && {
                    let banner_row = 2u16 + if app.agent_scroll_offset > 0 { 1 } else { 0 };
                    // [Respond] occupies the last 9 cols: col_in_inner >= iw-9 = agent_w-10
                    mouse.row == banner_row + 1 && col_in_inner >= agent_w.saturating_sub(10)
                } {
                    Some(AgentHover::EclipseRespond)
                } else {
                    let base_row = orbt_tui::tui::widgets::agent_monitor::card_start_row(
                        0,
                        app.agent_scroll_offset,
                        any_blocked,
                        0,
                    );
                    let mut card_row_start = base_row;
                    let mut found = None;
                    for (card_idx, _) in app.agents.iter().skip(app.agent_scroll_offset).enumerate()
                    {
                        // Slot+0 is separator; content rows 1-5; button row = slot+5.
                        if mouse.row > card_row_start && mouse.row < card_row_start + 6 {
                            let card_row = mouse.row - card_row_start;
                            if card_row == 5 {
                                let slot = if (1..=6).contains(&col_in_inner) {
                                    Some(0u8)
                                } else if (8..=13).contains(&col_in_inner) {
                                    Some(1)
                                } else if (15..=20).contains(&col_in_inner) {
                                    Some(2)
                                } else {
                                    None
                                };
                                if let Some(s) = slot {
                                    found = Some(AgentHover::CardBtn { card_idx, slot: s });
                                }
                            }
                            break;
                        }
                        card_row_start += 6;
                    }
                    found
                };

                if app.agent_hovered != new_hover {
                    app.agent_hovered = new_hover;
                    app.needs_redraw = true;
                }
            } else if app.agent_hovered.is_some() {
                app.agent_hovered = None;
                app.needs_redraw = true;
            }

            // Sidebar card hover
            if app.sidebar_visible && mouse.column < SIDEBAR_W {
                let mut y: u16 = 2;
                let mut hovered = None;
                for (i, _space) in app.spaces.iter().enumerate() {
                    if mouse.row >= y && mouse.row < y + 4 {
                        hovered = Some(i);
                        break;
                    }
                    y += 4;
                }
                if hovered.is_none() && mouse.row + 2 == term_h {
                    if mouse.column < SIDEBAR_W / 2 {
                        hovered = Some(app.spaces.len());
                    } else {
                        hovered = Some(app.spaces.len() + 1);
                    }
                }
                if app.sidebar_hovered != hovered {
                    app.sidebar_hovered = hovered;
                    app.needs_redraw = true;
                }
            } else if !app.sidebar_visible && mouse.column < SIDEBAR_COLLAPSED_W {
                let last = term_h.saturating_sub(2);
                let btn2 = term_h.saturating_sub(3);
                let hovered = if mouse.row == last {
                    Some(app.spaces.len() + 1)
                } else if mouse.row == btn2 {
                    Some(app.spaces.len())
                } else {
                    None
                };
                if app.sidebar_hovered != hovered {
                    app.sidebar_hovered = hovered;
                    app.needs_redraw = true;
                }
            } else if app.sidebar_hovered.is_some() {
                app.sidebar_hovered = None;
                app.needs_redraw = true;
            }
        }
        _ => {}
    }
}
