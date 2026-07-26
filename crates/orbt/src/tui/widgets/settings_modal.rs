use orbt_protocol::AgentProtocol;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear},
    Frame,
};

use crate::app::{AgentPanelMode, App};
use crate::tui::theme::*;
use crate::tui::widgets::agent_monitor::{status_icon, status_label};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if !app.settings_open {
        return;
    }

    let modal_w = 52u16.min(area.width.saturating_sub(4));
    // Dynamic height: base 12 for 3 settings rows + 2 for satellites header + N agent rows.
    let satellite_rows = app.agents.len().max(1) as u16;
    let modal_h = (14 + satellite_rows).min(area.height.saturating_sub(4));
    let x = area.x + area.width.saturating_sub(modal_w) / 2;
    let y = area.y + area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .style(Style::default().bg(bg_secondary()).fg(fg_primary()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border()))
        .title(Span::styled(
            " Settings ",
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(block, modal_area);

    let inner = Rect {
        x: modal_area.x + 1,
        y: modal_area.y + 1,
        width: modal_area.width.saturating_sub(2),
        height: modal_area.height.saturating_sub(2),
    };

    let theme_display: String = app
        .theme_name
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let sidebar_display = if app.sidebar_visible { "On" } else { "Off" };
    let agent_display = match app.agent_panel_mode {
        AgentPanelMode::Sidebar => "Sidebar",
        AgentPanelMode::Modal => "Modal",
        AgentPanelMode::Hidden => "Off",
    };

    let rows: Vec<(&str, String)> = vec![
        ("Theme", theme_display),
        ("Sidebar", sidebar_display.to_string()),
        ("Agent Panel", agent_display.to_string()),
    ];

    for (i, (label, value)) in rows.iter().enumerate() {
        let value = value.as_str();
        let is_selected = i == app.settings_selected;
        let row_y = inner.y + i as u16 + 1;
        let bg = if is_selected {
            bg_primary()
        } else {
            bg_secondary()
        };
        let fg_label = if is_selected {
            fg_primary()
        } else {
            fg_secondary()
        };
        let fg_value = if is_selected { accent() } else { fg_muted() };
        let marker = if is_selected {
            Span::styled("> ", Style::default().fg(accent()))
        } else {
            Span::raw("  ")
        };
        let bracket_str = format!("[{}]", value);
        let used = 2 + label.len() as u16 + bracket_str.len() as u16;
        let gap = inner.width.saturating_sub(used);
        let line = Line::from(vec![
            marker,
            Span::styled(*label, Style::default().fg(fg_label).bg(bg)),
            Span::styled(" ".repeat(gap as usize), Style::default().bg(bg)),
            Span::styled(bracket_str, Style::default().fg(fg_value).bg(bg)),
        ]);
        frame.render_widget(
            line,
            Rect {
                x: inner.x,
                y: row_y,
                width: inner.width,
                height: 1,
            },
        );
    }

    // --- Agents section (read-only) ---
    let sat_header_y = inner.y + 5;
    if sat_header_y < inner.y + inner.height.saturating_sub(2) {
        // "── Agents ──────" divider
        let header_label = "\u{2500}\u{2500} Agents ";
        let fill = (inner.width as usize).saturating_sub(header_label.len());
        let header_str = format!("{}{}", header_label, "\u{2500}".repeat(fill));
        frame.render_widget(
            Line::from(Span::styled(header_str, Style::default().fg(fg_muted()))),
            Rect {
                x: inner.x,
                y: sat_header_y,
                width: inner.width,
                height: 1,
            },
        );

        let sat_start_y = sat_header_y + 1;
        // Leave 1 row for possible overflow indicator + 1 for footer
        let sat_max_rows = (inner.y + inner.height).saturating_sub(sat_start_y + 2);

        if app.agents.is_empty() {
            if sat_start_y < inner.y + inner.height.saturating_sub(1) {
                frame.render_widget(
                    Line::from(Span::styled(
                        "  No agents detected",
                        Style::default().fg(fg_muted()),
                    )),
                    Rect {
                        x: inner.x,
                        y: sat_start_y,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
        } else {
            let visible = sat_max_rows as usize;
            let total = app.agents.len();
            for (i, agent) in app.agents.iter().take(visible).enumerate() {
                let row_y = sat_start_y + i as u16;
                let icon = status_icon(&agent.status);
                let slabel = status_label(&agent.status);
                let icon_color = match agent.status {
                    orbt_protocol::AgentStatus::Working => accent(),
                    orbt_protocol::AgentStatus::Idle => fg_muted(),
                    orbt_protocol::AgentStatus::Blocked => accent_blocked(),
                    orbt_protocol::AgentStatus::Error => accent_error(),
                    orbt_protocol::AgentStatus::Done => fg_muted(),
                };

                let is_acp = !matches!(agent.protocol, AgentProtocol::Heuristic);
                let (badge_str, badge_color) = if is_acp {
                    ("[ACP] ", accent_idle())
                } else {
                    ("[heur]", fg_muted())
                };

                // Layout: "  icon name<16>  badge<6>  status_label"
                let name_max = 16usize;
                let name = if agent.name.chars().count() > name_max {
                    let mut s: String = agent.name.chars().take(name_max - 1).collect();
                    s.push('\u{2026}');
                    s
                } else {
                    format!("{:<width$}", agent.name, width = name_max)
                };

                let right_part = format!("  {}", slabel);
                let right_len = right_part.len() as u16;
                // total fixed: 2 (indent) + 1 (icon) + 1 (space) + 16 (name) + 2 (pad) + 6 (badge)
                let fixed = 2u16 + 1 + 1 + name_max as u16 + 2 + badge_str.len() as u16;
                let pad = inner.width.saturating_sub(fixed + right_len) as usize;

                frame.render_widget(
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(icon, Style::default().fg(icon_color)),
                        Span::raw(" "),
                        Span::styled(name, Style::default().fg(fg_secondary())),
                        Span::raw("  "),
                        Span::styled(badge_str, Style::default().fg(badge_color)),
                        Span::styled(" ".repeat(pad), Style::default()),
                        Span::styled(right_part, Style::default().fg(fg_muted())),
                    ]),
                    Rect {
                        x: inner.x,
                        y: row_y,
                        width: inner.width,
                        height: 1,
                    },
                );
            }
            // Overflow indicator when agents don't all fit
            if total > visible {
                let more = total - visible;
                let overflow_y = sat_start_y + visible as u16;
                if overflow_y < inner.y + inner.height.saturating_sub(1) {
                    frame.render_widget(
                        Line::from(Span::styled(
                            format!("  \u{25BE} {more} more"),
                            Style::default().fg(fg_muted()),
                        )),
                        Rect {
                            x: inner.x,
                            y: overflow_y,
                            width: inner.width,
                            height: 1,
                        },
                    );
                }
            }
        }
    }

    let footer_y = inner.y + inner.height.saturating_sub(1);
    let footer = Line::from(vec![
        Span::styled("Esc ", Style::default().fg(accent())),
        Span::styled("close  ", Style::default().fg(fg_muted())),
        Span::styled("\u{2191}\u{2193} ", Style::default().fg(accent())),
        Span::styled("navigate  ", Style::default().fg(fg_muted())),
        Span::styled("Enter ", Style::default().fg(accent())),
        Span::styled("toggle", Style::default().fg(fg_muted())),
    ]);
    frame.render_widget(
        footer,
        Rect {
            x: inner.x,
            y: footer_y,
            width: inner.width,
            height: 1,
        },
    );
}
