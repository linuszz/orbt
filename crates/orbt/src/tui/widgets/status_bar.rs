use orbt_protocol::AgentStatus;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

use crate::app::{App, InputMode};
use crate::tui::theme::*;
use crate::tui::widgets::agent_monitor::{blocked_pulse_color, working_pulse_color};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let bg = Block::default().style(Style::default().bg(bg_secondary()).fg(fg_muted()));
    frame.render_widget(bg, area);

    let mut spans: Vec<Span> = vec![];

    if matches!(app.mode, InputMode::CommandPalette { .. }) {
        spans.push(Span::styled(
            " FLIGHT DECK  Esc:cancel ",
            Style::default()
                .fg(bg_primary())
                .bg(accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(border())));
    }

    if let InputMode::Scroll { offset } = &app.mode {
        spans.push(Span::styled(
            format!(" SCROLL  -{offset} "),
            Style::default()
                .fg(bg_primary())
                .bg(accent_idle())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " scroll mode — history not available ",
            Style::default().fg(fg_muted()),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(border())));
    }

    if app.agent_fleet_enabled && matches!(app.mode, InputMode::AgentPanel { .. }) {
        spans.push(Span::styled(
            " AGENT NAV ",
            Style::default()
                .fg(bg_primary())
                .bg(accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " \u{2191}\u{2193}:nav Enter:focus s:form S:stop m:density Esc:return ",
            Style::default().fg(fg_muted()),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(border())));
    }

    if app.agent_fleet_enabled && matches!(app.mode, InputMode::AgentFullScreen { .. }) {
        spans.push(Span::styled(
            " AGENT MODAL ",
            Style::default()
                .fg(bg_primary())
                .bg(accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " Tab:panels j/k:nav y:copy Enter:inspect s:sidebar Esc:close ",
            Style::default().fg(fg_muted()),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(border())));
    }

    if matches!(app.mode, InputMode::PromptInput { .. }) {
        spans.push(Span::styled(
            " PROMPT ",
            Style::default()
                .fg(bg_primary())
                .bg(accent_idle())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " Enter:send Esc:cancel ",
            Style::default().fg(fg_muted()),
        ));
        spans.push(Span::styled(" | ", Style::default().fg(border())));
    }

    spans.push(Span::styled(
        "[SPACE] ",
        Style::default().fg(fg_muted()).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        &app.space_name,
        Style::default().fg(fg_secondary()),
    ));
    spans.push(Span::styled(" | ", Style::default().fg(border())));

    spans.push(Span::styled(
        app.current_tab_name(),
        Style::default().fg(fg_muted()),
    ));
    spans.push(Span::styled(
        "*",
        Style::default().fg(accent()).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(" | ", Style::default().fg(border())));

    if app.agent_fleet_enabled {
        // Live agent fleet summary — highest-severity status wins.
        let n_blocked = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Blocked)
            .count();
        let n_error = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Error)
            .count();
        let n_working = app
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Working)
            .count();
        let n_idle = app
            .agents
            .iter()
            .filter(|a| matches!(a.status, AgentStatus::Idle | AgentStatus::Done))
            .count();

        let (icon, label, color) = if n_blocked > 0 {
            let s = if n_blocked == 1 {
                "blocked".to_string()
            } else {
                format!("{n_blocked} blocked")
            };
            ("\u{25CE}", s, blocked_pulse_color(app.tick_count))
        } else if n_error > 0 {
            let s = if n_error == 1 {
                "error".to_string()
            } else {
                format!("{n_error} error")
            };
            ("\u{25C9}", s, accent_error())
        } else if n_working > 0 {
            let s = if n_working == 1 {
                "working".to_string()
            } else {
                format!("{n_working} working")
            };
            ("\u{25CF}", s, working_pulse_color(app.tick_count))
        } else if n_idle > 0 {
            let s = if n_idle == 1 {
                "idle".to_string()
            } else {
                format!("{n_idle} idle")
            };
            ("\u{25CB}", s, accent_idle())
        } else {
            ("\u{25CB}", "idle".to_string(), accent_idle())
        };
        spans.push(Span::styled(
            format!("{icon} {label}"),
            Style::default().fg(color),
        ));
    }

    if !app.space_path.is_empty() && app.space_path != "." {
        spans.push(Span::styled(" | ", Style::default().fg(border())));
        spans.push(Span::styled(
            &app.space_path,
            Style::default().fg(fg_secondary()),
        ));
    }

    let (local_h, local_m, utc_h, utc_m) = clock_hm();
    spans.push(Span::styled(" | ", Style::default().fg(border())));
    spans.push(Span::styled(
        format!("{local_h:02}:{local_m:02}"),
        Style::default().fg(fg_secondary()),
    ));
    // The status bar is a single unmeasured line, so anything past the right
    // edge is simply gone — and the clock sits at the end of it. Give the UTC
    // half up before the clock itself is what the sidebar squeezes off screen.
    let utc = format!(" · {utc_h:02}:{utc_m:02} UTC");
    let used: usize = spans.iter().map(|s| s.width()).sum();
    if used + unicode_width::UnicodeWidthStr::width(utc.as_str()) <= area.width as usize {
        spans.push(Span::styled(utc, Style::default().fg(fg_muted())));
    }

    let line = Line::from(spans);
    frame.render_widget(line, area);
}

/// Fill `tm` with the local-zone breakdown of `secs`. `localtime_r` on unix,
/// `localtime_s` on Windows — the same conversion with swapped arguments and a
/// different return convention.
#[cfg(unix)]
fn localtime_into(secs: libc::time_t, tm: &mut libc::tm) -> bool {
    unsafe { !libc::localtime_r(&secs, tm).is_null() }
}

#[cfg(windows)]
fn localtime_into(secs: libc::time_t, tm: &mut libc::tm) -> bool {
    unsafe { libc::localtime_s(tm, &secs) == 0 }
}

/// Hours and minutes for the local zone and for UTC, as `(lh, lm, uh, um)`.
///
/// The local half goes through the platform's own conversion so the offset and
/// daylight saving come from the system rather than a hand-rolled guess. A
/// machine with no usable zone database falls back to UTC, which is what the
/// clock showed before this read the local zone at all.
fn clock_hm() -> (u8, u8, u8, u8) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (utc_h, utc_m) = ((secs / 3600 % 24) as u8, (secs / 60 % 60) as u8);
    // A `tm` is a plain C struct of integers and one pointer, so an all-zero
    // bit pattern is a valid value for the platform call to overwrite.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if localtime_into(secs as libc::time_t, &mut tm) {
        (tm.tm_hour as u8, tm.tm_min as u8, utc_h, utc_m)
    } else {
        (utc_h, utc_m, utc_h, utc_m)
    }
}

use ratatui::widgets::Block;
