use orbt_protocol::AgentStatus;
use ratatui::{layout::Rect, Frame};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Clear, Paragraph};

    use crate::tui::theme::*;

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(bg_primary())),
        area,
    );

    let (fleet_badge, agent_label, agent_badge_w): (String, String, u16) =
        if app.agent_fleet_enabled {
            let badge = if !app.agent_panel_mode.is_visible() && !app.agents.is_empty() {
                let n_blocked = app
                    .agents
                    .iter()
                    .filter(|a| a.status == AgentStatus::Blocked)
                    .count();
                let n_working = app
                    .agents
                    .iter()
                    .filter(|a| a.status == AgentStatus::Working)
                    .count();
                let n_error = app
                    .agents
                    .iter()
                    .filter(|a| a.status == AgentStatus::Error)
                    .count();
                if n_blocked > 0 {
                    format!(" \u{25CE}{}", n_blocked)
                } else if n_working > 0 {
                    format!(" \u{25CF}{}", n_working)
                } else if n_error > 0 {
                    format!(" \u{25C9}{}", n_error)
                } else {
                    format!(" \u{25CB}{}", app.agents.len())
                }
            } else {
                String::new()
            };
            let label = format!(" [A] Agent Fleet{} ", badge);
            let w = label.chars().count() as u16;
            (badge, label, w)
        } else {
            (String::new(), String::new(), 0)
        };

    let max_tabs_w = area.width.saturating_sub(3 + agent_badge_w + 1);
    let mut spans: Vec<Span> = Vec::new();
    let mut used_w: u16 = 0;
    let mut truncated = false;

    for (i, tab) in app.tabs.iter().enumerate() {
        let label = format!(" {} ", tab.name);
        let label_w = label.chars().count() as u16 + 1;
        if used_w + label_w > max_tabs_w {
            truncated = true;
            break;
        }
        if i > 0 {
            spans.push(Span::raw(" "));
            used_w += 1;
        }
        used_w += label_w - 1;
        let (bg, fg, mods) = if tab.id == app.active_tab_id {
            (accent(), bg_primary(), Modifier::BOLD)
        } else if app.tab_hovered == Some(i) {
            (accent_hover(), fg_primary(), Modifier::empty())
        } else {
            (bg_card(), fg_muted(), Modifier::empty())
        };
        spans.push(Span::styled(
            label,
            Style::default().fg(fg).bg(bg).add_modifier(mods),
        ));
    }

    if truncated {
        spans.push(Span::styled(
            " \u{2026} ",
            Style::default().fg(fg_muted()).bg(bg_secondary()),
        ));
    }

    let new_tab_fg = if app.tab_hovered == Some(app.tabs.len()) {
        accent()
    } else {
        fg_muted()
    };
    spans.push(Span::styled(
        " + ",
        Style::default().fg(new_tab_fg).bg(bg_card()),
    ));

    let used_width: u16 = spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum::<u16>();
    let fill_len = area.width.saturating_sub(used_width + agent_badge_w) as usize;
    spans.push(Span::styled(
        " ".repeat(fill_len),
        Style::default().bg(bg_secondary()),
    ));

    if app.agent_fleet_enabled {
        let (agent_fg, agent_bg) = if app.agent_panel_mode.is_visible() {
            (bg_primary(), accent())
        } else if app.tab_hovered == Some(app.tabs.len() + 1) {
            (fg_primary(), accent_hover())
        } else {
            (fg_muted(), bg_card())
        };
        let badge_icon_color = if !app.agent_panel_mode.is_visible() {
            let n_blocked = app
                .agents
                .iter()
                .filter(|a| a.status == AgentStatus::Blocked)
                .count();
            let n_working = app
                .agents
                .iter()
                .filter(|a| a.status == AgentStatus::Working)
                .count();
            if n_blocked > 0 {
                Some(accent_blocked())
            } else if n_working > 0 {
                Some(accent())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(icon_color) = badge_icon_color {
            let base = " [A] Agent Fleet";
            let badge = format!("{} ", fleet_badge);
            spans.push(Span::styled(
                base,
                Style::default().fg(agent_fg).bg(agent_bg),
            ));
            spans.push(Span::styled(
                badge,
                Style::default().fg(icon_color).bg(agent_bg),
            ));
        } else {
            spans.push(Span::styled(
                agent_label,
                Style::default().fg(agent_fg).bg(agent_bg),
            ));
        }
    }

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}
