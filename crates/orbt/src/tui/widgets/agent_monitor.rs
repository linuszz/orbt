use orbt_protocol::{AgentInfo, AgentMetrics, AgentProtocol, AgentStatus};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{AgentHover, App, InputMode};
use crate::tui::theme::*;

pub fn status_icon(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "\u{25CF}", // ●
        AgentStatus::Idle => "\u{25CB}",    // ○
        AgentStatus::Blocked => "\u{25CE}", // ◎
        AgentStatus::Error => "\u{25C9}",   // ◉
        AgentStatus::Done => "\u{25CC}",    // ◌
    }
}

fn status_color(status: &AgentStatus) -> ratatui::style::Color {
    match status {
        AgentStatus::Working => accent(),
        AgentStatus::Idle => fg_muted(),
        AgentStatus::Blocked => accent_blocked(),
        AgentStatus::Error => accent_error(),
        AgentStatus::Done => fg_muted(),
    }
}

/// Smooth lerp between two u8 values at phase in [0.0, 1.0].
#[inline(always)]
fn lerp_u8(a: u8, b: u8, phase: f32) -> u8 {
    (a as f32 + phase * (b as f32 - a as f32)) as u8
}

/// Triangle wave: returns phase in [0.0, 1.0] over `period` ticks, peaking at mid-cycle.
#[inline(always)]
fn triangle_phase(tick: u64, period: u64) -> f32 {
    let t = (tick % period) as f32;
    let half = period as f32 / 2.0;
    if t < half {
        t / half
    } else {
        (period as f32 - t) / half
    }
}

/// Working slow pulse color (90 ticks / ~1.5 s): ACCENT_DIM → ACCENT_BRIGHT.
pub fn working_pulse_color(tick: u64) -> ratatui::style::Color {
    let p = triangle_phase(tick, 90);
    ratatui::style::Color::Rgb(
        lerp_u8(120, 251, p), // #783c00 → #fba028
        lerp_u8(60, 160, p),
        lerp_u8(0, 40, p),
    )
}

/// Blocked fast pulse color (48 ticks / ~0.8 s): dark gold → accent_blocked().
pub fn blocked_pulse_color(tick: u64) -> ratatui::style::Color {
    let p = triangle_phase(tick, 48);
    ratatui::style::Color::Rgb(
        lerp_u8(100, 217, p), // dim → #d9ac00
        lerp_u8(85, 172, p),
        0,
    )
}

/// Error blink color (60 ticks / ~1.0 s): dark red → accent_error().
pub fn error_blink_color(tick: u64) -> ratatui::style::Color {
    let p = triangle_phase(tick, 60);
    ratatui::style::Color::Rgb(
        lerp_u8(80, 200, p), // dark red → #c8321e
        lerp_u8(10, 50, p),
        lerp_u8(5, 30, p),
    )
}

/// Animated status color per spec §3.3 animation table.
fn animated_status_color(status: &AgentStatus, tick: u64) -> ratatui::style::Color {
    match status {
        AgentStatus::Working => working_pulse_color(tick),
        AgentStatus::Blocked => blocked_pulse_color(tick),
        AgentStatus::Error => error_blink_color(tick),
        _ => status_color(status),
    }
}

/// Bracket-form status label for inline card display (§3.2 design spec).
pub fn status_label(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "[Working]",
        AgentStatus::Idle => "[Standby]",
        AgentStatus::Blocked => "[Blocked!]",
        AgentStatus::Error => "[Error]",
        AgentStatus::Done => "[Done]",
    }
}

// Returns ([btn_label, is_danger]; 3 slots).
// `wide` selects full labels (panel >= 25 cols / inner >= 24) vs compact (inner < 24).
// When `is_acp` is true, slot 2 is overridden with "[Detail]" (opens Agent Detail modal).
fn card_buttons(status: &AgentStatus, wide: bool, is_acp: bool) -> [(&'static str, bool); 3] {
    let mut btns = if wide {
        match status {
            AgentStatus::Working => [("[View]", false), ("[Stop]", false), ("[Chat]", false)],
            AgentStatus::Idle => [("[View]", false), ("[Chat]", false), ("[Remove]", true)],
            AgentStatus::Blocked => [("[View]", false), ("[Respond]", false), ("[Abort]", true)],
            AgentStatus::Error => [("[View]", false), ("[Restart]", false), ("[Remove]", true)],
            AgentStatus::Done => [("[View]", false), ("[Chat]", false), ("[Remove]", true)],
        }
    } else {
        match status {
            AgentStatus::Working => [("[View]", false), ("[Stop]", false), ("[Chat]", false)],
            AgentStatus::Idle => [("[View]", false), ("[Chat]", false), ("[Rmov]", true)],
            AgentStatus::Blocked => [("[View]", false), ("[Resp]", false), ("[Abrt]", true)],
            AgentStatus::Error => [("[View]", false), ("[Rstr]", false), ("[Rmov]", true)],
            AgentStatus::Done => [("[View]", false), ("[Chat]", false), ("[Rmov]", true)],
        }
    };
    if is_acp {
        btns[2] = ("[Detail]", false);
    }
    btns
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    }
}

fn format_rss(rss_kb: u32) -> String {
    if rss_kb < 1024 {
        format!("{rss_kb}k")
    } else if rss_kb < 1024 * 1024 {
        format!("{}M", rss_kb / 1024)
    } else {
        format!("{}G", rss_kb / 1024 / 1024)
    }
}

fn format_duration(secs: u32) -> String {
    if secs < 60 {
        "now".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .style(Style::default().bg(bg_primary()))
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(border()));
    frame.render_widget(block, area);

    let ix = area.x + 1; // inner x (after left border)
    let iw = area.width.saturating_sub(1); // inner width

    let any_blocked = app.agents.iter().any(|a| a.status == AgentStatus::Blocked);
    let blocked_agents: Vec<&AgentInfo> = app
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Blocked)
        .collect();

    // --- Header ---
    {
        let n = app.agents.len();
        let badge = format!("[{}]", n);
        let badge_color = if any_blocked {
            blocked_pulse_color(app.tick_count)
        } else {
            fg_muted()
        };
        // right side: "[+]×" = 4 chars
        let right_chars = 4u16;
        let fill = iw.saturating_sub(6 + 1 + badge.len() as u16 + right_chars) as usize;

        let (add_fg, add_bg) = if app.agent_hovered == Some(AgentHover::HeaderAdd) {
            (bg_primary(), accent_hover())
        } else {
            (fg_muted(), bg_secondary())
        };
        let (close_fg, close_bg) = if app.agent_hovered == Some(AgentHover::HeaderClose) {
            (bg_primary(), accent_error())
        } else {
            (fg_muted(), bg_secondary())
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "AGENTS",
                    Style::default()
                        .fg(fg_primary())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(badge, Style::default().fg(badge_color)),
                Span::raw(" ".repeat(fill)),
                Span::styled("[+]", Style::default().fg(add_fg).bg(add_bg)),
                Span::styled("\u{00D7}", Style::default().fg(close_fg).bg(close_bg)),
            ])),
            Rect {
                x: ix,
                y: area.y,
                width: iw,
                height: 1,
            },
        );
    }

    // --- Divider ---
    let div_y = area.y + 1;
    frame.render_widget(
        Line::from(Span::styled(
            "\u{2500}".repeat(iw as usize),
            Style::default().fg(border()),
        )),
        Rect {
            x: ix,
            y: div_y,
            width: iw,
            height: 1,
        },
    );

    let mut y = area.y + 2;

    // --- "N above" scroll indicator ---
    if app.agent_scroll_offset > 0 && y < area.y + area.height {
        let above_text = format!(" \u{25B4} {} above", app.agent_scroll_offset);
        frame.render_widget(
            Paragraph::new(Span::styled(above_text, Style::default().fg(fg_muted()))),
            Rect {
                x: ix,
                y,
                width: iw,
                height: 1,
            },
        );
        y += 1;
    }

    // --- Eclipse banner ---
    if !blocked_agents.is_empty() {
        let name_part = if blocked_agents.len() == 1 {
            truncate_str(&blocked_agents[0].name, 10)
        } else {
            format!("{} agents", blocked_agents.len())
        };
        let icon_color = blocked_pulse_color(app.tick_count);
        let prefix = " Eclipse \u{2014} ";
        let name_max = (iw as usize).saturating_sub(1 + prefix.len());
        let name_trunc = truncate_str(&name_part, name_max.max(2));
        let text_content = format!("{}{}", prefix, name_trunc);
        let text_part = format!(
            "{:<width$}",
            text_content,
            width = iw.saturating_sub(1) as usize
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "\u{25CE}",
                    Style::default().fg(icon_color).bg(bg_tertiary()),
                ),
                Span::styled(
                    text_part,
                    Style::default().fg(accent_blocked()).bg(bg_tertiary()),
                ),
            ])),
            Rect {
                x: ix,
                y,
                width: iw,
                height: 1,
            },
        );
        y += 1;

        let (resp_fg, resp_bg) = if app.agent_hovered == Some(AgentHover::EclipseRespond) {
            (bg_primary(), accent_blocked())
        } else {
            (accent_blocked(), bg_tertiary())
        };
        let block_preview = blocked_agents
            .first()
            .and_then(|a| a.detail.as_ref())
            .and_then(|d| d.block_msg.as_deref())
            .unwrap_or("");
        let fill = (iw as usize).saturating_sub(10);
        let preview_trunc = truncate_str(block_preview, fill);
        let preview_padded = format!("{:<fill$}", preview_trunc, fill = fill);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {}", preview_padded),
                    Style::default().fg(fg_muted()).bg(bg_tertiary()),
                ),
                Span::styled("[Respond]", Style::default().fg(resp_fg).bg(resp_bg)),
            ])),
            Rect {
                x: ix,
                y,
                width: iw,
                height: 1,
            },
        );
        y += 1;
    }

    // --- Cards or empty state ---
    if app.agents.is_empty() {
        let mid_y = (area.y + area.height) / 2;
        if mid_y >= y && mid_y < area.y + area.height {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(
                        "{:^width$}",
                        "\u{25CB} \u{25CB} \u{25CB}",
                        width = iw as usize
                    ),
                    Style::default().fg(fg_muted()),
                ))),
                Rect {
                    x: ix,
                    y: mid_y,
                    width: iw,
                    height: 1,
                },
            );
            if mid_y + 1 < area.y + area.height {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!("{:^width$}", "No agents running", width = iw as usize),
                        Style::default().fg(fg_muted()),
                    ))),
                    Rect {
                        x: ix,
                        y: mid_y + 1,
                        width: iw,
                        height: 1,
                    },
                );
            }
        }
    } else {
        let visible_agents: Vec<&AgentInfo> =
            app.agents.iter().skip(app.agent_scroll_offset).collect();
        let total = app.agents.len();
        let is_wide = iw >= 30;
        // Narrow slot: separator(1) + 5 content rows = 6. Wide slot: top/content/bottom/gap = 7.
        let min_slot: u16 = if is_wide { 7 } else { 6 };
        // Reserve 1 row at the bottom for the footer.
        let content_bottom = area.y + area.height.saturating_sub(1);
        for (card_idx, agent) in visible_agents.iter().enumerate() {
            if y + min_slot > content_bottom {
                let remaining = total - app.agent_scroll_offset - card_idx;
                if remaining > 0 && content_bottom >= 1 && y < content_bottom {
                    let more_text = format!(" \u{25BE} {} more", remaining);
                    frame.render_widget(
                        Paragraph::new(Span::styled(more_text, Style::default().fg(fg_muted()))),
                        Rect {
                            x: ix,
                            y: content_bottom.saturating_sub(1),
                            width: iw,
                            height: 1,
                        },
                    );
                }
                break;
            }
            let metrics = app.agent_metrics.get(&agent.id);
            let slot_h = render_card(frame, ix, y, iw, agent, card_idx, app, metrics);
            y += slot_h;
        }
    }

    render_footer(frame, ix, iw, area, app);
}

/// Dispatch to narrow or wide card renderer. Returns rows consumed (slot height).
fn render_card(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    agent: &AgentInfo,
    card_idx: usize,
    app: &App,
    metrics: Option<&AgentMetrics>,
) -> u16 {
    if w < 30 {
        render_card_narrow(frame, x, y, w, agent, card_idx, app, metrics)
    } else {
        render_card_wide(frame, x, y, w, agent, card_idx, app, metrics)
    }
}

/// Narrow card (iw < 30): separator rule + 5 content rows. Returns slot height 6.
/// Slot layout:
///   slot+0: ─ separator rule
///   slot+1: ● name  [Status] dur
///   slot+2: ▌ cwd · model  [ACP]
///   slot+3: ▌ task/block_msg
///   slot+4: ▌ progress bar
///   slot+5: ▌ [View] [Stop] [Chat]
fn render_card_narrow(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    agent: &AgentInfo,
    card_idx: usize,
    app: &App,
    metrics: Option<&AgentMetrics>,
) -> u16 {
    let sc = animated_status_color(&agent.status, app.tick_count);
    let icon = status_icon(&agent.status);
    let label = status_label(&agent.status);
    let is_acp = !matches!(agent.protocol, AgentProtocol::Heuristic);

    let is_selected = if let InputMode::AgentPanel { selected } = &app.mode {
        *selected == card_idx + app.agent_scroll_offset
    } else {
        false
    };
    let card_bg = if is_selected {
        bg_card()
    } else {
        bg_secondary()
    };

    // Selection/status mark for rows 2-5 (▸ selected, ▌ animated otherwise).
    let accent_mark = if is_selected {
        Span::styled("\u{25B8}", Style::default().fg(accent()).bg(card_bg))
    } else {
        Span::styled("\u{258C}", Style::default().fg(sc).bg(card_bg))
    };

    // slot+0: separator rule
    frame.render_widget(
        Paragraph::new(Span::styled(
            "\u{2500}".repeat(w as usize),
            Style::default().fg(border()).bg(bg_secondary()),
        )),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );

    // slot+1: icon + name + [Status] + dur
    {
        let duration_s = app
            .agent_start_times
            .get(&agent.id)
            .map(|t| t.elapsed().as_secs() as u32)
            .or_else(|| agent.detail.as_ref().map(|d| d.duration_s))
            .unwrap_or(0);
        let dur_str = if duration_s > 0 {
            format!(" {}", format_duration(duration_s))
        } else {
            String::new()
        };
        // icon(1) + space(1) + name_padded + space(1) + label + dur_str = w
        let right_total = 1 + label.len() + dur_str.len();
        let name_w = (w as usize).saturating_sub(2 + right_total);
        let name = truncate_str(&agent.name, name_w);
        let name_padded = format!("{:<width$}", name, width = name_w);
        let label_color = match agent.status {
            AgentStatus::Working => accent(),
            AgentStatus::Blocked => accent_blocked(),
            AgentStatus::Error => accent_error(),
            _ => fg_muted(),
        };
        let label_mod = if agent.status == AgentStatus::Blocked {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(icon, Style::default().fg(sc).bg(card_bg)),
                Span::styled(" ", Style::default().bg(card_bg)),
                Span::styled(
                    name_padded,
                    Style::default()
                        .fg(fg_primary())
                        .bg(card_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default().bg(card_bg)),
                Span::styled(
                    label,
                    Style::default()
                        .fg(label_color)
                        .bg(card_bg)
                        .add_modifier(label_mod),
                ),
                Span::styled(dur_str, Style::default().fg(fg_muted()).bg(card_bg)),
            ])),
            Rect {
                x,
                y: y + 1,
                width: w,
                height: 1,
            },
        );
    }

    // slot+2: ▌ + cwd · model + [ACP] + rss
    {
        let rss_str = metrics.and_then(|m| m.rss_kb).map(format_rss);
        let inner_w = w.saturating_sub(1) as usize;

        let cwd_short = app
            .spaces
            .iter()
            .find(|s| s.space_id == agent.space_id)
            .and_then(|s| {
                std::path::Path::new(&s.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            });

        let left_content = match (&cwd_short, agent.model.is_empty()) {
            (Some(cwd), false) if !cwd.is_empty() => {
                format!("{} \u{00B7} {}", cwd, agent.model)
            }
            (Some(cwd), true) if !cwd.is_empty() => cwd.clone(),
            (_, false) => agent.model.clone(),
            _ => String::new(),
        };

        let badge = if is_acp { " [ACP]" } else { "" };
        let badge_len = badge.len();
        let right = rss_str.unwrap_or_default();
        let right_w = if right.is_empty() { 0 } else { right.len() + 1 };
        let left_max = inner_w.saturating_sub(badge_len + right_w);
        let left = truncate_str(&left_content, left_max);
        let pad = inner_w.saturating_sub(left.len() + badge_len + right_w);

        let mut row_spans = vec![
            accent_mark.clone(),
            Span::styled(left, Style::default().fg(fg_muted()).bg(card_bg)),
        ];
        if is_acp {
            row_spans.push(Span::styled(
                badge,
                Style::default().fg(accent_idle()).bg(card_bg),
            ));
        }
        row_spans.push(Span::styled(" ".repeat(pad), Style::default().bg(card_bg)));
        if !right.is_empty() {
            row_spans.push(Span::styled(
                right,
                Style::default().fg(fg_muted()).bg(card_bg),
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(row_spans)),
            Rect {
                x,
                y: y + 2,
                width: w,
                height: 1,
            },
        );
    }

    // slot+3: ▌ + task/block_msg (or current ACP tool call when available)
    {
        let acp_tool = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .and_then(|a| a.current_tool.as_ref())
            .filter(|_| {
                matches!(
                    agent.status,
                    AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
                )
            });
        let task_str: std::borrow::Cow<str> = if let Some(ct) = acp_tool {
            std::borrow::Cow::Owned(format!("\u{25B6} {}({})", ct.tool, ct.args_summary))
        } else {
            let s = match agent.status {
                AgentStatus::Blocked => agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.block_msg.as_deref())
                    .unwrap_or(""),
                AgentStatus::Working => metrics
                    .and_then(|m| m.recent_lines.first().map(String::as_str))
                    .or_else(|| agent.detail.as_ref().and_then(|d| d.task.as_deref()))
                    .unwrap_or(""),
                _ => agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.task.as_deref())
                    .unwrap_or(""),
            };
            std::borrow::Cow::Borrowed(s)
        };
        let task = truncate_str(&task_str, w.saturating_sub(1) as usize);
        let task_body = format!("{:<width$}", task, width = w.saturating_sub(1) as usize);
        let (task_fg, task_mod) = if acp_tool.is_some() {
            (sc, Modifier::empty())
        } else {
            match agent.status {
                AgentStatus::Blocked => (accent_blocked(), Modifier::BOLD),
                AgentStatus::Error => (accent_error(), Modifier::empty()),
                _ => (fg_secondary(), Modifier::empty()),
            }
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                accent_mark.clone(),
                Span::styled(
                    task_body,
                    Style::default()
                        .fg(task_fg)
                        .bg(card_bg)
                        .add_modifier(task_mod),
                ),
            ])),
            Rect {
                x,
                y: y + 3,
                width: w,
                height: 1,
            },
        );
    }

    // slot+4: ▌ + progress bar
    {
        let show_bar = matches!(
            agent.status,
            AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
        );
        let progress = agent.detail.as_ref().and_then(|d| d.progress);
        if show_bar {
            let bar_w = w.saturating_sub(5) as usize;
            let (bar, suffix) = if let Some(pct) = progress {
                let pct = pct.clamp(0.0, 1.0);
                let filled = (pct * bar_w as f32) as usize;
                let b = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
                (b, format!("{:3.0}%", pct * 100.0))
            } else {
                let window = 4usize;
                let cycle = (bar_w + window + 2) as u64;
                let pos = ((app.tick_count / 5) % cycle) as usize;
                let b: String = (0..bar_w)
                    .map(|c| {
                        if c >= pos && c < pos + window {
                            "\u{2588}"
                        } else {
                            "\u{2591}"
                        }
                    })
                    .collect();
                let sfx = metrics
                    .and_then(|m| m.cpu_percent)
                    .map(|c| format!("{:3.0}%", c))
                    .unwrap_or_else(|| "    ".to_string());
                (b, sfx)
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    accent_mark.clone(),
                    Span::styled(bar, Style::default().fg(sc).bg(card_bg)),
                    Span::styled(suffix, Style::default().fg(fg_muted()).bg(card_bg)),
                ])),
                Rect {
                    x,
                    y: y + 4,
                    width: w,
                    height: 1,
                },
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    accent_mark.clone(),
                    Span::styled(
                        " ".repeat(w.saturating_sub(1) as usize),
                        Style::default().bg(card_bg),
                    ),
                ])),
                Rect {
                    x,
                    y: y + 4,
                    width: w,
                    height: 1,
                },
            );
        }
    }

    // slot+5: ▌ + buttons
    {
        let buttons = card_buttons(&agent.status, w >= 24, is_acp);
        let mut spans = vec![accent_mark];
        for (slot, (btn_label, is_danger)) in buttons.iter().enumerate() {
            if slot > 0 {
                spans.push(Span::styled(" ", Style::default().bg(card_bg)));
            }
            let hovered = app.agent_hovered
                == Some(AgentHover::CardBtn {
                    card_idx,
                    slot: slot as u8,
                });
            let (fg, bg) = if hovered {
                (
                    bg_primary(),
                    if *is_danger {
                        accent_error()
                    } else {
                        accent_hover()
                    },
                )
            } else if *is_danger {
                (accent_error(), card_bg)
            } else {
                (fg_muted(), card_bg)
            };
            spans.push(Span::styled(*btn_label, Style::default().fg(fg).bg(bg)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x,
                y: y + 5,
                width: w,
                height: 1,
            },
        );
    }

    6 // slot height: separator(1) + 5 content rows
}

/// Wide card (iw >= 30): full box border. Returns slot height 7.
/// Slot layout:
///   slot+0: ┌─● name ─── [Status] dur ─┐  (border color = animated status)
///   slot+1: │ cwd · model       [ACP]   │
///   slot+2: │ task/block_msg            │
///   slot+3: │ progress bar   cpu%       │
///   slot+4: │ [View]  [Stop]   [Chat]   │
///   slot+5: └───────────────────────────┘  (border color)
///   slot+6: blank gap
fn render_card_wide(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    agent: &AgentInfo,
    card_idx: usize,
    app: &App,
    metrics: Option<&AgentMetrics>,
) -> u16 {
    let sc = animated_status_color(&agent.status, app.tick_count);
    let icon = status_icon(&agent.status);
    let label = status_label(&agent.status);
    let is_acp = !matches!(agent.protocol, AgentProtocol::Heuristic);

    let is_selected = if let InputMode::AgentPanel { selected } = &app.mode {
        *selected == card_idx + app.agent_scroll_offset
    } else {
        false
    };
    let card_bg = if is_selected {
        bg_card()
    } else {
        bg_secondary()
    };
    let top_color = if is_selected { accent() } else { sc };
    let side_color = border();

    let duration_s = app
        .agent_start_times
        .get(&agent.id)
        .map(|t| t.elapsed().as_secs() as u32)
        .or_else(|| agent.detail.as_ref().map(|d| d.duration_s))
        .unwrap_or(0);
    let dur_str = if duration_s > 0 {
        format!(" {}", format_duration(duration_s))
    } else {
        String::new()
    };

    // slot+0: top border ┌─icon name ──── [Status] dur ─┐
    {
        // ┌─(2) + icon(1) + space(1) + name + ─*fill + label + dur_str + ─┐(2) = w
        let fixed = 6 + label.len() + dur_str.len();
        let name_max = (w as usize).saturating_sub(fixed + 1);
        let name_trunc = truncate_str(&agent.name, name_max);
        let fill_w = (w as usize)
            .saturating_sub(fixed + name_trunc.chars().count())
            .max(1);
        let top = format!(
            "\u{250C}\u{2500}{} {}{}{}{}\u{2500}\u{2510}",
            icon,
            name_trunc,
            "\u{2500}".repeat(fill_w),
            label,
            dur_str,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                top,
                Style::default().fg(top_color).bg(card_bg),
            )),
            Rect {
                x,
                y,
                width: w,
                height: 1,
            },
        );
    }

    let iw = w.saturating_sub(2) as usize;

    // slot+1: cwd · model + badge + rss
    {
        let cwd_short = app
            .spaces
            .iter()
            .find(|s| s.space_id == agent.space_id)
            .and_then(|s| {
                std::path::Path::new(&s.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            });
        let raw_left = match (&cwd_short, agent.model.is_empty()) {
            (Some(cwd), false) if !cwd.is_empty() => format!("{} \u{00B7} {}", cwd, agent.model),
            (Some(cwd), true) if !cwd.is_empty() => cwd.clone(),
            (_, false) => agent.model.clone(),
            _ => String::new(),
        };
        let badge = if is_acp { "[ACP]" } else { "" };
        let rss = metrics
            .and_then(|m| m.rss_kb)
            .map(format_rss)
            .unwrap_or_default();
        let right = match (is_acp, rss.is_empty()) {
            (true, true) => format!(" {}", badge),
            (true, false) => format!(" {} {}", badge, rss),
            (false, false) => format!(" {}", rss),
            _ => String::new(),
        };
        let left_max = iw.saturating_sub(right.len() + 1); // +1 for leading space
        let left = truncate_str(&raw_left, left_max);
        let inner_content = format!(
            " {}{}{} ",
            left,
            " ".repeat(iw.saturating_sub(1 + left.chars().count() + right.len() + 1)),
            right.trim_start()
        );
        let inner_padded = format!("{:<iw$}", inner_content, iw = iw);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
                Span::styled(
                    inner_padded,
                    Style::default().fg(fg_secondary()).bg(card_bg),
                ),
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
            ])),
            Rect {
                x,
                y: y + 1,
                width: w,
                height: 1,
            },
        );
    }

    // slot+2: task/block_msg (or current ACP tool call when available)
    {
        let acp_tool = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .and_then(|a| a.current_tool.as_ref())
            .filter(|_| {
                matches!(
                    agent.status,
                    AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
                )
            });
        let task_str: std::borrow::Cow<str> = if let Some(ct) = acp_tool {
            std::borrow::Cow::Owned(format!("\u{25B6} {}({})", ct.tool, ct.args_summary))
        } else {
            let s = match agent.status {
                AgentStatus::Blocked => agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.block_msg.as_deref())
                    .unwrap_or(""),
                AgentStatus::Working => metrics
                    .and_then(|m| m.recent_lines.first().map(String::as_str))
                    .or_else(|| agent.detail.as_ref().and_then(|d| d.task.as_deref()))
                    .unwrap_or(""),
                _ => agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.task.as_deref())
                    .unwrap_or(""),
            };
            std::borrow::Cow::Borrowed(s)
        };
        let task = truncate_str(&task_str, iw.saturating_sub(2));
        let inner_padded = format!(" {:<iw$}", task, iw = iw.saturating_sub(1));
        let (task_fg, task_mod) = if acp_tool.is_some() {
            (sc, Modifier::empty())
        } else {
            match agent.status {
                AgentStatus::Blocked => (accent_blocked(), Modifier::BOLD),
                AgentStatus::Error => (accent_error(), Modifier::empty()),
                _ => (fg_secondary(), Modifier::empty()),
            }
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
                Span::styled(
                    inner_padded,
                    Style::default()
                        .fg(task_fg)
                        .bg(card_bg)
                        .add_modifier(task_mod),
                ),
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
            ])),
            Rect {
                x,
                y: y + 2,
                width: w,
                height: 1,
            },
        );
    }

    // slot+3: progress bar
    {
        let show_bar = matches!(
            agent.status,
            AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
        );
        let progress = agent.detail.as_ref().and_then(|d| d.progress);
        let inner_padded = if show_bar {
            // " " + bar(bar_w) + "  " + suffix(4) + " " = bar_w + 8 = iw → bar_w = iw - 8
            let bar_w = iw.saturating_sub(8).max(1);
            let (bar, suffix) = if let Some(pct) = progress {
                let pct = pct.clamp(0.0, 1.0);
                let filled = (pct * bar_w as f32) as usize;
                let b = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
                (b, format!("{:3.0}%", pct * 100.0))
            } else {
                let window = 4usize;
                let cycle = (bar_w + window + 2) as u64;
                let pos = ((app.tick_count / 5) % cycle) as usize;
                let b: String = (0..bar_w)
                    .map(|c| {
                        if c >= pos && c < pos + window {
                            "\u{2588}"
                        } else {
                            "\u{2591}"
                        }
                    })
                    .collect();
                let sfx = metrics
                    .and_then(|m| m.cpu_percent)
                    .map(|c| format!("{:3.0}%", c))
                    .unwrap_or_else(|| "    ".to_string());
                (b, sfx)
            };
            // Pad to exactly iw
            let raw = format!(" {}  {} ", bar, suffix);
            format!("{:<iw$}", raw, iw = iw)
        } else {
            " ".repeat(iw)
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
                Span::styled(inner_padded, Style::default().fg(sc).bg(card_bg)),
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
            ])),
            Rect {
                x,
                y: y + 3,
                width: w,
                height: 1,
            },
        );
    }

    // slot+4: buttons [View]  [Stop]              [Chat]
    {
        let buttons = card_buttons(&agent.status, true, is_acp); // wide always uses full labels
        let b0 = buttons[0].0;
        let b1 = buttons[1].0;
        let b2 = buttons[2].0;
        let b2_danger = buttons[2].1;
        // Layout: " " + b0 + "  " + b1 + fill + b2 + " "
        let fixed = 1 + b0.len() + 2 + b1.len() + b2.len() + 1;
        let fill_w = iw.saturating_sub(fixed);

        let h0 = app.agent_hovered == Some(AgentHover::CardBtn { card_idx, slot: 0 });
        let h1 = app.agent_hovered == Some(AgentHover::CardBtn { card_idx, slot: 1 });
        let h2 = app.agent_hovered == Some(AgentHover::CardBtn { card_idx, slot: 2 });

        let (f0, bg0) = if h0 {
            (bg_primary(), accent_hover())
        } else {
            (fg_muted(), card_bg)
        };
        let (f1, bg1) = if h1 {
            (bg_primary(), accent_hover())
        } else {
            (fg_muted(), card_bg)
        };
        let (f2, bg2) = if h2 {
            (
                bg_primary(),
                if b2_danger {
                    accent_error()
                } else {
                    accent_hover()
                },
            )
        } else if b2_danger {
            (accent_error(), card_bg)
        } else {
            (fg_muted(), card_bg)
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
                Span::styled(" ", Style::default().bg(card_bg)),
                Span::styled(b0, Style::default().fg(f0).bg(bg0)),
                Span::styled("  ", Style::default().bg(card_bg)),
                Span::styled(b1, Style::default().fg(f1).bg(bg1)),
                Span::styled(" ".repeat(fill_w), Style::default().bg(card_bg)),
                Span::styled(b2, Style::default().fg(f2).bg(bg2)),
                Span::styled(" ", Style::default().bg(card_bg)),
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
            ])),
            Rect {
                x,
                y: y + 4,
                width: w,
                height: 1,
            },
        );
    }

    // slot+5: bottom border
    {
        let bot_color = match agent.status {
            AgentStatus::Blocked => {
                if is_selected {
                    accent()
                } else {
                    blocked_pulse_color(app.tick_count)
                }
            }
            _ => side_color,
        };
        let bottom = format!(
            "\u{2514}{}\u{2518}",
            "\u{2500}".repeat(w.saturating_sub(2) as usize)
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                bottom,
                Style::default().fg(bot_color).bg(card_bg),
            )),
            Rect {
                x,
                y: y + 5,
                width: w,
                height: 1,
            },
        );
    }

    // slot+6: blank gap row
    frame.render_widget(
        Paragraph::new(Span::styled(
            " ".repeat(w as usize),
            Style::default().bg(bg_secondary()),
        )),
        Rect {
            x,
            y: y + 6,
            width: w,
            height: 1,
        },
    );

    7 // slot height: top border + 4 content + bottom border + gap
}

/// Footer: "[+] Add Agent" pinned to the last row of the agent panel.
fn render_footer(frame: &mut Frame, ix: u16, iw: u16, area: Rect, app: &App) {
    let footer_y = area.y + area.height.saturating_sub(1);
    let (fg, bg) = if app.agent_hovered == Some(AgentHover::PanelFooter) {
        (bg_primary(), accent_hover())
    } else {
        (fg_muted(), bg_secondary())
    };
    let label = format!("{:<width$}", " [+] Add Agent", width = iw as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(label, Style::default().fg(fg).bg(bg))),
        Rect {
            x: ix,
            y: footer_y,
            width: iw,
            height: 1,
        },
    );
}

/// Returns the row (absolute) where agent card slot `card_idx` starts, given panel geometry.
/// For narrow cards, this is the separator row (slot row 0). Button row = this + 5.
/// `panel_y`: top row of the agent panel.
/// `scroll_offset`: number of agents scrolled past (adds 1 row for "N above" indicator).
/// `any_blocked`: whether the eclipse banner is showing (adds 2 rows).
pub fn card_start_row(
    panel_y: u16,
    scroll_offset: usize,
    any_blocked: bool,
    card_idx: usize,
) -> u16 {
    let above_row = if scroll_offset > 0 { 1u16 } else { 0 };
    let blocked_rows = if any_blocked { 2u16 } else { 0 };
    panel_y + 2 + above_row + blocked_rows + card_idx as u16 * 6
}

/// Returns the row height of one card slot: 6 for narrow cards (iw < 30), 7 for wide.
pub fn card_slot_height(iw: u16) -> u16 {
    if iw < 30 {
        6
    } else {
        7
    }
}

/// Render the Agent Fleet panel as a floating modal centered over `screen`.
/// Width: 36 cols (fits a wide card), height: up to 80% of screen or 32 rows.
pub fn render_modal(frame: &mut Frame, screen: Rect, app: &App) {
    let modal_w: u16 = 36.min(screen.width.saturating_sub(4));
    let n = app.agents.len().max(1);
    // Wide cards are 7 rows each; add 4 for header + footer.
    let content_h = (n as u16 * 7 + 4).min((screen.height * 4 / 5).max(10));
    let modal_h = content_h.min(screen.height.saturating_sub(4));

    let x = screen.x + screen.width.saturating_sub(modal_w) / 2;
    let y = screen.y + screen.height.saturating_sub(modal_h) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    frame.render_widget(Clear, area);

    let title = format!(" Agent Fleet ({}) ", app.agents.len());
    let mode_hint = " [a] close  [Tab] sidebar ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(title, Style::default().fg(accent())))
        .title_bottom(Span::styled(mode_hint, Style::default().fg(fg_muted())));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    render(frame, inner, app);
}

/// Mobile agents header row: " AGENTS (N)        [+ New]"
/// Call this before passing a sub-area (without the header row) to render().
pub fn render_mobile_agents_header(frame: &mut Frame, area: Rect, app: &App) {
    let w = area.width;
    let n = app.agents.len();
    let any_blocked = app.agents.iter().any(|a| a.status == AgentStatus::Blocked);

    let count_label = format!("({})", n);
    let count_color = if any_blocked {
        blocked_pulse_color(app.tick_count)
    } else {
        fg_muted()
    };

    // " AGENTS "(8) + count_label + fill + "[+ New]"(7) = w
    let new_btn = "[+ New]";
    let new_btn_len = new_btn.len() as u16;
    let fill = w.saturating_sub(8 + count_label.len() as u16 + new_btn_len) as usize;

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " AGENTS ",
                Style::default()
                    .fg(fg_primary())
                    .bg(bg_tertiary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                count_label,
                Style::default().fg(count_color).bg(bg_tertiary()),
            ),
            Span::styled(" ".repeat(fill), Style::default().bg(bg_tertiary())),
            Span::styled(new_btn, Style::default().fg(fg_muted()).bg(bg_tertiary())),
        ])),
        Rect {
            x: area.x,
            y: area.y,
            width: w,
            height: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::card_buttons;
    use orbt_protocol::AgentStatus;

    #[test]
    fn card_buttons_acp_overrides_slot2() {
        // ACP Working: slot 2 must be [Detail], not [Chat].
        let btns = card_buttons(&AgentStatus::Working, true, true);
        assert_eq!(btns[2].0, "[Detail]");
        assert!(!btns[2].1, "Detail should not be a danger button");

        // Heuristic Working: slot 2 stays [Chat].
        let btns = card_buttons(&AgentStatus::Working, true, false);
        assert_eq!(btns[2].0, "[Chat]");

        // ACP Idle: slot 2 is [Detail], not [Remove].
        let btns = card_buttons(&AgentStatus::Idle, true, true);
        assert_eq!(btns[2].0, "[Detail]");

        // ACP compact Blocked: slot 2 is [Detail], not [Abrt].
        let btns = card_buttons(&AgentStatus::Blocked, false, true);
        assert_eq!(btns[2].0, "[Detail]");
        assert!(!btns[2].1);

        // Heuristic Blocked compact: slot 2 is [Abrt] and is danger.
        let btns = card_buttons(&AgentStatus::Blocked, false, false);
        assert_eq!(btns[2].0, "[Abrt]");
        assert!(btns[2].1);
    }
}
