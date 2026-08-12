use orbt_protocol::{AgentInfo, AgentMetrics, AgentProtocol, AgentStatus};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{AgentHover, AgentMonitorMode, App, InputMode};
use crate::tui::theme::*;

pub fn status_icon(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "\u{25CF}", // ●
        AgentStatus::Idle => "\u{25CC}",    // ◌
        AgentStatus::Blocked => "\u{25CE}", // ◎
        AgentStatus::Error => "\u{25C9}",   // ◉
        AgentStatus::Done => "\u{2713}",    // ✓
    }
}

fn status_color(status: &AgentStatus) -> ratatui::style::Color {
    match status {
        AgentStatus::Working => accent(),
        AgentStatus::Idle => accent_idle(),
        AgentStatus::Blocked => accent_blocked(),
        AgentStatus::Error => accent_error(),
        AgentStatus::Done => fg_muted(),
    }
}

/// Returns the context bar fill color based on context_percent threshold.
/// < 0.70 → accent_idle (cyan), 0.70–0.90 → accent_blocked (amber), > 0.90 → accent_error (red).
fn context_bar_color(p: f32) -> ratatui::style::Color {
    if p < 0.70 {
        accent_idle()
    } else if p <= 0.90 {
        accent_blocked()
    } else {
        accent_error()
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

/// Status label for inline card display (design doc §3.3).
pub fn status_label(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "Working",
        AgentStatus::Idle => "Idle",
        AgentStatus::Blocked => "Blocked",
        AgentStatus::Error => "Error",
        AgentStatus::Done => "Done",
    }
}

// Returns ([btn_label, is_danger]; 3 slots).
pub fn card_buttons(status: &AgentStatus) -> [(&'static str, bool); 3] {
    match status {
        AgentStatus::Working => [
            ("[Focus]", false),
            ("[Interrupt]", false),
            ("[Stop]", false),
        ],
        AgentStatus::Idle => [("[Focus]", false), ("[Prompt]", false), ("[Stop]", false)],
        AgentStatus::Blocked => [("[Focus]", false), ("[Respond]", false), ("[Abort]", true)],
        AgentStatus::Error => [
            ("[Focus]", false),
            ("[Restart]", false),
            ("[Dismiss]", false),
        ],
        AgentStatus::Done => [
            ("[Focus]", false),
            ("[Restart]", false),
            ("[Dismiss]", false),
        ],
    }
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

/// Map a value to one of 8 sparkline block chars (▁–█).
/// When max is 0 (no data), returns the lowest bar.
fn sparkline_char(value: u32, max: u32) -> char {
    if max == 0 {
        return '\u{2581}'; // ▁
    }
    let chars = [
        '\u{2581}', // ▁
        '\u{2582}', // ▂
        '\u{2583}', // ▃
        '\u{2584}', // ▄
        '\u{2585}', // ▅
        '\u{2586}', // ▆
        '\u{2587}', // ▇
        '\u{2588}', // █
    ];
    let idx = ((value as usize * (chars.len() - 1)) / max as usize).min(chars.len() - 1);
    chars[idx]
}

/// Format a token count for compact display.
/// Returns "—" for 0, "340k" for thousands, "1.2M" for millions.
fn format_tokens(n: u64) -> String {
    if n == 0 {
        return "\u{2014}".to_string(); // —
    }
    if n < 1_000 {
        format!("{n}")
    } else if n < 1_000_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Sidebar header button geometry, relative to the inner x (after the left
/// border). Shared by the renderer and the mouse hit-testing in events.rs.
pub struct SidebarHeaderBtns {
    /// "[Modal]" — switches to the Modal form.
    pub switch: (u16, u16),
    /// "[+]" when unfocused; density hint text when focused (not clickable).
    pub middle: (u16, u16),
    /// "×" close.
    pub close: u16,
}

pub fn sidebar_header_btns(iw: u16, focused: bool) -> SidebarHeaderBtns {
    let middle_w: u16 = if focused {
        if iw >= 30 {
            9 // "m Compact"
        } else {
            1 // "m"
        }
    } else {
        3 // "[+]"
    };
    // Right-aligned cluster: "[Modal]"(7) " "(1) middle " "(1) "×"(1).
    let close = iw.saturating_sub(1);
    let middle_end = iw.saturating_sub(2);
    let middle = (middle_end.saturating_sub(middle_w), middle_end);
    let switch = (middle.0.saturating_sub(8), middle.0.saturating_sub(1));
    SidebarHeaderBtns {
        switch,
        middle,
        close,
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
        let (close_fg, close_bg) = if app.agent_hovered == Some(AgentHover::HeaderClose) {
            (bg_primary(), accent_error())
        } else {
            (fg_muted(), bg_secondary())
        };
        let (switch_fg, switch_bg) = if app.agent_hovered == Some(AgentHover::HeaderSwitch) {
            (bg_primary(), accent_hover())
        } else {
            (fg_muted(), bg_secondary())
        };

        let is_agent_panel = matches!(app.mode, InputMode::AgentPanel { .. });
        let btns = sidebar_header_btns(iw, is_agent_panel);
        let middle_text = if is_agent_panel {
            match app.agent_monitor_mode {
                AgentMonitorMode::Card if iw >= 30 => "m Compact",
                AgentMonitorMode::Compact if iw >= 30 => "m Card",
                _ => "m",
            }
        } else {
            "[+]"
        };
        let left_used: usize = 6 + 1 + badge.len();
        let cluster_w = 7 + 1 + (btns.middle.1 - btns.middle.0) as usize + 1 + 1;
        let fill = (iw as usize).saturating_sub(left_used + cluster_w);
        let mut header_spans = vec![
            Span::styled(
                "AGENTS",
                Style::default()
                    .fg(fg_primary())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(badge.clone(), Style::default().fg(badge_color)),
            Span::raw(" ".repeat(fill)),
            Span::styled("[Modal]", Style::default().fg(switch_fg).bg(switch_bg)),
            Span::raw(" "),
        ];
        if is_agent_panel {
            header_spans.push(Span::styled(middle_text, Style::default().fg(fg_muted())));
        } else {
            let (add_fg, add_bg) = if app.agent_hovered == Some(AgentHover::HeaderAdd) {
                (bg_primary(), accent_hover())
            } else {
                (fg_muted(), bg_secondary())
            };
            header_spans.push(Span::styled(
                middle_text,
                Style::default().fg(add_fg).bg(add_bg),
            ));
        }
        header_spans.push(Span::raw(" "));
        header_spans.push(Span::styled(
            "\u{00D7}",
            Style::default().fg(close_fg).bg(close_bg),
        ));

        frame.render_widget(
            Paragraph::new(Line::from(header_spans)),
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

    // --- Blocked banner (single row): icon + "Blocked — name [Respond]" ---
    if !blocked_agents.is_empty() {
        let name_part = if blocked_agents.len() == 1 {
            truncate_str(&blocked_agents[0].name, 10)
        } else {
            format!("{} agents", blocked_agents.len())
        };
        let icon_color = blocked_pulse_color(app.tick_count);
        // Row: ◎ + " Blocked \u{2014} " + name + fill + [Respond]
        // "◎" = 1 char; " Blocked — " = 11 chars; "[Respond]" = 9 chars
        let respond_label = "[Respond]";
        let respond_len: usize = respond_label.len(); // 9
        let prefix = " Blocked \u{2014} "; // 11 chars: space+Blocked+space+—+space
        let (resp_fg, resp_bg) = if app.agent_hovered == Some(AgentHover::EclipseRespond) {
            (bg_primary(), accent_blocked())
        } else {
            (accent_blocked(), bg_tertiary())
        };
        // Available for name: iw - icon(1) - prefix(11) - respond(9)
        let name_max = (iw as usize).saturating_sub(1 + prefix.len() + respond_len);
        let name_trunc = truncate_str(&name_part, name_max.max(1));
        let fill = (iw as usize).saturating_sub(1 + prefix.len() + name_trunc.len() + respond_len);
        let middle = format!("{}{}{:>fill$}", prefix, name_trunc, "", fill = fill);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "\u{25CE}",
                    Style::default().fg(icon_color).bg(bg_tertiary()),
                ),
                Span::styled(
                    middle,
                    Style::default().fg(accent_blocked()).bg(bg_tertiary()),
                ),
                Span::styled(respond_label, Style::default().fg(resp_fg).bg(resp_bg)),
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
        let mode = app.effective_monitor_mode(iw);
        // Slot heights: Compact = separator(1) + 2 content rows = 3.
        // Card-wide = 7; Card-narrow = 6 (but Card is only used when iw > 30).
        let slot_h = match mode {
            AgentMonitorMode::Compact => 3u16,
            AgentMonitorMode::Card => {
                if iw >= 30 {
                    7
                } else {
                    6
                }
            }
        };
        let visible_agents: Vec<&AgentInfo> =
            app.agents.iter().skip(app.agent_scroll_offset).collect();
        let total = app.agents.len();
        // Reserve 1 row at the bottom for the footer.
        let content_bottom = area.y + area.height.saturating_sub(1);
        // For Compact mode: render column header label row once before the first card.
        if mode == AgentMonitorMode::Compact && !visible_agents.is_empty() && y < content_bottom {
            render_compact_table_header(frame, ix, y, iw);
            y += 1;
        }
        for (card_idx, agent) in visible_agents.iter().enumerate() {
            if y + slot_h > content_bottom {
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
            match mode {
                AgentMonitorMode::Compact => {
                    render_compact_row(frame, ix, y, iw, agent, card_idx, app, metrics);
                }
                AgentMonitorMode::Card => {
                    render_card(frame, ix, y, iw, agent, card_idx, app, metrics);
                }
            }
            y += slot_h;
        }
    }

    render_footer(frame, ix, iw, area, app);
    render_prompt_input(frame, area, app);
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

    // slot+4: context bar (when context_percent available) or activity/progress bar
    {
        let show_bar = matches!(
            agent.status,
            AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
        );
        let ctx_pct = agent.detail.as_ref().and_then(|d| d.context_percent);
        if let Some(p) = ctx_pct {
            // Narrow context bar: mark(1) + "ctx "(4) + bar(bar_w) + " "(1) + pct(4) = mark + bar_w + 9
            // So bar_w = w - 1(mark) - 9 = w - 10
            let bar_w = (w as usize).saturating_sub(10).max(2);
            let p_clamped = p.clamp(0.0, 1.0);
            let filled = (p_clamped * bar_w as f32) as usize;
            let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
            let bar_color = context_bar_color(p);
            let pct_str = format!("{:3.0}%", p_clamped * 100.0);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    accent_mark.clone(),
                    Span::styled("ctx ", Style::default().fg(fg_muted()).bg(card_bg)),
                    Span::styled(bar, Style::default().fg(bar_color).bg(card_bg)),
                    Span::styled(" ", Style::default().bg(card_bg)),
                    Span::styled(pct_str, Style::default().fg(fg_secondary()).bg(card_bg)),
                ])),
                Rect {
                    x,
                    y: y + 4,
                    width: w,
                    height: 1,
                },
            );
        } else if show_bar {
            let progress = agent.detail.as_ref().and_then(|d| d.progress);
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
        let buttons = card_buttons(&agent.status);
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

    // slot+0: top border ┌─icon name ─── Status  model  dur─┐
    // Layout: ┌─(2) + icon(1) + space(1) + name + ─*fill + label + model_part + dur_str + ─┐(2) = w
    {
        // Compute base fixed cost without model first, then budget model to fit.
        // This prevents overflow when dur_str + model_part exceed available width.
        let base_fixed = 6 + label.len() + dur_str.len();
        let name_max = (w as usize).saturating_sub(base_fixed + 1);
        let name_trunc = truncate_str(&agent.name, name_max);
        // model_part: only include when there is space beyond name + base_fixed.
        let model_part = if !agent.model.is_empty() {
            let budget = (w as usize).saturating_sub(base_fixed + name_trunc.chars().count() + 4);
            if budget >= 1 {
                let m = truncate_str(&agent.model, budget.min(14));
                if m.is_empty() {
                    String::new()
                } else {
                    format!("  {m}")
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        let fixed = base_fixed + model_part.chars().count();
        let fill_w = (w as usize)
            .saturating_sub(fixed + name_trunc.chars().count())
            .max(1);
        let top = format!(
            "\u{250C}\u{2500}{} {}{}{}{}{}\u{2500}\u{2510}",
            icon,
            name_trunc,
            "\u{2500}".repeat(fill_w),
            label,
            model_part,
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

    // slot+1: cwd · [ACP] rss  (model is now in slot+0 top border)
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
            })
            .unwrap_or_default();
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
        let left = truncate_str(&cwd_short, left_max);
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

    // slot+3: context bar (when context_percent available) or activity/progress bar
    {
        let ctx_pct = agent.detail.as_ref().and_then(|d| d.context_percent);
        if let Some(p) = ctx_pct {
            // Context bar: " ctx {bar} {pct%}[ C{n}][ [Compact]] "
            // Layout inside iw: 1(lead) + 4("ctx ") + bar_w + 1(sep) + 4(pct) + opt(comp) + opt(btn) + 1(trail)
            let has_acp = agent.detail.as_ref().and_then(|d| d.acp.as_ref()).is_some();
            let compaction_count = agent
                .detail
                .as_ref()
                .map(|d| d.compaction_count)
                .unwrap_or(0);
            let show_compact = p > 0.80;
            let comp_display = if has_acp {
                format!(" C{}", compaction_count)
            } else {
                String::new()
            };
            let compact_btn = if show_compact { " [Compact]" } else { "" };
            let fixed: usize = 11 + comp_display.len() + compact_btn.len();
            let bar_w = iw.saturating_sub(fixed).max(2);
            let p_clamped = p.clamp(0.0, 1.0);
            let filled = (p_clamped * bar_w as f32) as usize;
            let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
            let bar_color = context_bar_color(p);
            let pct_str = format!("{:3.0}%", p_clamped * 100.0);

            let mut spans = vec![
                Span::styled("\u{2502}", Style::default().fg(side_color).bg(card_bg)),
                Span::styled(" ctx ", Style::default().fg(fg_muted()).bg(card_bg)),
                Span::styled(bar, Style::default().fg(bar_color).bg(card_bg)),
                Span::styled(" ", Style::default().bg(card_bg)),
                Span::styled(pct_str, Style::default().fg(fg_secondary()).bg(card_bg)),
            ];
            if !comp_display.is_empty() {
                spans.push(Span::styled(
                    comp_display,
                    Style::default().fg(fg_muted()).bg(card_bg),
                ));
            }
            if show_compact {
                spans.push(Span::styled(
                    compact_btn,
                    Style::default().fg(accent_blocked()).bg(card_bg),
                ));
            }
            spans.push(Span::styled(" ", Style::default().bg(card_bg)));
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(side_color).bg(card_bg),
            ));

            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    x,
                    y: y + 3,
                    width: w,
                    height: 1,
                },
            );
        } else {
            // Fall back to activity/progress bar (unchanged)
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
    }

    // slot+4: buttons [View]  [Stop]              [Chat]
    {
        let buttons = card_buttons(&agent.status);
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
/// `any_blocked`: whether the eclipse banner is showing (adds 1 row).
pub fn card_start_row(
    panel_y: u16,
    scroll_offset: usize,
    any_blocked: bool,
    card_idx: usize,
) -> u16 {
    let above_row = if scroll_offset > 0 { 1u16 } else { 0 };
    // Blocked banner is now a single row.
    let blocked_rows = if any_blocked { 1u16 } else { 0 };
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

/// Render the compact-mode column header label row (once, before the first agent).
/// Layout: "  AI Name      St[  Ctx][ Tok   T]" padded to `iw`.
fn render_compact_table_header(frame: &mut Frame, x: u16, y: u16, w: u16) {
    let show_tok = w >= 26;
    let show_ctx = w >= 20;
    // Base: mark(2) + badge(3) + name(10) + st(2) = 17 chars
    let mut header = format!("  {:<3}{:<10}{:<2}", "AI", "Name", "St");
    if show_ctx {
        header.push_str(&format!("{:>4}", "Ctx"));
    }
    if show_tok {
        // space(1) + tok(6) + space(1) + turns(3) = 11 chars
        header.push_str(&format!(" {:>6} {:>3}", "Tok", "T"));
    }
    let padded = format!("{:<width$}", header, width = w as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(padded, Style::default().fg(fg_muted()))),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
}

/// Compact Row mode: render one agent as 3 rows (separator + 2 content rows).
/// Slot layout:
///   slot+0: ─ separator rule
///   slot+1: [mark] [badge] [name] [icon] [ctx%] [tok] [turns]
///   slot+2: [indent]└─ [tool/status text or [Respond]]
fn render_compact_row(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    agent: &AgentInfo,
    card_idx: usize,
    app: &App,
    _metrics: Option<&AgentMetrics>,
) {
    let sc = animated_status_color(&agent.status, app.tick_count);
    let icon = status_icon(&agent.status);

    let is_selected = if let InputMode::AgentPanel { selected } = &app.mode {
        *selected == card_idx + app.agent_scroll_offset
    } else {
        false
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

    // slot+1: status/name/stats row
    {
        let show_tok = w >= 26;
        let show_ctx = w >= 20;

        let sel_mark = if is_selected {
            Span::styled("\u{25BA} ", Style::default().fg(accent()))
        } else {
            Span::styled("  ", Style::default().fg(fg_muted()))
        };

        // Agent CLI badge: 2-char uppercase abbreviation from agent_cli field.
        let badge = if let Some(d) = &agent.detail {
            if !d.agent_cli.is_empty() {
                let cli = d.agent_cli.to_uppercase();
                let chars: Vec<char> = cli.chars().take(2).collect();
                format!(
                    "{}{}",
                    chars.first().unwrap_or(&'?'),
                    chars.get(1).unwrap_or(&' ')
                )
            } else {
                "? ".to_string()
            }
        } else {
            "? ".to_string()
        };
        let badge_text = format!("{:<3}", badge);

        let name_fg = if is_selected {
            fg_primary()
        } else {
            fg_secondary()
        };
        let name_trunc = truncate_str(&agent.name, 10);
        let name_padded = format!("{:<10}", name_trunc);
        let icon_str = format!("{} ", icon); // icon(1) + space(1) = 2 chars

        let mut spans = vec![
            sel_mark,
            Span::styled(badge_text, Style::default().fg(fg_muted())),
            Span::styled(name_padded, Style::default().fg(name_fg)),
            Span::styled(icon_str, Style::default().fg(sc)),
        ];

        if show_ctx {
            // Use context_percent field (not the generic progress field).
            let ctx_pct = agent.detail.as_ref().and_then(|d| d.context_percent);
            let (ctx_str, ctx_fg) = if let Some(pct) = ctx_pct {
                let pct_val = (pct.clamp(0.0, 1.0) * 100.0) as u32;
                // 4-char right-aligned: "100%" or " 82%" or "  5%"
                let s = format!("{:3}%", pct_val);
                let color = context_bar_color(pct);
                (s, color)
            } else {
                (format!("{:>4}", "\u{2014}"), fg_muted())
            };
            spans.push(Span::styled(ctx_str, Style::default().fg(ctx_fg)));
        }

        if show_tok {
            let tok_str = agent
                .detail
                .as_ref()
                .and_then(|d| d.acp.as_ref())
                .map(|a| {
                    let total = (a.tokens_in as u64) + (a.tokens_out as u64);
                    format!("{:>6}", format_tokens(total))
                })
                .unwrap_or_else(|| format!("{:>6}", "\u{2014}"));
            // Turn count from AcpDetail (ACP agents only; show — for heuristic agents).
            let turns_str = agent
                .detail
                .as_ref()
                .and_then(|d| d.acp.as_ref())
                .map(|a| {
                    if a.turn_count > 0 {
                        format!("{}", a.turn_count.min(999))
                    } else {
                        "\u{2014}".to_string()
                    }
                })
                .unwrap_or_else(|| "\u{2014}".to_string());
            let turns_padded = format!("{:>3}", turns_str);
            spans.push(Span::raw(" "));
            spans.push(Span::styled(tok_str, Style::default().fg(fg_muted())));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(turns_padded, Style::default().fg(fg_muted())));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x,
                y: y + 1,
                width: w,
                height: 1,
            },
        );
    }

    // slot+2: tool/status detail row
    // Layout: "     └─ " (8 chars indent) + content + optional right-aligned [Respond]
    {
        let indent = "     \u{2514}\u{2500} "; // "     └─ "
        let indent_len: usize = 8;
        let available = (w as usize).saturating_sub(indent_len);

        let is_blocked = agent.status == AgentStatus::Blocked;
        let respond_label = "[Respond]";
        let respond_len: usize = respond_label.len(); // 9

        // Derive detail text from current tool or status.
        let current_tool_text = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .and_then(|a| a.current_tool.as_ref())
            .filter(|_| {
                matches!(
                    agent.status,
                    AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Error
                )
            })
            .map(|ct| format!("{} {}", ct.tool, ct.args_summary));

        let (detail_text, detail_fg): (String, ratatui::style::Color) = match &agent.status {
            AgentStatus::Blocked => {
                // Blocked: show empty detail — [Respond] is shown separately.
                (String::new(), fg_muted())
            }
            AgentStatus::Working => {
                let text = current_tool_text
                    .or_else(|| {
                        agent
                            .detail
                            .as_ref()
                            .and_then(|d| d.task.as_deref().map(str::to_string))
                    })
                    .unwrap_or_default();
                (text, fg_muted())
            }
            AgentStatus::Idle => ("idle".to_string(), fg_muted()),
            AgentStatus::Done => ("done".to_string(), fg_muted()),
            AgentStatus::Error => (
                current_tool_text.unwrap_or_else(|| "error".to_string()),
                accent_error(),
            ),
        };

        let text_max = if is_blocked {
            available.saturating_sub(respond_len)
        } else {
            available
        };
        let detail_trunc = truncate_str(&detail_text, text_max);

        let mut spans = vec![Span::styled(indent, Style::default().fg(fg_muted()))];

        if is_blocked {
            // Right-align [Respond] with fill between indent and it.
            let fill = available.saturating_sub(detail_trunc.len() + respond_len);
            if !detail_trunc.is_empty() {
                spans.push(Span::styled(detail_trunc, Style::default().fg(detail_fg)));
            }
            spans.push(Span::raw(" ".repeat(fill)));
            spans.push(Span::styled(
                respond_label,
                Style::default().fg(accent_blocked()),
            ));
        } else {
            let text_padded = format!("{:<width$}", detail_trunc, width = available);
            spans.push(Span::styled(text_padded, Style::default().fg(detail_fg)));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x,
                y: y + 2,
                width: w,
                height: 1,
            },
        );
    }
}

// ─── Full Screen modal ────────────────────────────────────────────────────────

/// Map tool name to its display color token.
pub fn tool_name_color(name: &str) -> ratatui::style::Color {
    match name {
        "Edit" | "Write" => accent_idle(),
        "Read" | "Glob" | "Grep" => fg_muted(),
        "Bash" => accent_hover(),
        "Agent" => fg_primary(),
        "Think" => accent_blocked(),
        _ => fg_secondary(),
    }
}

/// Compute how many filled bar chars to show given duration, max, and bar width.
pub fn compute_bar_filled(dur_ms: u64, max_dur: u64, bar_max: usize) -> usize {
    if max_dur == 0 || bar_max == 0 {
        return 0;
    }
    ((dur_ms * bar_max as u64) / max_dur).min(bar_max as u64) as usize
}

/// Render the Full Screen Agent Fleet modal (85% of terminal, floating).
/// Render a transient toast notification anchored to the lower-right corner of `area`.
/// Does nothing when `app.toast` is None or the area is too small.
pub fn render_toast(frame: &mut Frame, area: Rect, app: &App) {
    let Some(toast) = &app.toast else {
        return;
    };
    if area.height < 2 {
        return;
    }
    let toast_y = area.y + area.height - 1;
    let msg = format!("  {}  ", toast.message);
    let w = (msg.len() as u16).min(area.width);
    let x = area.x + area.width.saturating_sub(w);
    frame.render_widget(
        Paragraph::new(msg).style(Style::default().bg(bg_tertiary()).fg(fg_primary())),
        Rect {
            x,
            y: toast_y,
            width: w,
            height: 1,
        },
    );
}

/// Render the inline prompt input bar anchored to the bottom of `area`.
/// Does nothing when the mode is not PromptInput or the area is too small.
pub fn render_prompt_input(frame: &mut Frame, area: Rect, app: &App) {
    let InputMode::PromptInput { agent_id, input } = &app.mode else {
        return;
    };
    let agent_name = app
        .agents
        .iter()
        .find(|a| &a.id == agent_id)
        .map(|a| a.name.as_str())
        .unwrap_or("agent");

    if area.height < 3 {
        return;
    }

    let box_h = 3u16;
    let box_y = area.y + area.height.saturating_sub(box_h);
    let box_area = Rect {
        x: area.x,
        y: box_y,
        width: area.width,
        height: box_h,
    };

    frame.render_widget(Clear, box_area);

    let title = format!(" Prompt: {agent_name} ");
    let hint = " Enter: send  Esc: cancel ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent_blocked()))
        .title(Span::styled(title, Style::default().fg(accent_blocked())))
        .title_bottom(Span::styled(hint, Style::default().fg(fg_muted())));
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    // Blinking cursor: '|' for even half-second, ' ' for odd.
    let cursor_char = if (app.tick_count / 30) % 2 == 0 {
        '|'
    } else {
        ' '
    };
    let display = format!("> {}{}", input, cursor_char);
    frame.render_widget(
        Paragraph::new(display).style(Style::default().fg(fg_primary())),
        inner,
    );
}

/// Geometry of the Modal (full-screen) Agent Fleet form. Shared by the
/// renderer and the mouse hit-testing in events.rs — do not duplicate the math.
#[derive(Debug, Clone, Copy)]
pub struct FsModalLayout {
    /// Whole modal including the border.
    pub area: Rect,
    /// Header row inside the border (title + [+] [Sidebar] [×]).
    pub header_y: u16,
    /// Full-width blocked banner row, when any agent is blocked.
    pub banner_y: Option<u16>,
    /// Left agent-list region.
    pub left: Rect,
    /// Right detail region.
    pub right: Rect,
    /// Footer hint row inside the border.
    pub footer_hint_y: u16,
    /// Header button column ranges [start, end): [+], [Sidebar], [×].
    pub btn_add: (u16, u16),
    pub btn_switch: (u16, u16),
    pub btn_close: (u16, u16),
}

pub fn fs_modal_layout(screen: Rect, any_blocked: bool) -> FsModalLayout {
    let modal_w = (screen.width * 85 / 100).clamp(60, 168).min(screen.width);
    let modal_h = (screen.height * 85 / 100).clamp(20, 48).min(screen.height);
    let area = Rect {
        x: screen.x + screen.width.saturating_sub(modal_w) / 2,
        y: screen.y + screen.height.saturating_sub(modal_h) / 2,
        width: modal_w,
        height: modal_h,
    };
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: modal_w.saturating_sub(2),
        height: modal_h.saturating_sub(2),
    };
    let header_y = inner.y;
    let body_top = if any_blocked {
        inner.y + 4
    } else {
        inner.y + 2
    };
    let footer_hint_y = inner.y + inner.height.saturating_sub(1);
    let body_bottom = footer_hint_y.saturating_sub(1); // footer divider row
    let body_h = body_bottom.saturating_sub(body_top);

    let left_w = 46u16.min(inner.width.saturating_sub(21));
    let left = Rect {
        x: inner.x,
        y: body_top,
        width: left_w,
        height: body_h,
    };
    let right = Rect {
        x: inner.x + left_w + 1,
        y: body_top,
        width: inner.width.saturating_sub(left_w + 1),
        height: body_h,
    };

    // Right-aligned "[+]  [Sidebar]  [×]" on the header row (3+2+9+2+3 = 19 cols).
    let end = inner.x + inner.width;
    let btn_close = (end.saturating_sub(3), end);
    let btn_switch = (end.saturating_sub(14), end.saturating_sub(5));
    let btn_add = (end.saturating_sub(19), end.saturating_sub(16));

    FsModalLayout {
        area,
        header_y,
        banner_y: if any_blocked { Some(inner.y + 2) } else { None },
        left,
        right,
        footer_hint_y,
        btn_add,
        btn_switch,
        btn_close,
    }
}

pub fn render_fullscreen_modal(frame: &mut Frame, screen: Rect, app: &App) {
    if screen.width < 80 || screen.height < 24 {
        let msg = "Terminal too small for Agent Modal (need 80x24)";
        let w = (msg.len() as u16 + 4).min(screen.width);
        let area = Rect {
            x: screen.x + screen.width.saturating_sub(w) / 2,
            y: screen.y + screen.height / 2,
            width: w,
            height: 3,
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(msg).block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    }

    let any_blocked = app.agents.iter().any(|a| a.status == AgentStatus::Blocked);
    let layout = fs_modal_layout(screen, any_blocked);
    let area = layout.area;

    frame.render_widget(Clear, area);

    let border_color = if any_blocked {
        accent_blocked()
    } else {
        accent()
    };
    let block = Block::default()
        .style(Style::default().bg(bg_primary()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(block, area);

    // ── Header row: title left, [+] [Sidebar] [×] right ──
    {
        let n = app.agents.len();
        let working = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Working)
            .count();
        let blocked_n = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Blocked)
            .count();
        let mut counts = String::new();
        if working > 0 {
            counts.push_str(&format!("\u{25CF}{working} "));
        }
        if blocked_n > 0 {
            counts.push_str(&format!("\u{25CE}{blocked_n} "));
        }
        let title = format!(" Agent Fleet ({n})");
        // Header row is a bg_secondary title-bar strip; buttons sit flat on it.
        let strip = Style::default().bg(bg_secondary());
        let mut spans = vec![
            Span::styled(title, strip.fg(fg_primary()).add_modifier(Modifier::BOLD)),
            Span::styled(" ", strip),
            Span::styled(counts, strip.fg(fg_muted())),
        ];
        // Display width, not bytes: ●/◎ are 3-byte chars with width 1, and a
        // byte-counted fill pushes the buttons left of their hit ranges.
        let used: usize = spans.iter().map(|s| s.width()).sum();
        let iw = area.width.saturating_sub(2) as usize;
        // "[+]" + 2sp + "[Sidebar]" + 2sp + "[×]" = 19 cols, flush right.
        // fs_modal_layout's btn_* ranges mirror this math exactly.
        let fill = iw.saturating_sub(used + 19);
        spans.push(Span::styled(" ".repeat(fill), strip));
        let buttons: [(AgentHover, &str, bool); 3] = [
            (AgentHover::HeaderAdd, "[+]", false),
            (AgentHover::HeaderSwitch, "[Sidebar]", false),
            (AgentHover::HeaderClose, "[\u{00D7}]", true),
        ];
        for (i, (hover, label, danger)) in buttons.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", strip));
            }
            let (fg, bg) = if app.agent_hovered.as_ref() == Some(hover) {
                (
                    bg_primary(),
                    if *danger {
                        accent_error()
                    } else {
                        accent_hover()
                    },
                )
            } else if *danger {
                (accent_error(), bg_secondary())
            } else {
                (fg_muted(), bg_secondary())
            };
            spans.push(Span::styled(*label, Style::default().fg(fg).bg(bg)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: area.x + 1,
                y: layout.header_y,
                width: area.width.saturating_sub(2),
                height: 1,
            },
        );
    }

    let iw = area.width.saturating_sub(2);
    let divider = |y: u16, frame: &mut Frame| {
        frame.render_widget(
            Span::styled(
                "\u{2500}".repeat(iw as usize),
                Style::default().fg(border()),
            ),
            Rect {
                x: area.x + 1,
                y,
                width: iw,
                height: 1,
            },
        );
    };
    divider(layout.header_y + 1, frame);

    // ── Full-width blocked banner ──
    if let Some(banner_y) = layout.banner_y {
        let blocked: Vec<&AgentInfo> = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Blocked)
            .collect();
        let name_part = if blocked.len() == 1 {
            let reason = blocked[0]
                .detail
                .as_ref()
                .and_then(|d| d.block_msg.as_deref())
                .unwrap_or("needs input");
            format!(
                "{} \u{00B7} {}",
                truncate_str(&blocked[0].name, 12),
                truncate_str(reason, 24)
            )
        } else {
            format!("{} agents need input", blocked.len())
        };
        let pulse = blocked_pulse_color(app.tick_count);
        let respond = "[Respond]";
        let (resp_fg, resp_bg) = if app.agent_hovered == Some(AgentHover::EclipseRespond) {
            (bg_primary(), accent_blocked())
        } else {
            (accent_blocked(), bg_tertiary())
        };
        let mid_w = (iw as usize).saturating_sub(2 + 11 + respond.len());
        let mid = format!(" Blocked \u{2014} {:<width$}", name_part, width = mid_w);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(bg_tertiary())),
                Span::styled("\u{25CE}", Style::default().fg(pulse).bg(bg_tertiary())),
                Span::styled(mid, Style::default().fg(accent_blocked()).bg(bg_tertiary())),
                Span::styled(respond, Style::default().fg(resp_fg).bg(resp_bg)),
            ])),
            Rect {
                x: area.x + 1,
                y: banner_y,
                width: iw,
                height: 1,
            },
        );
        divider(banner_y + 1, frame);
    }

    // ── Vertical divider between columns ──
    let divider_x = layout.left.x + layout.left.width;
    for row in layout.left.y..layout.left.y + layout.left.height {
        frame.render_widget(
            Span::styled("\u{2502}", Style::default().fg(border())),
            Rect {
                x: divider_x,
                y: row,
                width: 1,
                height: 1,
            },
        );
    }

    // ── Footer hint (focus-aware) ──
    divider(layout.footer_hint_y.saturating_sub(1), frame);
    {
        let focus_right = matches!(
            app.mode,
            crate::app::InputMode::AgentFullScreen {
                focus_right: true,
                ..
            }
        );
        let hint = if focus_right {
            " j/k navigate \u{00B7} y copy \u{00B7} Enter inspect \u{00B7} f/i/r/S/d actions \u{00B7} Tab back \u{00B7} Esc close "
        } else {
            " j/k select \u{00B7} Enter details \u{00B7} n new \u{00B7} f focus pane \u{00B7} s sidebar \u{00B7} Esc close "
        };
        let strip = Style::default().bg(bg_secondary());
        let hint_trunc = truncate_str(hint, iw as usize);
        let pad = (iw as usize).saturating_sub(hint_trunc.chars().count());
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(hint_trunc.to_string(), strip.fg(fg_muted())),
                Span::styled(" ".repeat(pad), strip),
            ])),
            Rect {
                x: area.x + 1,
                y: layout.footer_hint_y,
                width: iw,
                height: 1,
            },
        );
    }

    if layout.left.height < 4 || layout.right.width < 21 {
        return;
    }
    render_fullscreen_left(frame, layout.left, app);
    render_fullscreen_right(frame, layout.right, app);
    render_prompt_input(frame, layout.right, app);
}

/// Left panel: section title + agent table with header and per-agent rows.
fn render_fullscreen_left(frame: &mut Frame, area: Rect, app: &App) {
    let (left_selected, focus_right) = match app.mode {
        crate::app::InputMode::AgentFullScreen {
            left_selected,
            focus_right,
            ..
        } => (left_selected, focus_right),
        _ => (0, false),
    };
    let panel_active = !focus_right;
    let w = area.width;
    let mut y = area.y;

    // Section title doubles as the focus indicator.
    if y < area.y + area.height {
        let title_color = if panel_active { accent() } else { fg_muted() };
        frame.render_widget(
            Paragraph::new(Span::styled(
                "AGENTS",
                Style::default()
                    .fg(title_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }

    // Column header.
    if y < area.y + area.height {
        render_fullscreen_table_header(frame, area.x, y, w);
        y += 1;
    }
    // Separator.
    if y < area.y + area.height {
        frame.render_widget(
            Span::styled("\u{2500}".repeat(w as usize), Style::default().fg(border())),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }

    // Selection-follow scrolling: keep the selected agent inside the window.
    // Each agent occupies 3 rows (main + detail + separator).
    let visible_agents = ((area.y + area.height).saturating_sub(y) / 3) as usize;
    let scroll = if visible_agents == 0 {
        app.fs_left_scroll
    } else if left_selected < app.fs_left_scroll {
        left_selected
    } else if left_selected >= app.fs_left_scroll + visible_agents {
        left_selected + 1 - visible_agents
    } else {
        app.fs_left_scroll
    };

    // Agent rows (2 rows each + separator).
    for (idx, agent) in app.agents.iter().enumerate().skip(scroll) {
        // Each agent: 2 content rows + 1 separator = 3.
        if y + 3 > area.y + area.height {
            break;
        }
        let is_selected = idx == left_selected;
        let row_bg = if is_selected && panel_active {
            bg_card()
        } else {
            bg_primary()
        };
        let sc = animated_status_color(&agent.status, app.tick_count);

        // Row 1: mark + badge + name + status icon + model + ctx + tok + turns.
        {
            let sel_mark = if is_selected && panel_active {
                Span::styled("\u{25BA} ", Style::default().fg(accent()).bg(row_bg))
            } else {
                Span::styled("  ", Style::default().fg(fg_muted()).bg(row_bg))
            };
            let badge = if let Some(d) = &agent.detail {
                if !d.agent_cli.is_empty() {
                    let cli = d.agent_cli.to_uppercase();
                    let chars: Vec<char> = cli.chars().take(2).collect();
                    format!(
                        "{}{} ",
                        chars.first().unwrap_or(&'?'),
                        chars.get(1).unwrap_or(&' ')
                    )
                } else {
                    "?  ".to_string()
                }
            } else {
                "?  ".to_string()
            };
            let icon = status_icon(&agent.status);
            let name_trunc = truncate_str(&agent.name, 12);
            let name_padded = format!("{:<12}", name_trunc);

            let show_model = w >= 30;
            // ctx column follows model in the span order, so require model to be visible too
            let show_ctx = w >= 34;
            let show_tok = w >= 36;
            let show_turns = w >= 45;

            let mut spans = vec![
                sel_mark,
                Span::styled(badge, Style::default().fg(fg_muted()).bg(row_bg)),
                Span::styled(
                    name_padded,
                    Style::default()
                        .fg(if is_selected {
                            fg_primary()
                        } else {
                            fg_secondary()
                        })
                        .bg(row_bg),
                ),
                Span::styled(
                    format!("{:<3}", format!("{} ", icon)),
                    Style::default().fg(sc).bg(row_bg),
                ),
            ];
            if show_model {
                let model_trunc = truncate_str(&agent.model, 10);
                // Leading space matches the header's " Model" field so columns align.
                spans.push(Span::styled(
                    format!(" {:<10}", model_trunc),
                    Style::default().fg(fg_muted()).bg(row_bg),
                ));
            }
            if show_ctx {
                let ctx_pct = agent.detail.as_ref().and_then(|d| d.context_percent);
                let (ctx_str, ctx_fg) = if let Some(p) = ctx_pct {
                    (
                        format!("{:3.0}%", p.clamp(0.0, 1.0) * 100.0),
                        context_bar_color(p),
                    )
                } else {
                    (format!("{:>4}", "\u{2014}"), fg_muted())
                };
                spans.push(Span::styled(
                    format!("{:>5}", ctx_str),
                    Style::default().fg(ctx_fg).bg(row_bg),
                ));
            }
            if show_tok {
                let tok_str = agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.acp.as_ref())
                    .map(|a| format_tokens((a.tokens_in as u64) + (a.tokens_out as u64)))
                    .unwrap_or_else(|| "\u{2014}".to_string());
                spans.push(Span::styled(
                    format!("{:>7}", tok_str),
                    Style::default().fg(fg_muted()).bg(row_bg),
                ));
            }
            if show_turns {
                let turns = agent
                    .detail
                    .as_ref()
                    .and_then(|d| d.acp.as_ref())
                    .map(|a| {
                        if a.turn_count > 0 {
                            format!("{:>6}", a.turn_count.min(9999))
                        } else {
                            format!("{:>6}", "\u{2014}")
                        }
                    })
                    .unwrap_or_else(|| format!("{:>6}", "\u{2014}"));
                spans.push(Span::styled(
                    turns,
                    Style::default().fg(fg_muted()).bg(row_bg),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    x: area.x,
                    y,
                    width: w,
                    height: 1,
                },
            );
        }
        y += 1;

        // Row 2: indent + tool/status.
        if y < area.y + area.height {
            let indent = "       \u{2514}\u{2500} "; // 9 chars
            let indent_len = 9usize;
            let available = (w as usize).saturating_sub(indent_len);
            let tool_text = agent
                .detail
                .as_ref()
                .and_then(|d| d.acp.as_ref())
                .and_then(|a| a.current_tool.as_ref())
                .filter(|_| matches!(agent.status, AgentStatus::Working | AgentStatus::Blocked))
                .map(|ct| format!("{} {}", ct.tool, ct.args_summary))
                .or_else(|| match agent.status {
                    AgentStatus::Idle => Some("idle".to_string()),
                    AgentStatus::Done => Some("done".to_string()),
                    AgentStatus::Blocked => agent
                        .detail
                        .as_ref()
                        .and_then(|d| d.block_msg.as_deref().map(str::to_string)),
                    _ => agent
                        .detail
                        .as_ref()
                        .and_then(|d| d.task.as_deref().map(str::to_string)),
                })
                .unwrap_or_default();
            let detail_fg = match agent.status {
                AgentStatus::Blocked => accent_blocked(),
                AgentStatus::Error => accent_error(),
                _ => fg_muted(),
            };
            let text_trunc = truncate_str(&tool_text, available);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(indent, Style::default().fg(fg_muted()).bg(row_bg)),
                    Span::styled(
                        format!("{:<width$}", text_trunc, width = available),
                        Style::default().fg(detail_fg).bg(row_bg),
                    ),
                ])),
                Rect {
                    x: area.x,
                    y,
                    width: w,
                    height: 1,
                },
            );
        }
        y += 1;

        // Separator after each agent.
        if y < area.y + area.height {
            frame.render_widget(
                Span::styled("\u{2500}".repeat(w as usize), Style::default().fg(border())),
                Rect {
                    x: area.x,
                    y,
                    width: w,
                    height: 1,
                },
            );
        }
        y += 1;
    }

    // Empty state.
    if app.agents.is_empty() {
        let mid_y = area.y + area.height / 2;
        if mid_y < area.y + area.height {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!("{:^width$}", "No agents running", width = w as usize),
                    Style::default().fg(fg_muted()),
                )),
                Rect {
                    x: area.x,
                    y: mid_y,
                    width: w,
                    height: 1,
                },
            );
        }
    }
}

/// Render the column header for the full-screen left panel.
pub fn render_fullscreen_table_header(frame: &mut Frame, x: u16, y: u16, w: u16) {
    let show_model = w >= 30;
    // ctx column follows model in the span order, so require model to be visible too
    let show_ctx = w >= 34;
    let show_tok = w >= 36;
    let show_turns = w >= 45;
    // mark(2) + badge(3) + name(12) + st(3) = 20; Tok(7) + Turns(6) = 13
    let mut header = format!("  {:<3}{:<12} {:<2}", "AI", "Name", "St");
    if show_model {
        // Add leading space so Model doesn't merge with St field.
        header.push_str(&format!(" {:<10}", "Model"));
    }
    if show_ctx {
        header.push_str(&format!("{:>5}", "Ctx"));
    }
    if show_tok {
        header.push_str(&format!("{:>7}", "Tok"));
    }
    if show_turns {
        header.push_str(&format!(" {:>5}", "Turns"));
    }
    let padded = format!("{:<width$}", header, width = w as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(padded, Style::default().fg(fg_muted()))),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
}

/// Right panel: detail header + tool call timeline + footer.
fn render_fullscreen_right(frame: &mut Frame, area: Rect, app: &App) {
    let (left_selected, right_selected, focus_right) = match app.mode {
        crate::app::InputMode::AgentFullScreen {
            left_selected,
            right_selected,
            focus_right,
        } => (left_selected, right_selected, focus_right),
        _ => (0, 0, false),
    };

    let Some(agent) = app.agents.get(left_selected) else {
        if area.height > 2 {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(
                        "{:^width$}",
                        "Select an agent with j/k",
                        width = area.width as usize
                    ),
                    Style::default().fg(fg_muted()),
                )),
                Rect {
                    x: area.x,
                    y: area.y + area.height / 2,
                    width: area.width,
                    height: 1,
                },
            );
        }
        return;
    };

    let w = area.width;
    // Pinned bottom stack: ctx row, meta row, action button row.
    let ctx_y = area.y + area.height.saturating_sub(3);
    let meta_y = area.y + area.height.saturating_sub(2);
    let buttons_y = area.y + area.height.saturating_sub(1);
    let flex_bottom = ctx_y.min(area.y + area.height);

    let mut y = area.y;

    // ── Identity line (doubles as focus indicator) ──
    if y < flex_bottom {
        let sc = animated_status_color(&agent.status, app.tick_count);
        let icon = status_icon(&agent.status);
        let name_color = if focus_right { accent() } else { fg_primary() };

        let session_part = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .map(|a| {
                let sid: String = a.session_id.chars().take(6).collect();
                if sid.is_empty() {
                    String::new()
                } else {
                    format!(" \u{00B7} {}", sid)
                }
            })
            .unwrap_or_default();
        let cwd_part = app
            .spaces
            .iter()
            .find(|s| s.space_id == agent.space_id)
            .map(|s| {
                let home = std::env::var("HOME").unwrap_or_default();
                if !home.is_empty() && s.cwd.starts_with(&home) {
                    format!("~{}", &s.cwd[home.len()..])
                } else {
                    s.cwd.clone()
                }
            })
            .unwrap_or_default();
        let dur_s = app
            .agent_start_times
            .get(&agent.id)
            .map(|t| t.elapsed().as_secs() as u32)
            .or_else(|| agent.detail.as_ref().map(|d| d.duration_s))
            .unwrap_or(0);
        let status_part = format!("{} {}", status_label(&agent.status), format_duration(dur_s));
        let right_info = format!(
            "{}{}",
            session_part,
            if cwd_part.is_empty() {
                String::new()
            } else {
                format!(" \u{00B7} {}", truncate_str(&cwd_part, 20))
            }
        );
        let left_len = agent.name.chars().count() + 2 + right_info.chars().count();
        let status_pad = (w as usize).saturating_sub(left_len + status_part.chars().count() + 2);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    agent.name.clone(),
                    Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {} ", icon), Style::default().fg(sc)),
                Span::styled(right_info, Style::default().fg(fg_muted())),
                Span::raw(" ".repeat(status_pad)),
                Span::styled(status_part, Style::default().fg(sc)),
            ])),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }

    // Task line.
    if y < flex_bottom {
        let task_str = agent
            .detail
            .as_ref()
            .and_then(|d| d.task.as_deref())
            .unwrap_or("");
        let task_trunc = truncate_str(task_str, (w as usize).saturating_sub(10));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("    task  ", Style::default().fg(fg_muted())),
                Span::styled(task_trunc, Style::default().fg(fg_secondary())),
            ])),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }
    y += 1; // blank row before the timeline

    // ── Timeline ──
    let tools: &[orbt_protocol::ToolCall] = agent
        .detail
        .as_ref()
        .and_then(|d| d.acp.as_ref())
        .map(|a| a.recent_tools.as_slice())
        .unwrap_or(&[]);
    let current_tool = agent
        .detail
        .as_ref()
        .and_then(|d| d.acp.as_ref())
        .and_then(|a| a.current_tool.as_ref());
    let show_think = agent.status == AgentStatus::Working && current_tool.is_none();
    let total_calls = tools.len() + usize::from(current_tool.is_some()) + usize::from(show_think);

    if y < flex_bottom {
        let duration_total_s: f64 = tools
            .iter()
            .filter_map(|t| t.duration_ms)
            .map(|d| d as f64 / 1000.0)
            .sum::<f64>()
            + current_tool.and_then(|ct| ct.duration_ms).unwrap_or(0) as f64 / 1000.0;
        let running_count = usize::from(current_tool.is_some()) + usize::from(show_think);
        let header = if running_count > 0 {
            format!(
                "    Timeline ({} calls \u{00B7} {:.1}s total \u{00B7} {} running)",
                total_calls, duration_total_s, running_count
            )
        } else {
            format!(
                "    Timeline ({} calls \u{00B7} {:.1}s total)",
                total_calls, duration_total_s
            )
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate_str(&header, w as usize),
                Style::default().fg(fg_secondary()),
            )),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }

    let max_dur: u64 = tools
        .iter()
        .filter_map(|t| t.duration_ms.map(|d| d as u64))
        .max()
        .unwrap_or(0);
    let bar_max_width: usize = 10.min(w as usize / 5).max(2);

    // Selection-follow scrolling over the logical row sequence
    // (recent_tools + current_tool + think row).
    let visible_rows = flex_bottom.saturating_sub(y) as usize;
    let scroll = if visible_rows == 0 || total_calls <= visible_rows {
        0
    } else if right_selected < app.fs_right_scroll {
        right_selected
    } else if right_selected >= app.fs_right_scroll + visible_rows {
        right_selected + 1 - visible_rows
    } else {
        app.fs_right_scroll
    };

    let longest_idx = tools
        .iter()
        .enumerate()
        .max_by_key(|(_, t)| t.duration_ms.unwrap_or(0))
        .map(|(i, _)| i);

    // Empty-state guidance when there is nothing to navigate/copy.
    if total_calls == 0 && y < flex_bottom {
        let note = if agent.detail.as_ref().and_then(|d| d.acp.as_ref()).is_some() {
            "    No tool calls yet"
        } else {
            "    Tool timeline needs an ACP-connected agent"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate_str(note, w as usize),
                Style::default().fg(fg_muted()),
            )),
            Rect {
                x: area.x,
                y,
                width: w,
                height: 1,
            },
        );
        y += 1;
    }

    let mut timeline_idx: usize = 0;
    for (i, tool) in tools.iter().enumerate() {
        if y >= flex_bottom {
            break;
        }
        let logical = timeline_idx;
        timeline_idx += 1;
        if logical < scroll {
            continue;
        }
        let is_row_selected = focus_right && logical == right_selected;
        render_timeline_row(
            frame,
            area.x,
            y,
            w,
            tool,
            false,
            is_row_selected,
            longest_idx == Some(i),
            max_dur,
            bar_max_width,
            app.tick_count,
        );
        y += 1;
    }

    if let Some(ct) = current_tool {
        if y < flex_bottom {
            let logical = timeline_idx;
            timeline_idx += 1;
            if logical >= scroll {
                let is_row_selected = focus_right && logical == right_selected;
                render_timeline_row(
                    frame,
                    area.x,
                    y,
                    w,
                    ct,
                    true,
                    is_row_selected,
                    false,
                    max_dur,
                    bar_max_width,
                    app.tick_count,
                );
                y += 1;
            }
        }
    }

    if show_think && y < flex_bottom {
        let logical = timeline_idx;
        if logical >= scroll {
            let is_row_selected = focus_right && logical == right_selected;
            let think_secs = app.tick_count / 60;
            render_think_row(
                frame,
                area.x,
                y,
                w,
                is_row_selected,
                think_secs,
                bar_max_width,
            );
            y += 1;
        }
    }

    // ── Sub-agents section (flex; truncated when space runs out) ──
    {
        let subagents = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .map(|a| a.subagents.as_slice())
            .unwrap_or(&[]);
        if !subagents.is_empty() && y + 1 < flex_bottom {
            y += 1; // blank separator
            let name_w = (w as usize).saturating_sub(30).max(12);
            let sa_header = format!("    Sub-agents ({})", subagents.len());
            frame.render_widget(
                Paragraph::new(Span::styled(
                    truncate_str(&sa_header, w as usize),
                    Style::default().fg(fg_muted()),
                )),
                Rect {
                    x: area.x,
                    y,
                    width: w,
                    height: 1,
                },
            );
            y += 1;
            for sa in subagents.iter() {
                if y >= flex_bottom {
                    break;
                }
                let icon = status_icon(&sa.status);
                let icon_color = status_color(&sa.status);
                let name_trunc = truncate_str(&sa.name, name_w);
                let name_padded = format!("{:<width$}", name_trunc, width = name_w);
                let lbl = status_label(&sa.status);
                let lbl_padded = format!("{:<10}", lbl);
                let tok_str = format!("{:>6}", format_tokens(sa.tokens));
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("    ", Style::default().fg(fg_muted())),
                        Span::styled(icon, Style::default().fg(icon_color)),
                        Span::styled(" ", Style::default()),
                        Span::styled(name_padded, Style::default().fg(fg_primary())),
                        Span::styled("  ", Style::default()),
                        Span::styled(lbl_padded, Style::default().fg(fg_secondary())),
                        Span::styled("  ", Style::default()),
                        Span::styled(tok_str, Style::default().fg(fg_muted())),
                    ])),
                    Rect {
                        x: area.x,
                        y,
                        width: w,
                        height: 1,
                    },
                );
                y += 1;
            }
        }
    }

    // ── Pinned: ctx row ──
    if area.height >= 3 {
        let pct = agent.detail.as_ref().and_then(|d| d.context_percent);
        let has_acp = agent.detail.as_ref().and_then(|d| d.acp.as_ref()).is_some();
        let compaction_count = agent
            .detail
            .as_ref()
            .map(|d| d.compaction_count)
            .unwrap_or(0);
        let sparkline: String = {
            let hist = agent
                .detail
                .as_ref()
                .and_then(|d| d.acp.as_ref())
                .map(|a| a.context_history.as_slice())
                .unwrap_or(&[]);
            let max = hist.iter().copied().max().unwrap_or(1);
            let start = hist.len().saturating_sub(8);
            hist[start..]
                .iter()
                .map(|&v| sparkline_char(v, max))
                .collect()
        };
        let mut ctx_spans = vec![Span::styled("    ctx ", Style::default().fg(fg_muted()))];
        if let Some(p) = pct {
            let comp_display = if has_acp {
                format!(" C{}", compaction_count)
            } else {
                String::new()
            };
            let sparkline_suffix = if sparkline.is_empty() {
                String::new()
            } else {
                format!("  {}", sparkline)
            };
            // "    ctx " (8) + bar + " " (1) + pct (4) + comp + sparkline
            let fixed: usize = 13 + comp_display.len() + sparkline_suffix.len();
            let bar_w = (w as usize).saturating_sub(fixed).max(2);
            let p_clamped = p.clamp(0.0, 1.0);
            let filled = (p_clamped * bar_w as f32) as usize;
            let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
            ctx_spans.push(Span::styled(bar, Style::default().fg(context_bar_color(p))));
            ctx_spans.push(Span::raw(" "));
            ctx_spans.push(Span::styled(
                format!("{:3.0}%", p_clamped * 100.0),
                Style::default().fg(fg_secondary()),
            ));
            if !comp_display.is_empty() {
                ctx_spans.push(Span::styled(comp_display, Style::default().fg(fg_muted())));
            }
            if !sparkline_suffix.is_empty() {
                ctx_spans.push(Span::styled(
                    sparkline_suffix,
                    Style::default().fg(fg_muted()),
                ));
            }
        } else {
            ctx_spans.push(Span::styled("\u{2014}", Style::default().fg(fg_muted())));
        }
        frame.render_widget(
            Paragraph::new(Line::from(ctx_spans)),
            Rect {
                x: area.x,
                y: ctx_y,
                width: w,
                height: 1,
            },
        );
    }

    // ── Pinned: meta row ──
    if area.height >= 2 {
        let dur_s = app
            .agent_start_times
            .get(&agent.id)
            .map(|t| t.elapsed().as_secs() as u32)
            .or_else(|| agent.detail.as_ref().map(|d| d.duration_s))
            .unwrap_or(0);
        let elapsed = format_duration(dur_s);
        let version = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .map(|a| a.agent_version.clone())
            .filter(|v| !v.is_empty())
            .unwrap_or_default();
        let turns = agent
            .detail
            .as_ref()
            .and_then(|d| d.acp.as_ref())
            .map(|a| a.turn_count)
            .unwrap_or(0);
        let mut meta_parts = Vec::new();
        if !version.is_empty() {
            meta_parts.push(version);
        }
        meta_parts.push(elapsed);
        if turns > 0 {
            meta_parts.push(format!("{} turns", turns));
        }
        if !agent.model.is_empty() {
            meta_parts.push(truncate_str(&agent.model, 16));
        }
        let meta_line = format!("    {}", meta_parts.join(" \u{00B7} "));
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate_str(&meta_line, w as usize),
                Style::default().fg(fg_muted()),
            )),
            Rect {
                x: area.x,
                y: meta_y,
                width: w,
                height: 1,
            },
        );
    }

    // ── Pinned: action buttons (hover targets AgentHover::FsActionBtn) ──
    if area.height >= 1 {
        let buttons = card_buttons(&agent.status);
        let mut btn_spans = vec![Span::raw("    ")];
        for (slot, (label, is_danger)) in buttons.iter().enumerate() {
            if slot > 0 {
                btn_spans.push(Span::raw("  "));
            }
            let (fg, bg) = if app.agent_hovered == Some(AgentHover::FsActionBtn(slot as u8)) {
                (
                    bg_primary(),
                    if *is_danger {
                        accent_error()
                    } else {
                        accent_hover()
                    },
                )
            } else if *is_danger {
                (accent_error(), bg_primary())
            } else {
                (fg_muted(), bg_primary())
            };
            btn_spans.push(Span::styled(*label, Style::default().fg(fg).bg(bg)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(btn_spans)),
            Rect {
                x: area.x,
                y: buttons_y,
                width: w,
                height: 1,
            },
        );
    }
}

/// Render one tool call timeline row.
// ratatui render functions have many positional params by nature; no meaningful struct abstraction available
#[allow(clippy::too_many_arguments)]
fn render_timeline_row(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    tool: &orbt_protocol::ToolCall,
    is_live: bool,
    is_selected: bool,
    is_longest: bool,
    max_dur: u64,
    bar_max: usize,
    tick: u64,
) {
    let tool_color = tool_name_color(&tool.tool);
    let row_bg = if is_selected { bg_card() } else { bg_primary() };

    let live_prefix = if is_live {
        let pulse = working_pulse_color(tick);
        Span::styled("\u{25CF} ", Style::default().fg(pulse).bg(row_bg))
    } else {
        Span::styled("  ", Style::default().bg(row_bg))
    };
    let sel_mark = if is_selected {
        Span::styled("\u{25BA} ", Style::default().fg(accent()).bg(row_bg))
    } else {
        Span::styled("  ", Style::default().bg(row_bg))
    };

    let tool_name = format!("{:<7}", truncate_str(&tool.tool, 7));
    let dur_ms = tool.duration_ms.map(|d| d as u64).unwrap_or(0);
    let filled = compute_bar_filled(dur_ms, max_dur, bar_max);
    let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_max - filled);

    let dur_str = if is_live {
        // Running: show elapsed as "…".
        format!("{:.1}s\u{2026}", dur_ms as f64 / 1000.0)
    } else {
        format!("{:.1}s", dur_ms as f64 / 1000.0)
    };
    let longest_mark = if is_longest && !is_live { "*" } else { " " };

    // Layout: live(2) + sel(2) + tool_name(7) + "  " + arg + "  " + bar + "  " + dur + longest = w
    let fixed_w = 2 + 2 + 7 + 2 + 2 + bar_max + 2 + dur_str.len() + 1;
    let arg_max = (w as usize).saturating_sub(fixed_w).max(1);
    let arg = truncate_str(&tool.args_summary, arg_max);
    let arg_padded = format!("{:<width$}", arg, width = arg_max);

    // Selected row with copyable output gets a right-aligned [y] affordance.
    let copy_chip = if is_selected && tool.output.is_some() && w >= 4 {
        Some(Span::styled(
            "[y]",
            Style::default().fg(accent_idle()).bg(row_bg),
        ))
    } else {
        None
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            live_prefix,
            sel_mark,
            Span::styled(tool_name, Style::default().fg(tool_color).bg(row_bg)),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(arg_padded, Style::default().fg(fg_secondary()).bg(row_bg)),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(bar, Style::default().fg(tool_color).bg(row_bg)),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(dur_str, Style::default().fg(fg_muted()).bg(row_bg)),
            Span::styled(longest_mark, Style::default().fg(fg_secondary()).bg(row_bg)),
        ])),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
    if let Some(chip) = copy_chip {
        frame.render_widget(
            chip,
            Rect {
                x: x + w - 3,
                y,
                width: 3,
                height: 1,
            },
        );
    }
}

/// Render the virtual "Think" row shown when agent is Working with no current tool.
fn render_think_row(
    frame: &mut Frame,
    x: u16,
    y: u16,
    w: u16,
    is_selected: bool,
    think_secs: u64,
    bar_max: usize,
) {
    let row_bg = if is_selected { bg_card() } else { bg_primary() };
    let pulse = working_pulse_color(think_secs * 60); // re-use pulse with slow counter
    let sel_mark = if is_selected {
        Span::styled("\u{25BA} ", Style::default().fg(accent()).bg(row_bg))
    } else {
        Span::styled("  ", Style::default().bg(row_bg))
    };
    // Bar: ~20% fill to indicate model is thinking.
    let filled = (bar_max / 5).max(1);
    let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_max - filled);
    let dur_str = format!("{:.1}s\u{2026}", think_secs);
    let fixed_w = 2 + 2 + 7 + 2 + 2 + bar_max + 2 + dur_str.len() + 1;
    let arg_max = (w as usize).saturating_sub(fixed_w).max(1);
    let arg_padded = format!("{:<width$}", "generating\u{2026}", width = arg_max);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("\u{25CF} ", Style::default().fg(pulse).bg(row_bg)),
            sel_mark,
            Span::styled(
                format!("{:<7}", "Think"),
                Style::default().fg(accent_blocked()).bg(row_bg),
            ),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(arg_padded, Style::default().fg(fg_muted()).bg(row_bg)),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(bar, Style::default().fg(accent_blocked()).bg(row_bg)),
            Span::styled("  ", Style::default().bg(row_bg)),
            Span::styled(dur_str, Style::default().fg(fg_muted()).bg(row_bg)),
            Span::styled(" ", Style::default().bg(row_bg)),
        ])),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
}

/// Render the Inspect Overlay — a full-detail view of a single ToolCall.
/// Renders as a 90%-screen-sized floating modal, last in Z-order (on top of everything).
/// No-ops when `app.inspect_overlay` is `None`.
pub fn render_inspect_overlay(frame: &mut Frame, screen: Rect, app: &App) {
    let Some(overlay) = &app.inspect_overlay else {
        return;
    };
    let tc = &overlay.tool_call;

    // Modal: 90% of screen, centered.
    let modal_w = (screen.width * 90 / 100).max(60).min(screen.width);
    let modal_h = (screen.height * 90 / 100).max(20).min(screen.height);
    let x = screen.x + screen.width.saturating_sub(modal_w) / 2;
    let y = screen.y + screen.height.saturating_sub(modal_h) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    frame.render_widget(Clear, area);

    let dur_str = tc
        .duration_ms
        .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
        .unwrap_or_else(|| "\u{2014}".to_string());
    let title = format!(" {} - {} - {} ", tc.tool, tc.args_summary, dur_str);
    let hint = " y copy   j/k scroll   Esc close ";

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent()))
        .title(Span::styled(title, Style::default().fg(accent())))
        .title_bottom(Span::styled(hint, Style::default().fg(fg_muted())));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build content lines from the tool output (or placeholder when unavailable).
    let content: String;
    let content_lines: Vec<&str> = match &tc.output {
        Some(s) => {
            content = s.clone();
            content.lines().collect()
        }
        None => {
            vec!["(output not available \u{2014} tool output transmission not yet implemented)"]
        }
    };

    let scroll = overlay.scroll;
    let visible_height = inner.height as usize;
    for (i, line) in content_lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .enumerate()
    {
        let row = inner.y + i as u16;
        if row >= inner.y + inner.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(*line).style(Style::default().fg(fg_primary())),
            Rect {
                x: inner.x,
                y: row,
                width: inner.width,
                height: 1,
            },
        );
    }

    // Scroll position indicator when content overflows.
    if content_lines.len() > visible_height {
        let scroll_info = format!(" {}/{} ", scroll + 1, content_lines.len());
        let w = scroll_info.len() as u16;
        let sx = area.x + area.width.saturating_sub(w + 1);
        let sy = area.y + area.height - 1;
        frame.render_widget(
            Span::styled(scroll_info, Style::default().fg(fg_muted())),
            Rect {
                x: sx,
                y: sy,
                width: w,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        card_buttons, compute_bar_filled, context_bar_color, format_tokens, fs_modal_layout,
        render_fullscreen_modal, render_fullscreen_table_header, sparkline_char, tool_name_color,
    };
    use crate::tui::theme::{
        accent_blocked, accent_error, accent_hover, accent_idle, fg_muted, fg_primary, fg_secondary,
    };
    use orbt_protocol::AgentStatus;

    #[test]
    fn card_buttons_v2_working() {
        let btns = card_buttons(&AgentStatus::Working);
        assert_eq!(btns[0].0, "[Focus]");
        assert_eq!(btns[1].0, "[Interrupt]");
        assert_eq!(btns[2].0, "[Stop]");
        assert!(!btns[2].1); // Stop is not danger
    }

    #[test]
    fn card_buttons_v2_blocked() {
        let btns = card_buttons(&AgentStatus::Blocked);
        assert_eq!(btns[1].0, "[Respond]");
        assert_eq!(btns[2].0, "[Abort]");
        assert!(btns[2].1); // Abort is danger
    }

    #[test]
    fn card_buttons_v2_done() {
        let btns = card_buttons(&AgentStatus::Done);
        assert_eq!(btns[1].0, "[Restart]");
        assert_eq!(btns[2].0, "[Dismiss]");
    }

    #[test]
    fn format_tokens_values() {
        assert_eq!(format_tokens(0), "\u{2014}");
        assert_eq!(format_tokens(340_000), "340k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }

    #[test]
    fn context_bar_color_thresholds() {
        // < 70% → accent_idle (cyan)
        let c = context_bar_color(0.5);
        assert_eq!(c, accent_idle());
        // 70–90% → accent_blocked (amber)
        let c = context_bar_color(0.8);
        assert_eq!(c, accent_blocked());
        // boundary: exactly 0.90 is still amber, not red
        assert_eq!(context_bar_color(0.90), accent_blocked());
        // > 90% → accent_error (pink-red)
        let c = context_bar_color(0.95);
        assert_eq!(c, accent_error());
    }

    #[test]
    fn tool_color_mapping() {
        assert_eq!(tool_name_color("Edit"), accent_idle());
        assert_eq!(tool_name_color("Write"), accent_idle());
        assert_eq!(tool_name_color("Bash"), accent_hover());
        assert_eq!(tool_name_color("Think"), accent_blocked());
        assert_eq!(tool_name_color("Read"), fg_muted());
        assert_eq!(tool_name_color("Glob"), fg_muted());
        assert_eq!(tool_name_color("Grep"), fg_muted());
        assert_eq!(tool_name_color("Agent"), fg_primary());
        assert_eq!(tool_name_color("Unknown"), fg_secondary());
    }

    #[test]
    fn timeline_bar_width_zero_max_dur() {
        // When all durations are 0, bar should show 0 filled chars without panic.
        let filled = compute_bar_filled(0, 0, 10);
        assert_eq!(filled, 0);
        // Also test non-zero duration with zero max (edge case).
        let filled = compute_bar_filled(500, 0, 10);
        assert_eq!(filled, 0);
    }

    #[test]
    fn sparkline_char_zero_max() {
        // When max is 0 (no data), return the lowest bar.
        assert_eq!(sparkline_char(0, 0), '\u{2581}');
    }

    #[test]
    fn sparkline_char_full() {
        // When value == max, return the highest bar.
        assert_eq!(sparkline_char(100, 100), '\u{2588}');
    }

    #[test]
    fn sparkline_char_mid() {
        // Mid-range returns a middle character from the 8-char set.
        let c = sparkline_char(50, 100);
        assert!("\u{2581}\u{2582}\u{2583}\u{2584}\u{2585}\u{2586}\u{2587}\u{2588}".contains(c));
    }

    #[test]
    fn effective_monitor_mode_narrow_overrides() {
        // Narrow panel (<=30) always returns Compact regardless of stored preference.
        use crate::app::AgentMonitorMode;
        let mut app = crate::app::tests::make_test_app(80, 24);
        app.agent_monitor_mode = AgentMonitorMode::Card;
        // Narrow width: preference is overridden to Compact.
        assert_eq!(app.effective_monitor_mode(25), AgentMonitorMode::Compact);
        // Wide width: stored preference is returned as-is.
        assert_eq!(app.effective_monitor_mode(50), AgentMonitorMode::Card);
    }

    // ── Regression tests for Agent Monitor v2 bugs ─────────────────────────────────

    /// BUG 4: Full-screen table header at w=60 must not merge "St" and "Model" columns.
    #[test]
    fn fullscreen_header_st_model_not_merged() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_fullscreen_table_header(f, 0, 0, 60))
            .unwrap();

        let buf = terminal.backend().buffer();
        let header_line: String = (0..80u16)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect::<String>();
        // "St" and "Model" must NOT appear as the merged substring "StModel".
        assert!(
            !header_line.contains("StModel"),
            "header should not have 'StModel' merged; got: {}",
            header_line
        );
        // There must be at least one space between "St" and "Model".
        assert!(
            header_line.contains("St M") || header_line.contains("St  M"),
            "header should have whitespace between 'St' and 'Model'; got: {}",
            header_line
        );
    }

    /// BUG 4: Full-screen table header at w>=45 must not merge "Tok" and "Turns" columns.
    #[test]
    fn fullscreen_header_tok_turns_not_merged() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_fullscreen_table_header(f, 0, 0, 50))
            .unwrap();

        let buf = terminal.backend().buffer();
        let header_line: String = (0..80u16)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect::<String>();
        // "Tok" and "Turns" must NOT appear as the merged substring "TokTurns".
        assert!(
            !header_line.contains("TokTurns"),
            "header should not have 'TokTurns' merged; got: {}",
            header_line
        );
    }

    /// Modal form shell: header row sits INSIDE the border with [+] [Sidebar] [×],
    /// and the action button row is pinned to the right region's bottom.
    #[test]
    fn fullscreen_modal_shell_layout() {
        use crate::app::tests::make_test_app;
        use ratatui::backend::TestBackend;
        use ratatui::layout::Rect;
        use ratatui::Terminal;

        let mut app = make_test_app(200, 50);
        app.agent_fleet_enabled = true;
        app.mode = crate::app::InputMode::AgentFullScreen {
            left_selected: 0,
            right_selected: 0,
            focus_right: false,
        };

        let backend = TestBackend::new(200, 50);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_fullscreen_modal(f, Rect::new(0, 0, 200, 50), &app))
            .unwrap();

        let buf = terminal.backend().buffer();
        let line_at = |y: u16| -> String {
            (0..200u16)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        };
        let layout = fs_modal_layout(Rect::new(0, 0, 200, 50), false);

        let header = line_at(layout.header_y);
        assert!(header.contains("Agent Fleet (1)"), "header: {}", header);
        assert!(header.contains("[+]"), "header buttons: {}", header);
        assert!(header.contains("[Sidebar]"), "header buttons: {}", header);
        assert!(header.contains("[\u{00D7}]"), "header buttons: {}", header);
        // Header is inside the border: border row above must not carry buttons.
        let border_row = line_at(layout.area.y);
        assert!(!border_row.contains("[+]"), "border row: {}", border_row);

        // Pinned action row at the bottom of the right region.
        let buttons_y = layout.right.y + layout.right.height - 1;
        let btn_row = line_at(buttons_y);
        assert!(btn_row.contains("[Focus]"), "button row: {}", btn_row);
        // Footer hint sits on the last inner row.
        let hint_row = line_at(layout.footer_hint_y);
        assert!(hint_row.contains("Esc close"), "hint row: {}", hint_row);
    }
}
