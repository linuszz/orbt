use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, EclipseModalState};
use crate::tui::theme::*;

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    }
}

/// Render the Satellite Eclipse intervention modal centered in `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(modal) = &app.eclipse_modal else {
        return;
    };

    let modal_w = 64u16.min(area.width.saturating_sub(4));
    let modal_h = 18u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(modal_w)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_h)) / 2;
    let modal_area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    // Clear background and draw outer block.
    frame.render_widget(Clear, modal_area);
    let title = format!(
        " \u{25CE} Agent Eclipse — {} ",
        truncate_str(&modal.agent_name, 20)
    );
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(accent_blocked())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent_blocked()))
        .style(Style::default().bg(bg_secondary()));
    frame.render_widget(block, modal_area);

    let inner_x = modal_area.x + 1;
    let inner_w = modal_area.width.saturating_sub(2);
    let mut row = modal_area.y + 1;

    // "AGENT BLOCKED" header
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "AGENT BLOCKED",
                Style::default()
                    .fg(accent_blocked())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect {
            x: inner_x,
            y: row,
            width: inner_w,
            height: 1,
        },
    );
    row += 1;

    // "agent requires intervention"
    let subtitle = format!(" {} requires intervention", modal.agent_name);
    frame.render_widget(
        Paragraph::new(Span::styled(
            truncate_str(&subtitle, inner_w as usize),
            Style::default().fg(fg_secondary()),
        )),
        Rect {
            x: inner_x,
            y: row,
            width: inner_w,
            height: 1,
        },
    );
    row += 1;

    // Blank row
    row += 1;

    // Agent context row 1: model | task (truncated).
    if row < modal_area.y + modal_area.height.saturating_sub(4) {
        let model_part = if modal.model.is_empty() {
            String::new()
        } else {
            format!(" Model: {}", truncate_str(&modal.model, 20))
        };
        let task_part = modal
            .task
            .as_deref()
            .map(|t| format!("  Task: {}", truncate_str(t, 20)))
            .unwrap_or_default();
        let ctx1 = truncate_str(&format!("{}{}", model_part, task_part), inner_w as usize);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{:<width$}", ctx1, width = inner_w as usize),
                Style::default().fg(fg_muted()),
            )),
            Rect {
                x: inner_x,
                y: row,
                width: inner_w,
                height: 1,
            },
        );
        row += 1;
    }

    // Agent context row 2: cwd | progress bar | blocked duration.
    if row < modal_area.y + modal_area.height.saturating_sub(4) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref cwd) = modal.cwd {
            parts.push(format!(" Dir: {}", truncate_str(cwd, 14)));
        }
        if let Some(pct) = modal.progress {
            let bar_w = 6usize;
            let filled = (pct.clamp(0.0, 1.0) * bar_w as f32) as usize;
            let bar = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_w - filled);
            parts.push(format!("  {} {:3.0}%", bar, pct * 100.0));
        }
        if modal.blocked_duration_s > 0 {
            let secs = modal.blocked_duration_s;
            let dur = if secs < 60 {
                format!("{secs}s")
            } else if secs < 3600 {
                format!("{}m{}s", secs / 60, secs % 60)
            } else {
                format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
            };
            parts.push(format!("  Blocked: {}", dur));
        }
        let ctx2 = truncate_str(&parts.concat(), inner_w as usize);
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("{:<width$}", ctx2, width = inner_w as usize),
                Style::default().fg(fg_muted()),
            )),
            Rect {
                x: inner_x,
                y: row,
                width: inner_w,
                height: 1,
            },
        );
        row += 1;
    }

    // Blank row before last message
    row += 1;

    // "Last message:" label
    frame.render_widget(
        Paragraph::new(Span::styled(
            " Last message:",
            Style::default().fg(fg_muted()),
        )),
        Rect {
            x: inner_x,
            y: row,
            width: inner_w,
            height: 1,
        },
    );
    row += 1;

    // Block message box (2 rows).
    let block_w = inner_w.saturating_sub(2);
    let msg_inner_x = inner_x + 1;
    {
        let border_top = format!("\u{250c}{}\u{2510}", "\u{2500}".repeat(block_w as usize));
        frame.render_widget(
            Paragraph::new(Span::styled(border_top, Style::default().fg(border()))),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(border())),
                Span::styled(
                    format!(
                        "{:<width$}",
                        truncate_str(&modal.block_msg, block_w as usize),
                        width = block_w as usize
                    ),
                    Style::default().fg(fg_secondary()),
                ),
                Span::styled("\u{2502}", Style::default().fg(border())),
            ])),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
        let border_bot = format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(block_w as usize));
        frame.render_widget(
            Paragraph::new(Span::styled(border_bot, Style::default().fg(border()))),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
    }

    // "Your response:" label
    frame.render_widget(
        Paragraph::new(Span::styled(
            " Your response:",
            Style::default().fg(fg_muted()),
        )),
        Rect {
            x: inner_x,
            y: row,
            width: inner_w,
            height: 1,
        },
    );
    row += 1;

    // Response input box.
    {
        let border_top = format!("\u{250c}{}\u{2510}", "\u{2500}".repeat(block_w as usize));
        frame.render_widget(
            Paragraph::new(Span::styled(border_top, Style::default().fg(accent()))),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
        let cursor = "\u{2588}"; // block cursor
        let input = format!("> {}{}", modal.response, cursor);
        let input_truncated = truncate_str(&input, block_w as usize);
        let input_padded = format!("{:<width$}", input_truncated, width = block_w as usize);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("\u{2502}", Style::default().fg(accent())),
                Span::styled(input_padded, Style::default().fg(fg_primary())),
                Span::styled("\u{2502}", Style::default().fg(accent())),
            ])),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
        let border_bot = format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(block_w as usize));
        frame.render_widget(
            Paragraph::new(Span::styled(border_bot, Style::default().fg(accent()))),
            Rect {
                x: msg_inner_x,
                y: row,
                width: block_w + 2,
                height: 1,
            },
        );
        row += 1;
    }

    // Blank row
    if row < modal_area.y + modal_area.height.saturating_sub(1) {
        row += 1;
    }

    // Buttons: [Send] [Cancel] [Abort Eclipse]
    if row < modal_area.y + modal_area.height {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("[Send]", Style::default().fg(accent())),
                Span::raw("  "),
                Span::styled("[Cancel]", Style::default().fg(fg_muted())),
                Span::raw("  "),
                Span::styled("[Abort Eclipse]", Style::default().fg(accent_error())),
                Span::styled("   Enter:send  Esc:cancel", Style::default().fg(fg_muted())),
            ])),
            Rect {
                x: inner_x,
                y: row,
                width: inner_w,
                height: 1,
            },
        );
    }
}

/// Open the Eclipse modal for the given agent info.
pub fn open(state: &mut crate::app::App, agent_id: orbt_protocol::AgentId) {
    if let Some(agent) = state.agents.iter().find(|a| a.id == agent_id) {
        let block_msg = agent
            .detail
            .as_ref()
            .and_then(|d| d.block_msg.clone())
            .unwrap_or_default();
        let task = agent.detail.as_ref().and_then(|d| d.task.clone());
        let progress = agent.detail.as_ref().and_then(|d| d.progress);
        let blocked_duration_s = state
            .agent_blocked_times
            .get(&agent_id)
            .map(|t| t.elapsed().as_secs() as u32)
            .unwrap_or(0);
        let cwd = state
            .spaces
            .iter()
            .find(|s| s.space_id == agent.space_id)
            .and_then(|s| {
                std::path::Path::new(&s.cwd)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| format!("~/{}", s))
            });
        state.eclipse_modal = Some(EclipseModalState {
            agent_id,
            agent_name: agent.name.clone(),
            block_msg,
            response: String::new(),
            model: agent.model.clone(),
            task,
            progress,
            cwd,
            blocked_duration_s,
        });
        state.needs_redraw = true;
    }
}
