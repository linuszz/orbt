pub mod theme;
pub mod widgets;

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use orbt_protocol::{PaneId, SplitDir, TermColor};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::io::{self, Stdout};

use crate::app::{AgentPanelMode, App, InputMode, MobileView, PaneState, Selection};
use orbt_protocol::Cell;
use orbt_protocol::PaneLayout;
use theme::*;
use unicode_width::UnicodeWidthChar;

pub type OrbitTerminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

pub fn setup_terminal() -> io::Result<OrbitTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    ratatui::Terminal::new(CrosstermBackend::new(stdout))
}

pub fn restore_terminal(terminal: &mut OrbitTerminal) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    Ok(())
}

pub fn term_color(c: &TermColor) -> Color {
    match c {
        TermColor::Default => Color::Reset,
        TermColor::Ansi(n) => Color::Indexed(*n),
        TermColor::Ansi256(n) => Color::Indexed(*n),
        TermColor::Rgb(r, g, b) => Color::Rgb(*r, *g, *b),
    }
}

pub const SIDEBAR_W: u16 = 24;
pub const SIDEBAR_COLLAPSED_W: u16 = 5;

/// §6.7 responsive agent panel width.
/// Only Sidebar mode occupies layout columns; Modal floats and returns 0.
/// Returns 0 when the fleet feature is disabled.
pub fn agent_panel_width(term_w: u16, mode: AgentPanelMode) -> u16 {
    if mode != AgentPanelMode::Sidebar || term_w < 80 {
        0
    } else if term_w >= 140 {
        25
    } else {
        22
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // §6.7: Compact (<80 cols) — sidebar collapsed to icon-only width.
    let sidebar_w = if area.width < 80 {
        SIDEBAR_COLLAPSED_W
    } else if app.sidebar_visible {
        SIDEBAR_W
    } else {
        SIDEBAR_COLLAPSED_W
    };
    let effective_mode = if app.agent_fleet_enabled {
        app.agent_panel_mode
    } else {
        AgentPanelMode::Hidden
    };
    let agent_w = agent_panel_width(area.width, effective_mode);

    let cols = ratatui::layout::Layout::horizontal([
        ratatui::layout::Constraint::Length(sidebar_w),
        ratatui::layout::Constraint::Fill(1),
        ratatui::layout::Constraint::Length(agent_w),
    ])
    .split(area);

    let sidebar_area = Rect {
        x: cols[0].x,
        y: cols[0].y,
        width: cols[0].width,
        height: cols[0].height.saturating_sub(1),
    };
    widgets::spaces_sidebar::render(frame, sidebar_area, app);

    let border_y = area.y + area.height - 1;
    let sep = "\u{2500}";
    let sep_style = Style::default().fg(border());
    if cols[0].width > 0 {
        let line: String = sep.repeat(cols[0].width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(line, sep_style))),
            Rect {
                x: cols[0].x,
                y: border_y,
                width: cols[0].width,
                height: 1,
            },
        );
    }

    let right = Rect {
        x: cols[1].x,
        y: cols[1].y,
        width: cols[1].width,
        height: cols[1].height,
    };

    let rows = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Fill(1),
        ratatui::layout::Constraint::Length(2),
    ])
    .split(right);

    widgets::tab_bar::render(frame, rows[0], app);
    frame.render_widget(Clear, rows[1]);
    render_pane_tree(frame, rows[1], app.pane_tree(), app);
    let status_inner = Rect {
        x: rows[2].x,
        y: rows[2].y,
        width: rows[2].width,
        height: 1,
    };
    let border_y = rows[2].y + 1;
    widgets::status_bar::render(frame, status_inner, app);

    if app.agent_fleet_enabled {
        match app.agent_panel_mode {
            AgentPanelMode::Sidebar => {
                let agent_area = Rect {
                    x: cols[2].x,
                    y: cols[2].y,
                    width: cols[2].width,
                    height: cols[2].height.saturating_sub(1),
                };
                widgets::agent_monitor::render(frame, agent_area, app);
            }
            AgentPanelMode::Hidden => {}
        }
    }

    let sep = "\u{2500}";
    let sep_style = Style::default().fg(border()).bg(bg_primary());
    if cols[0].width > 0 {
        let rect = Rect {
            x: cols[0].x,
            y: border_y,
            width: cols[0].width,
            height: 1,
        };
        let line: String = sep.repeat(rect.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(line, sep_style))),
            rect,
        );
    }
    if cols[1].width > 0 {
        let rect = Rect {
            x: cols[1].x,
            y: border_y,
            width: cols[1].width,
            height: 1,
        };
        let line: String = sep.repeat(rect.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(line, sep_style))),
            rect,
        );
    }
    if cols[2].width > 0 {
        let rect = Rect {
            x: cols[2].x,
            y: border_y,
            width: cols[2].width,
            height: 1,
        };
        let line: String = sep.repeat(rect.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(line, sep_style))),
            rect,
        );
    }

    if app.show_help {
        render_help_overlay(frame, area);
    }

    if app.context_menu.is_some() {
        widgets::context_menu::render(frame, area, app);
    }

    if matches!(app.mode, crate::app::InputMode::CommandPalette { .. }) {
        widgets::command_palette::render(frame, area, app);
    }

    if app.agent_fleet_enabled {
        if matches!(
            app.mode,
            crate::app::InputMode::AgentFullScreen { .. }
                | crate::app::InputMode::PromptInput { .. }
        ) {
            let any_blocked = app
                .agents
                .iter()
                .any(|a| a.status == orbt_protocol::AgentStatus::Blocked);
            let modal_area = widgets::agent_monitor::fs_modal_layout(area, any_blocked).area;
            let buf = frame.buffer_mut();
            let buf_area = buf.area;
            for cy in buf_area.y..buf_area.y + buf_area.height {
                for cx in buf_area.x..buf_area.x + buf_area.width {
                    let in_modal = cx >= modal_area.x
                        && cx < modal_area.x + modal_area.width
                        && cy >= modal_area.y
                        && cy < modal_area.y + modal_area.height;
                    if !in_modal {
                        let cell = &mut buf[(cx, cy)];
                        cell.modifier.insert(Modifier::DIM);
                    }
                }
            }
            widgets::agent_monitor::render_fullscreen_modal(frame, area, app);
        }
        if app.agent_detail_modal.is_some() {
            widgets::agent_detail_modal::render(frame, area, app);
        }
        if app.eclipse_modal.is_some() {
            widgets::eclipse_modal::render(frame, area, app);
        }
        widgets::agent_monitor::render_inspect_overlay(frame, area, app);
        widgets::agent_monitor::render_toast(frame, area, app);
    }

    if app.launch_modal.is_some() {
        widgets::launch_modal::render(frame, area, app);
    }

    if app.settings_open {
        widgets::settings_modal::render(frame, area, app);
    }
}

/// Mobile layout: header (1 row) + content (fills) + nav bar (1 row).
pub fn render_mobile(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.height < 3 {
        return;
    }

    let header_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let nav_area = Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    };
    let content_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(2),
    };

    widgets::mobile_nav::render_header(frame, header_area, app);
    widgets::mobile_nav::render_nav(frame, nav_area, app);

    match app.mobile_view {
        MobileView::Terminal => {
            frame.render_widget(Clear, content_area);
            render_pane_tree(frame, content_area, app.pane_tree(), app);

            // Overlay: command palette, eclipse modal, settings, etc.
            if matches!(app.mode, InputMode::CommandPalette { .. }) {
                widgets::command_palette::render(frame, content_area, app);
            }
            if app.agent_fleet_enabled {
                if app.eclipse_modal.is_some() {
                    widgets::eclipse_modal::render(frame, content_area, app);
                }
                if app.agent_detail_modal.is_some() {
                    widgets::agent_detail_modal::render(frame, content_area, app);
                }
            }

            if app.settings_open {
                widgets::settings_modal::render(frame, content_area, app);
            }
            if app.launch_modal.is_some() {
                widgets::launch_modal::render(frame, content_area, app);
            }
            if app.context_menu.is_some() {
                widgets::context_menu::render(frame, content_area, app);
            }
        }
        MobileView::Agents => {
            if app.agent_fleet_enabled {
                frame.render_widget(Clear, content_area);
                widgets::agent_monitor::render(frame, content_area, app);
                if app.eclipse_modal.is_some() {
                    widgets::eclipse_modal::render(frame, content_area, app);
                }
                if app.launch_modal.is_some() {
                    widgets::launch_modal::render(frame, content_area, app);
                }
                if app.agent_detail_modal.is_some() {
                    widgets::agent_detail_modal::render(frame, content_area, app);
                }
            }
        }
        MobileView::Windows => {
            widgets::mobile_spaces::render(frame, content_area, app);
            if app.mobile_close_confirm.is_some() {
                widgets::mobile_confirm::render(frame, content_area, app);
            }
        }
        MobileView::Actions => {
            // PTY underneath (same as Terminal view), palette floats on top.
            // render_mobile dims the full content_area uniformly (no sidebar offset).
            render_pane_tree(frame, content_area, app.pane_tree(), app);
            if app.settings_open {
                widgets::settings_modal::render(frame, content_area, app);
            } else {
                widgets::command_palette::render_mobile(frame, content_area, app);
            }
        }
    }
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let dim = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 14)));
    frame.render_widget(dim, area);

    let help_w = 48u16.min(area.width.saturating_sub(4));
    let help_h = 20u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width - help_w) / 2;
    let y = area.y + (area.height - help_h) / 2;
    let help_area = Rect {
        x,
        y,
        width: help_w,
        height: help_h,
    };

    let block = Block::default()
        .style(Style::default().bg(bg_secondary()).fg(fg_primary()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border()));
    frame.render_widget(block, help_area);

    let lines = vec![
        ("Ctrl+B", "prefix key (tmux-compatible)"),
        ("  %", "split pane horizontal (left|right)"),
        ("  \"", "split pane vertical (top/bottom)"),
        ("  x", "close current pane"),
        ("  o", "cycle focus between panes"),
        ("  z", "zoom pane (toggle fullscreen)"),
        ("  [", "enter copy/scroll mode"),
        ("  c", "new window (tab)"),
        ("  n / p", "next / previous window"),
        ("  0-9", "switch to window N"),
        ("  d", "detach (quit, keep session)"),
        ("  a", "toggle agent monitor"),
        ("  b", "toggle sidebar"),
        ("  ?", "this help"),
        ("Scroll: k/j/PgUp/PgDn/g/G/q", ""),
    ];

    let mut y_off = 1u16;
    let title = ratatui::text::Line::from(vec![ratatui::text::Span::styled(
        " Orbit — Keyboard Reference ",
        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(
        title,
        Rect {
            x: help_area.x + 1,
            y: help_area.y + y_off,
            width: help_w - 2,
            height: 1,
        },
    );
    y_off += 2;

    for (key, desc) in &lines {
        let line = if desc.is_empty() {
            ratatui::text::Line::from(vec![ratatui::text::Span::styled(
                *key,
                Style::default().fg(accent_idle()),
            )])
        } else {
            ratatui::text::Line::from(vec![
                ratatui::text::Span::styled(format!(" {:<14}", key), Style::default().fg(accent())),
                ratatui::text::Span::styled(*desc, Style::default().fg(fg_secondary())),
            ])
        };
        frame.render_widget(
            line,
            Rect {
                x: help_area.x + 1,
                y: help_area.y + y_off,
                width: help_w - 2,
                height: 1,
            },
        );
        y_off += 1;
    }

    y_off += 1;
    let hint = ratatui::text::Line::from(vec![ratatui::text::Span::styled(
        " Press any key to close ",
        Style::default().fg(fg_muted()),
    )]);
    frame.render_widget(
        hint,
        Rect {
            x: help_area.x + 1,
            y: help_area.y + y_off,
            width: help_w - 2,
            height: 1,
        },
    );
}

pub fn compute_leaf_areas(node: &PaneLayout, area: Rect) -> Vec<(PaneId, Rect)> {
    match node {
        PaneLayout::Leaf(pid) => vec![(*pid, area)],
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => {
            let (first_area, second_area) = split_area(area, direction, *ratio);
            let mut v = compute_leaf_areas(first, first_area);
            v.extend(compute_leaf_areas(second, second_area));
            v
        }
    }
}

pub fn find_split_at_cursor(
    node: &PaneLayout,
    area: Rect,
    col: u16,
    row: u16,
) -> Option<(PaneId, PaneId, SplitDir)> {
    match node {
        PaneLayout::Leaf(_) => None,
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => {
            let (first_area, second_area) = split_area(area, direction, *ratio);
            let near = match direction {
                SplitDir::Horizontal => {
                    let bx = first_area.x + first_area.width;
                    row >= area.y
                        && row < area.y + area.height
                        && col >= bx.saturating_sub(1)
                        && col <= bx
                }
                SplitDir::Vertical => {
                    let by = first_area.y + first_area.height;
                    col >= area.x
                        && col < area.x + area.width
                        && row >= by.saturating_sub(1)
                        && row <= by
                }
            };
            if near {
                let first_leaf = first.leaves().first().copied()?;
                let second_leaf = second.leaves().first().copied()?;
                Some((first_leaf, second_leaf, *direction))
            } else {
                find_split_at_cursor(first, first_area, col, row)
                    .or_else(|| find_split_at_cursor(second, second_area, col, row))
            }
        }
    }
}

fn render_pane_tree(frame: &mut Frame, area: Rect, node: &PaneLayout, app: &App) {
    match node {
        PaneLayout::Leaf(pid) => {
            render_single_pane(frame, area, *pid, app);
        }
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => {
            let (first_area, second_area) = split_area(area, direction, *ratio);

            render_pane_tree(frame, first_area, first, app);
            render_pane_tree(frame, second_area, second, app);
        }
    }
}

fn split_area(area: Rect, dir: &SplitDir, ratio: f32) -> (Rect, Rect) {
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.1, 0.9)
    } else {
        0.5
    };
    match dir {
        SplitDir::Horizontal => {
            let total = area.width;
            let min_w = 3u16;
            let first_w = if total <= 1 {
                total
            } else if total < 2 * min_w {
                total / 2
            } else {
                let max_w = total.saturating_sub(min_w);
                ((total as f32 * ratio) as u16).clamp(min_w, max_w)
            };
            let first = Rect {
                width: first_w,
                ..area
            };
            let second = Rect {
                x: area.x + first_w,
                width: total - first_w,
                ..area
            };
            (first, second)
        }
        SplitDir::Vertical => {
            let total = area.height;
            let min_h = 3u16;
            let first_h = if total <= 1 {
                total
            } else if total < 2 * min_h {
                total / 2
            } else {
                let max_h = total.saturating_sub(min_h);
                ((total as f32 * ratio) as u16).clamp(min_h, max_h)
            };
            let first = Rect {
                height: first_h,
                ..area
            };
            let second = Rect {
                y: area.y + first_h,
                height: total - first_h,
                ..area
            };
            (first, second)
        }
    }
}

fn render_single_pane(frame: &mut Frame, area: Rect, pane_id: PaneId, app: &App) {
    let is_active = pane_id == app.active_pane;
    let pane_idx = app
        .pane_tree()
        .leaves()
        .iter()
        .position(|&p| p == pane_id)
        .map(|i| i + 1)
        .unwrap_or(1);

    let border_color = if is_active { accent() } else { border() };

    let title = if is_active {
        format!(" {pane_idx}:~ *")
    } else {
        format!(" {pane_idx}:~ ")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if is_active { accent_idle() } else { fg_muted() })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(pane) = app.panes.get(&pane_id) {
        let scroll_offset = if is_active {
            if let InputMode::Scroll { offset } = &app.mode {
                Some(*offset)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(offset) = scroll_offset {
            render_cells_scrolled(frame, inner, pane, offset);
        } else {
            render_cells(
                frame,
                inner,
                pane,
                is_active && app.mode == InputMode::Normal,
                app.selection.as_ref(),
                pane_id,
            );
        }
    }
}

fn render_cells(
    frame: &mut Frame,
    area: Rect,
    pane: &PaneState,
    show_cursor: bool,
    selection: Option<&Selection>,
    pane_id: PaneId,
) {
    let grid = &pane.parser.grid;
    let rows = (area.height as usize).min(grid.rows as usize);
    let cols = (area.width as usize).min(grid.cols as usize);

    for row in 0..rows {
        let mut col = 0;
        while col < cols {
            let cell = &grid.cells[row * grid.cols as usize + col];
            let x = area.x + col as u16;
            let y = area.y + row as u16;

            // Skip spacer cells (placed after wide chars by VT parser)
            if cell.ch == '\0' {
                col += 1;
                continue;
            }

            let ch = cell.ch;
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);

            if let Some(buf_cell) = frame.buffer_mut().cell_mut((x, y)) {
                // Use set_symbol for wide chars so ratatui handles trailing cell
                if char_width > 1 {
                    let mut s = String::new();
                    s.push(ch);
                    buf_cell.set_symbol(&s);
                } else {
                    buf_cell.set_char(ch);
                }

                let mut fg = term_color(&cell.fg);
                let mut bg = term_color(&cell.bg);
                let in_selection = selection.is_some_and(|sel| {
                    sel.pane_id == pane_id && {
                        // Stream (line) selection: normalize start/end so start <= end
                        let (start, end) = if sel.start.1 < sel.end.1
                            || (sel.start.1 == sel.end.1 && sel.start.0 <= sel.end.0)
                        {
                            (sel.start, sel.end)
                        } else {
                            (sel.end, sel.start)
                        };
                        let (sc, sr) = (start.0 as usize, start.1 as usize);
                        let (ec, er) = (end.0 as usize, end.1 as usize);
                        if row < sr || row > er {
                            false
                        } else if sr == er {
                            // Single-line selection
                            col >= sc && col <= ec
                        } else if row == sr {
                            // First line: from start col to end of line
                            col >= sc
                        } else if row == er {
                            // Last line: from start of line to end col
                            col <= ec
                        } else {
                            // Middle lines: fully selected
                            true
                        }
                    }
                });
                if in_selection {
                    // Use a fixed highlight rather than swapping: swapping Default colors
                    // produces invisible or random-colored cells depending on the terminal.
                    bg = theme::accent();
                    fg = theme::bg_primary();
                }
                let mut style = Style::default().fg(fg).bg(bg);
                let mut mods = Modifier::empty();
                if cell.flags.bold() {
                    mods |= Modifier::BOLD;
                }
                if cell.flags.italic() {
                    mods |= Modifier::ITALIC;
                }
                if cell.flags.underline() {
                    mods |= Modifier::UNDERLINED;
                }
                if cell.flags.dim() {
                    mods |= Modifier::DIM;
                }
                if cell.flags.reverse() {
                    mods |= Modifier::REVERSED;
                }
                if !mods.is_empty() {
                    style = style.add_modifier(mods);
                }
                buf_cell.set_style(style);
            }

            // Skip the spacer column(s) that follow a wide char
            col += char_width;
        }
    }

    if show_cursor && grid.cursor_visible {
        let cx = area.x + grid.cursor_x.min(cols as u16);
        let cy = area.y + grid.cursor_y.min(rows as u16);
        frame.set_cursor_position((cx, cy));
    }
}

fn render_cells_scrolled(
    frame: &mut Frame,
    area: Rect,
    pane: &crate::app::PaneState,
    offset: usize,
) {
    let grid = &pane.parser.grid;
    let height = area.height as usize;
    let grid_rows = grid.rows as usize;
    let scrollback_len = pane.scrollback.len();
    let total = scrollback_len + grid_rows;

    let start = total.saturating_sub(height + offset);

    for display_row in 0..height {
        let content_idx = start + display_row;
        let y = area.y + display_row as u16;

        let row_cells: &[Cell] = if content_idx < scrollback_len {
            &pane.scrollback[content_idx]
        } else {
            let gr = content_idx - scrollback_len;
            if gr < grid_rows {
                let row_start = gr * grid.cols as usize;
                &grid.cells[row_start..row_start + grid.cols as usize]
            } else {
                continue;
            }
        };

        render_row(frame, area.x, y, row_cells, area.width as usize);
    }
}

fn render_row(frame: &mut Frame, x: u16, y: u16, cells: &[Cell], max_cols: usize) {
    let mut col = 0;
    while col < cells.len().min(max_cols) {
        let cell = &cells[col];

        // Skip spacer cells (placed after wide chars by VT parser)
        if cell.ch == '\0' {
            col += 1;
            continue;
        }

        let ch = cell.ch;
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);

        if let Some(buf_cell) = frame.buffer_mut().cell_mut((x + col as u16, y)) {
            if char_width > 1 {
                let mut s = String::new();
                s.push(ch);
                buf_cell.set_symbol(&s);
            } else {
                buf_cell.set_char(ch);
            }
            let fg = term_color(&cell.fg);
            let bg = term_color(&cell.bg);
            let mut style = Style::default().fg(fg).bg(bg);
            let mut mods = Modifier::empty();
            if cell.flags.bold() {
                mods |= Modifier::BOLD;
            }
            if cell.flags.italic() {
                mods |= Modifier::ITALIC;
            }
            if cell.flags.underline() {
                mods |= Modifier::UNDERLINED;
            }
            if cell.flags.dim() {
                mods |= Modifier::DIM;
            }
            if !mods.is_empty() {
                style = style.add_modifier(mods);
            }
            buf_cell.set_style(style);
        }

        col += char_width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbt_protocol::{
        AgentDetail, AgentInfo, AgentStatus, CellGrid, FullState, PaneInfo, SpaceId, SpaceInfo,
        SplitDir, TabId, TabInfo,
    };
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    use crate::app::App;

    /// Helper: build a minimal FullState for visual rendering tests.
    fn minimal_state() -> FullState {
        FullState {
            spaces: vec![SpaceInfo {
                id: SpaceId(1),
                name: "dev".to_string(),
                path: "/home/user/project".to_string(),
                tabs: vec![TabInfo {
                    id: TabId(1),
                    name: "main".to_string(),
                    layout: PaneLayout::Leaf(PaneId(1)),
                    active_pane: PaneId(1),
                }],
                active_tab: TabId(1),
                panes: vec![PaneInfo {
                    id: PaneId(1),
                    tab_id: TabId(1),
                    title: String::new(),
                    cwd: "/home/user/project".to_string(),
                    cell_grid: CellGrid::new(80, 24),
                }],
            }],
            active_space: SpaceId(1),
            agents: vec![],
        }
    }

    /// Helper: extract the text content from a ratatui Buffer as a Vec of row strings.
    fn buffer_lines(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buf = terminal.backend().buffer();
        let mut lines = Vec::new();
        for y in 0..buf.area.height {
            let mut line = String::new();
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                line.push_str(cell.symbol());
            }
            lines.push(line);
        }
        lines
    }

    /// Helper: check that a string appears somewhere in the rendered output.
    fn buffer_contains(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
        let buf = terminal.backend().buffer();
        for y in 0..buf.area.height {
            let mut line = String::new();
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                line.push_str(cell.symbol());
            }
            if line.contains(needle) {
                return true;
            }
        }
        false
    }

    /// Helper: check that a given position has a specific foreground color.
    fn cell_fg_at(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> Color {
        let buf = terminal.backend().buffer();
        buf[(x, y)].fg
    }

    // -------------------------------------------------------------------------
    // Visual rendering tests using TestBackend
    // -------------------------------------------------------------------------

    #[test]
    fn render_basic_layout_120x30() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.agent_fleet_enabled = true;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Tab bar should show the tab name "main"
        assert!(buffer_contains(&terminal, "main"));
        // Sidebar should show space name "dev"
        assert!(buffer_contains(&terminal, "dev"));
        // Pane border should show pane number
        assert!(buffer_contains(&terminal, "1:~"));
        // Status bar should show space name and idle satellite status
        assert!(buffer_contains(&terminal, "[SPACE]"));
        assert!(buffer_contains(&terminal, "idle"));
    }

    #[test]
    fn render_compact_layout_60x20() {
        let state = minimal_state();
        let app = App::from_welcome(&state, 60, 20);
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Should still render without panic at compact size
        assert!(buffer_contains(&terminal, "main"));
        // Sidebar should be collapsed (width=5) in compact mode
        let lines = buffer_lines(&terminal);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_ultra_wide_layout_160x40() {
        let state = minimal_state();
        let app = App::from_welcome(&state, 160, 40);
        let backend = TestBackend::new(160, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Should render full sidebar in ultra mode
        assert!(buffer_contains(&terminal, "dev"));
        assert!(buffer_contains(&terminal, "main"));
    }

    #[test]
    fn render_with_agent_panel_visible() {
        let mut state = minimal_state();
        state.agents.push(AgentInfo {
            id: orbt_protocol::AgentId(1),
            name: "claude-1".to_string(),
            space_id: SpaceId(1),
            model: "opus".to_string(),
            status: AgentStatus::Working,
            pane_id: Some(PaneId(1)),
            detail: Some(AgentDetail {
                task: Some("Fixing bug".to_string()),
                block_msg: None,
                progress: Some(0.5),
                duration_s: 120,
                acp: None,
                context_percent: None,
                compaction_count: 0,
                agent_cli: String::new(),
            }),
            protocol: orbt_protocol::AgentProtocol::Heuristic,
            launch_cmd: None,
        });
        // 140 cols = Ultra mode: 25-col agent panel (iw=24), enough room for the full name.
        let mut app = App::from_welcome(&state, 140, 30);
        app.agent_fleet_enabled = true;
        app.agent_panel_mode = AgentPanelMode::Sidebar;
        let backend = TestBackend::new(140, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Agent panel should show agent name
        assert!(buffer_contains(&terminal, "claude-1"));
        // Should show the working indicator
        // filled circle = Working
        assert!(buffer_contains(&terminal, "\u{25CF}"));
    }

    #[test]
    fn render_with_blocked_agent_shows_eclipse_status() {
        let mut state = minimal_state();
        state.agents.push(AgentInfo {
            id: orbt_protocol::AgentId(2),
            name: "aider-1".to_string(),
            space_id: SpaceId(1),
            model: "gpt-4".to_string(),
            status: AgentStatus::Blocked,
            pane_id: Some(PaneId(1)),
            detail: Some(AgentDetail {
                task: None,
                block_msg: Some("Needs permission".to_string()),
                progress: None,
                duration_s: 30,
                acp: None,
                context_percent: None,
                compaction_count: 0,
                agent_cli: String::new(),
            }),
            protocol: orbt_protocol::AgentProtocol::Heuristic,
            launch_cmd: None,
        });
        let mut app = App::from_welcome(&state, 120, 30);
        app.agent_fleet_enabled = true;
        app.agent_panel_mode = AgentPanelMode::Sidebar;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Should show blocked agent name
        assert!(buffer_contains(&terminal, "aider-1"));
        // Should show Eclipse indicator (circled circle)
        // ◎ = Blocked/Eclipse
        assert!(buffer_contains(&terminal, "\u{25CE}"));
    }

    #[test]
    fn render_split_panes_show_borders() {
        let mut state = minimal_state();
        state.spaces[0].tabs[0].layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
        };
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(1),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let app = App::from_welcome(&state, 120, 30);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Should show both pane numbers
        assert!(buffer_contains(&terminal, "1:~"));
        assert!(buffer_contains(&terminal, "2:~"));
    }

    #[test]
    fn render_active_pane_has_accent_border() {
        let mut state = minimal_state();
        state.spaces[0].tabs[0].layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.5,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
        };
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(1),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let app = App::from_welcome(&state, 120, 30);
        // Active pane is PaneId(1), its border should use accent color
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // The active pane border starts at x=24 (sidebar width), y=1 (below tab bar).
        // First cell at (24, 1) is the top-left corner of the block border.
        let corner_fg = cell_fg_at(&terminal, 24, 1);
        assert_eq!(corner_fg, accent()); // active pane border = accent (orange) color

        // The inactive pane (PaneId 2) should use the default border color.
        // It starts at ~x=72 in a 120-col terminal (sidebar=24, first pane half=48).
        let inactive_corner_fg = cell_fg_at(&terminal, 72, 1);
        assert_eq!(inactive_corner_fg, border()); // inactive pane = border color
    }

    #[test]
    fn render_help_overlay_shows_keybindings() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.show_help = true;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        assert!(buffer_contains(&terminal, "Ctrl+B"));
        assert!(buffer_contains(&terminal, "prefix key"));
        assert!(buffer_contains(&terminal, "split pane"));
    }

    #[test]
    fn render_command_palette_overlay() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.mode = InputMode::CommandPalette {
            search: String::new(),
            selected: 0,
            search_focused: true,
        };
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Command palette should show some command labels
        assert!(buffer_contains(&terminal, "Split Horizontal"));
        assert!(buffer_contains(&terminal, "New Tab"));
    }

    #[test]
    fn render_command_palette_with_search_filter() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.mode = InputMode::CommandPalette {
            search: "split".to_string(),
            selected: 0,
            search_focused: true,
        };
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Should show "Split" commands
        assert!(buffer_contains(&terminal, "Split"));
        // Should NOT show unrelated commands like "Detach"
        assert!(!buffer_contains(&terminal, "Detach"));
    }

    #[test]
    fn render_settings_modal() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.settings_open = true;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Settings modal should show theme options
        assert!(buffer_contains(&terminal, "orbt") || buffer_contains(&terminal, "Theme"));
    }

    #[test]
    fn render_multiple_tabs_in_tab_bar() {
        let mut state = minimal_state();
        state.spaces[0].tabs.push(TabInfo {
            id: TabId(2),
            name: "build".to_string(),
            layout: PaneLayout::Leaf(PaneId(2)),
            active_pane: PaneId(2),
        });
        state.spaces[0].panes.push(PaneInfo {
            id: PaneId(2),
            tab_id: TabId(2),
            title: String::new(),
            cwd: "/tmp".to_string(),
            cell_grid: CellGrid::new(80, 24),
        });
        let app = App::from_welcome(&state, 120, 30);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Both tab names should be visible
        assert!(buffer_contains(&terminal, "main"));
        assert!(buffer_contains(&terminal, "build"));
    }

    #[test]
    fn render_does_not_panic_at_minimum_size() {
        let state = minimal_state();
        let app = App::from_welcome(&state, 20, 5);
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        // Main assertion: does not panic
        terminal.draw(|f| render(f, &app)).unwrap();
    }

    #[test]
    fn render_sidebar_collapsed_in_narrow_terminal() {
        let state = minimal_state();
        let app = App::from_welcome(&state, 60, 24);
        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // In compact mode (<80), sidebar is only 5 cols wide
        // Check that the pane area starts early (at x=5)
        let lines = buffer_lines(&terminal);
        // Pane border should appear relatively close to the left
        let first_line = &lines[1]; // row 1 is where pane starts
                                    // At position 5 (after collapsed sidebar) we should see pane border
        assert!(first_line.len() >= 10);
    }

    #[test]
    fn render_theme_orange_changes_colors() {
        // Switch to orange theme before rendering
        theme::set_theme("orange");

        let state = minimal_state();
        let app = App::from_welcome(&state, 120, 30);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Orange theme uses orange accent (217, 119, 6) instead of orbit purple.
        let active_border_fg = cell_fg_at(&terminal, 24, 1);
        assert_eq!(active_border_fg, Color::Rgb(217, 119, 6));

        // Restore default theme for other tests
        theme::set_theme("orbt");
    }

    #[test]
    fn render_eclipse_modal_overlay() {
        let state = minimal_state();
        let mut app = App::from_welcome(&state, 120, 30);
        app.agent_fleet_enabled = true;
        app.eclipse_modal = Some(crate::app::EclipseModalState {
            agent_id: orbt_protocol::AgentId(1),
            agent_name: "claude-dev".to_string(),
            block_msg: "Needs user confirmation".to_string(),
            response: String::new(),
            model: "opus".to_string(),
            task: Some("Refactoring auth".to_string()),
            progress: Some(0.6),
            cwd: Some("/project".to_string()),
            blocked_duration_s: 45,
        });
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();

        // Eclipse modal should show the agent name and block message
        assert!(buffer_contains(&terminal, "claude-dev"));
        assert!(buffer_contains(&terminal, "Needs user confirmation"));
    }

    // -------------------------------------------------------------------------
    // Original layout/split tests
    // -------------------------------------------------------------------------

    #[test]
    fn split_area_normal_horizontal() {
        let area = Rect::new(10, 5, 20, 10);
        let (first, second) = split_area(area, &SplitDir::Horizontal, 0.3);
        assert_eq!(first.x, 10);
        assert_eq!(first.width, 6);
        assert_eq!(second.x, 16);
        assert_eq!(second.width, 14);
    }

    #[test]
    fn split_area_normal_vertical() {
        let area = Rect::new(0, 0, 10, 20);
        let (first, second) = split_area(area, &SplitDir::Vertical, 0.7);
        assert_eq!(first.y, 0);
        assert_eq!(first.height, 14);
        assert_eq!(second.y, 14);
        assert_eq!(second.height, 6);
    }

    #[test]
    fn split_area_minimum_sizes() {
        let area = Rect::new(0, 0, 10, 3);
        let (first, second) = split_area(area, &SplitDir::Horizontal, 0.05);
        assert_eq!(first.width, 3);
        assert_eq!(second.width, 7);
    }

    #[test]
    fn split_area_tiny_area_fallback() {
        let area = Rect::new(0, 0, 3, 3);
        let (first, second) = split_area(area, &SplitDir::Horizontal, 0.1);
        assert_eq!(first.width, 1);
        assert_eq!(second.width, 2);
    }

    #[test]
    fn split_area_zero_width_area() {
        let area = Rect::new(0, 0, 0, 5);
        let (first, second) = split_area(area, &SplitDir::Horizontal, 0.5);
        assert_eq!(first.width, 0);
        assert_eq!(second.width, 0);
    }

    #[test]
    fn split_area_nan_ratio() {
        let area = Rect::new(0, 0, 10, 10);
        let (first, second) = split_area(area, &SplitDir::Horizontal, f32::NAN);
        assert_eq!(first.width, 5);
        assert_eq!(second.width, 5);
    }

    #[test]
    fn find_split_at_cursor_horizontal() {
        let layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        };
        let area = Rect::new(0, 0, 20, 10);
        assert_eq!(
            find_split_at_cursor(&layout, area, 10, 5),
            Some((PaneId(1), PaneId(2), SplitDir::Horizontal))
        );
        assert_eq!(find_split_at_cursor(&layout, area, 5, 5), None);
    }

    #[test]
    fn compute_leaf_areas_basic() {
        let layout = PaneLayout::Split {
            direction: SplitDir::Horizontal,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        };
        let area = Rect::new(0, 0, 20, 10);
        let areas = compute_leaf_areas(&layout, area);
        assert_eq!(areas.len(), 2);
        assert_eq!(areas[0].0, PaneId(1));
        assert_eq!(areas[0].1.width, 10);
        assert_eq!(areas[1].0, PaneId(2));
        assert_eq!(areas[1].1.width, 10);
    }
}
