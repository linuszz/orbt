use orbt_protocol::AgentStatus;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, MobileView};
use crate::tui::theme::*;

/// Renders the compact 1-line header for mobile mode.
///
/// Layout: agent mini-indicator | session@host (centered) | status
pub fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }

    let n_working = app
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Working)
        .count();
    let n_blocked = app
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Blocked)
        .count();

    // Left: mini agent indicator (only when agents are present and fleet is enabled)
    let left = if app.agent_fleet_enabled && !app.agents.is_empty() {
        let blocked_str = if n_blocked > 0 {
            format!("\u{25CE}{}", n_blocked)
        } else {
            String::new()
        };
        let working_str = if n_working > 0 {
            format!("\u{25CF}{}", n_working)
        } else {
            String::new()
        };
        let parts: Vec<&str> = [blocked_str.as_str(), working_str.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect();
        if parts.is_empty() {
            format!("\u{25CB}{}", app.agents.len())
        } else {
            parts.join(" ")
        }
    } else {
        String::new()
    };

    // Center: space name (truncated)
    let session = if app.space_name.len() > 14 {
        format!("{}...", &app.space_name[..11])
    } else {
        app.space_name.clone()
    };

    let left_w = left.chars().count() as u16;
    let center_w = session.chars().count() as u16;
    let total = area.width;

    // Pad center to be centered, then fill right with spaces
    let center_x = total.saturating_sub(center_w) / 2;
    let left_pad = center_x.saturating_sub(left_w);
    let right_pad = total.saturating_sub(left_w + left_pad + center_w) as usize;

    let mut spans = Vec::new();

    // Background fill
    let blocked_icon_color = if n_blocked > 0 {
        accent_blocked()
    } else if n_working > 0 {
        accent()
    } else {
        fg_muted()
    };

    if !left.is_empty() {
        spans.push(Span::styled(
            left,
            Style::default().fg(blocked_icon_color).bg(bg_tertiary()),
        ));
    }
    if left_pad > 0 {
        spans.push(Span::styled(
            " ".repeat(left_pad as usize),
            Style::default().bg(bg_tertiary()),
        ));
    }
    spans.push(Span::styled(
        session,
        Style::default()
            .fg(fg_primary())
            .bg(bg_tertiary())
            .add_modifier(Modifier::BOLD),
    ));
    if right_pad > 0 {
        spans.push(Span::styled(
            " ".repeat(right_pad),
            Style::default().bg(bg_tertiary()),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Renders the 1-line bottom navigation bar for mobile mode.
///
/// Four equal tabs: TTY | SPACES | COMMAND | AGENTS
pub fn render_nav(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 || area.width < 8 {
        return;
    }

    // When fleet is disabled, show 3 tabs (no AGENTS).
    let (tab_w, tabs_vec): (u16, Vec<(&str, MobileView)>) = if app.agent_fleet_enabled {
        (
            area.width / 4,
            vec![
                ("TERMINAL", MobileView::Terminal),
                ("SPACES", MobileView::Windows),
                ("COMMAND", MobileView::Actions),
                ("AGENTS", MobileView::Agents),
            ],
        )
    } else {
        (
            area.width / 3,
            vec![
                ("TERMINAL", MobileView::Terminal),
                ("SPACES", MobileView::Windows),
                ("COMMAND", MobileView::Actions),
            ],
        )
    };
    let tabs = tabs_vec;

    let mut spans: Vec<Span> = Vec::new();

    for (i, (label, view)) in tabs.iter().enumerate() {
        let is_active = app.mobile_view == *view;

        // Badge for AGENTS tab: show blocked count if any
        let badge = if *view == MobileView::Agents && !app.agents.is_empty() {
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
                format!("\u{25CE}{}", n_blocked)
            } else if n_working > 0 {
                format!("\u{25CF}{}", n_working)
            } else {
                format!("{}", app.agents.len())
            }
        } else {
            String::new()
        };

        let full_label = if badge.is_empty() {
            label.to_string()
        } else {
            format!("{} {}", label, badge)
        };

        let last_i = tabs.len() - 1;
        let cell_w = if i < last_i {
            tab_w
        } else {
            area.width - tab_w * last_i as u16
        };
        let text_w = full_label.chars().count() as u16;
        let pad_l = cell_w.saturating_sub(text_w) / 2;
        let pad_r = cell_w.saturating_sub(text_w + pad_l);

        let (bg, fg, mods) = if is_active {
            (accent(), bg_primary(), Modifier::BOLD)
        } else {
            (bg_secondary(), fg_muted(), Modifier::empty())
        };

        if i > 0 {
            // thin separator between tabs
            spans.push(Span::styled(
                "\u{2502}",
                Style::default().fg(border()).bg(bg_secondary()),
            ));
        }

        spans.push(Span::styled(
            " ".repeat(pad_l as usize),
            Style::default().bg(bg),
        ));
        let badge_icon_color = if *view == MobileView::Agents && !badge.is_empty() {
            let n_blocked = app
                .agents
                .iter()
                .filter(|a| a.status == AgentStatus::Blocked)
                .count();
            if n_blocked > 0 {
                Some(accent_blocked())
            } else {
                Some(accent())
            }
        } else {
            None
        };

        if let Some(icon_color) = badge_icon_color {
            let label_only = label.to_string();
            let badge_str = format!(" {}", badge);
            spans.push(Span::styled(
                label_only,
                Style::default().fg(fg).bg(bg).add_modifier(mods),
            ));
            spans.push(Span::styled(
                badge_str,
                Style::default().fg(icon_color).bg(bg).add_modifier(mods),
            ));
        } else {
            spans.push(Span::styled(
                full_label,
                Style::default().fg(fg).bg(bg).add_modifier(mods),
            ));
        }

        if pad_r > 0 {
            spans.push(Span::styled(
                " ".repeat(pad_r as usize),
                Style::default().bg(bg),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
