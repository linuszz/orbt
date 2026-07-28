use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear},
    Frame,
};

use crate::app::{App, InputMode, COMMANDS};
use crate::tui::theme::*;

const FLEET_COMMAND_IDS: &[&str] = &["toggle_agent", "agent_scroll_up", "agent_scroll_down"];

fn filter_indices(search: &str, fleet_enabled: bool) -> Vec<usize> {
    let s = search.to_lowercase();
    COMMANDS
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            if !fleet_enabled && FLEET_COMMAND_IDS.contains(&c.id) {
                return false;
            }
            if search.is_empty() {
                return true;
            }
            c.label.to_lowercase().contains(&s)
        })
        .map(|(i, _)| i)
        .collect()
}

struct RowMap {
    total_rows: usize,
    item_rows: Vec<usize>,
}

fn build_row_map(filtered: &[usize], search: &str) -> RowMap {
    let mut item_rows = Vec::with_capacity(filtered.len());
    let mut row = 0usize;
    let mut last_group = "";
    for &cmd_idx in filtered {
        let cmd = &COMMANDS[cmd_idx];
        if cmd.group != last_group && search.is_empty() {
            row += 1;
            last_group = cmd.group;
        }
        item_rows.push(row);
        row += 1;
    }
    RowMap {
        total_rows: row,
        item_rows,
    }
}

/// Render the command palette over `area`, dimming everything except the sidebar.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let sb_w = if app.sidebar_visible {
        crate::tui::SIDEBAR_W
    } else {
        crate::tui::SIDEBAR_COLLAPSED_W
    };
    let dim_area = Rect {
        x: area.x + sb_w,
        y: area.y,
        width: area.width.saturating_sub(sb_w),
        height: area.height,
    };
    render_inner(frame, area, dim_area, app);
}

/// Render the command palette over `area` without any dim overlay.
/// Used by mobile COMMAND view — terminal content shows through unchanged.
pub fn render_mobile(frame: &mut Frame, area: Rect, app: &App) {
    let no_dim = Rect {
        x: area.x,
        y: area.y,
        width: 0,
        height: 0,
    };
    render_inner(frame, area, no_dim, app);
}

fn render_inner(frame: &mut Frame, area: Rect, dim_area: Rect, app: &App) {
    if let InputMode::CommandPalette {
        search,
        selected,
        search_focused,
    } = &app.mode
    {
        let palette_w = 50u16.min(area.width.saturating_sub(4));
        let palette_h = 20u16.min(area.height.saturating_sub(4));
        let x = area.x + (area.width - palette_w) / 2;
        let y = area.y + (area.height - palette_h) / 2;
        let palette_area = Rect {
            x,
            y,
            width: palette_w,
            height: palette_h,
        };

        let dim =
            Block::default().style(Style::default().bg(ratatui::style::Color::Rgb(10, 10, 14)));
        frame.render_widget(dim, dim_area);

        frame.render_widget(Clear, palette_area);

        let block = Block::default()
            .style(Style::default().bg(bg_secondary()).fg(fg_primary()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border()));
        frame.render_widget(block, palette_area);

        let inner = Rect {
            x: palette_area.x + 1,
            y: palette_area.y + 1,
            width: palette_area.width.saturating_sub(2),
            height: palette_area.height.saturating_sub(2),
        };

        let search_line = if search.is_empty() && !*search_focused {
            Line::from(vec![
                Span::styled("/ to search", Style::default().fg(fg_muted())),
                Span::raw("  "),
                Span::styled(
                    "up/down navigate  Enter select  Esc close  , settings",
                    Style::default().fg(fg_muted()),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(accent())),
                Span::styled(search.as_str(), Style::default().fg(fg_primary())),
                Span::styled(
                    "_",
                    Style::default()
                        .fg(fg_primary())
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ])
        };
        frame.render_widget(
            search_line,
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
        );

        let sep = Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            Style::default().fg(border()),
        );
        frame.render_widget(
            Line::from(sep),
            Rect {
                x: inner.x,
                y: inner.y + 1,
                width: inner.width,
                height: 1,
            },
        );

        let filtered = filter_indices(search, app.agent_fleet_enabled);
        let list_y = inner.y + 2;
        let list_h = inner.height.saturating_sub(3) as usize;

        let row_map = build_row_map(&filtered, search);
        let selected_row = row_map.item_rows.get(*selected).copied().unwrap_or(0);

        let scroll = if row_map.total_rows <= list_h {
            0
        } else {
            let half = list_h / 2;
            if selected_row < half {
                0
            } else if selected_row >= row_map.total_rows.saturating_sub(list_h - half) {
                row_map.total_rows.saturating_sub(list_h)
            } else {
                selected_row - half
            }
        };

        let mut current_y = list_y;
        let mut last_group = "";
        let mut rendered_rows = 0usize;

        for (vis_idx, &cmd_idx) in filtered.iter().enumerate() {
            let item_row = row_map.item_rows[vis_idx];
            if item_row < scroll {
                if search.is_empty() {
                    last_group = COMMANDS[cmd_idx].group;
                }
                continue;
            }
            if rendered_rows >= list_h {
                break;
            }

            let cmd = &COMMANDS[cmd_idx];

            if cmd.group != last_group && search.is_empty() {
                if rendered_rows < list_h {
                    let group_line = Line::from(vec![Span::styled(
                        cmd.group.to_uppercase(),
                        Style::default().fg(fg_muted()).add_modifier(Modifier::BOLD),
                    )]);
                    frame.render_widget(
                        group_line,
                        Rect {
                            x: inner.x,
                            y: current_y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                    current_y += 1;
                    rendered_rows += 1;
                }
                last_group = cmd.group;
            }

            if rendered_rows >= list_h {
                break;
            }

            let is_selected = vis_idx == *selected;
            let bg = if is_selected {
                bg_primary()
            } else {
                bg_secondary()
            };
            let fg = if is_selected {
                fg_primary()
            } else {
                fg_secondary()
            };
            let marker_color = if is_selected {
                accent()
            } else {
                bg_secondary()
            };

            let mut spans = vec![
                Span::styled(
                    if is_selected { ">" } else { " " },
                    Style::default().fg(marker_color).bg(bg),
                ),
                Span::styled(" ", Style::default().bg(bg)),
                Span::styled(cmd.label, Style::default().fg(fg).bg(bg)),
            ];

            if !cmd.shortcut.is_empty() {
                let pad = inner
                    .width
                    .saturating_sub(cmd.label.len() as u16 + cmd.shortcut.len() as u16 + 6);
                spans.push(Span::styled(
                    " ".repeat(pad as usize),
                    Style::default().bg(bg),
                ));
                spans.push(Span::styled(
                    cmd.shortcut,
                    Style::default().fg(accent()).bg(bg),
                ));
            }

            let line = Line::from(spans).style(Style::default().bg(bg));
            frame.render_widget(
                line,
                Rect {
                    x: inner.x,
                    y: current_y,
                    width: inner.width,
                    height: 1,
                },
            );
            current_y += 1;
            rendered_rows += 1;
        }

        if filtered.is_empty() {
            let empty = Line::from(vec![Span::styled(
                "No commands found",
                Style::default().fg(fg_muted()),
            )]);
            frame.render_widget(
                empty,
                Rect {
                    x: inner.x,
                    y: list_y,
                    width: inner.width,
                    height: 1,
                },
            );
        }

        if row_map.total_rows > list_h {
            let scroll_pct = if row_map.total_rows > 0 {
                ((scroll as f64 / row_map.total_rows as f64) * 100.0) as u16
            } else {
                0
            };
            let indicator = format!(" {}% ", scroll_pct);
            let footer_y = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Line::from(vec![
                    Span::styled("Esc ", Style::default().fg(accent())),
                    Span::styled("close  ", Style::default().fg(fg_muted())),
                    Span::styled("\u{2191}\u{2193} ", Style::default().fg(accent())),
                    Span::styled("navigate  ", Style::default().fg(fg_muted())),
                    Span::styled(", ", Style::default().fg(accent())),
                    Span::styled("settings", Style::default().fg(fg_muted())),
                    Span::raw(" ".repeat(inner.width.saturating_sub(36) as usize)),
                    Span::styled(indicator, Style::default().fg(fg_muted())),
                ]),
                Rect {
                    x: inner.x,
                    y: footer_y,
                    width: inner.width,
                    height: 1,
                },
            );
        } else {
            let footer_y = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Line::from(vec![
                    Span::styled("Esc ", Style::default().fg(accent())),
                    Span::styled("close  ", Style::default().fg(fg_muted())),
                    Span::styled("\u{2191}\u{2193} ", Style::default().fg(accent())),
                    Span::styled("navigate  ", Style::default().fg(fg_muted())),
                    Span::styled(", ", Style::default().fg(accent())),
                    Span::styled("settings", Style::default().fg(fg_muted())),
                ]),
                Rect {
                    x: inner.x,
                    y: footer_y,
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }
}
