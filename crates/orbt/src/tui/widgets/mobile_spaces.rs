use orbt_protocol::AgentStatus;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::app::{App, MobileColFocus};
use crate::tui::theme::*;
use crate::tui::widgets::agent_monitor::{blocked_pulse_color, working_pulse_color};

/// Each space card occupies this many terminal rows (mirrors the PC sidebar card height).
const SPACE_CARD_H: usize = 3;

/// Each tab card occupies this many terminal rows (taller than a single row = easier to tap).
const TAB_CARD_H: usize = 2;

/// Hit-test result for a click in the SPACES two-column view.
#[derive(Debug, Clone, PartialEq)]
pub enum SpacesHit {
    Space(usize),
    SpaceClose(usize),
    NewSpace,
    Tab(usize),
    TabClose(usize),
    NewTab,
    None,
}

/// Column widths: (left_w, right_w). Separator takes 1 col between them.
fn col_widths(area: Rect) -> (u16, u16) {
    let left = (area.width / 3).max(12).min(area.width.saturating_sub(2));
    let right = area.width.saturating_sub(left + 1);
    (left, right)
}

/// Scroll offset that keeps `cursor` visible in a window of `visible` cards.
fn scroll_offset(cursor: usize, visible: usize) -> usize {
    if visible == 0 || cursor < visible {
        0
    } else {
        cursor + 1 - visible
    }
}

/// Hit-test a mouse coordinate against the two-column SPACES view.
pub fn hit_test(col: u16, row: u16, area: Rect, app: &App) -> SpacesHit {
    if row < area.y || row >= area.y + area.height {
        return SpacesHit::None;
    }
    let (left_w, _) = col_widths(area);
    let sep_x = area.x + left_w;
    let y_rel = (row - area.y) as usize;
    let content_h = area.height as usize;

    if col == sep_x {
        return SpacesHit::None;
    }

    if col < sep_x {
        // Left column: spaces
        let visible = content_h / SPACE_CARD_H;
        let offset = scroll_offset(app.mobile_spaces_cursor, visible);
        let card_idx = y_rel / SPACE_CARD_H + offset;
        let card_row = y_rel % SPACE_CARD_H;
        let n = app.spaces.len();
        // Close button on header row only, last 3 chars of left column
        let close_start = sep_x.saturating_sub(3);
        if card_idx < n {
            if card_row == 0 && col >= close_start {
                SpacesHit::SpaceClose(card_idx)
            } else {
                SpacesHit::Space(card_idx)
            }
        } else if card_idx == n {
            SpacesHit::NewSpace
        } else {
            SpacesHit::None
        }
    } else {
        // Right column: tabs
        let visible = content_h / TAB_CARD_H;
        let offset = scroll_offset(app.mobile_tabs_cursor, visible);
        let card_idx = y_rel / TAB_CARD_H + offset;
        let card_row = y_rel % TAB_CARD_H;
        let n = app.tabs.len();
        let close_start = area.x + area.width.saturating_sub(3);
        if card_idx < n {
            if card_row == 0 && col >= close_start {
                SpacesHit::TabClose(card_idx)
            } else {
                SpacesHit::Tab(card_idx)
            }
        } else if card_idx == n {
            SpacesHit::NewTab
        } else {
            SpacesHit::None
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_primary())),
        area,
    );

    if area.height < 2 || area.width < 14 {
        return;
    }

    let (left_w, right_w) = col_widths(area);
    let sep_x = area.x + left_w;
    let right_x = sep_x + 1;
    let content_h = area.height as usize;

    // Vertical separator
    for dy in 0..area.height {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2502}",
                Style::default().fg(border()),
            ))),
            Rect {
                x: sep_x,
                y: area.y + dy,
                width: 1,
                height: 1,
            },
        );
    }

    // ── Left column: Space cards ────────────────────────────────────────────
    render_space_column(frame, area.x, area.y, left_w, content_h, app);

    // ── Right column: Tab cards ──────────────────────────────────────────────
    render_tab_column(frame, right_x, area.y, right_w, content_h, app);
}

fn render_space_column(
    frame: &mut Frame,
    x: u16,
    base_y: u16,
    w: u16,
    content_h: usize,
    app: &App,
) {
    let n = app.spaces.len();
    let visible = content_h / SPACE_CARD_H;
    let offset = scroll_offset(app.mobile_spaces_cursor, visible);
    let focused = app.mobile_col_focus == MobileColFocus::Left;

    for vis in 0..visible {
        let idx = vis + offset;
        let card_y = base_y + (vis * SPACE_CARD_H) as u16;

        if idx < n {
            let space = &app.spaces[idx];
            let is_active = idx == app.active_space_idx;
            let is_cursor = focused && idx == app.mobile_spaces_cursor;

            // Compute agent counts for this space
            let space_agents: Vec<_> = app
                .agents
                .iter()
                .filter(|a| a.space_id == space.space_id)
                .collect();
            let n_blocked = space_agents
                .iter()
                .filter(|a| a.status == AgentStatus::Blocked)
                .count();
            let n_error = space_agents
                .iter()
                .filter(|a| a.status == AgentStatus::Error)
                .count();
            let n_working = space_agents
                .iter()
                .filter(|a| a.status == AgentStatus::Working)
                .count();
            let n_idle = space_agents
                .iter()
                .filter(|a| a.status == AgentStatus::Idle)
                .count();

            let (row0_bg, row0_fg, row0_mod) = if is_cursor || is_active {
                (accent(), bg_primary(), Modifier::BOLD)
            } else {
                (bg_secondary(), fg_primary(), Modifier::empty())
            };
            let (row12_bg, row12_fg) = if is_cursor || is_active {
                (accent(), bg_primary())
            } else {
                (bg_secondary(), fg_secondary())
            };
            // Cursor indicator: underline row0 when cursor is here but space is NOT active
            let extra_mod = if is_cursor && !is_active {
                Modifier::UNDERLINED
            } else {
                Modifier::empty()
            };

            // Row 0: name + close button
            let close_str = "[\u{00D7}]"; // [×]
            let name_max = (w as usize).saturating_sub(1 + 3); // " " + "[×]"
            let name_trunc = truncate(&space.name, name_max);
            let name_fill = format!(" {:<fill$}", name_trunc, fill = name_max);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        name_fill,
                        Style::default()
                            .fg(row0_fg)
                            .bg(row0_bg)
                            .add_modifier(row0_mod | extra_mod),
                    ),
                    Span::styled(close_str, Style::default().fg(row0_fg).bg(row0_bg)),
                ])),
                Rect {
                    x,
                    y: card_y,
                    width: w,
                    height: 1,
                },
            );

            // Row 1: cwd last component
            let cwd_last = space
                .cwd
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&space.cwd);
            let cwd_trunc = truncate(cwd_last, (w as usize).saturating_sub(1));
            let cwd_fill = format!(
                " {:<fill$}",
                cwd_trunc,
                fill = (w as usize).saturating_sub(1)
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    cwd_fill,
                    Style::default().fg(row12_fg).bg(row12_bg),
                ))),
                Rect {
                    x,
                    y: card_y + 1,
                    width: w,
                    height: 1,
                },
            );

            // Row 2: "Nt Np Na" stats + priority state dot after the count
            let agent_count = space_agents.len();
            let base_stats = if agent_count > 0 {
                format!(
                    " {}t {}p {}a",
                    space.tab_count, space.pane_count, agent_count
                )
            } else {
                format!(" {}t {}p", space.tab_count, space.pane_count)
            };
            let mut char_count = base_stats.chars().count();
            let mut stat_spans: Vec<Span> = vec![Span::styled(
                base_stats,
                Style::default().fg(row12_fg).bg(row12_bg),
            )];
            // State dot: priority Blocked > Error > Working > Idle; Done = no dot.
            // 1-space gap between "Na" and the dot.
            if agent_count > 0 {
                let dot: Option<(&str, ratatui::style::Color)> = if n_blocked > 0 {
                    Some(("\u{25CE}", blocked_pulse_color(app.tick_count)))
                } else if n_error > 0 {
                    Some(("\u{25C9}", accent_error()))
                } else if n_working > 0 {
                    Some(("\u{25CF}", working_pulse_color(app.tick_count)))
                } else if n_idle > 0 {
                    Some(("\u{25CB}", fg_muted()))
                } else {
                    None
                };
                if let Some((sym, color)) = dot {
                    char_count += 2; // space + symbol
                    stat_spans.push(Span::styled(" ", Style::default().bg(row12_bg)));
                    stat_spans.push(Span::styled(sym, Style::default().fg(color).bg(row12_bg)));
                }
            }
            let pad = (w as usize).saturating_sub(char_count);
            if pad > 0 {
                stat_spans.push(Span::styled(" ".repeat(pad), Style::default().bg(row12_bg)));
            }
            frame.render_widget(
                Paragraph::new(Line::from(stat_spans)),
                Rect {
                    x,
                    y: card_y + 2,
                    width: w,
                    height: 1,
                },
            );
        } else if idx == n {
            // "+Space" action button spanning SPACE_CARD_H rows
            let is_cursor = focused && idx == app.mobile_spaces_cursor;
            render_action_card(
                frame,
                x,
                card_y,
                w,
                SPACE_CARD_H as u16,
                "+Space",
                is_cursor,
            );
        } else {
            // Fill remaining rows with bg_primary
            for dr in 0..SPACE_CARD_H as u16 {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        " ".repeat(w as usize),
                        Style::default().bg(bg_primary()),
                    ))),
                    Rect {
                        x,
                        y: card_y + dr,
                        width: w,
                        height: 1,
                    },
                );
            }
        }
    }
}

fn render_tab_column(frame: &mut Frame, x: u16, base_y: u16, w: u16, content_h: usize, app: &App) {
    let n = app.tabs.len();
    let visible = content_h / TAB_CARD_H;
    let offset = scroll_offset(app.mobile_tabs_cursor, visible);
    let focused = app.mobile_col_focus == MobileColFocus::Right;

    for vis in 0..visible {
        let idx = vis + offset;
        let card_y = base_y + (vis * TAB_CARD_H) as u16;

        if idx < n {
            let tab = &app.tabs[idx];
            let is_active = tab.id == app.active_tab_id;
            let is_cursor = focused && idx == app.mobile_tabs_cursor;

            let marker = if is_active { "\u{25CF} " } else { "\u{25CB} " }; // ● or ○
            let close_str = "[\u{00D7}]";

            let (bg, marker_fg, text_fg) = if is_cursor {
                (accent(), bg_primary(), bg_primary())
            } else if is_active {
                (bg_card(), accent_idle(), fg_primary())
            } else {
                (bg_secondary(), fg_muted(), fg_secondary())
            };
            let close_fg = if is_cursor { bg_primary() } else { fg_muted() };
            let extra_mod = if is_cursor && !is_active {
                Modifier::UNDERLINED
            } else {
                Modifier::empty()
            };

            // Row 0: marker + name + close
            let name_max = (w as usize).saturating_sub(2 + 3); // marker(2) + close(3)
            let name_trunc = truncate(&tab.name, name_max);
            let name_fill = format!("{:<fill$}", name_trunc, fill = name_max);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(marker_fg).bg(bg)),
                    Span::styled(
                        name_fill,
                        Style::default().fg(text_fg).bg(bg).add_modifier(extra_mod),
                    ),
                    Span::styled(close_str, Style::default().fg(close_fg).bg(bg)),
                ])),
                Rect {
                    x,
                    y: card_y,
                    width: w,
                    height: 1,
                },
            );

            // Row 1: blank (just tap area)
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " ".repeat(w as usize),
                    Style::default().bg(bg),
                ))),
                Rect {
                    x,
                    y: card_y + 1,
                    width: w,
                    height: 1,
                },
            );
        } else if idx == n {
            let is_cursor = focused && idx == app.mobile_tabs_cursor;
            render_action_card(
                frame,
                x,
                card_y,
                w,
                TAB_CARD_H as u16,
                "+ New Tab",
                is_cursor,
            );
        } else {
            for dr in 0..TAB_CARD_H as u16 {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        " ".repeat(w as usize),
                        Style::default().bg(bg_primary()),
                    ))),
                    Rect {
                        x,
                        y: card_y + dr,
                        width: w,
                        height: 1,
                    },
                );
            }
        }
    }
}

fn render_action_card(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    label: &str,
    is_cursor: bool,
) {
    let (fg, bg) = if is_cursor {
        (bg_primary(), accent())
    } else {
        (accent(), bg_primary())
    };
    // First row: label
    let text = format!("{:<fill$}", label, fill = w as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(fg).bg(bg),
        ))),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
    // Remaining rows: blank (same bg)
    for dr in 1..h {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " ".repeat(w as usize),
                Style::default().bg(bg),
            ))),
            Rect {
                x,
                y: y + dr,
                width: w,
                height: 1,
            },
        );
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if max == 0 {
        return "";
    }
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}
