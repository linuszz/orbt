use orbt_protocol::{AgentId, AgentStatus, FileKind, ToolCallStatus};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{AgentDetailModalState, App};
use crate::tui::theme::*;
use crate::tui::widgets::agent_monitor::{status_icon, status_label, working_pulse_color};

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('\u{2026}');
        t
    }
}

fn format_tokens(n: u32) -> String {
    // Insert thousands separators.
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_duration(secs: u32) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn render_row(frame: &mut Frame, text: &str, x: u16, y: u16, w: u16, color: ratatui::style::Color) {
    let padded = format!(
        "{:<width$}",
        truncate_str(text, w as usize),
        width = w as usize
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            padded,
            Style::default().fg(color).bg(bg_secondary()),
        )),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
}

fn render_section_header(frame: &mut Frame, title: &str, x: u16, y: u16, w: u16) {
    let line = format!("\u{2500}\u{2500} {} \u{2500}", title);
    let total = w as usize;
    let content = if line.chars().count() < total {
        let extra = total - line.chars().count();
        format!("{}{}", line, "\u{2500}".repeat(extra))
    } else {
        line.chars().take(total).collect()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            content,
            Style::default().fg(border()).bg(bg_secondary()),
        )),
        Rect {
            x,
            y,
            width: w,
            height: 1,
        },
    );
}

/// Open the Agent Detail modal for `agent_id`. No-op if the Eclipse modal is open.
pub fn open(app: &mut App, agent_id: AgentId) {
    if app.eclipse_modal.is_some() {
        return;
    }
    let Some(agent) = app.agents.iter().find(|a| a.id == agent_id) else {
        return;
    };

    let duration_s = app
        .agent_start_times
        .get(&agent_id)
        .map(|t| t.elapsed().as_secs() as u32)
        .unwrap_or_else(|| agent.detail.as_ref().map(|d| d.duration_s).unwrap_or(0));

    let cwd = app
        .spaces
        .iter()
        .find(|s| s.space_id == agent.space_id)
        .and_then(|s| {
            std::path::Path::new(&s.cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| format!("~/{}", n))
        });

    let (current_tool, recent_tools, total_tool_calls, tokens_in, tokens_out, files_touched) =
        if let Some(acp) = agent.detail.as_ref().and_then(|d| d.acp.as_ref()) {
            (
                acp.current_tool.clone(),
                acp.recent_tools.clone(),
                acp.total_tool_calls,
                acp.tokens_in,
                acp.tokens_out,
                acp.files_touched.clone(),
            )
        } else {
            (None, vec![], 0, 0, 0, vec![])
        };

    app.agent_detail_modal = Some(AgentDetailModalState {
        agent_id,
        agent_name: agent.name.clone(),
        status: agent.status.clone(),
        duration_s,
        model: agent.model.clone(),
        cwd,
        task: agent.detail.as_ref().and_then(|d| d.task.clone()),
        tokens_in,
        tokens_out,
        current_tool,
        recent_tools,
        total_tool_calls,
        files_touched,
        tool_scroll: 0,
        recent_output_lines: app
            .agent_metrics
            .get(&agent_id)
            .map(|m| m.recent_lines.clone())
            .unwrap_or_default(),
    });
    app.needs_redraw = true;
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(modal) = &app.agent_detail_modal else {
        return;
    };

    let modal_w = 70u16.min(area.width.saturating_sub(4));
    let modal_h = 30u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(modal_w)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_h)) / 2;
    let modal_area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    frame.render_widget(Clear, modal_area);

    let icon = status_icon(&modal.status);
    let title_color = match &modal.status {
        AgentStatus::Working => accent(),
        AgentStatus::Blocked => accent_blocked(),
        AgentStatus::Error => accent_error(),
        _ => fg_primary(),
    };
    let title = format!(
        " {} Agent Detail \u{2014} {} ",
        icon,
        truncate_str(&modal.agent_name, 24)
    );
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color))
        .style(Style::default().bg(bg_secondary()));
    frame.render_widget(block, modal_area);

    let ix = modal_area.x + 1;
    let iw = modal_area.width.saturating_sub(2);
    let mut row = modal_area.y + 1;
    // Reserve last 2 rows for divider + button row.
    let content_bottom = modal_area.y + modal_area.height.saturating_sub(3);

    // Info row 1: icon  status  ·  duration  [·  tokens]
    {
        let label = status_label(&modal.status).trim_matches(|c: char| c == '[' || c == ']');
        let dur = format_duration(modal.duration_s);
        let tok_part = if modal.tokens_in > 0 || modal.tokens_out > 0 {
            let total = modal.tokens_in.saturating_add(modal.tokens_out);
            format!(
                "  \u{00B7}  in:{} out:{} total:{}",
                format_tokens(modal.tokens_in),
                format_tokens(modal.tokens_out),
                format_tokens(total)
            )
        } else {
            String::new()
        };
        let info1 = format!(" {}  {}  \u{00B7}  {}{}", icon, label, dur, tok_part);
        render_row(frame, &info1, ix, row, iw, fg_primary());
        row += 1;
    }

    // Info row 2: model  cwd
    {
        let cwd_part = modal.cwd.as_deref().unwrap_or("");
        let info2 = if modal.model.is_empty() {
            format!(" {}", cwd_part)
        } else {
            format!(" {}  {}", modal.model, cwd_part)
        };
        render_row(frame, &info2, ix, row, iw, fg_muted());
        row += 1;
    }

    if row > content_bottom {
        render_buttons(frame, modal, ix, modal_area.y + modal_area.height - 2, iw);
        return;
    }

    // Current Operation section
    if let Some(ct) = &modal.current_tool {
        if row <= content_bottom {
            render_section_header(frame, "Current Operation", ix, row, iw);
            row += 1;
        }
        if row <= content_bottom {
            let op = format!(
                " \u{25B6} {}({})  running",
                ct.tool,
                truncate_str(&ct.args_summary, 40)
            );
            render_row(frame, &op, ix, row, iw, working_pulse_color(app.tick_count));
            row += 1;
        }
    } else if let Some(task) = &modal.task {
        if row <= content_bottom {
            render_section_header(frame, "Task", ix, row, iw);
            row += 1;
        }
        if row <= content_bottom {
            render_row(frame, &format!(" {}", task), ix, row, iw, fg_secondary());
            row += 1;
        }
    } else {
        // No task and no current operation — show a placeholder so the modal is not blank.
        if row <= content_bottom {
            render_section_header(frame, "Activity", ix, row, iw);
            row += 1;
        }
        if row <= content_bottom {
            render_row(frame, "  No activity data yet", ix, row, iw, fg_muted());
            row += 1;
        }
    }

    // Tool History section
    if row <= content_bottom && (!modal.recent_tools.is_empty() || modal.total_tool_calls > 0) {
        let hist_title = format!("Tool History  ({} total)", modal.total_tool_calls);
        render_section_header(frame, &hist_title, ix, row, iw);
        row += 1;

        // Above indicator
        if modal.tool_scroll > 0 && row <= content_bottom {
            render_row(
                frame,
                &format!(" \u{25B4} {} above", modal.tool_scroll),
                ix,
                row,
                iw,
                fg_muted(),
            );
            row += 1;
        }

        // +3 reserve for tokens/files/more indicator rows
        let available_rows = content_bottom.saturating_sub(row + 3) as usize;
        for (i, tc) in modal
            .recent_tools
            .iter()
            .skip(modal.tool_scroll)
            .take(available_rows)
            .enumerate()
        {
            if row > content_bottom {
                break;
            }
            let call_num = modal
                .total_tool_calls
                .saturating_sub((modal.tool_scroll + i) as u32);
            let (sym, col) = match tc.status {
                ToolCallStatus::Running => ("\u{25CF}", working_pulse_color(app.tick_count)),
                ToolCallStatus::Done => ("\u{2713}", fg_secondary()),
                ToolCallStatus::Error => ("\u{00D7}", accent_error()),
            };
            let dur_str = tc
                .duration_ms
                .map(|d| format!("  {}ms", d))
                .unwrap_or_default();
            let args_max = iw.saturating_sub(16 + dur_str.len() as u16) as usize;
            let line = format!(
                " #{:<3} {} {:<8} {:<args_max$}{}",
                call_num,
                sym,
                tc.tool,
                truncate_str(&tc.args_summary, args_max),
                dur_str
            );
            render_row(frame, &line, ix, row, iw, col);
            row += 1;
        }

        let shown = modal
            .recent_tools
            .len()
            .saturating_sub(modal.tool_scroll)
            .min(available_rows);
        let remaining = modal
            .recent_tools
            .len()
            .saturating_sub(modal.tool_scroll + shown);
        if remaining > 0 && row <= content_bottom {
            render_row(
                frame,
                &format!("  \u{25BE} {} more", remaining),
                ix,
                row,
                iw,
                fg_muted(),
            );
            row += 1;
        }
    }

    // Files Touched section
    if row <= content_bottom && !modal.files_touched.is_empty() {
        let files_title = format!("Files Touched  ({})", modal.files_touched.len());
        render_section_header(frame, &files_title, ix, row, iw);
        row += 1;

        let max_path = (iw / 2).saturating_sub(4) as usize;
        for chunk in modal.files_touched.chunks(2) {
            if row > content_bottom {
                break;
            }
            let mut spans: Vec<Span> = vec![Span::raw("  ")];
            for (ci, ft) in chunk.iter().enumerate() {
                if ci > 0 {
                    spans.push(Span::raw("   "));
                }
                let (kind_char, kind_color) = match ft.kind {
                    FileKind::Modified => ("M", accent_hover()),
                    FileKind::Read => ("R", fg_muted()),
                    FileKind::Created => ("C", accent()),
                    FileKind::Deleted => ("D", accent_error()),
                };
                spans.push(Span::styled(
                    kind_char,
                    Style::default().fg(kind_color).bg(bg_secondary()),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    truncate_str(&ft.path, max_path),
                    Style::default().fg(fg_secondary()).bg(bg_secondary()),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    x: ix,
                    y: row,
                    width: iw,
                    height: 1,
                },
            );
            row += 1;
        }
    }

    // Output section: last ≤10 lines from the heuristic ring buffer, newest at bottom.
    if row <= content_bottom && !modal.recent_output_lines.is_empty() {
        let divider = format!(
            "\u{2500}\u{2500} Output {}",
            "\u{2500}".repeat(iw.saturating_sub(10) as usize)
        );
        frame.render_widget(
            Line::from(Span::styled(divider, Style::default().fg(fg_muted()))),
            Rect {
                x: ix,
                y: row,
                width: iw,
                height: 1,
            },
        );
        row += 1;
        for line in modal
            .recent_output_lines
            .iter()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if row > content_bottom {
                break;
            }
            frame.render_widget(
                Line::from(Span::styled(
                    format!("  {}", line),
                    Style::default().fg(fg_muted()),
                )),
                Rect {
                    x: ix,
                    y: row,
                    width: iw,
                    height: 1,
                },
            );
            row += 1;
        }
    }

    // Button row pinned to bottom
    render_buttons(frame, modal, ix, modal_area.y + modal_area.height - 2, iw);
}

fn render_buttons(frame: &mut Frame, _modal: &AgentDetailModalState, ix: u16, y: u16, iw: u16) {
    let divider = "\u{2500}".repeat(iw as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(
            divider,
            Style::default().fg(border()).bg(bg_secondary()),
        )),
        Rect {
            x: ix,
            y: y.saturating_sub(1),
            width: iw,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "[View Pane]",
                Style::default().fg(fg_muted()).bg(bg_secondary()),
            ),
            Span::raw("  "),
            Span::styled(
                "[Stop Agent]",
                Style::default().fg(accent_error()).bg(bg_secondary()),
            ),
            Span::raw("  "),
            Span::styled(
                "[Close]",
                Style::default().fg(fg_muted()).bg(bg_secondary()),
            ),
            Span::styled(
                "  Esc:close",
                Style::default().fg(fg_muted()).bg(bg_secondary()),
            ),
        ])),
        Rect {
            x: ix,
            y,
            width: iw,
            height: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::make_test_app;

    #[test]
    fn render_detail_modal_no_acp() {
        let mut app = make_test_app(80, 30);
        // Open modal for agent 0 (heuristic agent from make_test_app).
        let agent_id = app.agents[0].id;
        open(&mut app, agent_id);
        assert!(app.agent_detail_modal.is_some());

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 30)).unwrap();
        terminal.draw(|f| render(f, f.area(), &app)).unwrap();
        // Modal title should contain agent name.
        let buf = terminal.backend().buffer().clone();
        let flat: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(flat.contains("Agent Detail"), "modal title not found");
    }
}
