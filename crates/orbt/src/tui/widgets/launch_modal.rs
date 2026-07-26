use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, LaunchFocus, LaunchModalState, LAUNCH_AGENTS};
use crate::tui::theme::*;

pub const MODAL_W: u16 = 66;
// Fixed inner height: section labels + 4 bordered boxes + blanks + footer
//   1 agent-label  +  1+4+1 agent-box  +  1 blank
//   1 name-label   +  1+1+1 name-box   +  1 blank
//   1 model-label  +  1+1+1 model-box  +  1 blank
//   1 cwd-label    +  1+1+1 cwd-box    +  1 blank  +  1 footer = 22
pub const INNER_H: u16 = 22;
pub const MODAL_H: u16 = INNER_H + 2; // + top/bottom borders

/// Render the "Launch Agent" configuration overlay centered in `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(modal) = &app.launch_modal else {
        return;
    };

    let modal_w = MODAL_W.min(area.width.saturating_sub(4));
    let modal_h = MODAL_H.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(modal_w)) / 2;
    let y = area.y + (area.height.saturating_sub(modal_h)) / 2;
    let modal_area = Rect { x, y, width: modal_w, height: modal_h };

    frame.render_widget(Clear, modal_area);

    let outer_block = Block::default()
        .title(" Launch Agent ")
        .title_style(Style::default().fg(accent()).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border()))
        .style(Style::default().bg(bg_secondary()));
    frame.render_widget(outer_block, modal_area);

    let ix = modal_area.x + 1;
    let iw = modal_area.width.saturating_sub(2);
    let mut row = modal_area.y + 1;
    let bottom = modal_area.y + modal_area.height.saturating_sub(1);

    // ── Agent type ────────────────────────────────────────────────────────────
    render_section_label(frame, ix, row, iw, "Agent type", "↑↓ navigate");
    row += 1;

    let box_w = iw.saturating_sub(2); // 2-char left/right padding inside outer block
    let box_x = ix + 1;
    let agent_focused = modal.focus == LaunchFocus::AgentList;
    let agent_box_h = (LAUNCH_AGENTS.len() as u16 + 2).min(bottom.saturating_sub(row + 12));
    let agent_list_h = agent_box_h.saturating_sub(2); // interior rows

    render_box(frame, box_x, row, box_w, agent_box_h, agent_focused);

    for i in 0..agent_list_h as usize {
        let list_row = row + 1 + i as u16;
        if list_row >= row + agent_box_h.saturating_sub(1) {
            break;
        }
        let Some(&(cmd, label, acp)) = LAUNCH_AGENTS.get(i) else { break };
        let selected = modal.selected_agent == i;
        let (pfx_fg, cmd_fg, lbl_fg, bg) = if selected {
            (accent(), fg_primary(), fg_secondary(), bg_card())
        } else {
            (fg_muted(), fg_muted(), fg_muted(), bg_secondary())
        };
        let prefix = if selected { "\u{25B8}" } else { " " };
        let badge = if acp { "[ACP]" } else { "[heur]" };
        let badge_fg = if acp { accent_idle() } else { fg_muted() };

        // cmd<12>  label<fill>  badge
        let cmd_w: usize = 12;
        let badge_w: usize = badge.len() + 1; // +1 for leading space
        let inner_w = (box_w.saturating_sub(4)) as usize; // 2-border + 2-padding
        let label_w = inner_w
            .saturating_sub(cmd_w + 2 + badge_w);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {prefix} "), Style::default().fg(pfx_fg).bg(bg)),
                Span::styled(
                    format!("{cmd:<cmd_w$}"),
                    Style::default().fg(cmd_fg).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }).bg(bg),
                ),
                Span::styled(
                    format!("  {:<label_w$}", label),
                    Style::default().fg(lbl_fg).bg(bg),
                ),
                Span::styled(
                    format!(" {badge}"),
                    Style::default().fg(badge_fg).bg(bg),
                ),
            ])),
            Rect { x: box_x + 1, y: list_row, width: box_w.saturating_sub(2), height: 1 },
        );
    }
    row += agent_box_h;

    // ── Name ─────────────────────────────────────────────────────────────────
    row += 1;
    render_section_label(frame, ix, row, iw, "Name", "auto-generated if blank");
    row += 1;
    render_box(frame, box_x, row, box_w, 3, modal.focus == LaunchFocus::Name);
    render_text_field(frame, box_x + 1, row + 1, box_w.saturating_sub(2), &modal.name, modal.focus == LaunchFocus::Name);
    row += 3;

    // ── Model ────────────────────────────────────────────────────────────────
    row += 1;
    render_section_label(frame, ix, row, iw, "Model", "agent default if blank");
    row += 1;
    render_box(frame, box_x, row, box_w, 3, modal.focus == LaunchFocus::Model);
    render_text_field(frame, box_x + 1, row + 1, box_w.saturating_sub(2), &modal.model, modal.focus == LaunchFocus::Model);
    row += 3;

    // ── Working directory ─────────────────────────────────────────────────────
    row += 1;
    render_section_label(frame, ix, row, iw, "Working directory", "");
    row += 1;
    render_box(frame, box_x, row, box_w, 3, modal.focus == LaunchFocus::Cwd);
    render_text_field(frame, box_x + 1, row + 1, box_w.saturating_sub(2), &modal.cwd, modal.focus == LaunchFocus::Cwd);
    row += 3;

    // ── Footer ────────────────────────────────────────────────────────────────
    row += 1;
    if row < bottom {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("Tab", Style::default().fg(fg_secondary())),
                Span::styled(": next  ", Style::default().fg(fg_muted())),
                Span::styled("Shift-Tab", Style::default().fg(fg_secondary())),
                Span::styled(": prev  ", Style::default().fg(fg_muted())),
                Span::styled("Enter", Style::default().fg(accent())),
                Span::styled(": launch  ", Style::default().fg(fg_muted())),
                Span::styled("Esc", Style::default().fg(fg_secondary())),
                Span::styled(": cancel", Style::default().fg(fg_muted())),
            ])),
            Rect { x: ix, y: row, width: iw, height: 1 },
        );
    }
}

/// Section label row: left = label (fg_secondary bold), right = hint (fg_muted)
fn render_section_label(frame: &mut Frame, x: u16, y: u16, w: u16, label: &str, hint: &str) {
    let hint_len = if hint.is_empty() { 0 } else { hint.len() + 2 };
    let pad = (w as usize).saturating_sub(label.len() + hint_len + 1);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(label, Style::default().fg(fg_secondary()).add_modifier(Modifier::BOLD)),
    ];
    if !hint.is_empty() {
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(hint, Style::default().fg(fg_muted())));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect { x, y, width: w, height: 1 },
    );
}

/// Draw a bordered box; border color = accent when focused, else border().
fn render_box(frame: &mut Frame, x: u16, y: u16, w: u16, h: u16, focused: bool) {
    let border_color = if focused { accent() } else { border() };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(bg_secondary()));
    frame.render_widget(block, Rect { x, y, width: w, height: h });
}

/// Render editable text with cursor at end (single-line).
fn render_text_field(frame: &mut Frame, x: u16, y: u16, w: u16, text: &str, focused: bool) {
    // Show only the last `w-1` chars to keep cursor visible when text is long.
    let display_w = (w as usize).saturating_sub(1);
    let display = if text.len() > display_w {
        &text[text.len() - display_w..]
    } else {
        text
    };
    let mut spans = vec![Span::styled(display, Style::default().fg(fg_primary()))];
    if focused {
        // Block cursor
        spans.push(Span::styled("\u{2588}", Style::default().fg(accent())));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect { x, y, width: w, height: 1 },
    );
}

/// Open the Launch Agent modal. Pre-fills cwd from the active space.
pub fn open(app: &mut App) {
    let cwd = app
        .spaces
        .first()
        .map(|s| s.cwd.clone())
        .unwrap_or_else(|| "~/".to_string());
    app.launch_modal = Some(LaunchModalState {
        selected_agent: 0,
        focus: LaunchFocus::AgentList,
        name: String::new(),
        model: String::new(),
        cwd,
    });
    app.needs_redraw = true;
}
