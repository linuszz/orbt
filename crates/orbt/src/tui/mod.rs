pub mod theme;
pub mod widgets;

use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use orbt_protocol::{PaneId, PaneLayout, SplitDir, StripColumn, TermColor};
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

/// Put the terminal back the way we found it if anything unwinds past it.
///
/// `main` restores the terminal on the paths that return an error, but a panic
/// skips that entirely: the process dies with raw mode still on and the
/// alternate screen still active, leaving a shell that cannot be typed into and
/// can only be escaped by closing the window. Unwinding runs this first, so the
/// terminal is usable again and the panic message is readable.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
        let _ = disable_raw_mode();
        previous(info);
    }));
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
/// A strip holding exactly one pane stretches it across the viewport.
///
/// A lone pane has nothing to tile against, so at its configured width it just
/// sits there with the rest of the band empty, which reads as broken. Widening it
/// leaves `column_width` alone, so opening a second pane brings it back to the
/// configured width and the strip becomes what it is meant to be. The pane is
/// resized to match, so nothing inside it is squeezed to fit.
pub fn strip_solo_width(columns: &[StripColumn], column_width: u16, area: Rect) -> u16 {
    if columns.len() != 1 || columns[0].panes.len() != 1 {
        return column_width;
    }
    area.width.max(column_width)
}
/// The overview is a floating panel over the strip, not a replacement for it: it
/// keeps a margin so the panes stay visible behind it, and it grows with the
/// tallest column so a card is never squeezed below the point of being readable.
pub fn overview_area(area: Rect, tallest_column: usize) -> Rect {
    let max_w = area.width.saturating_sub(6).max(24);
    let max_h = area.height.saturating_sub(4).max(8);
    let want_h = (tallest_column as u16).saturating_mul(4).saturating_add(4);
    let w = max_w.min(72.max(max_w));
    let h = want_h.clamp(10, max_h);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Rows per card the overview aims for, so it can size itself before drawing.
pub fn overview_tallest_column(node: &PaneLayout) -> usize {
    match node {
        PaneLayout::Strip { columns, .. } => {
            columns.iter().map(|c| c.panes.len()).max().unwrap_or(1)
        }
        _ => 1,
    }
}

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

    // Five rows: tab bar, the panes, the strip's navigation, the status line, and
    // the rule under it. The navigation row is zero height for a split tree, and
    // the status line keeps its own row so the two cannot paint over each other.
    let nav_h = strip_nav_height(&app.layout());
    let rows = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Fill(1),
        ratatui::layout::Constraint::Length(nav_h),
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Length(1),
    ])
    .split(right);

    widgets::tab_bar::render(frame, rows[0], app);
    frame.render_widget(Clear, rows[1]);
    render_pane_tree(frame, rows[1], &app.layout(), app);
    let status_inner = Rect {
        x: rows[3].x,
        y: rows[3].y,
        width: rows[3].width,
        height: 1,
    };
    let border_y = rows[3].y + 1;
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

    if app.show_overview {
        widgets::pane_overview::render(
            frame,
            overview_area(area, overview_tallest_column(app.pane_tree())),
            app,
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
            render_pane_tree(frame, content_area, &app.layout(), app);

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
                widgets::agent_monitor::render_fullscreen_modal(frame, content_area, app);
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
            render_pane_tree(frame, content_area, &app.layout(), app);
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
        ("  f / F", "cycle pane focus forward / back"),
        ("  ← →", "focus pane left / right"),
        ("  ⇧↑ ⇧↓", "focus pane above / below"),
        ("  o", "strip overview (click a pane to focus)"),
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

/// Scroll offset that shows `focus` with its column parked against the left
/// edge, so every column boundary lines up with the viewport edge.
pub fn strip_scroll_target(
    columns: &[StripColumn],
    column_width: u16,
    area: Rect,
    focus: PaneId,
) -> usize {
    if columns.is_empty() {
        return 0;
    }
    let width = column_width.max(1) as usize;
    let total = width * columns.len();
    let viewport = area.width as usize;
    if total <= viewport {
        return 0;
    }
    let max_scroll = total - viewport;
    let focus_col = columns
        .iter()
        .position(|c| c.panes.contains(&focus))
        .unwrap_or(0);

    if width >= viewport {
        // The focused column is wider than the viewport; keep its trailing edge in view.
        ((focus_col + 1) * width - viewport).min(max_scroll)
    } else {
        (focus_col * width).min(max_scroll)
    }
}

/// Lay out a scrollable strip inside `area` at a fixed scroll offset.
///
/// Columns are placed left to right at `column_width` each; panes inside a
/// column split its height evenly. Column edges are rounded so the rows tile
/// the column with no gaps or overlap. Columns scrolled past either edge get
/// zero-width rects so callers can skip them.
///
/// Each entry also reports how many of the pane's own columns fall to the left
/// of the viewport. A pane clipped by the left edge has to show its right-hand
/// side, and squeezing its first columns into the narrower rect instead is what
/// makes it look shrunk.
pub fn strip_areas_at(
    columns: &[StripColumn],
    column_width: u16,
    area: Rect,
    scroll: usize,
) -> Vec<(PaneId, Rect, u16)> {
    if columns.is_empty() {
        return Vec::new();
    }
    let width = column_width.max(1) as usize;
    let total = width * columns.len();
    let viewport = area.width as usize;
    let scroll = scroll.min(total.saturating_sub(viewport));

    let mut out = Vec::new();
    for (ci, column) in columns.iter().enumerate() {
        let x = ci * width;
        let visible_start = x.max(scroll);
        let visible_end = (x + width).min(scroll + viewport);
        let col_w = visible_end.saturating_sub(visible_start) as u16;
        if col_w == 0 {
            for &pane in &column.panes {
                out.push((
                    pane,
                    Rect {
                        x: area.x,
                        y: area.y,
                        width: 0,
                        height: 0,
                    },
                    0,
                ));
            }
            continue;
        }
        let col_x = area.x + (visible_start - scroll) as u16;

        // Even split with the remainder handed to the upper panes, so the last
        // pane always ends flush with the bottom of the column.
        let n = column.panes.len();
        let base = area.height / n as u16;
        let extra = area.height % n as u16;
        let mut y = area.y;
        for (i, &pane) in column.panes.iter().enumerate() {
            let h = base + u16::from(i < extra as usize);
            out.push((
                pane,
                Rect {
                    x: col_x,
                    y,
                    width: col_w,
                    height: h,
                },
                (visible_start - x) as u16,
            ));
            y += h;
        }
    }
    out
}

pub fn compute_leaf_areas(node: &PaneLayout, area: Rect, focus: PaneId) -> Vec<(PaneId, Rect)> {
    match node {
        PaneLayout::Leaf(pid) => vec![(*pid, area)],
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => {
            let (first_area, second_area) = split_area(area, direction, *ratio);
            let mut v = compute_leaf_areas(first, first_area, focus);
            v.extend(compute_leaf_areas(second, second_area, focus));
            v
        }
        PaneLayout::Strip {
            columns,
            column_width,
        } => {
            let scroll = strip_scroll_target(columns, *column_width, area, focus);
            strip_areas_at(columns, *column_width, area, scroll)
                .into_iter()
                .map(|(pane, rect, _)| (pane, rect))
                .collect()
        }
    }
}

/// The size each pane's terminal should be, in cells and excluding borders.
///
/// This is deliberately not derived from where a pane lands on screen. A strip
/// column that is only partly visible is still a full-size pane: sizing its
/// terminal to the visible slice makes the program inside reflow to a width
/// nobody chose, and scrolling back cannot undo that.
pub fn pane_terminal_sizes(node: &PaneLayout, area: Rect) -> Vec<(PaneId, u16, u16)> {
    match node {
        PaneLayout::Leaf(_) => Vec::new(),
        PaneLayout::Split {
            direction,
            first,
            second,
            ratio,
        } => {
            let (first_area, second_area) = split_area(area, direction, *ratio);
            let mut out = pane_terminal_sizes(first, first_area);
            out.extend(pane_terminal_sizes(second, second_area));
            out
        }
        PaneLayout::Strip {
            columns,
            column_width,
        } => {
            let mut out = Vec::new();
            for column in columns {
                let n = column.panes.len();
                if n == 0 {
                    continue;
                }
                let base = area.height / n as u16;
                let extra = area.height % n as u16;
                for (i, &pane) in column.panes.iter().enumerate() {
                    let h = base + u16::from(i < extra as usize);
                    out.push((
                        pane,
                        column_width.saturating_sub(2).max(1),
                        h.saturating_sub(2).max(1),
                    ));
                }
            }
            out
        }
    }
}

/// The strip's navigation bar: where the band is, how much of it is on screen,
/// and the controls for moving and growing it.
///
/// The bar sits under the panes rather than beside them. Arrows in a gutter cost
/// the panes two columns and read as part of the sidebar, and a control drawn in
/// the leftover space to the right of a lone pane is only reachable when there is
/// leftover space. A row of its own is always in the same place and costs the
/// panes nothing.
pub struct StripNav {
    pub bar: Rect,
    pub back: Rect,
    pub forward: Rect,
    pub add: Rect,
    pub track: Rect,
    /// The on-screen slice of the band, as a run of cells within `track`.
    pub window: Rect,
    pub can_back: bool,
    pub can_forward: bool,
}

/// Rows the strip navigation bar takes.
///
/// This has to answer the same question the renderer does — is a strip being
/// drawn — rather than what the layout setting says. The two can disagree, for
/// instance after switching to Split Tree while the current tab still holds a
/// strip, and reserving the wrong number of rows put the bar on top of the status
/// bar.
pub fn strip_nav_height(node: &PaneLayout) -> u16 {
    if node.is_strip() {
        1
    } else {
        0
    }
}

pub fn strip_nav(area: Rect, column_width: u16, column_count: usize, scroll: usize) -> StripNav {
    let bar = Rect {
        x: area.x,
        y: area.y + area.height,
        width: area.width,
        height: 1,
    };
    // Controls sit in their own cells with a blank between them, so an arrow
    // never reads as part of the rail next to it.
    let back = Rect {
        x: bar.x,
        y: bar.y,
        width: 1,
        height: 1,
    };
    let add = Rect {
        x: bar.x + bar.width.saturating_sub(1),
        y: bar.y,
        width: 1,
        height: 1,
    };
    let forward = Rect {
        x: add.x.saturating_sub(2),
        y: bar.y,
        width: 1,
        height: 1,
    };
    let track = Rect {
        x: back.x + 2,
        y: bar.y,
        width: forward.x.saturating_sub(3).saturating_sub(back.x + 2),
        height: 1,
    };

    let width = column_width.max(1) as usize;
    let total = width * column_count.max(1);
    let viewport = area.width.max(1) as usize;
    let max_scroll = total.saturating_sub(viewport);
    let scroll = scroll.min(max_scroll);

    // The rail stands for the whole band and the lit run for the part on screen,
    // so the bar says where the strip is and how much of it is left without the
    // user having to open anything.
    let (offset, span) = if track.width == 0 || total <= viewport {
        (0usize, track.width as usize)
    } else {
        let span = (track.width as usize * viewport / total).max(1);
        let room = track.width as usize - span;
        let offset = room * scroll / max_scroll.max(1);
        (offset, span)
    };
    let window = Rect {
        x: track.x + offset as u16,
        y: track.y,
        width: span.min(track.width as usize - offset) as u16,
        height: 1,
    };

    StripNav {
        bar,
        back,
        forward,
        add,
        track,
        window,
        can_back: scroll > 0,
        can_forward: scroll < max_scroll,
    }
}

fn render_strip_nav(
    frame: &mut Frame,
    columns: &[StripColumn],
    column_width: u16,
    area: Rect,
    scroll: usize,
) {
    let nav = strip_nav(area, column_width, columns.len(), scroll);
    if nav.bar.width < 8 || nav.track.width == 0 {
        return;
    }
    let arrow = |live: bool| {
        Style::default()
            .fg(if live { accent() } else { fg_muted() })
            .add_modifier(Modifier::BOLD)
    };
    // Bold glyphs in the accent colour, dimmed at an end with nothing past it,
    // rather than hairlines that disappear into the rail.
    frame.render_widget(
        Paragraph::new("\u{25c0}").style(arrow(nav.can_back)),
        nav.back,
    );
    frame.render_widget(
        Paragraph::new("\u{25b6}").style(arrow(nav.can_forward)),
        nav.forward,
    );
    frame.render_widget(
        Paragraph::new("+").style(Style::default().fg(accent()).add_modifier(Modifier::BOLD)),
        nav.add,
    );

    // End caps stop the rail reading as one long rule across the bottom.
    let rail = format!(
        "\u{2502}{}\u{2502}",
        "\u{2500}".repeat(nav.track.width.saturating_sub(2) as usize)
    );
    frame.render_widget(
        Paragraph::new(rail).style(Style::default().fg(border_dim())),
        Rect {
            x: nav.track.x,
            y: nav.track.y,
            width: nav.track.width,
            height: 1,
        },
    );
    if nav.window.width > 0 {
        let lit = "\u{2501}".repeat(nav.window.width as usize);
        frame.render_widget(
            Paragraph::new(lit).style(Style::default().fg(accent())),
            nav.window,
        );
    }
}

pub fn find_split_at_cursor(
    node: &PaneLayout,
    area: Rect,
    col: u16,
    row: u16,
    focus: PaneId,
) -> Option<(PaneId, PaneId, SplitDir)> {
    match node {
        PaneLayout::Leaf(_) => None,
        PaneLayout::Strip {
            columns,
            column_width,
        } => {
            let scroll = strip_scroll_target(columns, *column_width, area, focus);
            let rects = strip_areas_at(columns, *column_width, area, scroll);
            for w in rects.windows(2) {
                let (a_id, a, _) = w[0];
                let (b_id, b, _) = w[1];
                if a.width == 0 || b.width == 0 {
                    continue;
                }
                // Same column stacked panes share an x and differ in y.
                if a.x == b.x && a.y < b.y {
                    let by = b.y;
                    if col >= a.x && col < a.x + a.width && row + 1 >= by && row <= by {
                        return Some((a_id, b_id, SplitDir::Vertical));
                    }
                    continue;
                }
                // Otherwise this is a column boundary, a vertical line at b.x.
                if row < a.y || row >= b.y + b.height {
                    continue;
                }
                let bx = b.x;
                if col + 1 >= bx && col <= bx {
                    return Some((a_id, b_id, SplitDir::Horizontal));
                }
            }
            None
        }
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
                find_split_at_cursor(first, first_area, col, row, focus)
                    .or_else(|| find_split_at_cursor(second, second_area, col, row, focus))
            }
        }
    }
}

fn render_pane_tree(frame: &mut Frame, area: Rect, node: &PaneLayout, app: &App) {
    match node {
        PaneLayout::Leaf(pid) => {
            render_single_pane(frame, area, *pid, app, 0);
        }
        PaneLayout::Strip {
            columns,
            column_width,
        } => {
            let effective = strip_solo_width(columns, *column_width, area);
            let scroll = strip_scroll_target(columns, effective, area, app.active_pane);
            let areas = strip_areas_at(columns, effective, area, scroll);
            for (pid, rect, col_skip) in &areas {
                if rect.width > 0 {
                    render_single_pane(frame, *rect, *pid, app, *col_skip);
                }
            }
            render_strip_nav(frame, columns, effective, area, scroll);
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

fn render_single_pane(frame: &mut Frame, area: Rect, pane_id: PaneId, app: &App, col_skip: u16) {
    let is_active = pane_id == app.active_pane;
    let leaves = app.pane_tree().leaves();
    let total = leaves.len();
    let pane_idx = leaves
        .iter()
        .position(|&p| p == pane_id)
        .map(|i| i + 1)
        .unwrap_or(1);

    let border_color = if is_active { accent() } else { border_dim() };

    // Position within the strip, so a scrolled-away pane is still identifiable.
    let title = if total > 1 {
        if is_active {
            format!(" {pane_idx}/{total} *")
        } else {
            format!(" {pane_idx}/{total} ")
        }
    } else if is_active {
        " 1 *".to_string()
    } else {
        " 1".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(if is_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        )
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
                col_skip,
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
    col_skip: u16,
) {
    let grid = &pane.parser.grid;
    let rows = (area.height as usize).min(grid.rows as usize);
    let start = (col_skip as usize).min(grid.cols as usize);
    let cols = start + (area.width as usize).min(grid.cols as usize - start);

    for row in 0..rows {
        let mut col = start;
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
        assert!(buffer_contains(&terminal, "1 *"));
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

        // Both panes show their position in the strip; the focused one is marked.
        assert!(buffer_contains(&terminal, "1/2 *"));
        assert!(buffer_contains(&terminal, "2/2"));
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

        // The inactive pane (PaneId 2) should use the dimmed border colour so it
        // stays readable without competing with the focused pane.
        // It starts at ~x=72 in a 120-col terminal (sidebar=24, first pane half=48).
        let inactive_corner_fg = cell_fg_at(&terminal, 72, 1);
        assert_eq!(inactive_corner_fg, border_dim());
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
            find_split_at_cursor(&layout, area, 10, 5, PaneId(1)),
            Some((PaneId(1), PaneId(2), SplitDir::Horizontal))
        );
        assert_eq!(find_split_at_cursor(&layout, area, 5, 5, PaneId(1)), None);
    }

    /// Where `pane` lands, according to `compute_leaf_areas`.
    fn rect_of(areas: &[(PaneId, Rect)], pane: PaneId) -> Rect {
        areas
            .iter()
            .find(|(p, _)| *p == pane)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{pane:?} is missing"))
    }

    #[test]
    fn a_partly_visible_column_keeps_its_full_terminal_width() {
        let columns = vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
        ];
        let area = Rect {
            x: 0,
            y: 0,
            width: 50,
            height: 10,
        };
        // 160 wide of columns in a 50 wide viewport, so both are cut short.
        let areas = strip_areas_at(&columns, 80, area, 60);
        assert!(
            areas.iter().all(|(_, r, _)| r.width < 80),
            "both columns are only partly on screen"
        );

        let sizes = pane_terminal_sizes(
            &PaneLayout::Strip {
                columns,
                column_width: 80,
            },
            area,
        );
        assert_eq!(
            sizes,
            vec![(PaneId(1), 78, 8), (PaneId(2), 78, 8)],
            "the terminal keeps the column width, so nothing inside reflows"
        );
    }

    #[test]
    fn a_column_clipped_by_the_left_edge_reports_what_it_hides() {
        let columns = vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
            StripColumn::single(PaneId(3)),
        ];
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 10,
        };
        // 240 wide in a 100 wide viewport, scrolled to the end: the first
        // column is gone and the second is cut on the left.
        let areas = strip_areas_at(&columns, 80, area, 140);

        assert_eq!(areas[0].1.width, 0, "the first column is off screen");
        assert_eq!(areas[1].1.x, 0);
        assert_eq!(areas[1].1.width, 20, "only the tail of column two shows");
        assert_eq!(
            areas[1].2, 60,
            "the pane skips the columns that are off screen instead of squeezing its first ones"
        );
        assert_eq!(areas[2].2, 0, "an unclipped pane skips nothing");
    }

    #[test]
    fn hit_testing_follows_the_focus_that_the_renderer_used() {
        let node = PaneLayout::Strip {
            columns: vec![
                StripColumn::single(PaneId(1)),
                StripColumn::single(PaneId(2)),
                StripColumn::single(PaneId(3)),
            ],
            column_width: 80,
        };
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 10,
        };

        let on_first = compute_leaf_areas(&node, area, PaneId(1));
        let on_last = compute_leaf_areas(&node, area, PaneId(3));
        assert_eq!(rect_of(&on_first, PaneId(1)).x, 0, "focus parks left");
        assert_eq!(
            rect_of(&on_first, PaneId(2)).x,
            80,
            "the middle column starts where it does in the band"
        );
        assert_eq!(
            rect_of(&on_last, PaneId(2)).x,
            0,
            "after scrolling, the same pane sits elsewhere; a click landing on it \
             has to resolve to the same pane the user aimed at"
        );
        assert_eq!(
            pane_terminal_sizes(&node, area),
            vec![(PaneId(1), 78, 8), (PaneId(2), 78, 8), (PaneId(3), 78, 8)],
            "terminal sizes ignore the viewport entirely"
        );
    }

    #[test]
    fn the_bar_hands_out_reachable_cells_and_never_touches_the_panes() {
        let area = Rect {
            x: 24,
            y: 1,
            width: 100,
            height: 20,
        };
        let nav = strip_nav(area, 80, 3, 0);
        assert_eq!(nav.bar.y, 21, "one row below the panes, not beside them");
        assert_eq!(nav.back.x, area.x);
        assert_eq!(nav.add.x, area.x + area.width - 1);
        for spot in [nav.back, nav.forward, nav.add, nav.track, nav.window] {
            assert!(
                spot.y >= area.y + area.height,
                "controls live in their own row, so no pane loses a column to them"
            );
            assert!(spot.x >= area.x && spot.x + spot.width <= area.x + area.width);
        }
        assert_eq!(
            nav.track.x,
            nav.back.x + 2,
            "a blank separates the arrow from the rail, or they read as one mark"
        );
        assert_eq!(
            nav.forward.x,
            nav.add.x - 2,
            "and the plus keeps its own gap, so neither looks like the other"
        );
    }

    /// The rows the desktop layout hands to each part of the footer, so a test
    /// can assert the navigation bar and the status line are told apart.
    fn footer_rows(total: u16, nav_h: u16) -> Vec<ratatui::layout::Rect> {
        ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Fill(1),
            ratatui::layout::Constraint::Length(nav_h),
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(1),
        ])
        .split(Rect {
            x: 24,
            y: 0,
            width: 100,
            height: total,
        })
        .to_vec()
    }

    #[test]
    fn the_navigation_bar_and_the_status_line_are_on_different_rows() {
        let rows = footer_rows(24, 1);
        let nav = rows[2];
        let status = rows[3];
        assert!(nav.height > 0, "the strip gets its row");
        assert!(
            nav.y + nav.height <= status.y,
            "the bar sits above the status line, not on it: {:?} then {:?}",
            nav,
            status
        );

        let border = rows[4];
        assert!(status.y + status.height <= border.y);
    }

    #[test]
    fn a_split_tree_gives_the_row_back_and_draws_no_bar() {
        let rows = footer_rows(24, 0);
        assert_eq!(rows[2].height, 0, "the slot collapses");
        assert_eq!(rows[3].y, rows[1].y + rows[1].height, "status moves up");
        assert_eq!(
            strip_nav_height(&PaneLayout::Leaf(PaneId(1))),
            0,
            "and nothing asks for a bar"
        );
    }

    #[test]
    fn the_panes_stop_where_the_bar_begins() {
        // What compute_pane_area derives has to land on the row the layout gives
        // the panes, or the bar is drawn a row away from where it was reserved.
        for nav_h in [0u16, 1] {
            let total = 24u16;
            let rows = footer_rows(total, nav_h);
            let pane = content_area_for(total, 24, nav_h);
            assert_eq!(pane.height, rows[1].height, "nav_h={nav_h}");
            assert_eq!(pane.y + pane.height, rows[2].y, "nav_h={nav_h}");
        }
    }

    fn content_area_for(total: u16, _cols: u16, nav_h: u16) -> Rect {
        Rect {
            x: 24,
            y: 1,
            width: 100,
            height: total.saturating_sub(3 + nav_h).max(5),
        }
    }

    #[test]
    fn the_bar_reserves_a_row_only_when_a_strip_is_drawn() {
        let strip = PaneLayout::Strip {
            columns: vec![StripColumn::single(PaneId(1))],
            column_width: 80,
        };
        let tree = PaneLayout::Split {
            direction: SplitDir::Vertical,
            first: Box::new(PaneLayout::Leaf(PaneId(1))),
            second: Box::new(PaneLayout::Leaf(PaneId(2))),
            ratio: 0.5,
        };
        assert_eq!(
            strip_nav_height(&strip),
            1,
            "a strip needs its row, whatever the layout setting says"
        );
        assert_eq!(
            strip_nav_height(&tree),
            0,
            "a split tree gets the row back, so nothing is drawn over the status bar"
        );
    }

    #[test]
    fn the_window_sits_inside_the_rail_and_grows_as_there_is_more_to_see() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 10,
        };
        let nav = strip_nav(area, 80, 3, 0);
        assert!(nav.window.x >= nav.track.x);
        assert!(nav.window.x + nav.window.width <= nav.track.x + nav.track.width);
        assert!(
            !nav.can_back,
            "at the left end there is nowhere to go back to"
        );
        assert!(nav.can_forward);

        let many = strip_nav(area, 20, 20, 0);
        assert!(
            many.window.width < nav.window.width,
            "more panes means the visible slice is a smaller share of the bar"
        );

        let at_end = strip_nav(area, 80, 3, 140);
        assert!(at_end.can_back && !at_end.can_forward);

        let forward_at_start = strip_nav(area, 80, 3, 0);
        assert_eq!(
            forward_at_start.window.x, forward_at_start.track.x,
            "at the left end the lit run starts at the rail"
        );
    }

    #[test]
    fn a_band_that_fits_gives_the_whole_rail_and_no_arrows() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 10,
        };
        let nav = strip_nav(area, 80, 1, 0);
        assert_eq!(nav.window.width, nav.track.width);
        assert!(!nav.can_back && !nav.can_forward, "nothing to move");
        assert_eq!(
            nav.add.x,
            area.x + area.width - 1,
            "the add button is always reachable, even with one pane"
        );
    }

    #[test]
    fn a_lone_pane_fills_the_band_and_a_pair_keeps_the_configured_width() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 20,
        };
        let alone = vec![StripColumn::single(PaneId(1))];
        assert_eq!(
            strip_solo_width(&alone, 80, area),
            200,
            "one pane takes the whole band rather than looking stranded"
        );

        let pair = vec![
            StripColumn::single(PaneId(1)),
            StripColumn::single(PaneId(2)),
        ];
        assert_eq!(
            strip_solo_width(&pair, 80, area),
            80,
            "two panes go back to the configured width"
        );

        let stacked = vec![StripColumn {
            panes: vec![PaneId(1), PaneId(2)],
        }];
        assert_eq!(
            strip_solo_width(&stacked, 80, area),
            80,
            "a column of stacked panes is already using the height it was given"
        );
    }

    #[test]
    fn the_overview_is_a_panel_with_the_strip_left_visible() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 50,
        };
        let panel = overview_area(area, 1);
        assert!(panel.width < area.width, "not full screen");
        assert!(panel.height < area.height);
        assert!(
            panel.width >= 24 && panel.height >= 10,
            "but still readable"
        );
        assert!(
            panel.x > area.x && panel.x + panel.width < area.x + area.width,
            "centred, with the strip showing on both sides"
        );

        let tall = overview_area(area, 8);
        assert!(
            tall.height > panel.height,
            "a taller column needs a taller panel"
        );
        assert!(
            tall.height <= area.height,
            "but it never takes the whole area"
        );
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
        let areas = compute_leaf_areas(&layout, area, PaneId(1));
        assert_eq!(areas.len(), 2);
        assert_eq!(areas[0].0, PaneId(1));
        assert_eq!(areas[0].1.width, 10);
        assert_eq!(areas[1].0, PaneId(2));
        assert_eq!(areas[1].1.width, 10);
    }

    const W: u16 = 40;

    fn cols(n: usize) -> Vec<StripColumn> {
        (1..=n)
            .map(|i| StripColumn::single(PaneId(i as u32)))
            .collect()
    }

    fn scroll_for(columns: &[StripColumn], area: Rect, focus: PaneId) -> usize {
        strip_scroll_target(columns, W, area, focus)
    }

    #[test]
    fn strip_fits_without_scrolling() {
        let area = Rect::new(0, 0, 200, 10);
        let c = cols(4);
        let scroll = scroll_for(&c, area, PaneId(1));
        assert_eq!(scroll, 0);
        let areas = strip_areas_at(&c, W, area, scroll);
        assert_eq!(areas.len(), 4);
        for (i, (_, r, _)) in areas.iter().enumerate() {
            assert_eq!(r.x, i as u16 * 40, "column {i} x");
            assert_eq!(
                r.width, 40,
                "column {i} keeps its width when the strip fits"
            );
            assert_eq!(r.height, area.height, "a lone pane fills its column");
        }
    }

    #[test]
    fn strip_scroll_parks_the_focused_column_on_the_left() {
        let area = Rect::new(0, 0, 100, 10);
        let c = cols(4);
        let max_scroll = 4 * W as usize - area.width as usize;
        for (focus, want) in [
            (PaneId(1), 0usize),
            (PaneId(2), 40),
            (PaneId(3), 60),
            (PaneId(4), 60),
        ] {
            let scroll = scroll_for(&c, area, focus);
            assert_eq!(scroll, want, "scroll for {focus:?}");
            assert!(
                scroll % W as usize == 0 || scroll == max_scroll,
                "offset {scroll} is neither a column boundary nor the end of the content"
            );
        }
    }

    #[test]
    fn column_stacks_panes_and_fills_the_height() {
        let area = Rect::new(0, 0, 100, 21);
        let c = vec![StripColumn {
            panes: vec![PaneId(1), PaneId(2), PaneId(3)],
        }];
        let areas = strip_areas_at(&c, W, area, 0);
        assert_eq!(areas.len(), 3, "every pane in the column gets a rect");
        let rects: Vec<Rect> = areas.iter().map(|(_, r, _)| *r).collect();
        assert_eq!(rects[0].x, rects[1].x, "all share the column x");
        assert_eq!(rects[1].x, rects[2].x);
        assert_eq!(rects[0].y, area.y, "first pane starts at the top");
        assert_eq!(
            rects[2].y + rects[2].height,
            area.y + area.height,
            "last pane ends flush with the bottom"
        );
        // Rows must tile exactly: no gaps, no overlap.
        let summed: u16 = rects.iter().map(|r| r.height).sum();
        assert_eq!(summed, area.height, "heights sum to the column height");
        for w in rects.windows(2) {
            assert_eq!(w[0].y + w[0].height, w[1].y, "rows are contiguous");
        }
    }

    #[test]
    fn column_height_split_handles_remainder() {
        // 10 rows across 3 panes cannot divide evenly; the remainder must go
        // somewhere rather than leaving a gap.
        let area = Rect::new(0, 0, 100, 10);
        let c = vec![StripColumn {
            panes: vec![PaneId(1), PaneId(2), PaneId(3)],
        }];
        let areas = strip_areas_at(&c, W, area, 0);
        let rects: Vec<Rect> = areas.iter().map(|(_, r, _)| *r).collect();
        let summed: u16 = rects.iter().map(|r| r.height).sum();
        assert_eq!(summed, 10);
        for w in rects.windows(2) {
            assert_eq!(w[0].y + w[0].height, w[1].y);
        }
    }

    #[test]
    fn strip_scroll_is_clamped_to_content() {
        let area = Rect::new(0, 0, 100, 10);
        let c = cols(4);
        let s = scroll_for(&c, area, PaneId(4));
        assert!(s <= 60, "content is 160 columns so scroll cannot exceed 60");
        let areas = strip_areas_at(&c, W, area, s);
        let last = areas[3].1;
        assert!(last.x + last.width <= area.x + area.width);
    }

    #[test]
    fn focused_pane_is_always_fully_visible() {
        let area = Rect::new(0, 0, 100, 10);
        let c = cols(4);
        for focus in [PaneId(1), PaneId(2), PaneId(3), PaneId(4)] {
            let scroll = scroll_for(&c, area, focus);
            let areas = strip_areas_at(&c, W, area, scroll);
            let r = areas.iter().find(|(p, _, _)| *p == focus).unwrap().1;
            assert_eq!(r.width, W, "pane {focus:?} is never clipped horizontally");
            assert!(r.x >= area.x && r.x + r.width <= area.x + area.width);
        }
    }
}
